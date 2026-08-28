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
	access_list::{Access, CallAccess, CallWarmth, KeyFamily, StorageOp, Transfer, Warmth},
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

/// How a storage access is priced.
#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(Clone, Copy, Debug)]
pub enum StorageAccessKind {
	/// Persistent storage, priced by the slot's warmth and the operation
	/// performed on it.
	Persistent { warmth: Warmth, op: StorageOp },
	/// Transient storage, every access costs the same.
	Transient,
}

impl StorageAccessKind {
	/// Storage is keyed by slot.
	pub(crate) const KEY_FAMILY: KeyFamily = KeyFamily::Slot;

	pub fn new(transient: bool, op: StorageOp, warmth: impl FnOnce() -> Warmth) -> Self {
		if transient { Self::Transient } else { Self::persistent(op, warmth) }
	}

	pub fn persistent(op: StorageOp, warmth: impl FnOnce() -> Warmth) -> Self {
		Self::Persistent { warmth: warmth(), op }
	}

	/// Debug check that the access was touched with the operation the cost
	/// performs; a mismatch would price the access wrongly.
	fn checked_against(self, op: StorageOp) -> Self {
		debug_assert!(
			!matches!(self, Self::Persistent { op: touched, .. } if touched != op),
			"storage access touched with a different operation than it is priced for",
		);
		self
	}
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

	/// The overhead the access list adds to one touch.
	pub(crate) fn access_list_overhead<T: Config>(warmth: Warmth, key: KeyFamily) -> Weight {
		let touch_cost = |bench: Weight, base: Weight| bench.saturating_sub(base);
		// Both terms come from the touched key's own family: a slot key is longer than an address
		// and lives on the heap, so one family's baseline is no floor for the other's.
		match warmth {
			Warmth::Cold { revertible } => {
				let cost = match key {
					KeyFamily::Slot => touch_cost(
						T::WeightInfo::access_list_touch_cold_full(),
						T::WeightInfo::access_list_touch_cold_empty(),
					),
					KeyFamily::Address => touch_cost(
						T::WeightInfo::access_list_touch_cold_account_full(),
						T::WeightInfo::access_list_touch_cold_account_empty(),
					),
				};
				if revertible {
					cost.saturating_add(T::WeightInfo::access_list_rollback_amortization())
				} else {
					cost
				}
			},
			Warmth::Hot { .. } => match key {
				KeyFamily::Slot => touch_cost(
					T::WeightInfo::access_list_touch_hot_full(),
					T::WeightInfo::access_list_touch_hot_single_element(),
				),
				KeyFamily::Address => touch_cost(
					T::WeightInfo::access_list_touch_hot_account_full(),
					T::WeightInfo::access_list_touch_hot_account_single_element(),
				),
			},
		}
	}

	/// What journaling a `Read` to `Write` upgrade costs, on top of the touch itself.
	pub(crate) fn access_list_upgrade_overhead<T: Config>() -> Weight {
		// `access_list_touch_hot_upgrade` is benched on a full list of slots.
		T::WeightInfo::access_list_touch_hot_upgrade()
			.saturating_sub(T::WeightInfo::access_list_touch_hot_full())
	}

