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

//! This module contains routines for accessing and altering a contract related state.

pub mod meter;

use crate::{
	address::AddressMapper,
	exec::{AccountIdOf, Key},
	storage::meter::Diff,
	tracing::if_tracing,
	weights::WeightInfo,
	AccountInfoOf, BalanceOf, BalanceWithDust, Config, DeletionQueue, DeletionQueueCounter, Error,
	TrieId, SENTINEL,
};
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::{
	storage::child::{self, ChildInfo},
	weights::{Weight, WeightMeter},
	CloneNoBound, DebugNoBound, DefaultNoBound,
};
use scale_info::TypeInfo;
use sp_core::{Get, H160};
use sp_io::KillStorageResult;
use sp_runtime::{
	traits::{Hash, Saturating, Zero},
	DispatchError, RuntimeDebug,
};

pub enum AccountIdOrAddress<T: Config> {
	/// An account that is a contract.
	AccountId(AccountIdOf<T>),
	/// An externally owned account (EOA).
	Address(H160),
}

/// Represents the account information for a contract or an externally owned account (EOA).
#[derive(
	DefaultNoBound,
	Encode,
	Decode,
	CloneNoBound,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]
pub struct AccountInfo<T: Config> {
	/// The type of the account.
	pub account_type: AccountType<T>,

	// The  amount that was transferred to this account that is less than the
	// NativeToEthRatio, and can be represented in the native currency
	pub dust: u32,
}

/// The account type is used to distinguish between contracts and externally owned accounts.
#[derive(
	DefaultNoBound,
	Encode,
	Decode,
	CloneNoBound,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]
pub enum AccountType<T: Config> {
	/// An account that is a contract.
	Contract(ContractInfo<T>),

	/// An account that is an externally owned account (EOA).
	#[default]
	EOA,
}

/// Information for managing an account and its sub trie abstraction.
/// This is the required info to cache for an account.
#[derive(Encode, Decode, CloneNoBound, PartialEq, Eq, DebugNoBound, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct ContractInfo<T: Config> {
	/// Unique ID for the subtree encoded as a bytes vector.
	pub trie_id: TrieId,
	/// The code associated with a given account.
	pub code_hash: sp_core::H256,
	/// How many bytes of storage are accumulated in this contract's child trie.
	storage_bytes: u32,
	/// How many items of storage are accumulated in this contract's child trie.
	storage_items: u32,
	/// This records to how much deposit the accumulated `storage_bytes` amount to.
	pub storage_byte_deposit: BalanceOf<T>,
	/// This records to how much deposit the accumulated `storage_items` amount to.
	storage_item_deposit: BalanceOf<T>,
	/// This records how much deposit is put down in order to pay for the contract itself.
	///
	/// We need to store this information separately so it is not used when calculating any refunds
	/// since the base deposit can only ever be refunded on contract termination.
	storage_base_deposit: BalanceOf<T>,
	/// The size of the immutable data of this contract.
	immutable_data_len: u32,
}

impl<T: Config> From<H160> for AccountIdOrAddress<T> {
	fn from(address: H160) -> Self {
		AccountIdOrAddress::Address(address)
	}
}

impl<T: Config> AccountIdOrAddress<T> {
	pub fn address(&self) -> H160 {
		match self {
			AccountIdOrAddress::AccountId(id) =>
				<T::AddressMapper as AddressMapper<T>>::to_address(id),
			AccountIdOrAddress::Address(address) => *address,
		}
	}

	pub fn account_id(&self) -> AccountIdOf<T> {
		match self {
			AccountIdOrAddress::AccountId(id) => id.clone(),
			AccountIdOrAddress::Address(address) => T::AddressMapper::to_account_id(address),
		}
	}
}

impl<T: Config> From<ContractInfo<T>> for AccountType<T> {
	fn from(contract_info: ContractInfo<T>) -> Self {
		AccountType::Contract(contract_info)
	}
}

