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

use crate::{
	AccountInfoOf, BalanceOf, BalanceWithDust, CodeInfoOf, Config, DeletionQueue,
	DeletionQueueCounter, Error, LOG_TARGET, NativeDepositOf, SENTINEL, TrieId,
	address::AddressMapper,
	exec::{AccountIdOf, Key},
	metering::FrameMeter,
	tracing::if_tracing,
	vm::CodeInfo,
	weights::WeightInfo,
};
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::{
	CloneNoBound, DebugNoBound, DefaultNoBound,
	storage::{
		TransactionOutcome,
		child::{self, ChildInfo},
		with_transaction,
	},
	traits::{
		fungible::Inspect,
		tokens::{Fortitude, Preservation},
	},
	weights::{Weight, WeightMeter},
};
use scale_info::TypeInfo;
use sp_core::{Get, H160};
use sp_io::KillStorageResult;
use sp_runtime::{
	Debug, DispatchError,
	traits::{Hash, Saturating, Zero},
};

use crate::metering::Diff;

pub enum AccountIdOrAddress<T: Config> {
	/// An account that is a contract.
	AccountId(AccountIdOf<T>),
	/// An externally owned account (EOA).
	Address(H160),
}

/// Represents the account information for a contract or an externally owned account (EOA).
#[derive(
	DefaultNoBound, Encode, Decode, CloneNoBound, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
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
	DefaultNoBound, Encode, Decode, CloneNoBound, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]
pub enum AccountType<T: Config> {
	/// An account that is a contract.
	Contract(ContractInfo<T>),

	/// An externally owned account (no delegation).
	#[default]
	EOA,

	/// An EOA that has been delegated via EIP-7702.
	/// Once delegated, the account stays `DelegatedEOA` even after clearing.
	DelegatedEOA {
		/// When `Some`, the account delegates code execution to that address.
		delegate_target: Option<H160>,
		/// Storage accounting for this EOA's child trie.
		contract_info: ContractInfo<T>,
		/// Account that paid the current `contract_info.storage_base_deposit`. `Some` whenever a
		/// non-zero deposit is held; `None` for fresh delegations and post-clear leftovers. Used
		/// on clear/re-delegation so the refund flows back to the original payer regardless of
		/// who relays the next authorization.
		payer: Option<T::AccountId>,
	},
}

/// Deposit movement caused by [`AccountInfo::set_delegation`].
///
/// `previous` is the `storage_base_deposit` that was held under the *prior* delegation (0 if the
/// account is freshly delegated). `current` is the deposit required by the *new* delegation
/// target (0 if the target carries no code). The caller is responsible for refunding `previous`
/// to whoever originally paid it (look up via [`AccountInfo::get_delegation_payer`]) and charging
/// `current` from the new payer.
#[derive(Debug, PartialEq, Eq)]
pub struct DelegationDepositChange<T: Config> {
	pub previous: BalanceOf<T>,
	pub current: BalanceOf<T>,
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
	pub storage_bytes: u32,
	/// How many items of storage are accumulated in this contract's child trie.
	pub storage_items: u32,
	/// This records to how much deposit the accumulated `storage_bytes` amount to.
	pub storage_byte_deposit: BalanceOf<T>,
	/// This records to how much deposit the accumulated `storage_items` amount to.
	pub storage_item_deposit: BalanceOf<T>,
	/// This records how much deposit is put down in order to pay for the contract itself.
	///
	/// We need to store this information separately so it is not used when calculating any refunds
	/// since the base deposit can only ever be refunded on contract termination.
	pub storage_base_deposit: BalanceOf<T>,
	/// The size of the immutable data of this contract.
	pub immutable_data_len: u32,
}

impl<T: Config> From<H160> for AccountIdOrAddress<T> {
	fn from(address: H160) -> Self {
		AccountIdOrAddress::Address(address)
	}
}

