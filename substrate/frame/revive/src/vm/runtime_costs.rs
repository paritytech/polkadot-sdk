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

use crate::{
	Config,
	access_list::{CallWarmth, StorageAccessKind, StorageOp, Warmth},
	limits,
	metering::Token,
	weightinfo_extension::OnFinalizeBlockParts,
	weights::WeightInfo,
};
use frame_support::{
	traits::Get,
	weights::{Weight, constants::WEIGHT_REF_TIME_PER_SECOND},
};

/// Current approximation of the gas/s consumption considering
/// EVM execution over compiled WASM (on 4.4Ghz CPU).
/// Given the 2000ms Weight, from which 75% only are used for transactions,
/// the total EVM execution gas limit is: GAS_PER_SECOND * 2 * 0.75 ~= 60_000_000.
const GAS_PER_SECOND: u64 = 40_000_000;

/// Approximate ratio of the amount of Weight per Gas.
/// u64 works for approximations because Weight is a very small unit compared to
/// gas.
const WEIGHT_PER_GAS: u64 = WEIGHT_REF_TIME_PER_SECOND / GAS_PER_SECOND;

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Copy, Clone)]
pub enum RuntimeCosts {
	/// Base Weight of calling a host function.
	HostFn,
	/// Weight charged for executing the extcodecopy instruction.
	ExtCodeCopy(u32),
	/// Weight charged for copying data from the sandbox.
	CopyFromContract(u32),
	/// Weight charged for copying data to the sandbox.
	CopyToContract(u32),
	/// Weight of calling `seal_call_data_load``.
	CallDataLoad,
	/// Weight of calling `seal_call_data_copy`.
	CallDataCopy(u32),
	/// Weight of calling `seal_caller`.
	Caller,
	/// Weight of calling `seal_call_data_size`.
	CallDataSize,
	/// Weight of calling `seal_return_data_size`.
	ReturnDataSize,
	/// Weight of calling `toAccountId` on the `System` pre-compile.
	ToAccountId,
	/// Weight of calling `seal_origin`.
	Origin,
	/// Weight of calling `seal_code_hash`.
	CodeHash,
	/// Weight of calling `ownCodeHash` on the `System` pre-compile.
	OwnCodeHash,
	/// Weight of calling `seal_code_size`.
	CodeSize,
	/// Weight of calling `callerIsOrigin` on the `System` pre-compile.
	CallerIsOrigin,
	/// Weight of calling `callerIsRoot` on the `System` pre-compile.
	CallerIsRoot,
	/// Weight of calling `originIsRoot` on the `System` pre-compile.
	OriginIsRoot,
	/// Weight of calling `seal_address`.
	Address,
	/// Weight of calling `seal_ref_time_left`.
	RefTimeLeft,
	/// Weight of calling `weightLeft` on the `System` pre-compile.
	WeightLeft,
	/// Weight of calling `seal_balance`.
	Balance,
	/// Weight of calling `seal_balance_of`.
	BalanceOf,
	/// Weight of calling `seal_value_transferred`.
	ValueTransferred,
	/// Weight of calling `minimumBalance` on the `System` pre-compile.
	MinimumBalance,
	/// Weight of calling `seal_block_number`.
	BlockNumber,
	/// Weight of calling `seal_block_hash`.
	BlockHash,
	/// Weight of calling `seal_block_author`.
	BlockAuthor,
	/// Weight of calling `seal_gas_price`.
	GasPrice,
	/// Weight of calling `seal_base_fee`.
	BaseFee,
	/// Weight of calling `seal_now`.
	Now,
	/// Weight of calling `seal_gas_limit`.
	GasLimit,
	/// Weight of calling `seal_terminate`.
	Terminate { code_removed: bool },
	/// Weight of calling `seal_deposit_event` with the given number of topics and event size.
	DepositEvent { num_topic: u32, len: u32 },
	/// Weight of `seal_set_storage` / `seal_set_transient_storage`. `kind` picks
	/// the persistent (cold/hot) or transient bench.
	SetStorage { new_bytes: u32, old_bytes: u32, kind: StorageAccessKind },
	/// Weight of the `clearStorage` precompile / `seal_clear_transient_storage`.
	ClearStorage { len: u32, kind: StorageAccessKind },
	/// Weight of the `containsStorage` precompile / `seal_contains_transient_storage`.
	ContainsStorage { len: u32, kind: StorageAccessKind },
	/// Weight of `seal_get_storage` / `seal_get_transient_storage`.
	GetStorage { len: u32, kind: StorageAccessKind },
	/// Weight of the `takeStorage` precompile / `seal_take_transient_storage`.
	TakeStorage { len: u32, kind: StorageAccessKind },
	/// Base weight of a call-family operation.
	CallBase(CallWarmth),
	/// Weight of calling a precompile.
	PrecompileBase,
	/// Weight of calling a precompile that has a contract info.
	PrecompileWithInfoBase,
	/// Weight of reading and decoding the input to a precompile.
	PrecompileDecode(u32),
	/// Weight of the transfer performed during a call.
	/// parameter `dust_transfer` indicates whether the transfer has a `dust` value.
	CallTransferSurcharge { dust_transfer: bool },
	/// Weight per byte that is cloned by supplying the `CLONE_INPUT` flag.
	CallInputCloned(u32),
	/// Weight of calling `seal_instantiate`.
	Instantiate { input_data_len: u32, balance_transfer: bool, dust_transfer: bool },
	/// Weight of calling `Create` opcode.
	Create { init_code_len: u32, balance_transfer: bool, dust_transfer: bool },
	/// Weight of calling `Ripemd160` precompile for the given input size.
	Ripemd160(u32),
	/// Weight of calling `Sha256` precompile for the given input size.
	HashSha256(u32),
	/// Weight of calling the `System::hashBlake256` precompile function for the given input
	HashKeccak256(u32),
	/// Weight of calling the `System::hash_blake2_256` precompile function for the given input
	/// size.
	HashBlake256(u32),
	/// Weight of calling `System::hashBlake128` precompile function for the given input size.
	HashBlake128(u32),
	/// Weight of calling `ECERecover` precompile.
	EcdsaRecovery,
	/// Weight of calling `P256Verify` precompile.
	P256Verify,
	/// Weight of calling `seal_sr25519_verify` for the given input size.
	Sr25519Verify(u32),
	/// Weight charged by a precompile.
	Precompile(Weight),
	/// Weight of calling `ecdsa_to_eth_address`
	EcdsaToEthAddress,
	/// Weight of calling `get_immutable_dependency`
	GetImmutableData(u32),
	/// Weight of calling `set_immutable_dependency`
	SetImmutableData(u32),
	/// Weight of calling `Bn128Add` precompile
	Bn128Add,
	/// Weight of calling `Bn128Add` precompile
	Bn128Mul,
	/// Weight of calling `Bn128Pairing` precompile for the given number of input pairs.
	Bn128Pairing(u32),
	/// Weight of calling `Identity` precompile for the given number of input length.
	Identity(u32),
	/// Weight of calling `Blake2F` precompile for the given number of rounds.
	Blake2F(u32),
	/// Weight of calling `Modexp` precompile
	Modexp(u64),
}

