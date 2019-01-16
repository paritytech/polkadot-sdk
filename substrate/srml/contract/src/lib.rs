// Copyright 2018 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate. If not, see <http://www.gnu.org/licenses/>.

//! Smart-contract module for runtime; Allows deployment and execution of smart-contracts
//! expressed in WebAssembly.
//!
//! This module provides an ability to create smart-contract accounts and send them messages.
//! A smart-contract is an account with associated code and storage. When such an account receives a message,
//! the code associated with that account gets executed.
//!
//! The code is allowed to alter the storage entries of the associated account,
//! create smart-contracts or send messages to existing smart-contracts.
//!
//! For any actions invoked by the smart-contracts fee must be paid. The fee is paid in gas.
//! Gas is bought upfront up to the, specified in transaction, limit. Any unused gas is refunded
//! after the transaction (regardless of the execution outcome). If all gas is used,
//! then changes made for the specific call or create are reverted (including balance transfers).
//!
//! Failures are typically not cascading. That, for example, means that if contract A calls B and B errors
//! somehow, then A can decide if it should proceed or error.
//!
//! # Interaction with the system
//!
//! ## Finalization
//!
//! This module requires performing some finalization steps at the end of the block. If not performed
//! the module will have incorrect behavior.
//!
//! Call [`Module::execute`] at the end of the block. The order in relation to
//! the other module doesn't matter.
//!
//! ## Account killing
//!
//! When `staking` module determines that account is dead (e.g. account's balance fell below
//! exsistential deposit) then it reaps the account. That will lead to deletion of the associated
//! code and storage of the account.
//!
//! [`Module::execute`]: struct.Module.html#impl-OnFinalise

#![cfg_attr(not(feature = "std"), no_std)]

#[macro_use]
extern crate parity_codec_derive;

extern crate parity_wasm;
extern crate pwasm_utils;

extern crate parity_codec as codec;
extern crate sr_io as runtime_io;
extern crate sr_sandbox as sandbox;

#[cfg_attr(not(feature = "std"), macro_use)]
extern crate sr_std as rstd;

extern crate srml_balances as balances;
extern crate srml_system as system;

#[macro_use]
extern crate srml_support as runtime_support;

extern crate sr_primitives as runtime_primitives;

#[cfg(test)]
extern crate substrate_primitives;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

#[cfg(test)]
extern crate wabt;

mod account_db;
mod exec;
mod vm;
mod gas;

#[cfg(test)]
mod tests;

use exec::ExecutionContext;
use account_db::{AccountDb, OverlayAccountDb};

use rstd::prelude::*;
use rstd::marker::PhantomData;
use codec::{Codec, HasCompact};
use runtime_primitives::traits::{Hash, As, SimpleArithmetic, StaticLookup};
use runtime_support::dispatch::Result;
use runtime_support::{Parameter, StorageMap, StorageValue, StorageDoubleMap};
use system::ensure_signed;
use runtime_io::{blake2_256, twox_128};

pub trait Trait: balances::Trait {
	/// Function type to get the contract address given the creator.
	type DetermineContractAddress: ContractAddressFor<Self::AccountId>;

