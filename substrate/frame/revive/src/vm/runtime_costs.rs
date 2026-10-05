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
	access_list::{
		Access, CallItems, CallWarmth, CreateItems, KeyFamily, StorageItems, Summarized,
		TransferWarmth, Warmth, WarmthSummary,
	},
	limits,
	metering::Token,
	weightinfo_extension::OnFinalizeBlockParts,
	weights::WeightInfo,
};
use frame_support::{
	defensive_assert,
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
	CallBase(Summarized<CallWarmth>),
	/// Weight of calling a precompile.
	PrecompileBase,
	/// Weight of calling a precompile that has a contract info.
	PrecompileWithInfoBase,
	/// Weight of reading and decoding the input to a precompile.
	PrecompileDecode(u32),
	/// Weight of the transfer performed during a call.
	CallTransferSurcharge(TransferAccessKind),
	/// Weight per byte that is cloned by supplying the `CLONE_INPUT` flag.
	CallInputCloned(u32),
	/// Weight of calling `seal_instantiate`, and of warming the entries in `warming_summary`.
	Instantiate {
		input_data_len: u32,
		balance_transfer: bool,
		dust_transfer: bool,
		warming_summary: WarmthSummary,
	},
	/// Weight of calling `Create` opcode, and of warming the entries in `warming_summary`.
	Create {
		init_code_len: u32,
		balance_transfer: bool,
		dust_transfer: bool,
		warming_summary: WarmthSummary,
	},
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
	/// Weight of processing EIP-7702 authorization tuples.
	///
	/// `invalid_accounts` covers every tuple that produced no state change: those that
	/// fail the chain-id check, fail signature recovery, or pass recovery but then fail
	/// validation (bad nonce, non-EOA authority, etc.) or post-validation (set_delegation
	/// error). All are billed at the signature-recovery cost — a conservative over-estimate
	/// for the chain-id failures, which bail before recovery — and incur no
	/// account creation/update work.
	Delegations { new_accounts: u32, existing_accounts: u32, invalid_accounts: u32 },
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
	/// Computes the extra ref_time a hot state read pays to look up the block's overlay.
	fn hot_storage_overlay_overhead<T: Config>() -> Weight {
		let per_read = |weight_fn: fn(u32) -> Weight| weight_fn(1).saturating_sub(weight_fn(0));
		per_read(T::WeightInfo::overlay_probe_full)
			.saturating_sub(per_read(T::WeightInfo::overlay_probe_empty))
	}

	/// Computes the overhead the access list adds to one touch.
	pub fn access_list_overhead<T: Config>(summary: WarmthSummary, key: KeyFamily) -> Weight {
		let [cold_full, cold_base, hot_full, hot_base]: [fn() -> Weight; 4] = match key {
			KeyFamily::Slot => [
				T::WeightInfo::access_list_touch_cold_full,
				T::WeightInfo::access_list_touch_cold_empty,
				T::WeightInfo::access_list_touch_hot_full,
				T::WeightInfo::access_list_touch_hot_single_element,
			],
			KeyFamily::Address => [
				T::WeightInfo::access_list_touch_cold_address_full,
				T::WeightInfo::access_list_touch_cold_address_empty,
				T::WeightInfo::access_list_touch_hot_address_full,
				T::WeightInfo::access_list_touch_hot_address_single_element,
			],
		};
		let cold_touch = cold_full().saturating_sub(cold_base());
		let hot_touch = hot_full().saturating_sub(hot_base());
		cold_touch
			.saturating_mul(summary.cold().into())
			.saturating_add(hot_touch.saturating_mul(summary.hot().into()))
			.saturating_add(
				T::WeightInfo::access_list_rollback_amortization()
					.saturating_mul(summary.cold_revertible().into()),
			)
	}

	/// Computes the cost of journaling a `Read` to `Write` upgrade, on top of the touch itself.
	pub fn access_list_upgrade_overhead<T: Config>() -> Weight {
		T::WeightInfo::access_list_touch_hot_upgrade()
			.saturating_sub(T::WeightInfo::access_list_touch_hot_full())
	}

	/// Computes the cost a hot write adds on top of the cold read that warmed the key: re-hashing
	/// its trie path when the block's storage root is computed.
	fn deferred_write_cost<T: Config>() -> Weight {
		let db = T::DbWeight::get();
		db.writes(1).saturating_sub(db.reads(1))
	}

	/// Computes the surcharge a write owes on a key that only paid for a read.
	fn write_surcharge<T: Config>(summary: WarmthSummary) -> Weight {
		Self::deferred_write_cost::<T>()
			.saturating_add(Self::access_list_upgrade_overhead::<T>())
			.saturating_mul(summary.upgrades().into())
	}
}