/// For functions that modify storage, benchmarks are performed with one item in the
/// storage. To account for the worst-case scenario, the weight of the overhead of
/// writing to or reading from full storage is included. For transient storage writes,
/// the rollback weight is added to reflect the worst-case scenario for this operation.
macro_rules! cost_storage {
    (write_transient, $name:ident $(, $arg:expr )*) => {
        T::WeightInfo::$name($( $arg ),*)
            .saturating_add(T::WeightInfo::rollback_transient_storage())
            .saturating_add(T::WeightInfo::set_transient_storage_full()
            .saturating_sub(T::WeightInfo::set_transient_storage_empty()))
    };

    (read_transient, $name:ident $(, $arg:expr )*) => {
        T::WeightInfo::$name($( $arg ),*)
            .saturating_add(T::WeightInfo::get_transient_storage_full()
            .saturating_sub(T::WeightInfo::get_transient_storage_empty()))
    };

    (write_cold, $name:ident $(, $arg:expr )*) => {
        T::WeightInfo::$name($( $arg ),*)
            .saturating_add(T::WeightInfo::set_storage_full()
            .saturating_sub(T::WeightInfo::set_storage_empty()))
    };

    (read_cold, $name:ident $(, $arg:expr )*) => {
        T::WeightInfo::$name($( $arg ),*)
            .saturating_add(T::WeightInfo::get_storage_full()
            .saturating_sub(T::WeightInfo::get_storage_empty()))
    };
}