	// As<u32> is needed for wasm-utils
	type Gas: Parameter + Default + Codec + SimpleArithmetic + Copy + As<Self::Balance> + As<u64> + As<u32>;

	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

pub trait ContractAddressFor<AccountId: Sized> {
	fn contract_address_for(code: &[u8], data: &[u8], origin: &AccountId) -> AccountId;
}

/// Simple contract address determintator.
///
/// Address calculated from the code (of the constructor), input data to the constructor
/// and account id which requested the account creation.
///
/// Formula: `blake2_256(blake2_256(code) + blake2_256(data) + origin)`
pub struct SimpleAddressDeterminator<T: Trait>(PhantomData<T>);

impl<T: Trait> ContractAddressFor<T::AccountId> for SimpleAddressDeterminator<T>
where
	T::AccountId: From<T::Hash> + AsRef<[u8]>
{
	fn contract_address_for(code: &[u8], data: &[u8], origin: &T::AccountId) -> T::AccountId {
		let code_hash = T::Hashing::hash(code);
		let data_hash = T::Hashing::hash(data);

		let mut buf = Vec::new();
		buf.extend_from_slice(code_hash.as_ref());
		buf.extend_from_slice(data_hash.as_ref());
		buf.extend_from_slice(origin.as_ref());

		T::Hashing::hash(&buf[..]).into()
	}
}

decl_module! {
	/// Contracts module.
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn deposit_event<T>() = default;
		// TODO: Change AccountId to staking::Address
		/// Make a call to a specified account, optionally transferring some balance.
		/// Make a call to a specified account, optionally transferring some balance.
		fn call(
			origin,
			dest: <T::Lookup as StaticLookup>::Source,
			value: <T::Balance as HasCompact>::Type,
			gas_limit: <T::Gas as HasCompact>::Type,
			data: Vec<u8>
		) -> Result {
			let origin = ensure_signed(origin)?;
			let value = value.into();
			let gas_limit = gas_limit.into();
			let dest = T::Lookup::lookup(dest)?;

			// Pay for the gas upfront.
			//
			// NOTE: it is very important to avoid any state changes before
			// paying for the gas.
			let mut gas_meter = gas::buy_gas::<T>(&origin, gas_limit)?;

			let cfg = Config::preload();
			let mut ctx = ExecutionContext {
				self_account: origin.clone(),
				depth: 0,
				overlay: OverlayAccountDb::<T>::new(&account_db::DirectAccountDb),
				events: Vec::new(),
				config: &cfg,
			};

			let mut output_data = Vec::new();
			let result = ctx.call(origin.clone(), dest, value, &mut gas_meter, &data, &mut output_data);

			if let Ok(_) = result {
				// Commit all changes that made it thus far into the persistant storage.
				account_db::DirectAccountDb.commit(ctx.overlay.into_change_set());

				// Then deposit all events produced.
				ctx.events.into_iter().for_each(Self::deposit_event);
			}

			// Refund cost of the unused gas.
			//
			// NOTE: this should go after the commit to the storage, since the storage changes
			// can alter the balance of the caller.
			gas::refund_unused_gas::<T>(&origin, gas_meter);

			result.map(|_| ())
		}

		/// Create a new contract, optionally transfering some balance to the created account.
		///
		/// Creation is executed as follows:
		///
		/// - the destination address is computed based on the sender and hash of the code.
		/// - account is created at the computed address.
		/// - the `ctor_code` is executed in the context of the newly created account. Buffer returned
		///   after the execution is saved as the `code` of the account. That code will be invoked
		///   upon any message received by this account.
		fn create(
			origin,
			endowment: <T::Balance as HasCompact>::Type,
			gas_limit: <T::Gas as HasCompact>::Type,
			ctor_code: Vec<u8>,
			data: Vec<u8>
		) -> Result {
			let origin = ensure_signed(origin)?;
			let endowment = endowment.into();
			let gas_limit = gas_limit.into();

			// Pay for the gas upfront.
			//
			// NOTE: it is very important to avoid any state changes before
			// paying for the gas.
			let mut gas_meter = gas::buy_gas::<T>(&origin, gas_limit)?;

			let cfg = Config::preload();
			let mut ctx = ExecutionContext {
				self_account: origin.clone(),
				depth: 0,
				overlay: OverlayAccountDb::<T>::new(&account_db::DirectAccountDb),
				events: Vec::new(),
				config: &cfg,
			};
			let result = ctx.create(origin.clone(), endowment, &mut gas_meter, &ctor_code, &data);

			if let Ok(ref r) = result {
				// Commit all changes that made it thus far into the persistant storage.
				account_db::DirectAccountDb.commit(ctx.overlay.into_change_set());

				// Then deposit all events produced.
				ctx.events.into_iter().for_each(Self::deposit_event);

				Self::deposit_event(RawEvent::Created(origin.clone(), r.address.clone()));
			}

			// Refund cost of the unused gas.
			//
			// NOTE: this should go after the commit to the storage, since the storage changes
			// can alter the balance of the caller.
			gas::refund_unused_gas::<T>(&origin, gas_meter);

			result.map(|_| ())
		}

		fn on_finalise() {
			<GasSpent<T>>::kill();
		}
	}
}

decl_event! {
	pub enum Event<T>
	where
		<T as balances::Trait>::Balance,
		<T as system::Trait>::AccountId
	{
		/// Transfer happened `from` -> `to` with given `value` as part of a `message-call` or `create`.
		Transfer(AccountId, AccountId, Balance),

		/// Contract deployed by address at the specified address.
		Created(AccountId, AccountId),
	}
}

