// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! A module responsible for computing the right amount of weight and charging it.

use crate::{
	AliveContractInfo, BalanceOf, ContractInfo, ContractInfoOf, Module, RawEvent,
	TombstoneContractInfo, Config, CodeHash, ConfigCache, Error,
};
use sp_std::prelude::*;
use sp_io::hashing::blake2_256;
use sp_core::crypto::UncheckedFrom;
use frame_support::storage::child;
use frame_support::traits::{Currency, ExistenceRequirement, Get, OnUnbalanced, WithdrawReasons};
use frame_support::StorageMap;
use pallet_contracts_primitives::{ContractAccessError, RentProjection, RentProjectionResult};
use sp_runtime::{
	DispatchError,
	traits::{Bounded, CheckedDiv, CheckedMul, SaturatedConversion, Saturating, Zero},
};

/// The amount to charge.
///
/// This amount respects the contract's rent allowance and the subsistence deposit.
/// Because of that, charging the amount cannot remove the contract.
struct OutstandingAmount<T: Config> {
	amount: BalanceOf<T>,
}

impl<T: Config> OutstandingAmount<T> {
	/// Create the new outstanding amount.
	///
	/// The amount should be always withdrawable and it should not kill the account.
	fn new(amount: BalanceOf<T>) -> Self {
		Self { amount }
	}

	/// Returns the amount this instance wraps.
	fn peek(&self) -> BalanceOf<T> {
		self.amount
	}

	/// Withdraws the outstanding amount from the given account.
	fn withdraw(self, account: &T::AccountId) {
		if let Ok(imbalance) = T::Currency::withdraw(
			account,
			self.amount,
			WithdrawReasons::FEE,
			ExistenceRequirement::KeepAlive,
		) {
			// This should never fail. However, let's err on the safe side.
			T::RentPayment::on_unbalanced(imbalance);
		}
	}
}

enum Verdict<T: Config> {
	/// The contract is exempted from paying rent.
	///
	/// For example, it already paid its rent in the current block, or it has enough deposit for not
	/// paying rent at all.
	Exempt,
	/// Funds dropped below the subsistence deposit.
	///
	/// Remove the contract along with it's storage.
	Kill,
	/// The contract cannot afford payment within its rent budget so it gets evicted. However,
	/// because its balance is greater than the subsistence threshold it leaves a tombstone.
	Evict {
		amount: Option<OutstandingAmount<T>>,
	},
	/// Everything is OK, we just only take some charge.
	Charge { amount: OutstandingAmount<T> },
}

pub struct Rent<T>(sp_std::marker::PhantomData<T>);