impl<T: Config> AccountInfo<T> {
	/// Returns true if the account is a contract.
	pub fn is_contract(address: &H160) -> bool {
		let Some(info) = <AccountInfoOf<T>>::get(address) else { return false };
		matches!(info.account_type, AccountType::Contract(_))
	}

	/// Returns the balance of the account at the given address.
	pub fn balance(account: AccountIdOrAddress<T>) -> BalanceWithDust<BalanceOf<T>> {
		use frame_support::traits::{
			fungible::Inspect,
			tokens::{Fortitude::Polite, Preservation::Preserve},
		};

		let value = T::Currency::reducible_balance(&account.account_id(), Preserve, Polite);
		let dust = <AccountInfoOf<T>>::get(account.address()).map(|a| a.dust).unwrap_or_default();
		BalanceWithDust::new_unchecked::<T>(value, dust)
	}

	/// Loads the contract information for a given address.
	pub fn load_contract(address: &H160) -> Option<ContractInfo<T>> {
		let Some(info) = <AccountInfoOf<T>>::get(address) else { return None };
		let AccountType::Contract(contract_info) = info.account_type else { return None };
		Some(contract_info)
	}

	/// Insert a contract, existing dust if any will be unchanged.
	pub fn insert_contract(address: &H160, contract: ContractInfo<T>) {
		AccountInfoOf::<T>::mutate(address, |account| {
			if let Some(account) = account {
				account.account_type = contract.clone().into();
			} else {
				*account = Some(AccountInfo { account_type: contract.clone().into(), dust: 0 });
			}
		});
	}
}

impl<T: Config> ContractInfo<T> {
	/// Constructs a new contract info **without** writing it to storage.
	///
	/// This returns an `Err` if an contract with the supplied `account` already exists
	/// in storage.
	pub fn new(
		address: &H160,
		nonce: T::Nonce,
		code_hash: sp_core::H256,
	) -> Result<Self, DispatchError> {
		if <AccountInfo<T>>::is_contract(address) {
			return Err(Error::<T>::DuplicateContract.into());
		}

		let trie_id = {
			let buf = ("bcontract_trie_v1", address, nonce).using_encoded(T::Hashing::hash);
			buf.as_ref()
				.to_vec()
				.try_into()
				.expect("Runtime uses a reasonable hash size. Hence sizeof(T::Hash) <= 128; qed")
		};

		let contract = Self {
			trie_id,
			code_hash,
			storage_bytes: 0,
			storage_items: 0,
			storage_byte_deposit: Zero::zero(),
			storage_item_deposit: Zero::zero(),
			storage_base_deposit: Zero::zero(),
			immutable_data_len: 0,
		};

		Ok(contract)
	}

	/// Associated child trie unique id is built from the hash part of the trie id.
	pub fn child_trie_info(&self) -> ChildInfo {
		ChildInfo::new_default(self.trie_id.as_ref())
	}

	/// The deposit paying for the accumulated storage generated within the contract's child trie.
	pub fn extra_deposit(&self) -> BalanceOf<T> {
		self.storage_byte_deposit.saturating_add(self.storage_item_deposit)
	}

	/// Same as [`Self::extra_deposit`] but including the base deposit.
	pub fn total_deposit(&self) -> BalanceOf<T> {
		self.extra_deposit().saturating_add(self.storage_base_deposit)
	}

	/// Returns the storage base deposit of the contract.
	pub fn storage_base_deposit(&self) -> BalanceOf<T> {
		self.storage_base_deposit
	}

	/// Reads a storage kv pair of a contract.
	///
	/// The read is performed from the `trie_id` only. The `address` is not necessary. If the
	/// contract doesn't store under the given `key` `None` is returned.
	pub fn read(&self, key: &Key) -> Option<Vec<u8>> {
		let value = child::get_raw(&self.child_trie_info(), key.hash().as_slice());
		log::trace!(target: crate::LOG_TARGET, "contract storage: read value {:?} for key {:x?}", value, key);
		if_tracing(|t| {
			t.storage_read(key, value.as_deref());
		});
		return value
	}