impl<T: Config> AccountIdOrAddress<T> {
	pub fn address(&self) -> H160 {
		match self {
			AccountIdOrAddress::AccountId(id) => {
				<T::AddressMapper as AddressMapper<T>>::to_address(id)
			},
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

impl<T: Config> AccountType<T> {
	/// Returns the ContractInfo if this account type has loadable contract code.
	///
	/// For `DelegatedEOA`, only returns `Some` when delegation is active and the
	/// code_hash is non-default (i.e., the target is a contract).
	pub fn contract_info(self) -> Option<ContractInfo<T>> {
		match self {
			AccountType::Contract(info) => Some(info),
			AccountType::DelegatedEOA { delegate_target: Some(_), contract_info, .. }
				if !contract_info.code_hash.is_zero() =>
			{
				Some(contract_info)
			},
			_ => None,
		}
	}
}

impl<T: Config> AccountInfo<T> {
	/// Returns true if the account is a contract.
	pub fn is_contract(address: &H160) -> bool {
		let Some(info) = <AccountInfoOf<T>>::get(address) else { return false };
		matches!(info.account_type, AccountType::Contract(_))
	}

	/// Returns the balance of the account at the given address.
	pub fn balance_of(account: AccountIdOrAddress<T>) -> BalanceWithDust<BalanceOf<T>> {
		let info = <AccountInfoOf<T>>::get(account.address()).unwrap_or_default();
		info.balance(&account.account_id(), Preservation::Preserve)
	}

	/// Returns the balance of this account info.
	pub fn balance(
		&self,
		account: &AccountIdOf<T>,
		preservation: Preservation,
	) -> BalanceWithDust<BalanceOf<T>> {
		let value = T::Currency::reducible_balance(account, preservation, Fortitude::Polite);
		BalanceWithDust::new_unchecked::<T>(value, self.dust)
	}

	/// All the remaining in an account including ed and locked balances.
	pub fn total_balance(account: AccountIdOrAddress<T>) -> BalanceWithDust<BalanceOf<T>> {
		let value = T::Currency::total_balance(&account.account_id());
		let dust = <AccountInfoOf<T>>::get(account.address()).map(|a| a.dust).unwrap_or_default();
		BalanceWithDust::new_unchecked::<T>(value, dust)
	}

	/// Loads the `ContractInfo` backing the address's storage namespace.
	///
	/// Returns `Some` for deployed contracts *and* for EIP-7702 delegated EOAs with an
	/// active delegation; in the latter case the returned info is the authority's own.
	/// Use [`Self::is_contract`] for a strict "deployed contract" check.
	pub fn load_contract(address: &H160) -> Option<ContractInfo<T>> {
		<AccountInfoOf<T>>::get(address)?.account_type.contract_info()
	}

	/// [`Self::load_contract`] plus the EIP-7702 delegation target, from a single read.
	///
	/// Callers that need both must use this rather than pairing `load_contract` with
	/// [`Self::get_delegation_target`], which decodes the same entry twice.
	pub fn load_contract_with_delegation(
		address: &H160,
	) -> (Option<ContractInfo<T>>, Option<H160>) {
		let Some(info) = <AccountInfoOf<T>>::get(address) else { return (None, None) };
		let target = match &info.account_type {
			AccountType::DelegatedEOA { delegate_target, .. } => *delegate_target,
			_ => None,
		};
		(info.account_type.contract_info(), target)
	}

	/// Insert a contract, existing dust if any will be unchanged.
	pub fn insert_contract(address: &H160, contract: ContractInfo<T>) {
		AccountInfoOf::<T>::mutate(address, |account| {
			if let Some(account) = account {
				match &mut account.account_type {
					AccountType::DelegatedEOA { contract_info, .. } => {
						*contract_info = contract;
					},
					_ => account.account_type = contract.into(),
				}
			} else {
				*account = Some(AccountInfo { account_type: contract.into(), dust: 0 });
			}
		});
	}

	/// Updates the ContractInfo for storage operations at a given address.
	pub fn update_contract_info(address: &H160, contract_info: ContractInfo<T>) {
		AccountInfoOf::<T>::mutate(address, |account| {
			if let Some(account) = account {
				match &mut account.account_type {
					AccountType::Contract(info) => *info = contract_info,
					AccountType::DelegatedEOA { contract_info: info, .. } => *info = contract_info,
					AccountType::EOA => {},
				}
			}
		});
	}

	/// EIP-7702: Check if an account has a delegation indicator set
	pub fn is_delegated(address: &H160) -> bool {
		let Some(info) = <AccountInfoOf<T>>::get(address) else { return false };
		matches!(info.account_type, AccountType::DelegatedEOA { delegate_target: Some(_), .. })
	}

	/// EIP-7702: Get the delegation target for an address
	pub fn get_delegation_target(address: &H160) -> Option<H160> {
		let info = <AccountInfoOf<T>>::get(address)?;
		match info.account_type {
			AccountType::DelegatedEOA { delegate_target: Some(target), .. } => Some(target),
			_ => None,
		}
	}

	/// EIP-7702: `true` when a call to `address` must halt because it delegates to a target that
	/// is itself delegated. The spec forbids following the chain: the call retrieves the target's
	/// `0xef0100 || ..` indicator and traps on the leading `0xef`.
	///
	/// Costs one [`AccountInfoOf`] read for an undelegated `address`, two otherwise.
	pub fn is_chained_delegation(address: &H160) -> bool {
		Self::get_delegation_target(address).is_some_and(|target| Self::is_delegated(&target))
	}

	/// EIP-7702: Read the account that paid the currently held delegation deposit, if any.
	pub(crate) fn get_delegation_payer(address: &H160) -> Option<T::AccountId> {
		let info = <AccountInfoOf<T>>::get(address)?;
		match info.account_type {
			AccountType::DelegatedEOA { payer, .. } => payer,
			_ => None,
		}
	}

	/// EIP-7702: Update the delegation payer in place. No-op if the account is not delegated.
	pub(crate) fn set_delegation_payer(address: &H160, new_payer: Option<T::AccountId>) {
		<AccountInfoOf<T>>::mutate(address, |slot| {
			if let Some(AccountInfo {
				account_type: AccountType::DelegatedEOA { payer, .. }, ..
			}) = slot
			{
				*payer = new_payer;
			}
		});
	}

	/// EIP-7702: Build the 23-byte delegation indicator `0xef0100 || target`.
	pub fn delegation_indicator(target: &H160) -> [u8; 23] {
		let mut buf = [0u8; 23];
		buf[0] = 0xef;
		buf[1] = 0x01;
		buf[2] = 0x00;
		buf[3..23].copy_from_slice(target.as_bytes());
		buf
	}

	/// EIP-7702: Set a delegation indicator for an EOA
	///
	/// Marks the account as delegated to the target address.
	/// The `DelegatedEOA` variant always carries a `ContractInfo`, but per EIP-7702 it only
	/// snapshots the target's `code_hash`/deposit when the target is a contract. If the target is
	/// itself delegated or a plain EOA, those fields are zeroed (no chain following). Existing
	/// deposit accounting is preserved across re-delegations.
	///
	/// Returns the previous and new delegation deposits (see [`DelegationDepositChange`]) so the
	/// caller can refund the old deposit and charge the new one.
	///
	/// # Spec deviation: code is resolved at delegation time, not at call time
	///
	/// The target's `code_hash` (and the resulting `ContractInfo`) is snapshotted from
	/// `AccountInfoOf::<T>::get(&target)` here, not looked up live on every call. This is
	/// stable when `target` is already a deployed contract: the only way to change a
	/// contract's code is via root `set_code`, so the snapshot stays accurate.
	///
	/// It is **not** spec-compliant when `target` is **empty** at delegation time and a
	/// contract is later deployed to that address (e.g., via `CREATE2` or Nick's method,
	/// possibly even in the same transaction as the delegation). Spec-compliant clients
	/// resolve code at call time, so a post-delegation deployment would "wake up" the
	/// delegation. Here, the snapshot stays at zero and the authority continues to
	/// behave like a no-code EOA. The niche but real case this breaks is a single EIP-7702
	/// transaction that calls a factory which deploys to the future target *and* delegates
	/// to it — on revive the delegation never activates.
	pub(crate) fn set_delegation(
		address: &H160,
		target: H160,
	) -> Result<DelegationDepositChange<T>, DispatchError> {
		// Atomic: a failed refcount update below must roll back the account mutation.
		with_transaction(|| -> TransactionOutcome<Result<_, DispatchError>> {
			let result = (|| -> Result<DelegationDepositChange<T>, DispatchError> {
				// `Some` iff target is a deployed contract with a real (non-zero) code
				// hash. Precompiles and other special accounts surface as
				// `AccountType::Contract` with `code_hash == 0` and have no `CodeInfo`;
				// per EIP-7702 they should be delegated to successfully and behave as
				// empty code on call, so we filter them out here. The deposit is looked
				// up separately so a contract with a non-zero hash but missing
				// `CodeInfo` (malformed state) still snapshots and surfaces via the
				// refcount bump below.
				let target_code_hash: Option<sp_core::H256> = <AccountInfoOf<T>>::get(&target)
					.and_then(|info| match info.account_type {
						AccountType::Contract(c) if !c.code_hash.is_zero() => Some(c.code_hash),
						_ => None,
					});
				let target_code_deposit: Option<BalanceOf<T>> =
					target_code_hash.and_then(|h| CodeInfoOf::<T>::get(h).map(|ci| ci.deposit()));

				// Ensure the account is `DelegatedEOA` (creating one if necessary), then
				// update its fields in a single pass. Returns the previous non-zero
				// code_hash and the deposit delta for refcount/deposit accounting below.
				let (old_code_hash, old_deposit, new_deposit) =
					AccountInfoOf::<T>::mutate(address, |slot| {
						let fresh_delegated = || AccountType::DelegatedEOA {
							delegate_target: None,
							contract_info: ContractInfo::<T>::new_for_delegation(
								address,
								Default::default(),
							),
							payer: None,
						};
						match slot.as_mut() {
							None => {
								*slot =
									Some(AccountInfo { account_type: fresh_delegated(), dust: 0 })
							},
							Some(AccountInfo {
								account_type: AccountType::DelegatedEOA { .. },
								..
							}) => {},
							Some(account) => {
								debug_assert!(
									!matches!(account.account_type, AccountType::Contract(_)),
									"set_delegation must not be called on contract accounts"
								);
								// Preserve `dust`; only swap `account_type`.
								account.account_type = fresh_delegated();
							},
						}

						let Some(AccountInfo {
							account_type:
								AccountType::DelegatedEOA { delegate_target, contract_info, .. },
							..
						}) = slot
						else {
							unreachable!("initialized to DelegatedEOA above; qed")
						};

						let old_code_hash = Some(contract_info.code_hash).filter(|h| !h.is_zero());
						let old_deposit = contract_info.storage_base_deposit;

						*delegate_target = Some(target);
						let new_deposit = match target_code_hash {
							Some(code_hash) => {
								contract_info.code_hash = code_hash;
								// Deposit is only updated if we found the `CodeInfo`; if not,
								// `new_deposit` stays at zero and the failing `increment_refcount`
								// below propagates the malformed-state error.
								target_code_deposit
									.map(|d| contract_info.update_base_deposit(d))
									.unwrap_or(Zero::zero())
							},
							None => {
								// Target is not a contract — clear any stale snapshot so a
								// later re-delegation doesn't double-account refcount/deposit.
								contract_info.code_hash = Default::default();
								contract_info.storage_base_deposit = Zero::zero();
								Zero::zero()
							},
						};

						(old_code_hash, old_deposit, new_deposit)
					});

				// Manage code refcounts, skipping when the hash is unchanged.
				if let Some(new_hash) = target_code_hash &&
					Some(new_hash) != old_code_hash
				{
					CodeInfo::<T>::increment_refcount(new_hash).inspect_err(|e| {
						log::warn!(target: LOG_TARGET, "increment_refcount({new_hash:?}) failed: {e:?}");
					})?;
				}
				if let Some(old_hash) = old_code_hash &&
					Some(old_hash) != target_code_hash
				{
					let _ = CodeInfo::<T>::decrement_refcount(old_hash).inspect_err(|e| {
						log::warn!(target: LOG_TARGET, "decrement_refcount({old_hash:?}) failed: {e:?}");
					})?;
				}

				Ok(DelegationDepositChange { previous: old_deposit, current: new_deposit })
			})();

			match result {
				Ok(deposit) => TransactionOutcome::Commit(Ok(deposit)),
				Err(err) => TransactionOutcome::Rollback(Err(err)),
			}
		})
	}

	/// EIP-7702: Clear delegation indicator.
	///
	/// The account stays `DelegatedEOA` with `delegate_target = None` so that
	/// the child trie and deposit accounting are preserved for future re-delegation.
	///
	/// Returns the previously held `storage_base_deposit` (now released). The caller must
	/// refund this amount to whoever originally paid it — look up via
	/// [`AccountInfo::get_delegation_payer`] before invoking `clear_delegation`.
	pub(crate) fn clear_delegation(address: &H160) -> Result<BalanceOf<T>, DispatchError> {
		// Atomic: a failed `decrement_refcount` must roll back `delegate_target = None`.
		with_transaction(|| -> TransactionOutcome<Result<_, DispatchError>> {
			let result = AccountInfoOf::<T>::mutate(
				address,
				|account| -> Result<BalanceOf<T>, DispatchError> {
					let mut refund: BalanceOf<T> = Zero::zero();
					if let Some(AccountInfo {
						account_type:
							AccountType::DelegatedEOA { delegate_target, contract_info, .. },
						..
					}) = account
					{
						*delegate_target = None;
						if !contract_info.code_hash.is_zero() {
							let _ = CodeInfo::<T>::decrement_refcount(contract_info.code_hash).inspect_err(|e| {
								log::warn!(target: LOG_TARGET, "decrement_refcount({:?}) failed: {e:?}", contract_info.code_hash);
							})?;
							refund = core::mem::take(&mut contract_info.storage_base_deposit);
							contract_info.code_hash = Default::default();
						}
					}
					Ok(refund)
				},
			);

			match result {
				Ok(deposit) => TransactionOutcome::Commit(Ok(deposit)),
				Err(err) => TransactionOutcome::Rollback(Err(err)),
			}
		})
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

		// Reject reuse of an address whose previous occupant still has unflushed
		// `NativeDepositOf` rows in the deletion queue. The on_idle drain will eventually
		// clear them; until it does, instantiating here would let the new contract inherit
		// stale per-payer entitlements.
		let account_id = T::AddressMapper::to_fallback_account_id(address);
		if NativeDepositOf::<T>::iter_prefix(&account_id).next().is_some() {
			return Err(Error::<T>::PendingDepositCleanup.into());
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

	/// Constructs a new contract info for a delegated account (EIP-7702).
	///
	/// Delegated accounts have their own child trie for storage but use the code hash
	/// of the target contract they delegate to. The trie_id is derived solely from the
	/// address so that storage persists across re-delegations to different targets.
	pub fn new_for_delegation(address: &H160, target_code_hash: sp_core::H256) -> Self {
		let trie_id = {
			let buf = ("delegated_trie_v1", address).using_encoded(T::Hashing::hash);
			buf.as_ref()
				.to_vec()
				.try_into()
				.expect("Runtime uses a reasonable hash size. Hence sizeof(T::Hash) <= 128; qed")
		};

		Self {
			trie_id,
			code_hash: target_code_hash,
			storage_bytes: 0,
			storage_items: 0,
			storage_byte_deposit: Zero::zero(),
			storage_item_deposit: Zero::zero(),
			storage_base_deposit: Zero::zero(),
			immutable_data_len: 0,
		}
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
		return value;
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
		frame_meter: Option<&mut FrameMeter<T>>,
		take: bool,
	) -> Result<WriteOutcome, DispatchError> {
		log::trace!(target: crate::LOG_TARGET, "contract storage: writing value {:?} for key {:x?}", new_value, key);
		let hashed_key = key.hash();
		if_tracing(|t| {
			let old = child::get_raw(&self.child_trie_info(), hashed_key.as_slice());
			t.storage_write(key, old, new_value.as_deref());
		});

		self.write_raw(&hashed_key, new_value.as_deref(), frame_meter, take)
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
		frame_meter: Option<&mut FrameMeter<T>>,
		take: bool,
	) -> Result<WriteOutcome, DispatchError> {
		let child_trie_info = &self.child_trie_info();
		let (old_len, old_value) = if take {
			let val = child::get_raw(child_trie_info, key);
			(val.as_ref().map(|v| v.len() as u32), val)
		} else {
			(child::len(child_trie_info, key), None)
		};

		if let Some(frame_meter) = frame_meter {
			let mut diff = Diff::default();
			let key_len = key.len() as u32;
			match (old_len, new_value.as_ref().map(|v| v.len() as u32)) {
				(Some(old_len), Some(new_len)) => {
					if new_len > old_len {
						diff.bytes_added = new_len - old_len;
					} else {
						diff.bytes_removed = old_len - new_len;
					}
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
			frame_meter.record_contract_storage_changes(&diff)?;
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
		let contract_deposit = {
			let bytes_added: u32 =
				(self.encoded_size() as u32).saturating_add(self.immutable_data_len);
			let items_added: u32 = if self.immutable_data_len == 0 { 1 } else { 2 };

			T::DepositPerByte::get()
				.saturating_mul(bytes_added.into())
				.saturating_add(T::DepositPerItem::get().saturating_mul(items_added.into()))
		};

		// Instantiating the contract prevents its code to be deleted, therefore the base deposit
		// includes a fraction (`T::CodeHashLockupDepositPercent`) of the original storage deposit
		// to prevent abuse.
		let code_deposit = T::CodeHashLockupDepositPercent::get().mul_ceil(code_deposit);

		let deposit = contract_deposit.saturating_add(code_deposit);
		self.storage_base_deposit = deposit;
		deposit
	}

	/// Push a contract's trie and account to the deletion queue for lazy removal.
	///
	/// You must make sure that the contract is also removed when queuing for deletion.
	/// Both the contract's child trie and any [`NativeDepositOf`] entries it held are drained
	/// lazily in `on_idle`.
	pub fn queue_for_deletion(trie_id: TrieId, contract: AccountIdOf<T>) {
		DeletionQueueManager::<T>::load().insert(DeletionQueueItem::new(trie_id, contract));
	}

	/// Returns the total weight available for deletion-queue processing after subtracting
	/// the fixed [`WeightInfo::deletion_queue_batch`] base.
	pub fn deletion_budget(meter: &WeightMeter) -> Weight {
		meter.limit().saturating_sub(T::WeightInfo::deletion_queue_batch())
	}

	/// Delete as many items from the deletion queue as possible within the supplied weight
	/// limit.
	pub fn process_deletion_queue_batch(meter: &mut WeightMeter) {
		if meter.try_consume(T::WeightInfo::deletion_queue_batch()).is_err() {
			return;
		};

		let mut queue = <DeletionQueueManager<T>>::load();
		if queue.is_empty() {
			return;
		}

		let weight_per_entry = T::WeightInfo::deletion_queue_per_entry()
			.saturating_sub(T::WeightInfo::deletion_queue_batch());
		let weight_per_native_key = T::WeightInfo::deletion_queue_per_native_deposit_key(1)
			.saturating_sub(T::WeightInfo::deletion_queue_per_native_deposit_key(0));
		let weight_per_trie_key = T::WeightInfo::deletion_queue_per_trie_key(1)
			.saturating_sub(T::WeightInfo::deletion_queue_per_trie_key(0));

		let budget = Self::deletion_budget(&meter);
		let mut remaining = budget;

		let key_budget_for = |remaining: Weight, w: Weight| -> u32 {
			// `w == 0` would be a benchmark misconfiguration; refuse to touch keys in that case
			// rather than loop forever.
			remaining.checked_div_per_component(&w).unwrap_or(0).min(u32::MAX as u64) as u32
		};

		loop {
			let Some(entry) = queue.next() else { break };

			// Charge the per-entry overhead.
			let Some(after_entry) = remaining.checked_sub(&weight_per_entry) else { break };
			remaining = after_entry;

			// Phase 1: drain `NativeDepositOf` rows for this contract.
			let key_budget = key_budget_for(remaining, weight_per_native_key);
			if key_budget == 0 {
				break;
			}
			let result =
				NativeDepositOf::<T>::clear_prefix(&entry.value.account_id, key_budget, None);
			remaining = remaining
				.saturating_sub(weight_per_native_key.saturating_mul(u64::from(result.unique)));
			if result.maybe_cursor.is_some() {
				break;
			}

			// Phase 2: kill the child trie.
			let key_budget = key_budget_for(remaining, weight_per_trie_key);
			if key_budget == 0 {
				break;
			}
			#[allow(deprecated)]
			let outcome = child::kill_storage(
				&ChildInfo::new_default(&entry.value.trie_id),
				Some(key_budget),
			);
			match outcome {
				KillStorageResult::SomeRemaining(keys_removed) => {
					remaining = remaining
						.saturating_sub(weight_per_trie_key.saturating_mul(keys_removed.into()));
					break;
				},
				KillStorageResult::AllRemoved(keys_removed) => {
					remaining = remaining.saturating_sub(
						weight_per_trie_key.saturating_mul(u64::from(keys_removed)),
					);
					entry.remove();
				},
			};
		}

		meter.consume(budget.saturating_sub(remaining));
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
#[derive(Clone, Eq, PartialEq, Encode, Decode, Debug, TypeInfo)]
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

/// A contract queued for lazy cleanup.
///
/// Holds the data needed to drain both the contract's [`NativeDepositOf`] rows and its child
/// trie. Cleanup runs in two phases per batch (native rows first, then the trie); the entry
/// stays in the queue until both phases have finished for it.
#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, CloneNoBound, DebugNoBound, PartialEq, Eq)]
#[scale_info(skip_type_params(T))]
pub struct DeletionQueueItem<T: Config> {
	/// The contract's child trie.
	pub trie_id: TrieId,
	/// The contract account whose [`NativeDepositOf`] entries must be cleared.
	pub account_id: AccountIdOf<T>,
}

impl<T: Config> DeletionQueueItem<T> {
	pub fn new(trie_id: TrieId, account_id: AccountIdOf<T>) -> Self {
		Self { trie_id, account_id }
	}
}

/// View on a contract that is marked for deletion.
struct DeletionQueueEntry<'a, T: Config> {
	/// The queued deletion record.
	value: DeletionQueueItem<T>,

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
	fn insert(&mut self, value: DeletionQueueItem<T>) {
		<DeletionQueue<T>>::insert(self.insert_counter, value);
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
			return None;
		}

		let entry = <DeletionQueue<T>>::get(self.delete_counter);
		entry.map(|value| DeletionQueueEntry { value, queue: self })
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
