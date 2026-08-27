// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Loading a contract's code and what it costs.
//!
//! The cost of a load is split into three parts:
//!
//! - flat: reading both keys, whatever the code's length.
//! - refcount write: an instantiate bumps the refcount; the benches whitelist that key, so the
//!   write is owed here.
//! - blob: the code's bytes, plus PVM compilation.

use super::{BytecodeType, CodeInfo, RuntimeCosts, runtime_costs::weight_by_warmth};
use crate::{
	CodeInfoOf, Config, Error, PristineCode, Weight,
	access_list::{CodeLoadWarmth, StorageOp, TouchedKey, Warmth},
	metering::{ResourceMeter, State, Token},
	weights::WeightInfo,
};
use alloc::vec::Vec;
use sp_core::H256;
use sp_runtime::DispatchError;

/// The price of one load, fixed once its warmth is known.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Clone, Copy)]
pub struct CodeLoadPricing {
	warmth: CodeLoadWarmth,
	/// `Write` for an instantiate, which bumps the refcount.
	code_info_op: StorageOp,
}

impl CodeLoadPricing {
	pub fn new(warmth: CodeLoadWarmth, code_info_op: StorageOp) -> Self {
		Self { warmth, code_info_op }
	}

	/// The `code_load` flat cost.
	fn flat<T: Config>(&self) -> Weight {
		weight_by_warmth::<T, _>(
			[self.warmth.info, self.warmth.blob],
			TouchedKey::Address,
			|| T::WeightInfo::code_load(),
			|| Weight::zero(), // nothing beyond the overhead every hot read pays
		)
	}

	/// The write half of the refcount bump; `flat` already paid the read half.
	fn refcount_write<T: Config>(&self) -> Weight {
		if self.code_info_op != StorageOp::Write {
			return Weight::zero();
		}
		match self.warmth.info {
			// Upgrading a tracked entry to `Write` also journals the upgrade.
			Warmth::Hot { charged } if !charged.covers(StorageOp::Write) => {
				RuntimeCosts::deferred_write_cost::<T>()
					.saturating_add(RuntimeCosts::access_list_upgrade_overhead::<T>())
			},
			Warmth::Hot { .. } => Weight::zero(),
			Warmth::Cold { .. } => RuntimeCosts::deferred_write_cost::<T>(),
		}
	}

	/// The blob cost.
	fn blob<T: Config>(&self, code_len: u32, code_type: BytecodeType) -> Weight {
		let blob_cost_of =
			|weight_fn: fn(u32) -> Weight| weight_fn(code_len).saturating_sub(weight_fn(0));
		let (cold_weight, hot_weight, compilation_weight) = match code_type {
			BytecodeType::Pvm => (
				blob_cost_of(T::WeightInfo::call_with_pvm_code_per_byte),
				blob_cost_of(T::WeightInfo::call_with_pvm_code_per_byte_hot),
				// The proof size impact is accounted for in `call_with_pvm_code_per_byte`, so
				// the compilation term drops its proof. It double-charges the first
				// BASIC_BLOCK_SIZE instructions; we keep that as a safety margin.
				T::WeightInfo::basic_block_compilation(1)
					.saturating_sub(T::WeightInfo::basic_block_compilation(0))
					.set_proof_size(0),
			),
			BytecodeType::Evm => (
				blob_cost_of(T::WeightInfo::call_with_evm_code_per_byte),
				blob_cost_of(T::WeightInfo::call_with_evm_code_per_byte_hot),
				Weight::zero(),
			),
		};
		let bytes_weight = if self.warmth.blob.is_hot() { hot_weight } else { cold_weight };
		bytes_weight.saturating_add(compilation_weight)
	}

	/// What the two charges of a load add up to.
	#[cfg(test)]
	pub(crate) fn total<T: Config>(&self, code_len: u32, code_type: BytecodeType) -> Weight {
		let flat_and_refcount_write =
			Token::<T>::weight(&CodeLoadToken::FlatAndRefcountWrite(*self));
		let blob = Token::<T>::weight(&CodeLoadToken::Blob { pricing: *self, code_len, code_type });
		flat_and_refcount_write.saturating_add(blob)
	}
}

/// The load's cost.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Clone, Copy)]
enum CodeLoadToken {
	/// The flat part and the refcount write.
	FlatAndRefcountWrite(CodeLoadPricing),
	/// The blob cost.
	Blob { pricing: CodeLoadPricing, code_len: u32, code_type: BytecodeType },
}