	/// Returns `Some(len)` (in bytes) if a storage item exists at `key`.
	///
	/// Returns `None` if the `key` wasn't previously set by `set_storage` or
	/// was deleted.
	pub fn size(&self, key: &Key) -> Option<u32> {
		child::len(&self.child_trie_info(), key.hash().as_slice())
	}

	/// Update a storage entry into a contract's kv storage.
	///
	/// If the `new_value` is `None` then the kv pair is removed. If `take` is true
	/// a [`WriteOutcome::Taken`] is returned instead of a [`WriteOutcome::Overwritten`].
	///
	/// This function also records how much storage was created or removed if a `storage_meter`
	/// is supplied. It should only be absent for testing or benchmarking code.
	pub fn write(
		&self,
		key: &Key,
		new_value: Option<Vec<u8>>,
		storage_meter: Option<&mut meter::NestedMeter<T>>,
		take: bool,
	) -> Result<WriteOutcome, DispatchError> {
		log::trace!(target: crate::LOG_TARGET, "contract storage: writing value {:?} for key {:x?}", new_value, key);
		let hashed_key = key.hash();
		if_tracing(|t| {
			let old = child::get_raw(&self.child_trie_info(), hashed_key.as_slice());
			t.storage_write(key, old, new_value.as_deref());
		});

		self.write_raw(&hashed_key, new_value.as_deref(), storage_meter, take)
	}