macro_rules! cost_args {
	// cost_args!(name, a, b, c) -> T::WeightInfo::name(a, b, c).saturating_sub(T::WeightInfo::name(0, 0, 0))
	($name:ident, $( $arg: expr ),+) => {
		(T::WeightInfo::$name($( $arg ),+).saturating_sub(cost_args!(@call_zero $name, $( $arg ),+)))
	};
	// Transform T::WeightInfo::name(a, b, c) into T::WeightInfo::name(0, 0, 0)
	(@call_zero $name:ident, $( $arg:expr ),*) => {
		T::WeightInfo::$name($( cost_args!(@replace_token $arg) ),*)
	};
	// Replace the token with 0.
	(@replace_token $_in:tt) => { 0 };
}

impl RuntimeCosts {
	/// Extra ref_time a hot state read pays to look up the block's overlay.
	fn hot_storage_overlay_overhead<T: Config>() -> Weight {
		let per_read = |weight_fn: fn(u32) -> Weight| weight_fn(1).saturating_sub(weight_fn(0));
		per_read(T::WeightInfo::overlay_probe_full)
			.saturating_sub(per_read(T::WeightInfo::overlay_probe_empty))
	}

	/// Weight of one access-list touch, plus the prepaid rollback for a revertible cold insert.
	fn access_list_overhead<T: Config>(warmth: Warmth) -> Weight {
		let touch_cost =
			|bench: fn() -> Weight, base: fn() -> Weight| bench().saturating_sub(base());
		match warmth {
			Warmth::Cold { revertible } => {
				let cost = touch_cost(
					T::WeightInfo::access_list_touch_cold_full,
					T::WeightInfo::access_list_touch_cold_empty,
				);
				if revertible {
					cost.saturating_add(T::WeightInfo::access_list_rollback_amortization())
				} else {
					cost
				}
			},
			Warmth::Hot(_) => touch_cost(
				T::WeightInfo::access_list_touch_hot_full,
				T::WeightInfo::access_list_touch_hot_single_element,
			),
		}
	}

	/// What a hot write pays on top of the cold read that warmed the key:
	/// re-hashing its trie path when the block's storage root is computed.
	pub(crate) fn deferred_write_cost<T: Config>() -> Weight {
		let db = T::DbWeight::get();
		db.writes(1).saturating_sub(db.reads(1))
	}

	/// Pick the matching storage bench for the access `kind`.
	fn weight_for_storage_access<T: Config>(
		op: StorageOp,
		kind: StorageAccessKind,
		cold: impl FnOnce() -> Weight,
		hot: impl FnOnce() -> Weight,
		transient: impl FnOnce() -> Weight,
	) -> Weight {
		match kind {
			StorageAccessKind::Persistent(warmth) => {
				let surcharge = match warmth {
					Warmth::Hot(paid) if !paid.covers(op) => Self::deferred_write_cost::<T>(),
					_ => Weight::zero(),
				};
				weight_by_warmth::<T, _>([warmth], cold, hot).saturating_add(surcharge)
			},
			StorageAccessKind::Transient => transient(),
		}
	}
}

/// Computes the weight of an operation, given the warmth of each state item it reads.
/// Prices hot only if every item is hot.
pub(crate) fn weight_by_warmth<T: Config, I: IntoIterator<Item = Warmth>>(
	items: I,
	cold: impl FnOnce() -> Weight,
	hot: impl FnOnce() -> Weight,
) -> Weight {
	let (count, all_hot, overhead) = items.into_iter().fold(
		(0u64, true, Weight::zero()),
		|(count, all_hot, overhead), warmth| {
			(
				count + 1,
				all_hot && warmth.is_hot(),
				overhead.saturating_add(RuntimeCosts::access_list_overhead::<T>(warmth)),
			)
		},
	);
	debug_assert!(count > 0, "an access reads at least one state item");
	let operation_weight = if all_hot {
		// One overlay lookup per item, since each stands for one state read.
		hot()
			.saturating_add(RuntimeCosts::hot_storage_overlay_overhead::<T>().saturating_mul(count))
	} else {
		cold()
	};
	operation_weight.saturating_add(overhead)
}

