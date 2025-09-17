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

use crate::{gas::Token, weights::WeightInfo, Config};
use frame_support::weights::{constants::WEIGHT_REF_TIME_PER_SECOND, Weight};

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
	/// Weight of calling `to_account_id`.
	ToAccountId,
	/// Weight of calling `seal_origin`.
	Origin,
	/// Weight of calling `seal_code_hash`.
	CodeHash,
	/// Weight of calling `seal_own_code_hash`.
	OwnCodeHash,
	/// Weight of calling `seal_code_size`.
	CodeSize,
	/// Weight of calling `seal_caller_is_origin`.
	CallerIsOrigin,
	/// Weight of calling `caller_is_root`.
	CallerIsRoot,
	/// Weight of calling `seal_address`.
	Address,
	/// Weight of calling `seal_ref_time_left`.
	RefTimeLeft,
	/// Weight of calling `seal_weight_left`.
	WeightLeft,
	/// Weight of calling `seal_balance`.
	Balance,
	/// Weight of calling `seal_balance_of`.
	BalanceOf,
	/// Weight of calling `seal_value_transferred`.
	ValueTransferred,
	/// Weight of calling `seal_minimum_balance`.
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
	/// Weight of calling `seal_set_storage` for the given storage item sizes.
	SetStorage { old_bytes: u32, new_bytes: u32 },
	/// Weight of calling `seal_clear_storage` per cleared byte.
	ClearStorage(u32),
	/// Weight of calling `seal_contains_storage` per byte of the checked item.
	ContainsStorage(u32),
	/// Weight of calling `seal_get_storage` with the specified size in storage.
	GetStorage(u32),
	/// Weight of calling `seal_take_storage` for the given size.
	TakeStorage(u32),
	/// Weight of calling `seal_set_transient_storage` for the given storage item sizes.
	SetTransientStorage { old_bytes: u32, new_bytes: u32 },
	/// Weight of calling `seal_clear_transient_storage` per cleared byte.
	ClearTransientStorage(u32),
	/// Weight of calling `seal_contains_transient_storage` per byte of the checked item.
	ContainsTransientStorage(u32),
	/// Weight of calling `seal_get_transient_storage` with the specified size in storage.
	GetTransientStorage(u32),
	/// Weight of calling `seal_take_transient_storage` for the given size.
	TakeTransientStorage(u32),
	/// Base weight of calling `seal_call`.
	CallBase,
	/// Weight of calling `seal_delegate_call` for the given input size.
	DelegateCallBase,
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
	/// Weight of calling `seal_sr25519_verify` for the given input size.
	Sr25519Verify(u32),
	/// Weight charged by a precompile.
	Precompile(Weight),
	/// Weight of calling `seal_set_code_hash`
	SetCodeHash { old_code_removed: bool },
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

    (write, $name:ident $(, $arg:expr )*) => {
        T::WeightInfo::$name($( $arg ),*)
            .saturating_add(T::WeightInfo::set_storage_full()
            .saturating_sub(T::WeightInfo::set_storage_empty()))
    };

    (read, $name:ident $(, $arg:expr )*) => {
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

impl<T: Config> Token<T> for RuntimeCosts {
	fn influence_lowest_gas_limit(&self) -> bool {
		true
	}

	fn weight(&self) -> Weight {
		use self::RuntimeCosts::*;
		match *self {
			HostFn => cost_args!(noop_host_fn, 1),
			ExtCodeCopy(len) => T::WeightInfo::extcodecopy(len),
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
			OwnCodeHash => T::WeightInfo::seal_own_code_hash(),
			CallerIsOrigin => T::WeightInfo::seal_caller_is_origin(),
			CallerIsRoot => T::WeightInfo::seal_caller_is_root(),
			Address => T::WeightInfo::seal_address(),
			RefTimeLeft => T::WeightInfo::seal_ref_time_left(),
			WeightLeft => T::WeightInfo::seal_weight_left(),
			Balance => T::WeightInfo::seal_balance(),
			BalanceOf => T::WeightInfo::seal_balance_of(),
			ValueTransferred => T::WeightInfo::seal_value_transferred(),
			MinimumBalance => T::WeightInfo::seal_minimum_balance(),
			BlockNumber => T::WeightInfo::seal_block_number(),
			BlockHash => T::WeightInfo::seal_block_hash(),
			BlockAuthor => T::WeightInfo::seal_block_author(),
			GasPrice => T::WeightInfo::seal_gas_price(),
			BaseFee => T::WeightInfo::seal_base_fee(),
			Now => T::WeightInfo::seal_now(),
			GasLimit => T::WeightInfo::seal_gas_limit(),
			Terminate { code_removed } => T::WeightInfo::seal_terminate(code_removed.into()),
			DepositEvent { num_topic, len } => T::WeightInfo::seal_deposit_event(num_topic, len),
			SetStorage { new_bytes, old_bytes } => {
				cost_storage!(write, seal_set_storage, new_bytes, old_bytes)
			},
			ClearStorage(len) => cost_storage!(write, seal_clear_storage, len),
			ContainsStorage(len) => cost_storage!(read, seal_contains_storage, len),
			GetStorage(len) => cost_storage!(read, seal_get_storage, len),
			TakeStorage(len) => cost_storage!(write, seal_take_storage, len),
			SetTransientStorage { new_bytes, old_bytes } => {
				cost_storage!(write_transient, seal_set_transient_storage, new_bytes, old_bytes)
			},
			ClearTransientStorage(len) => {
				cost_storage!(write_transient, seal_clear_transient_storage, len)
			},
			ContainsTransientStorage(len) => {
				cost_storage!(read_transient, seal_contains_transient_storage, len)
			},
			GetTransientStorage(len) => {
				cost_storage!(read_transient, seal_get_transient_storage, len)
			},
			TakeTransientStorage(len) => {
				cost_storage!(write_transient, seal_take_transient_storage, len)
			},
			CallBase => T::WeightInfo::seal_call(0, 0, 0),
			DelegateCallBase => T::WeightInfo::seal_delegate_call(),
			PrecompileBase => T::WeightInfo::seal_call_precompile(0, 0),
			PrecompileWithInfoBase => T::WeightInfo::seal_call_precompile(1, 0),
			PrecompileDecode(len) => cost_args!(seal_call_precompile, 0, len),
			CallTransferSurcharge { dust_transfer } =>
				cost_args!(seal_call, 1, dust_transfer.into(), 0),
			CallInputCloned(len) => cost_args!(seal_call, 0, 0, len),
			Instantiate { input_data_len, balance_transfer, dust_transfer } =>
				T::WeightInfo::seal_instantiate(
					input_data_len,
					balance_transfer.into(),
					dust_transfer.into(),
				),
			HashSha256(len) => T::WeightInfo::sha2_256(len),
			Ripemd160(len) => T::WeightInfo::ripemd_160(len),
			HashKeccak256(len) => T::WeightInfo::seal_hash_keccak_256(len),
			HashBlake256(len) => T::WeightInfo::hash_blake2_256(len),
			HashBlake128(len) => T::WeightInfo::hash_blake2_128(len),
			EcdsaRecovery => T::WeightInfo::ecdsa_recover(),
			Sr25519Verify(len) => T::WeightInfo::seal_sr25519_verify(len),
			Precompile(weight) => weight,
			SetCodeHash { old_code_removed } =>
				T::WeightInfo::seal_set_code_hash(old_code_removed.into()),
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