	/// Update a storage entry into a contract's kv storage.
	/// Function used in benchmarks, which can simulate prefix collision in keys.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn bench_write_raw(
		&self,
		key: &[u8],
		new_value: Option<Vec<u8>>,
		take: bool,
	) -> Result<WriteOutcome, DispatchError> {
		self.write_raw(key, new_value.as_deref(), None, take)
	}

	fn write_raw(
		&self,
		key: &[u8],
		new_value: Option<&[u8]>,
		storage_meter: Option<&mut meter::NestedMeter<T>>,
		take: bool,
	) -> Result<WriteOutcome, DispatchError> {
		let child_trie_info = &self.child_trie_info();
		let (old_len, old_value) = if take {
			let val = child::get_raw(child_trie_info, key);
			(val.as_ref().map(|v| v.len() as u32), val)
		} else {
			(child::len(child_trie_info, key), None)
		};

		if let Some(storage_meter) = storage_meter {
			let mut diff = meter::Diff::default();
			let key_len = key.len() as u32;
			match (old_len, new_value.as_ref().map(|v| v.len() as u32)) {
				(Some(old_len), Some(new_len)) =>
					if new_len > old_len {
						diff.bytes_added = new_len - old_len;
					} else {
						diff.bytes_removed = old_len - new_len;
					},
				(None, Some(new_len)) => {
					diff.bytes_added = new_len.saturating_add(key_len);
					diff.items_added = 1;
				},
				(Some(old_len), None) => {
					diff.bytes_removed = old_len.saturating_add(key_len);
					diff.items_removed = 1;
				},
				(None, None) => (),
			}
			storage_meter.charge(&diff);
		}

		match &new_value {
			Some(new_value) => child::put_raw(child_trie_info, key, new_value),
			None => child::kill(child_trie_info, key),
		}

		Ok(match (old_len, old_value) {
			(None, _) => WriteOutcome::New,
			(Some(old_len), None) => WriteOutcome::Overwritten(old_len),
			(Some(_), Some(old_value)) => WriteOutcome::Taken(old_value),
		})
	}

	/// Sets and returns the contract base deposit.
	///
	/// The base deposit is updated when the `code_hash` of the contract changes, as it depends on
	/// the deposit paid to upload the contract's code. It also depends on the size of immutable
	/// storage which is also changed when the code hash of a contract is changed.
	pub fn update_base_deposit(&mut self, code_deposit: BalanceOf<T>) -> BalanceOf<T> {
		let contract_deposit = Diff {
			bytes_added: (self.encoded_size() as u32).saturating_add(self.immutable_data_len),
			items_added: if self.immutable_data_len == 0 { 1 } else { 2 },
			..Default::default()
		}
		.update_contract::<T>(None)
		.charge_or_zero();

		// Instantiating the contract prevents its code to be deleted, therefore the base deposit
		// includes a fraction (`T::CodeHashLockupDepositPercent`) of the original storage deposit
		// to prevent abuse.
		let code_deposit = T::CodeHashLockupDepositPercent::get().mul_ceil(code_deposit);

		let deposit = contract_deposit.saturating_add(code_deposit);
		self.storage_base_deposit = deposit;
		deposit
	}

	/// Push a contract's trie to the deletion queue for lazy removal.
	///
	/// You must make sure that the contract is also removed when queuing the trie for deletion.
	pub fn queue_trie_for_deletion(&self) {
		DeletionQueueManager::<T>::load().insert(self.trie_id.clone());
	}

	/// Calculates the weight that is necessary to remove one key from the trie and how many
	/// of those keys can be deleted from the deletion queue given the supplied weight limit.
	pub fn deletion_budget(meter: &WeightMeter) -> (Weight, u32) {
		let base_weight = T::WeightInfo::on_process_deletion_queue_batch();
		let weight_per_key = T::WeightInfo::on_initialize_per_trie_key(1) -
			T::WeightInfo::on_initialize_per_trie_key(0);

		// `weight_per_key` being zero makes no sense and would constitute a failure to
		// benchmark properly. We opt for not removing any keys at all in this case.
		let key_budget = meter
			.limit()
			.saturating_sub(base_weight)
			.checked_div_per_component(&weight_per_key)
			.unwrap_or(0) as u32;

		(weight_per_key, key_budget)
	}

	/// Delete as many items from the deletion queue possible within the supplied weight limit.
	pub fn process_deletion_queue_batch(meter: &mut WeightMeter) {
		if meter.try_consume(T::WeightInfo::on_process_deletion_queue_batch()).is_err() {
			return
		};

		let mut queue = <DeletionQueueManager<T>>::load();
		if queue.is_empty() {
			return;
		}

		let (weight_per_key, budget) = Self::deletion_budget(&meter);
		let mut remaining_key_budget = budget;
		while remaining_key_budget > 0 {
			let Some(entry) = queue.next() else { break };

			#[allow(deprecated)]
			let outcome = child::kill_storage(
				&ChildInfo::new_default(&entry.trie_id),
				Some(remaining_key_budget),
			);

			match outcome {
				// This happens when our budget wasn't large enough to remove all keys.
				KillStorageResult::SomeRemaining(keys_removed) => {
					remaining_key_budget.saturating_reduce(keys_removed);
					break
				},
				KillStorageResult::AllRemoved(keys_removed) => {
					entry.remove();
					// charge at least one key even if none were removed.
					remaining_key_budget = remaining_key_budget.saturating_sub(keys_removed.max(1));
				},
			};
		}

		meter.consume(weight_per_key.saturating_mul(u64::from(budget - remaining_key_budget)))
	}

	/// Returns the code hash of the contract specified by `account` ID.
	pub fn load_code_hash(account: &AccountIdOf<T>) -> Option<sp_core::H256> {
		<AccountInfo<T>>::load_contract(&T::AddressMapper::to_address(account)).map(|i| i.code_hash)
	}

	/// Returns the amount of immutable bytes of this contract.
	pub fn immutable_data_len(&self) -> u32 {
		self.immutable_data_len
	}

	/// Set the number of immutable bytes of this contract.
	pub fn set_immutable_data_len(&mut self, immutable_data_len: u32) {
		self.immutable_data_len = immutable_data_len;
	}
}

/// Information about what happened to the pre-existing value when calling [`ContractInfo::write`].
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum WriteOutcome {
	/// No value existed at the specified key.
	New,
	/// A value of the returned length was overwritten.
	Overwritten(u32),
	/// The returned value was taken out of storage before being overwritten.
	///
	/// This is only returned when specifically requested because it causes additional work
	/// depending on the size of the pre-existing value. When not requested [`Self::Overwritten`]
	/// is returned instead.
	Taken(Vec<u8>),
}