	/// What `op` owes on a hot key: nothing if the transaction already paid for it, since the trie
	/// re-hashes a dirty key once, otherwise the re-hash plus the journal push of the upgrade.
	fn write_commit_owed<T: Config>(warmth: Warmth, op: StorageOp) -> Weight {
		match warmth {
			Warmth::Hot { charged } if !charged.covers(op) => Self::deferred_write_cost::<T>()
				.saturating_add(Self::access_list_upgrade_overhead::<T>()),
			_ => Weight::zero(),
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
		kind: StorageAccessKind,
		cold: impl FnOnce() -> Weight,
		hot: impl FnOnce() -> Weight,
		transient: impl FnOnce() -> Weight,
	) -> Weight {
		match kind {
			StorageAccessKind::Persistent { warmth, op } => {
				let surcharge = Self::write_commit_owed::<T>(warmth, op);
				weight_by_warmth::<T, _>([warmth], StorageAccessKind::KEY_FAMILY, cold, hot)
					.saturating_add(surcharge)
			},
			StorageAccessKind::Transient => transient(),
		}
	}
}

/// Computes the weight of an operation, given the warmth of each state item it reads.
/// Prices hot only if every item is hot.
pub(crate) fn weight_by_warmth<T: Config, I: IntoIterator<Item = Warmth>>(
	items: I,
	key: KeyFamily,
	cold: impl FnOnce() -> Weight,
	hot: impl FnOnce() -> Weight,
) -> Weight {
	let (count, all_hot, overhead) = items.into_iter().fold(
		(0u64, true, Weight::zero()),
		|(count, all_hot, overhead), warmth| {
			(
				count + 1,
				all_hot && warmth.is_hot(),
				overhead.saturating_add(RuntimeCosts::access_list_overhead::<T>(warmth, key)),
			)
		},
	);
	debug_assert!(count > 0, "an access reads at least one state item");
	// An empty access would price hot, so charge cold if that ever happens.
	let operation_weight = if all_hot && count > 0 {
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
				kind.checked_against(StorageOp::Write),
				|| cost_storage!(write_cold, seal_set_storage, new_bytes, old_bytes),
				|| T::WeightInfo::seal_set_storage_hot(new_bytes, old_bytes),
				|| cost_storage!(write_transient, seal_set_transient_storage, new_bytes, old_bytes),
			),
			ClearStorage { len, kind } => Self::weight_for_storage_access::<T>(
				kind.checked_against(StorageOp::Write),
				|| cost_storage!(write_cold, clear_storage, len),
				|| T::WeightInfo::clear_storage_hot(len),
				|| cost_storage!(write_transient, seal_clear_transient_storage, len),
			),
			ContainsStorage { len, kind } => Self::weight_for_storage_access::<T>(
				kind.checked_against(StorageOp::Read),
				|| cost_storage!(read_cold, contains_storage, len),
				|| T::WeightInfo::contains_storage_hot(len),
				|| cost_storage!(read_transient, seal_contains_transient_storage, len),
			),
			GetStorage { len, kind } => Self::weight_for_storage_access::<T>(
				kind.checked_against(StorageOp::Read),
				|| cost_storage!(read_cold, seal_get_storage, len),
				|| T::WeightInfo::seal_get_storage_hot(len),
				|| cost_storage!(read_transient, seal_get_transient_storage, len),
			),
			TakeStorage { len, kind } => Self::weight_for_storage_access::<T>(
				kind.checked_against(StorageOp::Write),
				|| cost_storage!(write_cold, take_storage, len),
				|| T::WeightInfo::take_storage_hot(len),
				|| cost_storage!(write_transient, seal_take_transient_storage, len),
			),
			CallBase(access_kind) => match access_kind {
				CallWarmth::Plain {
					account,
					sender_account,
					original_account,
					account_info,
					sender_account_info,
					dust,
				} => {
					// The transfer rides in the bench the warmth picks, so it prices at that
					// warmth.
					let balance_transfer = u32::from(account.is_some());
					let dust_transfer = u32::from(dust);
					let info_op = Transfer::info_op(dust);
					let items = account
						.into_iter()
						.chain(sender_account)
						.chain(sender_account_info)
						.chain([original_account, account_info]);
					let base = weight_by_warmth::<T, _>(
						items,
						CallAccess::KEY_FAMILY,
						|| T::WeightInfo::seal_call(balance_transfer, dust_transfer, 0),
						|| T::WeightInfo::seal_call_hot(balance_transfer, dust_transfer),
					);
					// The hot bench whitelists these keys, which drops their writes with their
					// reads, so each one owes its own commit. Same rule as a storage write.
					let writes = [
						(account, StorageOp::Write),
						(sender_account, StorageOp::Write),
						(Some(account_info), info_op),
						(sender_account_info, info_op),
					]
					.into_iter()
					.filter_map(|(warmth, op)| warmth.map(|warmth| (warmth, op)))
					.map(|(warmth, op)| Self::write_commit_owed::<T>(warmth, op))
					.fold(Weight::zero(), |sum, owed| sum.saturating_add(owed));
					base.saturating_add(writes)
				},
				CallWarmth::Delegate { account_info } => weight_by_warmth::<T, _>(
					[account_info],
					CallAccess::KEY_FAMILY,
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
	use crate::tests::Test;

	#[test]
	fn cold_hot_pricing_cold_is_strictly_more_expensive_than_hot() {
		let len = 64u32;
		let cold = Warmth::Cold { revertible: false };
		let cold_revertible = Warmth::Cold { revertible: true };
		let read_paid = Warmth::Hot { charged: StorageOp::Read };
		let write_paid = Warmth::Hot { charged: StorageOp::Write };

		// Each cost carries its own operation: a write cost priced with `op: Read` would skip
		// the surcharge and assert a case that cannot occur.
		let with_warmth = |warmth: Warmth| -> Vec<RuntimeCosts> {
			let read_kind = StorageAccessKind::Persistent { warmth, op: StorageOp::Read };
			let write_kind = StorageAccessKind::Persistent { warmth, op: StorageOp::Write };
			vec![
				RuntimeCosts::GetStorage { len, kind: read_kind },
				RuntimeCosts::SetStorage { new_bytes: len, old_bytes: len, kind: write_kind },
				RuntimeCosts::ClearStorage { len, kind: write_kind },
				RuntimeCosts::ContainsStorage { len, kind: read_kind },
				RuntimeCosts::TakeStorage { len, kind: write_kind },
			]
		};

		for paid_level in [read_paid, write_paid] {
			for (cold_cost, hot_cost) in with_warmth(cold).into_iter().zip(with_warmth(paid_level))
			{
				let cold_weight = <RuntimeCosts as Token<Test>>::weight(&cold_cost);
				let hot_weight = <RuntimeCosts as Token<Test>>::weight(&hot_cost);
				assert!(
					cold_weight.ref_time() > hot_weight.ref_time(),
					"expected cold > hot ref_time for {cold_cost:?}: \
					 cold={cold_weight:?} hot={hot_weight:?}",
				);
				assert_eq!(
					hot_weight.proof_size(),
					0,
					"hot proof_size {hot_cost:?}: {hot_weight:?}"
				);
				assert!(
					cold_weight.proof_size() > 0,
					"cold proof_size {cold_cost:?}: {cold_weight:?}",
				);
			}
		}

		for (rev_cost, non_rev_cost) in
			with_warmth(cold_revertible).into_iter().zip(with_warmth(cold))
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
			account: Some(Warmth::Hot { charged: StorageOp::Read }),
			sender_account: Some(Warmth::Hot { charged: StorageOp::Read }),
			sender_account_info: Some(Warmth::Hot { charged: StorageOp::Read }),
			dust: false,
			original_account: Warmth::Hot { charged: StorageOp::Read },
			account_info: Warmth::Hot { charged: StorageOp::Read },
		}));
		let all_cold = weight_of(RuntimeCosts::CallBase(CallWarmth::Plain {
			account: Some(Warmth::cold_non_revertible()),
			sender_account: Some(Warmth::cold_non_revertible()),
			sender_account_info: Some(Warmth::cold_non_revertible()),
			dust: false,
			original_account: Warmth::cold_non_revertible(),
			account_info: Warmth::cold_non_revertible(),
		}));
		let mixed = weight_of(RuntimeCosts::CallBase(CallWarmth::Plain {
			account: Some(Warmth::cold_non_revertible()),
			sender_account: Some(Warmth::Hot { charged: StorageOp::Read }),
			sender_account_info: Some(Warmth::Hot { charged: StorageOp::Read }),
			dust: false,
			original_account: Warmth::Hot { charged: StorageOp::Read },
			account_info: Warmth::Hot { charged: StorageOp::Read },
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
			sender_account: Some(Warmth::cold_revertible()),
			sender_account_info: Some(Warmth::cold_non_revertible()),
			dust: false,
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
			account_info: Warmth::Hot { charged: StorageOp::Read },
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
			sender_account: None,
			sender_account_info: None,
			dust: false,
			original_account: Warmth::Hot { charged: StorageOp::Read },
			account_info: Warmth::Hot { charged: StorageOp::Read },
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
			sender_account: None,
			sender_account_info: None,
			dust: false,
			original_account: Warmth::Hot { charged: StorageOp::Read },
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

		let deferred_write = RuntimeCosts::deferred_write_cost::<Test>();
		let db = <Test as frame_system::Config>::DbWeight::get();
		assert!(
			deferred_write.ref_time() > 0 && deferred_write.ref_time() < db.writes(1).ref_time(),
			"the deferred write is part of a write: above zero, below all of it: {deferred_write:?}",
		);
		assert_eq!(
			deferred_write.proof_size(),
			0,
			"the deferred write adds no proof: {deferred_write:?}",
		);
		// A first write also journals the read to write upgrade.
		let surcharge =
			deferred_write.saturating_add(RuntimeCosts::access_list_upgrade_overhead::<Test>());

		let read_paid = Warmth::Hot { charged: StorageOp::Read };
		let write_paid = Warmth::Hot { charged: StorageOp::Write };

		let write_costs = |warmth: Warmth| {
			let kind = StorageAccessKind::Persistent { warmth, op: StorageOp::Write };
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

		let read_costs = |warmth: Warmth| {
			let kind = StorageAccessKind::Persistent { warmth, op: StorageOp::Read };
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
	fn a_hot_transfer_pays_the_write_the_bench_whitelisted() {
		let weight_of = |account, sender_account| {
			<RuntimeCosts as Token<Test>>::weight(&RuntimeCosts::CallBase(CallWarmth::Plain {
				account: Some(account),
				sender_account: Some(sender_account),
				sender_account_info: Some(Warmth::Hot { charged: StorageOp::Write }),
				dust: false,
				original_account: Warmth::Hot { charged: StorageOp::Read },
				account_info: Warmth::Hot { charged: StorageOp::Read },
			}))
		};
		let read_paid = Warmth::Hot { charged: StorageOp::Read };
		let write_paid = Warmth::Hot { charged: StorageOp::Write };
		let owed = RuntimeCosts::deferred_write_cost::<Test>()
			.saturating_add(RuntimeCosts::access_list_upgrade_overhead::<Test>());

		assert_eq!(
			weight_of(read_paid, write_paid).saturating_sub(weight_of(write_paid, write_paid)),
			owed,
			"a transfer to an account that only paid for a read owes the write's re-hash",
		);
		assert_eq!(
			weight_of(read_paid, read_paid).saturating_sub(weight_of(write_paid, write_paid)),
			owed.saturating_mul(2),
			"both accounts owe it independently",
		);
		assert_eq!(
			weight_of(write_paid, write_paid),
			<RuntimeCosts as Token<Test>>::weight(&RuntimeCosts::CallBase(CallWarmth::Plain {
				account: Some(write_paid),
				sender_account: Some(write_paid),
				sender_account_info: Some(write_paid),
				dust: false,
				original_account: Warmth::Hot { charged: StorageOp::Read },
				account_info: Warmth::Hot { charged: StorageOp::Read },
			})),
			"a key the transaction already wrote is re-hashed once, so it owes nothing",
		);
	}

	#[test]
	fn a_value_call_prices_the_transfer_at_its_own_warmth() {
		let write_paid = Warmth::Hot { charged: StorageOp::Write };
		// Every written key already paid for a write, so nothing is owed on top of the bench.
		let weight_of = |original_account, transfer: Option<bool>| {
			<RuntimeCosts as Token<Test>>::weight(&RuntimeCosts::CallBase(CallWarmth::Plain {
				account: transfer.map(|_| write_paid),
				sender_account: transfer.map(|_| write_paid),
				sender_account_info: transfer.map(|_| write_paid),
				dust: transfer.unwrap_or(false),
				original_account,
				account_info: write_paid,
			}))
		};
		let arms = [
			(
				"hot",
				Warmth::Hot { charged: StorageOp::Read },
				<Test as Config>::WeightInfo::seal_call_hot(1, 0)
					.saturating_sub(<Test as Config>::WeightInfo::seal_call_hot(0, 0)),
			),
			(
				"cold",
				Warmth::cold_non_revertible(),
				<Test as Config>::WeightInfo::seal_call(1, 0, 0)
					.saturating_sub(<Test as Config>::WeightInfo::seal_call(0, 0, 0)),
			),
		];
		for (arm, original_account, transfer_term) in arms {
			assert!(
				weight_of(original_account, Some(true)).ref_time() >
					weight_of(original_account, Some(false)).ref_time(),
				"{arm}: the dust half of the transfer is priced apart from the value half",
			);
			assert!(
				weight_of(original_account, Some(false))
					.saturating_sub(weight_of(original_account, None))
					.all_gte(transfer_term),
				"{arm}: a call that moves value carries the transfer term of its own bench",
			);
		}
	}

	#[test]
	fn a_dust_transfer_pays_the_contract_info_writes() {
		let read_paid = Warmth::Hot { charged: StorageOp::Read };
		let write_paid = Warmth::Hot { charged: StorageOp::Write };
		// Both infos at the same paid level, so the difference is what the writes owe.
		let weight_of = |dust, infos| {
			<RuntimeCosts as Token<Test>>::weight(&RuntimeCosts::CallBase(CallWarmth::Plain {
				account: Some(write_paid),
				sender_account: Some(write_paid),
				sender_account_info: Some(infos),
				dust,
				original_account: read_paid,
				account_info: infos,
			}))
		};
		let owed = RuntimeCosts::deferred_write_cost::<Test>()
			.saturating_add(RuntimeCosts::access_list_upgrade_overhead::<Test>());

		assert_eq!(
			weight_of(true, read_paid).saturating_sub(weight_of(true, write_paid)),
			owed.saturating_mul(2),
			"a dust transfer writes both parties' contract info, so each one that only paid for a \
			 read owes the write's re-hash",
		);
		assert_eq!(
			weight_of(false, read_paid),
			weight_of(false, write_paid),
			"without dust the infos are only read, so what they paid for makes no difference",
		);
	}

	#[test]
	fn derived_overheads_stay_positive() {
		let cold = Warmth::cold_non_revertible();
		let hot = Warmth::Hot { charged: StorageOp::Read };
		let overlay = RuntimeCosts::hot_storage_overlay_overhead::<Test>();
		let derived = [
			("cold slot touch", RuntimeCosts::access_list_overhead::<Test>(cold, KeyFamily::Slot)),
			(
				"cold address touch",
				RuntimeCosts::access_list_overhead::<Test>(cold, KeyFamily::Address),
			),
			("hot slot touch", RuntimeCosts::access_list_overhead::<Test>(hot, KeyFamily::Slot)),
			(
				"hot address touch",
				RuntimeCosts::access_list_overhead::<Test>(hot, KeyFamily::Address),
			),
			("journaled upgrade", RuntimeCosts::access_list_upgrade_overhead::<Test>()),
			("deferred write", RuntimeCosts::deferred_write_cost::<Test>()),
			("hot storage overlay", overlay),
		];
		for (name, weight) in derived {
			assert!(
				weight.ref_time() > 0,
				"{name} is the difference between two benched values, so a regen that inverts \
				 the pair floors it to zero through `saturating_sub` instead of failing here",
			);
		}
		assert_eq!(overlay.proof_size(), 0, "the overlay probe is in-memory only: {overlay:?}");
	}
}