impl Summarized<CallWarmth> {
	/// Computes the call cost from the warmth of the entries it reads.
	pub fn weight<T: Config>(self) -> Weight {
		match self.entries() {
			CallWarmth::Plain { .. } => weight_from_warmth_summary::<T>(
				self.summary(),
				CallItems::KEY_FAMILY,
				|| T::WeightInfo::seal_call(0, 0, 0),
				T::WeightInfo::seal_call_hot,
			),
			CallWarmth::Delegate { .. } => weight_from_warmth_summary::<T>(
				self.summary(),
				CallItems::KEY_FAMILY,
				T::WeightInfo::seal_delegate_call,
				T::WeightInfo::seal_delegate_call_hot,
			),
		}
	}
}

/// Computes the cost of an access from the warmth of its entries.
///
/// Each bench measures the whole access, so the hot one applies only when every entry is hot. The
/// access list's own costs come on top, entry by entry.
pub fn weight_from_warmth_summary<T: Config>(
	summary: WarmthSummary,
	key: KeyFamily,
	cold: impl FnOnce() -> Weight,
	hot: impl FnOnce() -> Weight,
) -> Weight {
	defensive_assert!(summary.total() > 0, "an access touches at least one state item");
	// With no entries `all_hot` is vacuously true, so charge cold instead.
	let operation_weight = if summary.all_hot() && summary.total() > 0 {
		// One overlay lookup per entry, since each stands for one state read.
		hot().saturating_add(
			RuntimeCosts::hot_storage_overlay_overhead::<T>()
				.saturating_mul(summary.total().into()),
		)
	} else {
		cold()
	};
	operation_weight
		.saturating_add(RuntimeCosts::access_list_overhead::<T>(summary, key))
		.saturating_add(RuntimeCosts::write_surcharge::<T>(summary))
}

/// How a storage access is priced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageAccessKind {
	/// Persistent storage, priced by its access-list warmth.
	Persistent(Summarized<Warmth>),
	/// Transient storage, every access costs the same.
	Transient,
}

impl StorageAccessKind {
	/// Builds the storage access kind. `warmth` is called only for persistent storage.
	pub fn new(transient: bool, warmth: impl FnOnce() -> Summarized<Warmth>) -> Self {
		if transient { Self::Transient } else { Self::Persistent(warmth()) }
	}

	/// Computes the cost of an access of this kind.
	fn weight<T: Config>(
		self,
		cold: impl FnOnce() -> Weight,
		hot: impl FnOnce() -> Weight,
		transient: impl FnOnce() -> Weight,
	) -> Weight {
		match self {
			Self::Persistent(warmth) => weight_from_warmth_summary::<T>(
				warmth.summary(),
				StorageItems::KEY_FAMILY,
				cold,
				hot,
			),
			Self::Transient => transient(),
		}
	}
}

/// How a call's value transfer is priced.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Clone, Copy)]
pub enum TransferAccessKind {
	/// A transfer the access list tracks, priced by the warmth of its entries.
	Tracked(Summarized<TransferWarmth>),
	/// A transfer the access list does not track, priced by its benchmark alone.
	Untracked { dust: bool },
}

