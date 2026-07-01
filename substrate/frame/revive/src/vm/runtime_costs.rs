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
	access_list::{StorageAccessKind, Warmth},
	limits,
	metering::Token,
	weightinfo_extension::OnFinalizeBlockParts,
	weights::WeightInfo,
};
use core::marker::PhantomData;
use frame_support::weights::{Weight, constants::WEIGHT_REF_TIME_PER_SECOND};

#[cfg(test)]
use frame_support::{DebugNoBound, EqNoBound, PartialEqNoBound};

/// Identifies a contract execution backend and exposes the parts of the cost
/// schedule that genuinely diverge between backends.
///
/// Implemented by zero-sized marker types ([`InterpreterBackend`],
/// [`super::JitBackend`], [`EvmBackend`]). All dispatch is monomorphized —
/// picking a backend at a sync site costs nothing at runtime. Add a method
/// here when another host-call base cost is found to differ between
/// backends.
pub trait WeightBackend<T: Config> {
	/// Base cost of `seal_call` for this backend.
	fn call_base_weight() -> Weight;

	/// Base cost of `seal_delegate_call` for this backend.
	fn delegate_call_base_weight() -> Weight;

	/// Per-ecall dispatch overhead for this backend.
	///
	/// Derived from the per-call slope of the `noop_host_fn` family of
	/// benchmarks (one per backend). Charged once per ecall in the proc-macro
	/// emitted `handle_ecall` prelude.
	fn host_fn_weight() -> Weight;
}

/// PolkaVM-only weight policy.
///
/// Required by paths that translate between engine fuel and substrate
/// `ref_time` — the engine ↔ meter sync paths only ever run for polkavm
/// contracts.
pub trait PolkaVmWeightBackend<T: Config>: WeightBackend<T> {
	/// How much substrate `ref_time` corresponds to one PolkaVM engine fuel unit.
	fn ref_time_per_fuel() -> u64;
}

/// Marker for the in-tree polkavm interpreter backend.
///
/// The interpreter's `ref_time_per_fuel` ratio is benchmark-derived from
/// `WeightInfo::instr` / `WeightInfo::instr_empty_loop`.
pub struct InterpreterBackend;

/// Marker for the EVM bytecode interpreter (revm) backend.
pub struct EvmBackend;

impl<T: Config> WeightBackend<T> for InterpreterBackend {
	fn call_base_weight() -> Weight {
		T::WeightInfo::seal_call(0, 0, 0)
	}

	fn delegate_call_base_weight() -> Weight {
		T::WeightInfo::seal_delegate_call()
	}

	fn host_fn_weight() -> Weight {
		T::WeightInfo::noop_host_fn(1).saturating_sub(T::WeightInfo::noop_host_fn(0))
	}
}

impl<T: Config> PolkaVmWeightBackend<T> for InterpreterBackend {
	fn ref_time_per_fuel() -> u64 {
		let loop_iteration =
			T::WeightInfo::instr(1).saturating_sub(T::WeightInfo::instr(0)).ref_time();
		let empty_loop_iteration = T::WeightInfo::instr_empty_loop(1)
			.saturating_sub(T::WeightInfo::instr_empty_loop(0))
			.ref_time();
		loop_iteration.saturating_sub(empty_loop_iteration)
	}
}

impl<T: Config> WeightBackend<T> for EvmBackend {
	fn call_base_weight() -> Weight {
		T::WeightInfo::evm_call()
	}

	/// Preserves the pre-`BackendCosts` charging for the EVM `DELEGATECALL` opcode.
	///
	/// The old `RuntimeCosts::DelegateCallBase` token resolved to
	/// `seal_delegate_call()` regardless of backend; that stays the source of
	/// truth for the EVM opcode until a dedicated `evm_delegate_call` bench
	/// specializes it the same way `evm_call` specializes `call_base_weight`.
	fn delegate_call_base_weight() -> Weight {
		T::WeightInfo::seal_delegate_call()
	}

	/// Currently zero — EVM dispatch overhead is uncharged.
	///
	/// TODO: EVM `seal_*` benchmarks measure only the raw syscall body, so
	/// there is no analog of `InterpreterBackend::host_fn_weight` to charge
	/// from. A dedicated `noop_host_fn_evm` bench would let us charge the
	/// actual EVM dispatch slope here.
	fn host_fn_weight() -> Weight {
		Weight::zero()
	}
}