impl<T: Config> Token<T> for CodeLoadToken {
	fn weight(&self) -> Weight {
		match *self {
			Self::FlatAndRefcountWrite(pricing) => {
				pricing.flat::<T>().saturating_add(pricing.refcount_write::<T>())
			},
			Self::Blob { pricing, code_len, code_type } => pricing.blob::<T>(code_len, code_type),
		}
	}
}

/// Reads the info and the blob behind `code_hash`, charging each read before it happens.
pub(crate) fn charge_and_read<T: Config, S: State>(
	code_hash: H256,
	meter: &mut ResourceMeter<T, S>,
	pricing: CodeLoadPricing,
) -> Result<(CodeInfo<T>, Vec<u8>), DispatchError> {
	meter.charge_weight_token(CodeLoadToken::FlatAndRefcountWrite(pricing))?;
	let code_info = <CodeInfoOf<T>>::get(code_hash).ok_or(Error::<T>::CodeNotFound)?;
	meter.charge_weight_token(CodeLoadToken::Blob {
		pricing,
		code_len: code_info.code_len,
		code_type: code_info.code_type,
	})?;
	let code = <PristineCode<T>>::get(&code_hash).ok_or(Error::<T>::CodeNotFound)?;
	Ok((code_info, code))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		metering::TransactionMeter,
		test_utils::ALICE,
		tests::{ExtBuilder, Test},
	};

	#[test]
	fn the_refcount_write_is_charged_exactly_when_owed() {
		let refcount_write_cost_for = |info| {
			CodeLoadPricing::new(
				CodeLoadWarmth { info, blob: Warmth::cold_non_revertible() },
				StorageOp::Write,
			)
			.refcount_write::<Test>()
		};

		assert_eq!(
			refcount_write_cost_for(Warmth::cold_non_revertible()),
			RuntimeCosts::deferred_write_cost::<Test>(),
			"a cold bump inserts the entry at `Write`, so it owes the re-hash but no upgrade",
		);
		assert_eq!(
			refcount_write_cost_for(Warmth::Hot { charged: StorageOp::Read }),
			RuntimeCosts::deferred_write_cost::<Test>()
				.saturating_add(RuntimeCosts::access_list_upgrade_overhead::<Test>()),
			"the bump that finds the key read-paid owes the re-hash and journals the upgrade",
		);
		assert_eq!(
			refcount_write_cost_for(Warmth::Hot { charged: StorageOp::Write }),
			Weight::zero(),
			"the bump that finds the key write-paid owes nothing: the re-hash is paid once",
		);
		for info in Warmth::ALL {
			assert_eq!(
				CodeLoadPricing::new(
					CodeLoadWarmth { info, blob: Warmth::cold_non_revertible() },
					StorageOp::Read
				)
				.refcount_write::<Test>(),
				Weight::zero(),
				"a load that only reads `CodeInfoOf` owes no write, whatever its warmth: {info:?}",
			);
		}
	}

	#[test]
	fn each_cost_lands_in_exactly_one_term() {
		let flat_cost_of = |info, blob| {
			CodeLoadPricing::new(CodeLoadWarmth { info, blob }, StorageOp::Read).flat::<Test>()
		};
		let all_cold = flat_cost_of(Warmth::cold_non_revertible(), Warmth::cold_non_revertible());

		let rollback_prepay = <Test as Config>::WeightInfo::access_list_rollback_amortization();
		assert_eq!(
			flat_cost_of(Warmth::cold_non_revertible(), Warmth::cold_revertible())
				.saturating_sub(all_cold),
			rollback_prepay,
			"a revertible blob entry prepays its rollback inside `flat`",
		);
		assert_eq!(
			flat_cost_of(Warmth::cold_revertible(), Warmth::cold_non_revertible())
				.saturating_sub(all_cold),
			rollback_prepay,
			"a revertible info entry prepays its rollback inside `flat`",
		);
		for info in Warmth::ALL {
			assert_eq!(
				flat_cost_of(info, Warmth::cold_non_revertible()),
				CodeLoadPricing::new(
					CodeLoadWarmth { info, blob: Warmth::cold_non_revertible() },
					StorageOp::Write,
				)
				.flat::<Test>(),
				"the refcount bump lives in `refcount_write`, not in `flat`: {info:?}",
			);
		}
		for blob in Warmth::ALL {
			let load_cost = CodeLoadPricing::new(
				CodeLoadWarmth { info: Warmth::cold_non_revertible(), blob },
				StorageOp::Read,
			);
			assert_eq!(
				load_cost.blob::<Test>(0, BytecodeType::Evm),
				Weight::zero(),
				"the entries' touch overheads live in `flat`, not in `blob`: {blob:?}",
			);
			assert_eq!(
				load_cost.blob::<Test>(0, BytecodeType::Pvm).proof_size(),
				0,
				"compilation carries no proof: {blob:?}",
			);
		}
	}

	#[test]
	fn code_load_cold_hot_pricing() {
		let code_len = 1024_u32;
		let weight_of = |code_type, info, blob| {
			CodeLoadPricing::new(CodeLoadWarmth { info, blob }, StorageOp::Read)
				.total::<Test>(code_len, code_type)
		};
		let hot = Warmth::Hot { charged: StorageOp::Read };
		let cold_entry = Warmth::cold_non_revertible();

		for code_type in [BytecodeType::Pvm, BytecodeType::Evm] {
			let cold = weight_of(code_type, cold_entry, cold_entry);
			let all_hot = weight_of(code_type, hot, hot);
			assert!(
				cold.ref_time() > all_hot.ref_time(),
				"expected cold > hot ref_time for {code_type:?}: cold={cold:?} hot={all_hot:?}",
			);
			assert_eq!(all_hot.proof_size(), 0, "hot proof_size {code_type:?}: {all_hot:?}");

			let both_reads_proof = <Test as Config>::WeightInfo::code_load().proof_size();
			assert!(
				cold.proof_size() >= both_reads_proof + u64::from(code_len),
				"cold load must include the {both_reads_proof}-byte proof of its two reads \
				 plus {code_len} per-byte proof: {cold:?}",
			);
			let hot_info_cold_blob = weight_of(code_type, hot, cold_entry);
			assert!(
				hot_info_cold_blob.proof_size() >= both_reads_proof + u64::from(code_len),
				"a cold blob walks a trie path even when info is hot, so it owes the \
				 {both_reads_proof}-byte base too: {hot_info_cold_blob:?}",
			);

			let twice_as_long = CodeLoadPricing::new(
				CodeLoadWarmth { info: cold_entry, blob: cold_entry },
				StorageOp::Read,
			)
			.total::<Test>(code_len * 2, code_type);
			assert!(
				twice_as_long.proof_size().saturating_sub(cold.proof_size()) >= u64::from(code_len),
				"doubling the code adds at least {code_len} bytes of proof: \
				 twice={twice_as_long:?} cold={cold:?}",
			);
		}
	}

	#[test]
	fn a_load_charges_each_read_before_making_it() {
		ExtBuilder::default().build().execute_with(|| {
			let code = vec![0u8; 1024];
			let code_len = code.len() as u32;
			let mut code_info = CodeInfo::<Test>::new(ALICE);
			code_info.code_len = code_len;
			let stored = H256::repeat_byte(1);
			<CodeInfoOf<Test>>::insert(stored, code_info.clone());
			<PristineCode<Test>>::insert(stored, code.clone());
			// Info without a blob: a load that read the blob before paying for it would fail
			// with `CodeNotFound` instead of running out of weight.
			let blob_missing = H256::repeat_byte(2);
			<CodeInfoOf<Test>>::insert(blob_missing, code_info);

			let cold = Warmth::cold_non_revertible();
			let load_cost =
				CodeLoadPricing::new(CodeLoadWarmth { info: cold, blob: cold }, StorageOp::Read);
			let new_meter = |weight_limit| {
				TransactionMeter::<Test>::new_from_limits(weight_limit, u128::MAX)
					.expect("a weight-and-deposit meter always builds")
			};

			let mut meter = new_meter(Weight::MAX);
			let (_, loaded) =
				charge_and_read(stored, &mut meter, load_cost).expect("the load fits");
			assert_eq!(loaded, code, "the blob read back is the stored one");
			assert_eq!(
				meter.weight_consumed(),
				load_cost.total::<Test>(code_len, BytecodeType::Pvm),
				"a full load consumes exactly its total",
			);

			let first_charge =
				Token::<Test>::weight(&CodeLoadToken::FlatAndRefcountWrite(load_cost));
			let mut meter = new_meter(first_charge);
			assert_eq!(
				charge_and_read(blob_missing, &mut meter, load_cost).map(|_| ()),
				Err(Error::<Test>::OutOfGas.into()),
				"with weight for the info only, the blob's charge fails before the blob is read",
			);
			assert_eq!(meter.weight_consumed(), first_charge, "only the first charge landed");
		});
	}
}