impl WriteOutcome {
	/// Extracts the size of the overwritten value or `0` if there
	/// was no value in storage.
	pub fn old_len(&self) -> u32 {
		match self {
			Self::New => 0,
			Self::Overwritten(len) => *len,
			Self::Taken(value) => value.len() as u32,
		}
	}

	/// Extracts the size of the overwritten value or `SENTINEL` if there
	/// was no value in storage.
	///
	/// # Note
	///
	/// We cannot use `0` as sentinel value because there could be a zero sized
	/// storage entry which is different from a non existing one.
	pub fn old_len_with_sentinel(&self) -> u32 {
		match self {
			Self::New => SENTINEL,
			Self::Overwritten(len) => *len,
			Self::Taken(value) => value.len() as u32,
		}
	}
}

/// Manage the removal of contracts storage that are marked for deletion.
///
/// When a contract is deleted by calling `seal_terminate` it becomes inaccessible
/// immediately, but the deletion of the storage items it has accumulated is performed
/// later by pulling the contract from the queue in the `on_idle` hook.
#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, DefaultNoBound, Clone)]
#[scale_info(skip_type_params(T))]
pub struct DeletionQueueManager<T: Config> {
	/// Counter used as a key for inserting a new deleted contract in the queue.
	/// The counter is incremented after each insertion.
	insert_counter: u32,
	/// The index used to read the next element to be deleted in the queue.
	/// The counter is incremented after each deletion.
	delete_counter: u32,

	_phantom: PhantomData<T>,
}

/// View on a contract that is marked for deletion.
struct DeletionQueueEntry<'a, T: Config> {
	/// the trie id of the contract to delete.
	trie_id: TrieId,

	/// A mutable reference on the queue so that the contract can be removed, and none can be added
	/// or read in the meantime.
	queue: &'a mut DeletionQueueManager<T>,
}

impl<'a, T: Config> DeletionQueueEntry<'a, T> {
	/// Remove the contract from the deletion queue.
	fn remove(self) {
		<DeletionQueue<T>>::remove(self.queue.delete_counter);
		self.queue.delete_counter = self.queue.delete_counter.wrapping_add(1);
		<DeletionQueueCounter<T>>::set(self.queue.clone());
	}
}

impl<T: Config> DeletionQueueManager<T> {
	/// Load the `DeletionQueueCounter`, so we can perform read or write operations on the
	/// DeletionQueue storage.
	fn load() -> Self {
		<DeletionQueueCounter<T>>::get()
	}

	/// Returns `true` if the queue contains no elements.
	fn is_empty(&self) -> bool {
		self.insert_counter.wrapping_sub(self.delete_counter) == 0
	}

	/// Insert a contract in the deletion queue.
	fn insert(&mut self, trie_id: TrieId) {
		<DeletionQueue<T>>::insert(self.insert_counter, trie_id);
		self.insert_counter = self.insert_counter.wrapping_add(1);
		<DeletionQueueCounter<T>>::set(self.clone());
	}

	/// Fetch the next contract to be deleted.
	///
	/// Note:
	/// we use the delete counter to get the next value to read from the queue and thus don't pay
	/// the cost of an extra call to `sp_io::storage::next_key` to lookup the next entry in the map
	fn next(&mut self) -> Option<DeletionQueueEntry<'_, T>> {
		if self.is_empty() {
			return None
		}

		let entry = <DeletionQueue<T>>::get(self.delete_counter);
		entry.map(|trie_id| DeletionQueueEntry { trie_id, queue: self })
	}
}

#[cfg(test)]
impl<T: Config> DeletionQueueManager<T> {
	pub fn from_test_values(insert_counter: u32, delete_counter: u32) -> Self {
		Self { insert_counter, delete_counter, _phantom: Default::default() }
	}
	pub fn as_test_tuple(&self) -> (u32, u32) {
		(self.insert_counter, self.delete_counter)
	}
}