decl_storage! {
	trait Store for Module<T: Trait> as Contract {
		/// The fee required to create a contract. At least as big as staking's ReclaimRebate.
		ContractFee get(contract_fee) config(): T::Balance = T::Balance::sa(21);
		/// The fee charged for a call into a contract.
		CallBaseFee get(call_base_fee) config(): T::Gas = T::Gas::sa(135);
		/// The fee charged for a create of a contract.
		CreateBaseFee get(create_base_fee) config(): T::Gas = T::Gas::sa(175);
		/// The price of one unit of gas.
		GasPrice get(gas_price) config(): T::Balance = T::Balance::sa(1);
		/// The maximum nesting level of a call/create stack.
		MaxDepth get(max_depth) config(): u32 = 100;
		/// The maximum amount of gas that could be expended per block.
		BlockGasLimit get(block_gas_limit) config(): T::Gas = T::Gas::sa(1_000_000);
		/// Gas spent so far in this block.
		GasSpent get(gas_spent): T::Gas;
		/// Current cost schedule for contracts.
		CurrentSchedule get(current_schedule) config(): Schedule<T::Gas> = Schedule::default();
		/// The code associated with an account.
		pub CodeOf: map T::AccountId => Vec<u8>;	// TODO Vec<u8> values should be optimised to not do a length prefix.
	}
}

// TODO: consider storing upper-bound for contract's gas limit in fixed-length runtime
// code in contract itself and use that.

/// The storage items associated with an account/key.
///
/// TODO: keys should also be able to take AsRef<KeyType> to ensure Vec<u8>s can be passed as &[u8]
pub(crate) struct StorageOf<T>(::rstd::marker::PhantomData<T>);
impl<T: Trait> StorageDoubleMap for StorageOf<T> {
	const PREFIX: &'static [u8] = b"con:sto:";
	type Key1 = T::AccountId;
	type Key2 = Vec<u8>;
	type Value = Vec<u8>;

	/// Hashed by XX
	fn derive_key1(key1_data: Vec<u8>) -> Vec<u8> {
		twox_128(&key1_data).to_vec()
	}

	/// Blake2 is used for `Key2` is because it will be used as a key for contract's storage and
	/// thus will be susceptible for a untrusted input.
	fn derive_key2(key2_data: Vec<u8>) -> Vec<u8> {
		blake2_256(&key2_data).to_vec()
	}
}

impl<T: Trait> balances::OnFreeBalanceZero<T::AccountId> for Module<T> {
	fn on_free_balance_zero(who: &T::AccountId) {
		<CodeOf<T>>::remove(who);
		<StorageOf<T>>::remove_prefix(who.clone());
	}
}

/// In-memory cache of configuration values.
///
/// We assume that these values can't be changed in the
/// course of transaction execution.
pub struct Config<T: Trait> {
	pub schedule: Schedule<T::Gas>,
	pub existential_deposit: T::Balance,
	pub max_depth: u32,
	pub contract_account_create_fee: T::Balance,
	pub account_create_fee: T::Balance,
	pub transfer_fee: T::Balance,
	pub call_base_fee: T::Gas,
	pub create_base_fee: T::Gas,
}

impl<T: Trait> Config<T> {
	fn preload() -> Config<T> {
		Config {
			schedule: <Module<T>>::current_schedule(),
			existential_deposit: <balances::Module<T>>::existential_deposit(),
			max_depth: <Module<T>>::max_depth(),
			contract_account_create_fee: <Module<T>>::contract_fee(),
			account_create_fee: <balances::Module<T>>::creation_fee(),
			transfer_fee: <balances::Module<T>>::transfer_fee(),
			call_base_fee: <Module<T>>::call_base_fee(),
			create_base_fee: <Module<T>>::create_base_fee(),
		}
	}
}

/// Definition of the cost schedule and other parameterizations for wasm vm.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize, Debug))]
#[derive(Clone, Encode, Decode)]
pub struct Schedule<Gas> {
	/// Gas cost of a growing memory by single page.
	pub grow_mem_cost: Gas,

	/// Gas cost of a regular operation.
	pub regular_op_cost: Gas,

	/// Gas cost per one byte returned.
	pub return_data_per_byte_cost: Gas,

	/// Gas cost per one byte read from the sandbox memory.
	sandbox_data_read_cost: Gas,

	/// Gas cost per one byte written to the sandbox memory.
	sandbox_data_write_cost: Gas,

	/// How tall the stack is allowed to grow?
	///
	/// See https://wiki.parity.io/WebAssembly-StackHeight to find out
	/// how the stack frame cost is calculated.
	pub max_stack_height: u32,

	//// What is the maximal memory pages amount is allowed to have for
	/// a contract.
	pub max_memory_pages: u32,
}

impl<Gas: As<u64>> Default for Schedule<Gas> {
	fn default() -> Schedule<Gas> {
		Schedule {
			grow_mem_cost: Gas::sa(1),
			regular_op_cost: Gas::sa(1),
			return_data_per_byte_cost: Gas::sa(1),
			sandbox_data_read_cost: Gas::sa(1),
			sandbox_data_write_cost: Gas::sa(1),
			max_stack_height: 64 * 1024,
			max_memory_pages: 16,
		}
	}
}