impl TransferAccessKind {
	/// Returns whether the transfer moves dust.
	fn moves_dust(&self) -> bool {
		match self {
			Self::Tracked(transfer) => transfer.moves_dust(),
			Self::Untracked { dust } => *dust,
		}
	}
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
			SetStorage { new_bytes, old_bytes, kind } => kind.weight::<T>(
				|| cost_storage!(write_cold, seal_set_storage, new_bytes, old_bytes),
				|| T::WeightInfo::seal_set_storage_hot(new_bytes, old_bytes),
				|| cost_storage!(write_transient, seal_set_transient_storage, new_bytes, old_bytes),
			),
			ClearStorage { len, kind } => kind.weight::<T>(
				|| cost_storage!(write_cold, clear_storage, len),
				|| T::WeightInfo::clear_storage_hot(len),
				|| cost_storage!(write_transient, seal_clear_transient_storage, len),
			),
			ContainsStorage { len, kind } => kind.weight::<T>(
				|| cost_storage!(read_cold, contains_storage, len),
				|| T::WeightInfo::contains_storage_hot(len),
				|| cost_storage!(read_transient, seal_contains_transient_storage, len),
			),
			GetStorage { len, kind } => kind.weight::<T>(
				|| cost_storage!(read_cold, seal_get_storage, len),
				|| T::WeightInfo::seal_get_storage_hot(len),
				|| cost_storage!(read_transient, seal_get_transient_storage, len),
			),
			TakeStorage { len, kind } => kind.weight::<T>(
				|| cost_storage!(write_cold, take_storage, len),
				|| T::WeightInfo::take_storage_hot(len),
				|| cost_storage!(write_transient, seal_take_transient_storage, len),
			),
			CallBase(warmth) => warmth.weight::<T>(),
			PrecompileBase => T::WeightInfo::seal_call_precompile(0, 0),
			PrecompileWithInfoBase => T::WeightInfo::seal_call_precompile(1, 0),
			PrecompileDecode(len) => cost_args!(seal_call_precompile, 0, len),
			CallTransferSurcharge(transfer) => {
				let dust = u32::from(transfer.moves_dust());
				let cold = || cost_args!(seal_call, 1, dust, 0);
				match transfer {
					TransferAccessKind::Untracked { .. } => cold(),
					TransferAccessKind::Tracked(warmth) => weight_from_warmth_summary::<T>(
						warmth.summary(),
						CallItems::KEY_FAMILY,
						cold,
						|| {
							T::WeightInfo::seal_call_transfer_hot(dust)
								.saturating_sub(T::WeightInfo::seal_call_hot())
						},
					),
				}
			},
			CallInputCloned(len) => cost_args!(seal_call, 0, 0, len),
			Instantiate { input_data_len, balance_transfer, dust_transfer, warming_summary } => {
				T::WeightInfo::seal_instantiate(
					balance_transfer.into(),
					dust_transfer.into(),
					input_data_len,
				)
				.saturating_add(Self::access_list_overhead::<T>(
					warming_summary,
					CreateItems::KEY_FAMILY,
				))
			},
			Create { init_code_len, balance_transfer, dust_transfer, warming_summary } => {
				T::WeightInfo::evm_instantiate(
					balance_transfer.into(),
					dust_transfer.into(),
					init_code_len,
				)
				.saturating_add(Self::access_list_overhead::<T>(
					warming_summary,
					CreateItems::KEY_FAMILY,
				))
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
			Delegations { new_accounts, existing_accounts, invalid_accounts } => {
				T::WeightInfo::process_new_account_authorization(new_accounts)
					.saturating_add(T::WeightInfo::process_existing_account_authorization(
						existing_accounts,
					))
					.saturating_add(T::WeightInfo::process_invalid_authorization(invalid_accounts))
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		access_list::{StorageOp, TransferItems},
		tests::Test,
	};
	use alloc::{vec, vec::Vec};
	use sp_core::H160;

	/// Returns the weight the runtime charges for `cost`.
	fn weight(cost: &RuntimeCosts) -> Weight {
		<RuntimeCosts as Token<Test>>::weight(cost)
	}

	#[test]
	fn storage_pricing_by_access_kind() {
		let len = 64u32;
		let cold_non_revertible = Warmth::cold_non_revertible();
		let cold_revertible = Warmth::cold_revertible();
		let hot_warmths = [Warmth::read_paid(), Warmth::write_paid()];

		let persistent =
			|warmth| move |op| StorageAccessKind::Persistent(Summarized::new_single(warmth, op));
		let transient = |_op| StorageAccessKind::Transient;
		let with_kind = |kind: &dyn Fn(StorageOp) -> StorageAccessKind| -> Vec<RuntimeCosts> {
			vec![
				RuntimeCosts::GetStorage { len, kind: kind(StorageOp::Read) },
				RuntimeCosts::SetStorage {
					new_bytes: len,
					old_bytes: len,
					kind: kind(StorageOp::Write),
				},
				RuntimeCosts::ClearStorage { len, kind: kind(StorageOp::Write) },
				RuntimeCosts::ContainsStorage { len, kind: kind(StorageOp::Read) },
				RuntimeCosts::TakeStorage { len, kind: kind(StorageOp::Write) },
			]
		};

		for hot in hot_warmths {
			for (cold_cost, hot_cost) in with_kind(&persistent(cold_non_revertible))
				.into_iter()
				.zip(with_kind(&persistent(hot)))
			{
				let cold_weight = weight(&cold_cost);
				let hot_weight = weight(&hot_cost);
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

		for (rev_cost, non_rev_cost) in with_kind(&persistent(cold_revertible))
			.into_iter()
			.zip(with_kind(&persistent(cold_non_revertible)))
		{
			let rev_weight = weight(&rev_cost);
			let non_rev_weight = weight(&non_rev_cost);
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

		for transient_cost in with_kind(&transient) {
			let weight = weight(&transient_cost);
			assert_eq!(
				weight.proof_size(),
				0,
				"transient storage is priced without proof: {transient_cost:?}: {weight:?}"
			);
			assert!(
				weight.ref_time() > 0,
				"transient storage ref_time must be above zero: {transient_cost:?}: {weight:?}"
			);
		}
	}

	#[test]
	fn the_first_hot_write_pays_the_surcharge() {
		const LEN: u32 = 64;

		let deferred_write = RuntimeCosts::deferred_write_cost::<Test>();
		let db = <Test as frame_system::Config>::DbWeight::get();

		assert!(
			deferred_write.ref_time() > 0 && deferred_write.ref_time() < db.writes(1).ref_time(),
			"the deferred write is only part of a write: {deferred_write:?}",
		);
		assert_eq!(deferred_write.proof_size(), 0, "the deferred write adds nothing to the proof",);

		let surcharge_of = |warmth| {
			RuntimeCosts::write_surcharge::<Test>(
				WarmthSummary::default().count(warmth, StorageOp::Write),
			)
		};
		let write_surcharge = surcharge_of(Warmth::read_paid());

		assert_eq!(
			surcharge_of(Warmth::cold_non_revertible()),
			Weight::zero(),
			"a cold key owes no surcharge",
		);

		assert!(
			write_surcharge.ref_time() > deferred_write.ref_time(),
			"a first write journals the upgrade too: {write_surcharge:?} > {deferred_write:?}",
		);

		let read_paid_slot = Warmth::read_paid();
		let write_paid_slot = Warmth::write_paid();
		let write_costs = |warmth| {
			let kind =
				StorageAccessKind::Persistent(Summarized::new_single(warmth, StorageOp::Write));
			[
				RuntimeCosts::SetStorage { new_bytes: LEN, old_bytes: LEN, kind },
				RuntimeCosts::ClearStorage { len: LEN, kind },
				RuntimeCosts::TakeStorage { len: LEN, kind },
			]
		};
		for (write_to_read_paid_slot, write_to_write_paid_slot) in
			write_costs(read_paid_slot).into_iter().zip(write_costs(write_paid_slot))
		{
			assert_eq!(
				weight(&write_to_read_paid_slot).saturating_sub(weight(&write_to_write_paid_slot)),
				write_surcharge,
				"a write to a read-paid slot pays exactly the surcharge: {write_to_read_paid_slot:?}",
			);
		}

		let read_costs = |warmth| {
			let kind =
				StorageAccessKind::Persistent(Summarized::new_single(warmth, StorageOp::Read));
			[
				RuntimeCosts::GetStorage { len: LEN, kind },
				RuntimeCosts::ContainsStorage { len: LEN, kind },
			]
		};
		for (read_of_read_paid_slot, read_of_write_paid_slot) in
			read_costs(read_paid_slot).into_iter().zip(read_costs(write_paid_slot))
		{
			assert_eq!(
				weight(&read_of_read_paid_slot),
				weight(&read_of_write_paid_slot),
				"a read is covered at either paid level: {read_of_read_paid_slot:?}",
			);
		}
	}

	#[test]
	fn a_transient_access_never_consults_the_access_list() {
		assert_eq!(
			StorageAccessKind::new(true, || unreachable!("transient storage has no warmth")),
			StorageAccessKind::Transient,
		);
	}

	#[test]
	fn the_hot_price_is_charged_only_when_every_item_is_hot() {
		// Distinct proof sizes, so the result shows which bench was charged.
		let cold_bench = || Weight::from_parts(1_000_000, 500);
		let hot_bench = || Weight::from_parts(10_000, 7);
		let price = |items: &[Warmth]| {
			let summary = items.iter().fold(WarmthSummary::default(), |summary, warmth| {
				summary.count(*warmth, StorageOp::Read)
			});
			weight_from_warmth_summary::<Test>(summary, KeyFamily::Slot, cold_bench, hot_bench)
		};
		let hot = Warmth::read_paid();
		let cold = Warmth::cold_non_revertible();

		assert_eq!(price(&[hot, hot]).proof_size(), hot_bench().proof_size());

		for items in [[hot, cold], [cold, hot]] {
			assert_eq!(
				price(&items).proof_size(),
				cold_bench().proof_size(),
				"the cold bench applies when a single item is cold: {items:?}",
			);
		}

		let all_cold = price(&[cold, cold]);
		let revertible = price(&[Warmth::cold_revertible(), Warmth::cold_revertible()]);
		assert!(
			revertible.ref_time() > all_cold.ref_time(),
			"a revertible cold touch prepays its rollback: rev={revertible:?} cold={all_cold:?}",
		);
		assert_eq!(
			revertible.proof_size(),
			all_cold.proof_size(),
			"the rollback prepayment is ref_time only",
		);
	}

	#[test]
	fn call_base_cold_hot_pricing() {
		let hot = Warmth::read_paid();
		let cold = Warmth::cold_non_revertible();
		let target = H160::repeat_byte(2);
		let plain = |warmth| {
			weight(&RuntimeCosts::CallBase(Summarized::from_warmths(
				CallItems::new(target, false),
				[warmth, warmth],
			)))
		};
		let delegate = |warmth| {
			weight(&RuntimeCosts::CallBase(Summarized::from_warmths(
				CallItems::new(target, true),
				[warmth],
			)))
		};

		assert!(plain(hot).ref_time() > delegate(hot).ref_time());
		assert!(plain(cold).ref_time() > delegate(cold).ref_time());
		assert!(plain(cold).proof_size() > delegate(cold).proof_size());

		for (shape, cold_weight, hot_weight) in
			[("plain", plain(cold), plain(hot)), ("delegate", delegate(cold), delegate(hot))]
		{
			assert!(
				cold_weight.ref_time() > hot_weight.ref_time(),
				"{shape}: a warm call must not cost more than a cold one: \
				 cold={cold_weight:?} hot={hot_weight:?}",
			);
			assert_eq!(hot_weight.proof_size(), 0, "{shape}: a warm call doesn't pay proof");
			assert!(cold_weight.proof_size() > 0, "{shape}: a cold call pays proof",);
		}
	}

	#[test]
	fn a_value_call_prices_the_transfer_at_its_own_warmth() {
		let write_paid = Warmth::write_paid();
		let transfer = |dust| TransferItems { from: H160::zero(), to: H160::repeat_byte(2), dust };
		let weight_of = |dust, warmth: Option<Warmth>| {
			weight(&RuntimeCosts::CallTransferSurcharge(match warmth {
				Some(warmth) => TransferAccessKind::Tracked(Summarized::from_warmths(
					transfer(dust),
					[warmth, warmth, write_paid, warmth],
				)),
				None => TransferAccessKind::Untracked { dust },
			}))
		};

		for (arm, warmth) in [
			("hot", Some(write_paid)),
			("cold", Some(Warmth::cold_non_revertible())),
			("untracked (precompile)", None),
		] {
			assert!(
				weight_of(true, warmth).ref_time() > weight_of(false, warmth).ref_time(),
				"{arm}: a transfer carrying dust costs more than one without",
			);
		}

		assert!(
			weight_of(false, None).ref_time() <
				weight_of(false, Some(Warmth::cold_non_revertible())).ref_time(),
			"untracked state pays the bench alone, with no access-list overhead on top",
		);
	}

	#[test]
	fn a_transfer_owes_the_surcharge_for_each_key_it_writes() {
		let read_paid = Warmth::read_paid();
		let write_paid = Warmth::write_paid();
		let transfer = |dust| TransferItems { from: H160::zero(), to: H160::repeat_byte(2), dust };
		// Each pair at the same paid level, so a difference is only what the writes owe.
		let weight_of = |dust, accounts, infos| {
			weight(&RuntimeCosts::CallTransferSurcharge(TransferAccessKind::Tracked(
				Summarized::from_warmths(transfer(dust), [accounts, accounts, infos, infos]),
			)))
		};

		let per_item_write_surcharge = RuntimeCosts::write_surcharge::<Test>(
			WarmthSummary::default().count(read_paid, StorageOp::Write),
		);
		let value_transfer_write_paid = weight_of(true, write_paid, write_paid);

		assert_eq!(
			weight_of(true, read_paid, write_paid).saturating_sub(value_transfer_write_paid),
			per_item_write_surcharge.saturating_mul(2),
			"the read-paid `System::Account` entries each owe the surcharge",
		);

		assert_eq!(
			weight_of(true, write_paid, read_paid).saturating_sub(value_transfer_write_paid),
			per_item_write_surcharge.saturating_mul(2),
			"with dust, the read-paid `AccountInfoOf` entries each owe the surcharge",
		);

		assert_eq!(
			weight_of(false, write_paid, read_paid),
			weight_of(false, write_paid, write_paid),
			"without dust the `AccountInfoOf` entries are only read, so nothing is owed"
		);
	}

	#[test]
	fn a_contract_creation_pays_for_warming_what_it_creates() {
		let instantiate = |warming_summary| RuntimeCosts::Instantiate {
			input_data_len: 0,
			balance_transfer: false,
			dust_transfer: false,
			warming_summary,
		};
		let create = |warming_summary| RuntimeCosts::Create {
			init_code_len: 0,
			balance_transfer: false,
			dust_transfer: false,
			warming_summary,
		};
		let no_warming = WarmthSummary::default();
		let some_warming = CreateItems::warming_summary(false);

		assert_eq!(
			weight(&instantiate(no_warming)),
			<Test as Config>::WeightInfo::seal_instantiate(0, 0, 0),
			"with no warming to charge, an instantiate costs only its bench",
		);
		assert_eq!(
			weight(&create(no_warming)),
			<Test as Config>::WeightInfo::evm_instantiate(0, 0, 0),
			"with no warming to charge, a CREATE costs only its bench",
		);
		assert!(
			weight(&instantiate(some_warming)).ref_time() >
				weight(&instantiate(no_warming)).ref_time(),
			"an instantiate charges for warming on top of its bench",
		);
		assert!(
			weight(&create(some_warming)).ref_time() > weight(&create(no_warming)).ref_time(),
			"a CREATE charges for warming on top of its bench",
		);
	}

	#[test]
	fn derived_overheads_stay_positive() {
		let cold = Warmth::cold_non_revertible();
		let hot = Warmth::read_paid();
		let overlay = RuntimeCosts::hot_storage_overlay_overhead::<Test>();
		let touch_overhead = |warmth, key| {
			let summary = WarmthSummary::default().count(warmth, StorageOp::Read);
			RuntimeCosts::access_list_overhead::<Test>(summary, key)
		};
		let derived = [
			("cold slot touch", touch_overhead(cold, KeyFamily::Slot)),
			("cold address touch", touch_overhead(cold, KeyFamily::Address)),
			("hot slot touch", touch_overhead(hot, KeyFamily::Slot)),
			("hot address touch", touch_overhead(hot, KeyFamily::Address)),
			("journaled upgrade", RuntimeCosts::access_list_upgrade_overhead::<Test>()),
			("deferred write", RuntimeCosts::deferred_write_cost::<Test>()),
			("hot storage overlay", overlay),
		];
		for (name, weight) in derived {
			assert!(
				weight.ref_time() > 0,
				"{name} collapsed to zero: its benches inverted, so the cost is no longer charged",
			);
		}

		assert_eq!(overlay.proof_size(), 0, "the overlay probe adds nothing to the proof");

		assert!(
			<Test as Config>::WeightInfo::seal_call_transfer_hot(0).ref_time() >
				<Test as Config>::WeightInfo::seal_call_hot().ref_time(),
			"a hot transfer must measure above the hot call it contains, or it is charged nothing",
		);
	}
}