/// Costs that genuinely differ by execution backend.
///
/// Implements [`Token<T>`] directly through [`WeightBackend`], so it slots
/// into `charge` / `charge_or_halt` / `adjust_weight` wherever the backend
/// type is in scope. Add a new variant when another host-call cost is found
/// to diverge between backends.
#[cfg_attr(test, derive(DebugNoBound, PartialEqNoBound, EqNoBound))]
pub enum BackendCosts<B> {
	/// Base cost of `seal_call`.
	CallBase,
	/// Base cost of `seal_delegate_call`.
	DelegateCallBase,
	/// Per-ecall dispatch overhead.
	HostFn,
	#[doc(hidden)]
	_Phantom(PhantomData<B>),
}

impl<T: Config, B: WeightBackend<T> + 'static> Token<T> for BackendCosts<B> {
	fn weight(&self) -> Weight {
		match self {
			Self::CallBase => B::call_base_weight(),
			Self::DelegateCallBase => B::delegate_call_base_weight(),
			Self::HostFn => B::host_fn_weight(),
			Self::_Phantom(_) => unreachable!(),
		}
	}
}

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
	/// Extra ref_time a hot storage access pays to look up the block's overlay.
	fn hot_storage_overlay_overhead<T: Config>() -> Weight {
		let per_read = |weight_fn: fn(u32) -> Weight| weight_fn(1).saturating_sub(weight_fn(0));
		per_read(T::WeightInfo::overlay_probe_full)
			.saturating_sub(per_read(T::WeightInfo::overlay_probe_empty))
	}

	/// Pick the matching storage bench for the access `kind`.
	fn weight_for_storage_access<T: Config>(
		kind: StorageAccessKind,
		cold: impl FnOnce() -> Weight,
		hot: impl FnOnce() -> Weight,
		transient: impl FnOnce() -> Weight,
	) -> Weight {
		match kind {
			StorageAccessKind::Persistent(Warmth::Cold { revertible }) => {
				let cost = cold()
					.saturating_add(T::WeightInfo::access_list_touch_cold_full())
					.saturating_sub(T::WeightInfo::access_list_touch_cold_empty());
				if revertible {
					cost.saturating_add(T::WeightInfo::access_list_rollback_amortization())
				} else {
					cost
				}
			},
			StorageAccessKind::Persistent(Warmth::Hot) => hot()
				.saturating_add(Self::hot_storage_overlay_overhead::<T>())
				.saturating_add(T::WeightInfo::access_list_touch_hot_full())
				.saturating_sub(T::WeightInfo::access_list_touch_hot_single_element()),
			StorageAccessKind::Transient => transient(),
		}
	}
}

impl<T: Config> Token<T> for RuntimeCosts {
	fn weight(&self) -> Weight {
		use self::RuntimeCosts::*;
		match *self {
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
				kind,
				|| cost_storage!(write_cold, seal_set_storage, new_bytes, old_bytes),
				|| T::WeightInfo::seal_set_storage_hot(new_bytes, old_bytes),
				|| cost_storage!(write_transient, seal_set_transient_storage, new_bytes, old_bytes),
			),
			ClearStorage { len, kind } => Self::weight_for_storage_access::<T>(
				kind,
				|| cost_storage!(write_cold, clear_storage, len),
				|| T::WeightInfo::clear_storage_hot(len),
				|| cost_storage!(write_transient, seal_clear_transient_storage, len),
			),
			ContainsStorage { len, kind } => Self::weight_for_storage_access::<T>(
				kind,
				|| cost_storage!(read_cold, contains_storage, len),
				|| T::WeightInfo::contains_storage_hot(len),
				|| cost_storage!(read_transient, seal_contains_transient_storage, len),
			),
			GetStorage { len, kind } => Self::weight_for_storage_access::<T>(
				kind,
				|| cost_storage!(read_cold, seal_get_storage, len),
				|| T::WeightInfo::seal_get_storage_hot(len),
				|| cost_storage!(read_transient, seal_get_transient_storage, len),
			),
			TakeStorage { len, kind } => Self::weight_for_storage_access::<T>(
				kind,
				|| cost_storage!(write_cold, take_storage, len),
				|| T::WeightInfo::take_storage_hot(len),
				|| cost_storage!(write_transient, seal_take_transient_storage, len),
			),
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
		let cold = StorageAccessKind::Persistent(Warmth::Cold { revertible: false });
		let cold_revertible = StorageAccessKind::Persistent(Warmth::Cold { revertible: true });
		let hot = StorageAccessKind::Persistent(Warmth::Hot);

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
	fn hot_storage_overlay_overhead_is_not_zero() {
		let overhead = RuntimeCosts::hot_storage_overlay_overhead::<Test>();
		assert!(
			overhead.ref_time() > 0,
			"the per-read cost of overlay_probe_full must stay above overlay_probe_empty",
		);
		assert_eq!(overhead.proof_size(), 0, "the overlay probe is in-memory only: {overhead:?}");
	}
}