impl<T: Config> Token<T> for RuntimeCosts {
	fn influence_lowest_weight_limit(&self) -> bool {
		true
	}

	fn weight(&self) -> Weight {
		use self::RuntimeCosts::*;
		match *self {
			HostFn => cost_args!(noop_host_fn, 1),
			// `extcodecopy` charges `CodeSize` separately; subtract it so its read isn't counted
			// twice.
			ExtCodeCopy(len) => {
				T::WeightInfo::extcodecopy(len).saturating_sub(T::WeightInfo::seal_code_size())
			},
			CopyToContract(len) => T::WeightInfo::seal_copy_to_contract(len),
			CopyFromContract(len) => T::WeightInfo::seal_return(len),
			CallDataSize => T::WeightInfo::seal_call_data_size(),
			ReturnDataSize => T::WeightInfo::seal_return_data_size(),
			CallDataLoad => T::WeightInfo::seal_call_data_load(),
			CallDataCopy(len) => T::WeightInfo::seal_call_data_copy(len),
			Caller => T::WeightInfo::seal_caller(),
			Origin => T::WeightInfo::seal_origin(),
			ToAccountId => T::WeightInfo::to_account_id(),
			CodeHash => T::WeightInfo::seal_code_hash(),
			CodeSize => T::WeightInfo::seal_code_size(),
			OwnCodeHash => T::WeightInfo::own_code_hash(),
			CallerIsOrigin => T::WeightInfo::caller_is_origin(),
			CallerIsRoot => T::WeightInfo::caller_is_root(),
			OriginIsRoot => T::WeightInfo::origin_is_root(),
			Address => T::WeightInfo::seal_address(),
			RefTimeLeft => T::WeightInfo::seal_ref_time_left(),
			WeightLeft => T::WeightInfo::weight_left(),
			Balance => T::WeightInfo::seal_balance(),
			BalanceOf => T::WeightInfo::seal_balance_of(),
			ValueTransferred => T::WeightInfo::seal_value_transferred(),
			MinimumBalance => T::WeightInfo::minimum_balance(),
			BlockNumber => T::WeightInfo::seal_block_number(),
			BlockHash => T::WeightInfo::seal_block_hash(),
			BlockAuthor => T::WeightInfo::seal_block_author(),
			GasPrice => T::WeightInfo::seal_gas_price(),
			BaseFee => T::WeightInfo::seal_base_fee(),
			Now => T::WeightInfo::seal_now(),
			GasLimit => T::WeightInfo::seal_gas_limit(),
			Terminate { code_removed } => {
				// logic only runs if code is removed
				if code_removed {
					T::WeightInfo::seal_terminate(code_removed.into())
						.saturating_add(T::WeightInfo::seal_terminate_logic())
				} else {
					T::WeightInfo::seal_terminate(code_removed.into())
				}
			},
			DepositEvent { num_topic, len } => T::WeightInfo::seal_deposit_event(num_topic, len)
				.saturating_add(T::WeightInfo::on_finalize_block_per_event(len))
				.saturating_add(Weight::from_parts(
					limits::EXTRA_EVENT_CHARGE_PER_BYTE.saturating_mul(len.into()).into(),
					0,
				)),
			SetStorage { new_bytes, old_bytes, kind } => Self::weight_for_storage_access::<T>(
				StorageOp::Write,
				kind,
				|| cost_storage!(write_cold, seal_set_storage, new_bytes, old_bytes),
				|| T::WeightInfo::seal_set_storage_hot(new_bytes, old_bytes),
				|| cost_storage!(write_transient, seal_set_transient_storage, new_bytes, old_bytes),
			),
			ClearStorage { len, kind } => Self::weight_for_storage_access::<T>(
				StorageOp::Write,
				kind,
				|| cost_storage!(write_cold, clear_storage, len),
				|| T::WeightInfo::clear_storage_hot(len),
				|| cost_storage!(write_transient, seal_clear_transient_storage, len),
			),
			ContainsStorage { len, kind } => Self::weight_for_storage_access::<T>(
				StorageOp::Read,
				kind,
				|| cost_storage!(read_cold, contains_storage, len),
				|| T::WeightInfo::contains_storage_hot(len),
				|| cost_storage!(read_transient, seal_contains_transient_storage, len),
			),
			GetStorage { len, kind } => Self::weight_for_storage_access::<T>(
				StorageOp::Read,
				kind,
				|| cost_storage!(read_cold, seal_get_storage, len),
				|| T::WeightInfo::seal_get_storage_hot(len),
				|| cost_storage!(read_transient, seal_get_transient_storage, len),
			),
			TakeStorage { len, kind } => Self::weight_for_storage_access::<T>(
				StorageOp::Write,
				kind,
				|| cost_storage!(write_cold, take_storage, len),
				|| T::WeightInfo::take_storage_hot(len),
				|| cost_storage!(write_transient, seal_take_transient_storage, len),
			),
			CallBase(access_kind) => match access_kind {
				CallWarmth::Plain { account, original_account, account_info } => {
					let items = account.into_iter().chain([original_account, account_info]);
					weight_by_warmth::<T, _>(
						items,
						|| T::WeightInfo::seal_call(0, 0, 0),
						T::WeightInfo::seal_call_hot,
					)
				},
				CallWarmth::Delegate { account_info } => weight_by_warmth::<T, _>(
					[account_info],
					T::WeightInfo::seal_delegate_call,
					T::WeightInfo::seal_delegate_call_hot,
				),
			},
			PrecompileBase => T::WeightInfo::seal_call_precompile(0, 0),
			PrecompileWithInfoBase => T::WeightInfo::seal_call_precompile(1, 0),
			PrecompileDecode(len) => cost_args!(seal_call_precompile, 0, len),
			CallTransferSurcharge { dust_transfer } => {
				cost_args!(seal_call, 1, dust_transfer.into(), 0)
			},
			CallInputCloned(len) => cost_args!(seal_call, 0, 0, len),
			Instantiate { input_data_len, balance_transfer, dust_transfer } => {
				T::WeightInfo::seal_instantiate(
					balance_transfer.into(),
					dust_transfer.into(),
					input_data_len,
				)
			},
			Create { init_code_len, balance_transfer, dust_transfer } => {
				T::WeightInfo::evm_instantiate(
					balance_transfer.into(),
					dust_transfer.into(),
					init_code_len,
				)
			},
			HashSha256(len) => T::WeightInfo::sha2_256(len),
			Ripemd160(len) => T::WeightInfo::ripemd_160(len),
			HashKeccak256(len) => T::WeightInfo::seal_hash_keccak_256(len),
			HashBlake256(len) => T::WeightInfo::hash_blake2_256(len),
			HashBlake128(len) => T::WeightInfo::hash_blake2_128(len),
			EcdsaRecovery => T::WeightInfo::ecdsa_recover(),
			P256Verify => T::WeightInfo::p256_verify(),
			Sr25519Verify(len) => T::WeightInfo::seal_sr25519_verify(len),
			Precompile(weight) => weight,
			EcdsaToEthAddress => T::WeightInfo::seal_ecdsa_to_eth_address(),
			GetImmutableData(len) => T::WeightInfo::seal_get_immutable_data(len),
			SetImmutableData(len) => T::WeightInfo::seal_set_immutable_data(len),
			Bn128Add => T::WeightInfo::bn128_add(),
			Bn128Mul => T::WeightInfo::bn128_mul(),
			Bn128Pairing(len) => T::WeightInfo::bn128_pairing(len),
			Identity(len) => T::WeightInfo::identity(len),
			Blake2F(rounds) => T::WeightInfo::blake2f(rounds),
			Modexp(gas) => Weight::from_parts(gas.saturating_mul(WEIGHT_PER_GAS), 0),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{access_list::Paid, tests::Test};

	#[test]
	fn cold_hot_pricing_cold_is_strictly_more_expensive_than_hot() {
		let len = 64u32;
		let cold = StorageAccessKind::Persistent(Warmth::Cold { revertible: false });
		let cold_revertible = StorageAccessKind::Persistent(Warmth::Cold { revertible: true });
		let hot = StorageAccessKind::Persistent(Warmth::Hot(Paid::Write));

		let with_kind = |kind: StorageAccessKind| -> Vec<RuntimeCosts> {
			vec![
				RuntimeCosts::GetStorage { len, kind },
				RuntimeCosts::SetStorage { new_bytes: len, old_bytes: len, kind },
				RuntimeCosts::ClearStorage { len, kind },
				RuntimeCosts::ContainsStorage { len, kind },
				RuntimeCosts::TakeStorage { len, kind },
			]
		};

		for (cold_cost, hot_cost) in with_kind(cold).into_iter().zip(with_kind(hot)) {
			let cold_weight = <RuntimeCosts as Token<Test>>::weight(&cold_cost);
			let hot_weight = <RuntimeCosts as Token<Test>>::weight(&hot_cost);
			assert!(
				cold_weight.ref_time() > hot_weight.ref_time(),
				"expected cold > hot ref_time for {cold_cost:?}: cold={cold_weight:?} hot={hot_weight:?}",
			);
			assert_eq!(hot_weight.proof_size(), 0, "hot proof_size {hot_cost:?}: {hot_weight:?}");
			assert!(cold_weight.proof_size() > 0, "cold proof_size {cold_cost:?}: {cold_weight:?}",);
		}

		for (rev_cost, non_rev_cost) in with_kind(cold_revertible).into_iter().zip(with_kind(cold))
		{
			let rev_weight = <RuntimeCosts as Token<Test>>::weight(&rev_cost);
			let non_rev_weight = <RuntimeCosts as Token<Test>>::weight(&non_rev_cost);
			assert!(
				rev_weight.ref_time() > non_rev_weight.ref_time(),
				"expected revertible > non-revertible ref_time for {rev_cost:?}: \
				 rev={rev_weight:?} non={non_rev_weight:?}",
			);
			assert_eq!(
				rev_weight.proof_size(),
				non_rev_weight.proof_size(),
				"proof_size differs {rev_cost:?}: rev={rev_weight:?} non={non_rev_weight:?}",
			);
		}
	}

	#[test]
	fn call_base_cold_hot_pricing() {
		let weight_of = |cost: RuntimeCosts| <RuntimeCosts as Token<Test>>::weight(&cost);

		let all_hot = weight_of(RuntimeCosts::CallBase(CallWarmth::Plain {
			account: Some(Warmth::Hot(Paid::Read)),
			original_account: Warmth::Hot(Paid::Read),
			account_info: Warmth::Hot(Paid::Read),
		}));
		let all_cold = weight_of(RuntimeCosts::CallBase(CallWarmth::Plain {
			account: Some(Warmth::cold_non_revertible()),
			original_account: Warmth::cold_non_revertible(),
			account_info: Warmth::cold_non_revertible(),
		}));
		let mixed = weight_of(RuntimeCosts::CallBase(CallWarmth::Plain {
			account: Some(Warmth::cold_non_revertible()),
			original_account: Warmth::Hot(Paid::Read),
			account_info: Warmth::Hot(Paid::Read),
		}));

		assert!(
			all_cold.ref_time() > all_hot.ref_time(),
			"cold call must be more expensive than hot: cold={all_cold:?} hot={all_hot:?}",
		);
		assert_eq!(all_hot.proof_size(), 0, "hot call adds nothing to the proof: {all_hot:?}");
		assert!(all_cold.proof_size() > 0, "cold call pays proof size: {all_cold:?}");
		assert_eq!(
			mixed.proof_size(),
			all_cold.proof_size(),
			"any cold item prices the call as fully cold: mixed={mixed:?} all_cold={all_cold:?}",
		);

		let revertible = weight_of(RuntimeCosts::CallBase(CallWarmth::Plain {
			account: Some(Warmth::cold_revertible()),
			original_account: Warmth::cold_non_revertible(),
			account_info: Warmth::cold_non_revertible(),
		}));
		assert!(
			revertible.ref_time() > all_cold.ref_time(),
			"a revertible cold touch prepays the rollback: rev={revertible:?} cold={all_cold:?}",
		);
		assert_eq!(
			revertible.proof_size(),
			all_cold.proof_size(),
			"the rollback prepayment is ref_time only: rev={revertible:?} cold={all_cold:?}",
		);

		let delegate_hot = weight_of(RuntimeCosts::CallBase(CallWarmth::Delegate {
			account_info: Warmth::Hot(Paid::Read),
		}));
		let delegate_cold = weight_of(RuntimeCosts::CallBase(CallWarmth::Delegate {
			account_info: Warmth::cold_non_revertible(),
		}));
		assert!(
			delegate_cold.ref_time() > delegate_hot.ref_time(),
			"cold delegate call must be more expensive than hot: cold={delegate_cold:?} hot={delegate_hot:?}",
		);
		assert_eq!(delegate_hot.proof_size(), 0, "hot delegate call: {delegate_hot:?}");
		assert!(delegate_cold.proof_size() > 0, "cold delegate call: {delegate_cold:?}");

		let zero_value_hot = weight_of(RuntimeCosts::CallBase(CallWarmth::Plain {
			account: None,
			original_account: Warmth::Hot(Paid::Read),
			account_info: Warmth::Hot(Paid::Read),
		}));
		assert!(
			zero_value_hot.ref_time() < all_hot.ref_time(),
			"a zero-value call prices fewer items than a transferring one",
		);
		assert!(
			zero_value_hot.ref_time() > delegate_hot.ref_time(),
			"but more than a delegate call's single item",
		);
		let zero_value_mixed = weight_of(RuntimeCosts::CallBase(CallWarmth::Plain {
			account: None,
			original_account: Warmth::Hot(Paid::Read),
			account_info: Warmth::cold_non_revertible(),
		}));
		assert_eq!(
			zero_value_mixed.proof_size(),
			all_cold.proof_size(),
			"one cold item prices the zero-value call fully cold: {zero_value_mixed:?}",
		);
	}

	#[test]
	fn the_first_hot_write_pays_the_surcharge() {
		const LEN: u32 = 64;
		let weight = |cost: &RuntimeCosts| <RuntimeCosts as Token<Test>>::weight(cost);

		let surcharge = RuntimeCosts::deferred_write_cost::<Test>();
		let db = <Test as frame_system::Config>::DbWeight::get();
		assert!(
			surcharge.ref_time() > 0 && surcharge.ref_time() < db.writes(1).ref_time(),
			"the surcharge is part of a write: above zero, below all of it: {surcharge:?}",
		);
		assert_eq!(surcharge.proof_size(), 0, "the surcharge adds no proof: {surcharge:?}");

		let read_paid = StorageAccessKind::Persistent(Warmth::Hot(Paid::Read));
		let write_paid = StorageAccessKind::Persistent(Warmth::Hot(Paid::Write));

		let write_costs = |kind: StorageAccessKind| {
			[
				RuntimeCosts::SetStorage { new_bytes: LEN, old_bytes: LEN, kind },
				RuntimeCosts::ClearStorage { len: LEN, kind },
				RuntimeCosts::TakeStorage { len: LEN, kind },
			]
		};
		for (write_to_read_paid_slot, write_to_write_paid_slot) in
			write_costs(read_paid).into_iter().zip(write_costs(write_paid))
		{
			assert_eq!(
				weight(&write_to_read_paid_slot).saturating_sub(weight(&write_to_write_paid_slot)),
				surcharge,
				"a write to a read-paid slot pays exactly the surcharge: \
				 {write_to_read_paid_slot:?}",
			);
		}

		let read_costs = |kind: StorageAccessKind| {
			[
				RuntimeCosts::GetStorage { len: LEN, kind },
				RuntimeCosts::ContainsStorage { len: LEN, kind },
			]
		};
		for (read_of_read_paid_slot, read_of_write_paid_slot) in
			read_costs(read_paid).into_iter().zip(read_costs(write_paid))
		{
			assert_eq!(
				weight(&read_of_read_paid_slot),
				weight(&read_of_write_paid_slot),
				"a read is covered at either paid level: {read_of_read_paid_slot:?}",
			);
		}
	}

	#[test]
	fn hot_storage_overlay_overhead_is_not_zero() {
		let overhead = RuntimeCosts::hot_storage_overlay_overhead::<Test>();
		assert!(
			overhead.ref_time() > 0,
			"the per-read cost of overlay_probe_full must stay above overlay_probe_empty",
		);
		assert_eq!(overhead.proof_size(), 0, "the overlay probe is in-memory only: {overhead:?}");
	}
}