impl<T> Rent<T>
where
	T: Config,
	T::AccountId: UncheckedFrom<T::Hash> + AsRef<[u8]>
{
	/// Returns a fee charged per block from the contract.
	///
	/// This function accounts for the storage rent deposit. I.e. if the contract possesses enough funds
	/// then the fee can drop to zero.
	fn compute_fee_per_block(
		free_balance: &BalanceOf<T>,
		contract: &AliveContractInfo<T>
	) -> BalanceOf<T> {
		let free_storage = free_balance
			.checked_div(&T::RentDepositOffset::get())
			.unwrap_or_else(Zero::zero);

		// For now, we treat every empty KV pair as if it was one byte long.
		let empty_pairs_equivalent = contract.empty_pair_count;

		let effective_storage_size = <BalanceOf<T>>::from(
			contract.storage_size + T::StorageSizeOffset::get() + empty_pairs_equivalent,
		)
		.saturating_sub(free_storage);

		effective_storage_size
			.checked_mul(&T::RentByteFee::get())
			.unwrap_or_else(|| <BalanceOf<T>>::max_value())
	}

	/// Returns amount of funds available to consume by rent mechanism.
	///
	/// Rent mechanism cannot consume more than `rent_allowance` set by the contract and it cannot make
	/// the balance lower than [`subsistence_threshold`].
	///
	/// In case the toal_balance is below the subsistence threshold, this function returns `None`.
	fn rent_budget(
		total_balance: &BalanceOf<T>,
		free_balance: &BalanceOf<T>,
		contract: &AliveContractInfo<T>,
	) -> Option<BalanceOf<T>> {
		let subsistence_threshold = ConfigCache::<T>::subsistence_threshold_uncached();
		// Reserved balance contributes towards the subsistence threshold to stay consistent
		// with the existential deposit where the reserved balance is also counted.
		if *total_balance < subsistence_threshold {
			return None;
		}

		// However, reserved balance cannot be charged so we need to use the free balance
		// to calculate the actual budget (which can be 0).
		let rent_allowed_to_charge = free_balance.saturating_sub(subsistence_threshold);
		Some(<BalanceOf<T>>::min(
			contract.rent_allowance,
			rent_allowed_to_charge,
		))
	}

	/// Consider the case for rent payment of the given account and returns a `Verdict`.
	///
	/// Use `handicap` in case you want to change the reference block number. (To get more details see
	/// `snitch_contract_should_be_evicted` ).
	fn consider_case(
		account: &T::AccountId,
		current_block_number: T::BlockNumber,
		handicap: T::BlockNumber,
		contract: &AliveContractInfo<T>,
	) -> Verdict<T> {
		// How much block has passed since the last deduction for the contract.
		let blocks_passed = {
			// Calculate an effective block number, i.e. after adjusting for handicap.
			let effective_block_number = current_block_number.saturating_sub(handicap);
			effective_block_number.saturating_sub(contract.deduct_block)
		};
		if blocks_passed.is_zero() {
			// Rent has already been paid
			return Verdict::Exempt;
		}

		let total_balance = T::Currency::total_balance(account);
		let free_balance = T::Currency::free_balance(account);

		// An amount of funds to charge per block for storage taken up by the contract.
		let fee_per_block = Self::compute_fee_per_block(&free_balance, contract);
		if fee_per_block.is_zero() {
			// The rent deposit offset reduced the fee to 0. This means that the contract
			// gets the rent for free.
			return Verdict::Exempt;
		}

		let rent_budget = match Self::rent_budget(&total_balance, &free_balance, contract) {
			Some(rent_budget) => rent_budget,
			None => {
				// The contract's total balance is already below subsistence threshold. That
				// indicates that the contract cannot afford to leave a tombstone.
				//
				// So cleanly wipe the contract.
				return Verdict::Kill;
			}
		};

		let dues = fee_per_block
			.checked_mul(&blocks_passed.saturated_into::<u32>().into())
			.unwrap_or_else(|| <BalanceOf<T>>::max_value());
		let insufficient_rent = rent_budget < dues;

		// If the rent payment cannot be withdrawn due to locks on the account balance, then evict the
		// account.
		//
		// NOTE: This seems problematic because it provides a way to tombstone an account while
		// avoiding the last rent payment. In effect, someone could retroactively set rent_allowance
		// for their contract to 0.
		let dues_limited = dues.min(rent_budget);
		let can_withdraw_rent = T::Currency::ensure_can_withdraw(
			account,
			dues_limited,
			WithdrawReasons::FEE,
			free_balance.saturating_sub(dues_limited),
		)
		.is_ok();

		if insufficient_rent || !can_withdraw_rent {
			// The contract cannot afford the rent payment and has a balance above the subsistence
			// threshold, so it leaves a tombstone.
			let amount = if can_withdraw_rent {
				Some(OutstandingAmount::new(dues_limited))
			} else {
				None
			};
			return Verdict::Evict { amount };
		}

		return Verdict::Charge {
			// We choose to use `dues_limited` here instead of `dues` just to err on the safer side.
			amount: OutstandingAmount::new(dues_limited),
		};
	}

	/// Enacts the given verdict and returns the updated `ContractInfo`.
	///
	/// `alive_contract_info` should be from the same address as `account`.
	fn enact_verdict(
		account: &T::AccountId,
		alive_contract_info: AliveContractInfo<T>,
		current_block_number: T::BlockNumber,
		verdict: Verdict<T>,
	) -> Option<ContractInfo<T>> {
		match verdict {
			Verdict::Exempt => return Some(ContractInfo::Alive(alive_contract_info)),
			Verdict::Kill => {
				<ContractInfoOf<T>>::remove(account);
				child::kill_storage(
					&alive_contract_info.child_trie_info(),
				);
				<Module<T>>::deposit_event(RawEvent::Evicted(account.clone(), false));
				None
			}
			Verdict::Evict { amount } => {
				if let Some(amount) = amount {
					amount.withdraw(account);
				}

				// Note: this operation is heavy.
				let child_storage_root = child::root(
					&alive_contract_info.child_trie_info(),
				);

				let tombstone = <TombstoneContractInfo<T>>::new(
					&child_storage_root[..],
					alive_contract_info.code_hash,
				);
				let tombstone_info = ContractInfo::Tombstone(tombstone);
				<ContractInfoOf<T>>::insert(account, &tombstone_info);

				child::kill_storage(
					&alive_contract_info.child_trie_info(),
				);

				<Module<T>>::deposit_event(RawEvent::Evicted(account.clone(), true));
				Some(tombstone_info)
			}
			Verdict::Charge { amount } => {
				let contract_info = ContractInfo::Alive(AliveContractInfo::<T> {
					rent_allowance: alive_contract_info.rent_allowance - amount.peek(),
					deduct_block: current_block_number,
					..alive_contract_info
				});
				<ContractInfoOf<T>>::insert(account, &contract_info);

				amount.withdraw(account);
				Some(contract_info)
			}
		}
	}

	/// Make account paying the rent for the current block number
	///
	/// NOTE this function performs eviction eagerly. All changes are read and written directly to
	/// storage.
	pub fn collect(account: &T::AccountId) -> Option<ContractInfo<T>> {
		let contract_info = <ContractInfoOf<T>>::get(account);
		let alive_contract_info = match contract_info {
			None | Some(ContractInfo::Tombstone(_)) => return contract_info,
			Some(ContractInfo::Alive(contract)) => contract,
		};

		let current_block_number = <frame_system::Module<T>>::block_number();
		let verdict = Self::consider_case(
			account,
			current_block_number,
			Zero::zero(),
			&alive_contract_info,
		);
		Self::enact_verdict(account, alive_contract_info, current_block_number, verdict)
	}

	/// Process a report that a contract under the given address should be evicted.
	///
	/// Enact the eviction right away if the contract should be evicted and return true.
	/// Otherwise, **do nothing** and return false.
	///
	/// The `handicap` parameter gives a way to check the rent to a moment in the past instead
	/// of current block. E.g. if the contract is going to be evicted at the current block,
	/// `handicap = 1` can defer the eviction for 1 block. This is useful to handicap certain snitchers
	/// relative to others.
	///
	/// NOTE this function performs eviction eagerly. All changes are read and written directly to
	/// storage.
	pub fn snitch_contract_should_be_evicted(
		account: &T::AccountId,
		handicap: T::BlockNumber,
	) -> bool {
		let contract_info = <ContractInfoOf<T>>::get(account);
		let alive_contract_info = match contract_info {
			None | Some(ContractInfo::Tombstone(_)) => return false,
			Some(ContractInfo::Alive(contract)) => contract,
		};
		let current_block_number = <frame_system::Module<T>>::block_number();
		let verdict = Self::consider_case(
			account,
			current_block_number,
			handicap,
			&alive_contract_info,
		);

		// Enact the verdict only if the contract gets removed.
		match verdict {
			Verdict::Kill | Verdict::Evict { .. } => {
				Self::enact_verdict(account, alive_contract_info, current_block_number, verdict);
				true
			}
			_ => false,
		}
	}

	/// Returns the projected time a given contract will be able to sustain paying its rent. The
	/// returned projection is relevant for the current block, i.e. it is as if the contract was
	/// accessed at the beginning of the current block. Returns `None` in case if the contract was
	/// evicted before or as a result of the rent collection.
	///
	/// The returned value is only an estimation. It doesn't take into account any top ups, changing the
	/// rent allowance, or any problems coming from withdrawing the dues.
	///
	/// NOTE that this is not a side-effect free function! It will actually collect rent and then
	/// compute the projection. This function is only used for implementation of an RPC method through
	/// `RuntimeApi` meaning that the changes will be discarded anyway.
	pub fn compute_projection(
		account: &T::AccountId,
	) -> RentProjectionResult<T::BlockNumber> {
		let contract_info = <ContractInfoOf<T>>::get(account);
		let alive_contract_info = match contract_info {
			None | Some(ContractInfo::Tombstone(_)) => return Err(ContractAccessError::IsTombstone),
			Some(ContractInfo::Alive(contract)) => contract,
		};
		let current_block_number = <frame_system::Module<T>>::block_number();
		let verdict = Self::consider_case(
			account,
			current_block_number,
			Zero::zero(),
			&alive_contract_info,
		);
		let new_contract_info =
			Self::enact_verdict(account, alive_contract_info, current_block_number, verdict);

		// Check what happened after enaction of the verdict.
		let alive_contract_info = match new_contract_info {
			None | Some(ContractInfo::Tombstone(_)) => return Err(ContractAccessError::IsTombstone),
			Some(ContractInfo::Alive(contract)) => contract,
		};

		// Compute how much would the fee per block be with the *updated* balance.
		let total_balance = T::Currency::total_balance(account);
		let free_balance = T::Currency::free_balance(account);
		let fee_per_block = Self::compute_fee_per_block(&free_balance, &alive_contract_info);
		if fee_per_block.is_zero() {
			return Ok(RentProjection::NoEviction);
		}

		// Then compute how much the contract will sustain under these circumstances.
		let rent_budget = Self::rent_budget(&total_balance, &free_balance, &alive_contract_info).expect(
			"the contract exists and in the alive state;
			the updated balance must be greater than subsistence deposit;
			this function doesn't return `None`;
			qed
			",
		);
		let blocks_left = match rent_budget.checked_div(&fee_per_block) {
			Some(blocks_left) => blocks_left,
			None => {
				// `fee_per_block` is not zero here, so `checked_div` can return `None` if
				// there is an overflow. This cannot happen with integers though. Return
				// `NoEviction` here just in case.
				return Ok(RentProjection::NoEviction);
			}
		};

		let blocks_left = blocks_left.saturated_into::<u32>().into();
		Ok(RentProjection::EvictionAt(
			current_block_number + blocks_left,
		))
	}

	/// Restores the destination account using the origin as prototype.
	///
	/// The restoration will be performed iff:
	/// - origin exists and is alive,
	/// - the origin's storage is not written in the current block
	/// - the restored account has tombstone
	/// - the tombstone matches the hash of the origin storage root, and code hash.
	///
	/// Upon succesful restoration, `origin` will be destroyed, all its funds are transferred to
	/// the restored account. The restored account will inherit the last write block and its last
	/// deduct block will be set to the current block.
	pub fn restore_to(
		origin: T::AccountId,
		dest: T::AccountId,
		code_hash: CodeHash<T>,
		rent_allowance: BalanceOf<T>,
		delta: Vec<crate::exec::StorageKey>,
	) -> Result<(), DispatchError> {
		let mut origin_contract = <ContractInfoOf<T>>::get(&origin)
			.and_then(|c| c.get_alive())
			.ok_or(Error::<T>::InvalidSourceContract)?;

		let child_trie_info = origin_contract.child_trie_info();

		let current_block = <frame_system::Module<T>>::block_number();

		if origin_contract.last_write == Some(current_block) {
			return Err(Error::<T>::InvalidContractOrigin.into());
		}

		let dest_tombstone = <ContractInfoOf<T>>::get(&dest)
			.and_then(|c| c.get_tombstone())
			.ok_or(Error::<T>::InvalidDestinationContract)?;

		let last_write = if !delta.is_empty() {
			Some(current_block)
		} else {
			origin_contract.last_write
		};

		let key_values_taken = delta.iter()
			.filter_map(|key| {
				child::get_raw(&child_trie_info, &blake2_256(key)).map(|value| {
					child::kill(&child_trie_info, &blake2_256(key));
					(key, value)
				})
			})
			.collect::<Vec<_>>();

		let tombstone = <TombstoneContractInfo<T>>::new(
			// This operation is cheap enough because last_write (delta not included)
			// is not this block as it has been checked earlier.
			&child::root(&child_trie_info)[..],
			code_hash,
		);

		if tombstone != dest_tombstone {
			for (key, value) in key_values_taken {
				child::put_raw(&child_trie_info, &blake2_256(key), &value);
			}
			return Err(Error::<T>::InvalidTombstone.into());
		}

		origin_contract.storage_size -= key_values_taken.iter()
			.map(|(_, value)| value.len() as u32)
			.sum::<u32>();

		<ContractInfoOf<T>>::remove(&origin);
		<ContractInfoOf<T>>::insert(&dest, ContractInfo::Alive(AliveContractInfo::<T> {
			trie_id: origin_contract.trie_id,
			storage_size: origin_contract.storage_size,
			empty_pair_count: origin_contract.empty_pair_count,
			total_pair_count: origin_contract.total_pair_count,
			code_hash,
			rent_allowance,
			deduct_block: current_block,
			last_write,
		}));

		let origin_free_balance = T::Currency::free_balance(&origin);
		T::Currency::make_free_balance_be(&origin, <BalanceOf<T>>::zero());
		T::Currency::deposit_creating(&dest, origin_free_balance);

		Ok(())
	}
}
