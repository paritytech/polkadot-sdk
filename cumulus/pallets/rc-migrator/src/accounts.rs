// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Account/Balance data migrator module.

/*
TODO: remove when not needed

Regular native asset teleport from Relay (mint authority) to Asset Hub looks like:

Relay: burn_from(source, amount) // publishes Balances::Burned event
Relay: mint_into(checking, amount) // publishes Balances::Minted event
Relay: no effect on total issuance
Relay: XCM with teleport sent
AH: mint_into(dest, amount) // publishes Balances::Minted event
AH: total issuance increased by `amount`
Relay: XCM teleport processed

^ The minimum what we should replay while moving accounts from Relay to AH

When the Asset Hub turned to the mint authority

Relay: let checking_total = // total checking account balance
Relay: burn_from(checking, checking_total) // publishes Balances::Burned event
AH: let total_issuance = // total issuance on AH
AH: mint_into(checking, checking_total - total_issuance) // publishes Balances::Minted event

^ Ensure that this is the desired method of communicating the mint authority change via events.

*/

use crate::{types::*, *};
use codec::DecodeAll;
use frame_support::{traits::tokens::IdAmount, weights::WeightMeter};
use frame_system::Account as SystemAccount;
use pallet_balances::{AccountData, BalanceLock};
use sp_core::ByteArray;
use sp_runtime::{traits::Zero, BoundedVec};

/// Account type meant to transfer data between RC and AH.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct Account<AccountId, Balance, HoldReason, FreezeReason> {
	/// The account address
	pub who: AccountId,
	/// Free balance.
	///
	/// `free` + `reserved` - the total balance to be minted for `who` on the Asset Hub.
	pub free: Balance,
	/// Reserved balance.
	///
	/// This is not used to establish the reserved balance on the Asset Hub, but used to assert the
	/// total reserve balance after applying all `holds` and `unnamed_reserve`.
	pub reserved: Balance,
	/// Frozen balance.
	///
	/// This is not used to establish the reserved balance on the Asset Hub, but used to assert the
	/// total reserve balance after applying all `freezes` and `locks`.
	pub frozen: Balance,
	/// Account holds from Relay Chain.
	///
	/// Expected hold reasons:
	/// - DelegatedStaking: StakingDelegation (only on Kusama)
	/// - Preimage: Preimage
	/// - Staking: Staking - later instead of "staking " lock
	pub holds: BoundedVec<IdAmount<HoldReason, Balance>, ConstU32<5>>,
	/// Account freezes from Relay Chain.
	///
	/// Expected freeze reasons:
	/// - NominationPools: PoolMinBalance
	pub freezes: BoundedVec<IdAmount<FreezeReason, Balance>, ConstU32<5>>,
	/// Account locks from Relay Chain.
	///
	/// Expected lock ids:
	/// - "staking " - should be transformed to hold with https://github.com/paritytech/polkadot-sdk/pull/5501
	/// - "vesting "
	/// - "pyconvot"
	pub locks: BoundedVec<BalanceLock<Balance>, ConstU32<5>>,
	/// Unnamed reserve.
	///
	/// Only unnamed reserves for Polkadot and Kusama (no named ones).
	pub unnamed_reserve: Balance,
	/// Consumer ref count of migrating to Asset Hub pallets except a reference for `reserved` and
	/// `frozen` balance.
	///
	/// Since the `reserved` and `frozen` balances will be known on a receiving side (AH) they will
	/// be calculated there.
	pub consumers: u8,
	/// Provider ref count of migrating to Asset Hub pallets except the reference for existential
	/// deposit.
	///
	/// Since the `free` balance will be known on a receiving side (AH) the ref count will be
	/// calculated there.
	pub providers: u8,
}

impl<AccountId, Balance: Zero, HoldReason, FreezeReason>
	Account<AccountId, Balance, HoldReason, FreezeReason>
{
	/// Check if the total account balance is liquid.
	pub fn is_liquid(&self) -> bool {
		self.unnamed_reserve.is_zero() &&
			self.freezes.is_empty() &&
			self.locks.is_empty() &&
			self.holds.is_empty()
	}
}

/// The state for the Relay Chain accounts.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum AccountState<Balance> {
	/// The account should be migrated to AH and removed on RC.
	Migrate,
	/// The account must stay on RC.
	///
	/// E.g. RC system account.
	Preserve,

	// We might not need the `Part` variation since there are no many cases for `Part` we can just
	// keep the whole account balance on RC
	/// The part of the account must be preserved on RC.
	///
	/// Cases:
	/// - accounts placed deposit for parachain registration (paras_registrar pallet);
	/// - accounts placed deposit for hrmp channel registration (parachains_hrmp pallet);
	Part {
		/// The reserved balance that must be preserved on RC.
		///
		/// In practice reserved by old `Currency` api and has no associated reason.
		reserved: Balance,
	},
}

pub type AccountStateFor<T> = AccountState<<T as pallet_balances::Config>::Balance>;
pub type AccountFor<T> = Account<
	<T as frame_system::Config>::AccountId,
	<T as pallet_balances::Config>::Balance,
	<T as pallet_balances::Config>::RuntimeHoldReason,
	<T as pallet_balances::Config>::FreezeIdentifier,
>;

/// Helper struct tracking total balance kept on RC and total migrated.
#[derive(Encode, Decode, Default, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct MigratedBalances<Balance: Default> {
	pub kept: Balance,
	pub migrated: Balance,
}

pub struct AccountsMigrator<T> {
	_phantom: sp_std::marker::PhantomData<T>,
}

impl<T: Config> PalletMigration for AccountsMigrator<T> {
	type Key = T::AccountId;
	type Error = Error<T>;

	/// Migrate accounts from RC to AH.
	///
	/// Parameters:
	/// - `last_key` - the last migrated account from RC to AH if any
	/// - `weight_counter` - the weight meter
	///
	/// Result:
	/// - None - no accounts left to be migrated to AH.
	/// - Some(maybe_last_key) - the last migrated account from RC to AH if any
	fn migrate_many(
		last_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Error<T>> {
		// we should not send more than we allocated on AH for the migration.
		let mut ah_weight = WeightMeter::with_limit(T::MaxAhWeight::get());
		// accounts batch for the current iteration.
		let mut batch = XcmBatchAndMeter::new_from_config::<T>();

		let mut iter = if let Some(ref last_key) = last_key {
			SystemAccount::<T>::iter_from_key(last_key)
		} else {
			SystemAccount::<T>::iter()
		};

		let mut maybe_last_key = last_key;
		loop {
			// account the weight for migrating a single account on Relay Chain.
			if weight_counter.try_consume(T::RcWeightInfo::withdraw_account()).is_err() ||
				weight_counter.try_consume(batch.consume_weight()).is_err()
			{
				log::info!("RC weight limit reached at batch length {}, stopping", batch.len());
				if batch.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}

			let Some((who, account_info)) = iter.next() else {
				maybe_last_key = None;
				break;
			};

			let withdraw_res =
				with_transaction_opaque_err::<Option<AccountFor<T>>, Error<T>, _>(|| {
					match Self::withdraw_account(
						who.clone(),
						account_info.clone(),
						&mut ah_weight,
						batch.len() as u32,
					) {
						Ok(ok) => TransactionOutcome::Commit(Ok(ok)),
						Err(e) => TransactionOutcome::Rollback(Err(e)),
					}
				})
				.expect("Always returning Ok; qed");

			match withdraw_res {
				// Account does not need to be migrated
				Ok(None) => {
					// if this the last account to handle at this iteration, we skip it next time.
					maybe_last_key = Some(who);
					continue;
				},
				Ok(Some(ah_account)) => {
					// if this the last account to handle at this iteration, we skip it next time.
					maybe_last_key = Some(who);
					batch.push(ah_account)
				},
				// Not enough weight, lets try again in the next block since we made some progress.
				Err(Error::OutOfWeight) if !batch.is_empty() => {
					break;
				},
				// Not enough weight and was unable to make progress, bad.
				Err(Error::OutOfWeight) if batch.is_empty() => {
					defensive!("Not enough weight to migrate a single account");
					return Err(Error::OutOfWeight);
				},
				Err(e) => {
					// if this the last account to handle at this iteration, we skip it next time.
					maybe_last_key = Some(who.clone());
					defensive!("Error while migrating account");
					log::error!(
						target: LOG_TARGET,
						"Error while migrating account: {:?}, error: {:?}",
						who.to_ss58check(),
						e
					);
					continue;
				},
			}
		}

		if !batch.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				batch,
				|batch| types::AhMigratorCall::<T>::ReceiveAccounts { accounts: batch },
				|n| T::AhWeightInfo::receive_liquid_accounts(n),
			)?;
		}

		Ok(maybe_last_key)
	}
}

impl<T: Config> AccountsMigrator<T> {
	/// Migrate a single account out of the Relay chain and return it.
	///
	/// The account on the relay chain is modified as part of this operation.
	pub fn withdraw_account(
		who: T::AccountId,
		account_info: AccountInfoFor<T>,
		ah_weight: &mut WeightMeter,
		batch_len: u32,
	) -> Result<Option<AccountFor<T>>, Error<T>> {
		if let AccountState::Preserve = Self::get_rc_state(&who) {
			log::info!(
				target: LOG_TARGET,
				"Preserving account on Relay Chain: '{:?}'",
				who.to_ss58check(),
			);
			return Ok(None);
		}

		log::trace!(
			target: LOG_TARGET,
			"Migrating account '{}'",
			who.to_ss58check(),
		);

		// migrate the target account:
		// - keep `balance`, `holds`, `freezes`, .. in memory
		// - check if there is anything to migrate
		// - release all `holds`, `freezes`, ...
		// - burn from target account the `balance` to be moved from RC to AH
		// - add `balance`, `holds`, `freezes`, .. to the accounts package to be sent via XCM

		let account_data: AccountData<T::Balance> = account_info.data.clone();

		if !Self::can_migrate_account(&who, &account_info) {
			log::info!(target: LOG_TARGET, "Account '{}' is not migrated", who.to_ss58check());
			return Ok(None);
		}

		let freezes: Vec<IdAmount<T::FreezeIdentifier, T::Balance>> =
			pallet_balances::Freezes::<T>::get(&who).into();

		for freeze in &freezes {
			if let Err(e) = <T as Config>::Currency::thaw(&freeze.id, &who) {
				log::error!(target: LOG_TARGET,
					"Failed to thaw freeze: {:?} \
					for account: {:?} \
					with error: {:?}",
					freeze.id,
					who.to_ss58check(),
					e
				);
				return Err(Error::FailedToWithdrawAccount);
			}
		}

		let ed = <T as Config>::Currency::minimum_balance();
		let holds: Vec<IdAmount<<T as Config>::RuntimeHoldReason, T::Balance>> =
			pallet_balances::Holds::<T>::get(&who).into();

		for hold in &holds {
			let IdAmount { id, amount } = hold.clone();
			let free = <T as Config>::Currency::balance(&who);

			// When the free balance is below the minimum balance and we attempt to release a hold,
			// the `fungible` implementation would burn the entire free balance while zeroing the
			// hold. To prevent this, we partially release the hold just enough to raise the free
			// balance to the minimum balance, while maintaining some balance on hold. This approach
			// prevents the free balance from being burned.
			// This scenario causes a panic in the test environment - see:
			// https://github.com/paritytech/polkadot-sdk/blob/35e6befc5dd61deb154ff0eb7c180a038e626d66/substrate/frame/balances/src/impl_fungible.rs#L285
			let amount = if free < ed && amount.saturating_sub(ed - free) > 0 {
				log::debug!(
					target: LOG_TARGET,
					"Partially releasing hold to prevent the free balance from being burned"
				);
				let partial_amount = ed - free;
				if let Err(e) =
					<T as Config>::Currency::release(&id, &who, partial_amount, Precision::Exact)
				{
					log::error!(target: LOG_TARGET,
						"Failed to partially release hold: {:?} \
						for account: {:?}, \
						partial amount: {:?}, \
						with error: {:?}",
						id,
						who.to_ss58check(),
						partial_amount,
						e
					);
					return Err(Error::FailedToWithdrawAccount);
				}
				amount - partial_amount
			} else {
				amount
			};

			if let Err(e) = <T as Config>::Currency::release(&id, &who, amount, Precision::Exact) {
				log::error!(target: LOG_TARGET,
					"Failed to release the hold: {:?} \
					for account: {:?}, \
					amount: {:?}, \
					with error: {:?}",
					id,
					who.to_ss58check(),
					amount,
					e
				);
				return Err(Error::FailedToWithdrawAccount);
			}
		}

		let locks: Vec<BalanceLock<T::Balance>> =
			pallet_balances::Locks::<T>::get(&who).into_inner();

		for lock in &locks {
			// Expected lock ids:
			// - "staking " // should be transformed to hold with https://github.com/paritytech/polkadot-sdk/pull/5501
			// - "vesting "
			// - "pyconvot"
			<T as Config>::Currency::remove_lock(lock.id, &who);
		}

		let rc_state = Self::get_rc_state(&who);
		let (rc_reserve, rc_free_min) = match rc_state {
			AccountState::Part { reserved } => {
				log::debug!(
					target: LOG_TARGET,
					"Keep part of account '{:?}' on Relay Chain. reserved: {}",
					who.to_ss58check(),
					&reserved,
				);
				(reserved, <T as Config>::Currency::minimum_balance())
			},
			// migrate the entire account
			AccountState::Migrate => (0, 0),
			// this should not happen bc AccountState::Preserve is checked at the very beginning.
			_ => {
				log::warn!(
					target: LOG_TARGET,
					"Unexpected account state for '{:?}' on Relay Chain: {:?}",
					who.to_ss58check(),
					rc_state,
				);
				return Err(Error::FailedToWithdrawAccount);
			},
		};

		// unreserve the unnamed reserve but keep some reserve on RC if needed.
		let unnamed_reserve = <T as Config>::Currency::reserved_balance(&who)
			.checked_sub(rc_reserve)
			.defensive_unwrap_or_default();
		let _ = <T as Config>::Currency::unreserve(&who, unnamed_reserve);

		// ensuring the account can be fully withdrawn from RC to AH requires force-updating
		// the references here. Instead, for accounts meant to be fully migrated to the AH, we will
		// calculate the actual reference counts based on the migrating pallets and transfer the
		// counts to AH. This is done using the `Self::get_consumer_count` and
		// `Self::get_provider_count` functions.
		//
		// check accounts.md for more details.
		//
		// accounts fully migrating to AH will have a consumer count of `0` on Relay Chain since all
		// holds and freezes are removed. Accounts keeping some reserve on RC will have a consumer
		// count of `1` as they have consumers of the reserve. The provider count is set to `1` to
		// allow reaping accounts that provided the ED at the `burn_from` below.
		let consumers = if rc_reserve > 0 { 1 } else { 0 };
		SystemAccount::<T>::mutate(&who, |a| {
			a.consumers = consumers;
			a.providers = 1;
		});

		let total_balance = <T as Config>::Currency::total_balance(&who);
		let teleport_total = <T as Config>::Currency::reducible_balance(
			&who,
			Preservation::Expendable,
			Fortitude::Polite,
		);

		let teleport_free =
			account_data.free.checked_sub(rc_free_min).defensive_unwrap_or_default();
		let teleport_reserved =
			account_data.reserved.checked_sub(rc_reserve).defensive_unwrap_or_default();

		defensive_assert!(teleport_total == total_balance - rc_free_min - rc_reserve);
		defensive_assert!(teleport_total == teleport_free + teleport_reserved);

		let burned = match <T as Config>::Currency::burn_from(
			&who,
			teleport_total,
			Preservation::Expendable,
			Precision::Exact,
			Fortitude::Polite,
		) {
			Ok(burned) => burned,
			Err(e) => {
				log::error!(
					target: LOG_TARGET,
					"Failed to burn balance from account: {}, error: {:?}",
					who.to_ss58check(),
					e
				);
				return Err(Error::FailedToWithdrawAccount);
			},
		};

		debug_assert!(teleport_total == burned);

		Self::update_migrated_balance(&who, teleport_total)?;

		let consumers = Self::get_consumer_count(&who, &account_info);
		let providers = Self::get_provider_count(&who, &account_info, &holds);
		let withdrawn_account = Account {
			who: who.clone(),
			free: teleport_free,
			reserved: teleport_reserved,
			frozen: account_data.frozen,
			holds: BoundedVec::defensive_truncate_from(holds),
			freezes: BoundedVec::defensive_truncate_from(freezes),
			locks: BoundedVec::defensive_truncate_from(locks),
			unnamed_reserve,
			consumers,
			providers,
		};

		// account the weight for receiving a single account on Asset Hub.
		let ah_receive_weight = Self::weight_ah_receive_account(batch_len, &withdrawn_account);
		if ah_weight.try_consume(ah_receive_weight).is_err() {
			log::info!("AH weight limit reached at batch length {}, stopping", batch_len);
			return Err(Error::OutOfWeight);
		}

		Ok(Some(withdrawn_account))
	}

	/// Actions to be done after the accounts migration is finished.
	pub fn finish_balances_migration() {
		pallet_balances::InactiveIssuance::<T>::put(0);
	}

	/// Check if the account can be withdrawn and migrated to AH.
	pub fn can_migrate_account(who: &T::AccountId, account: &AccountInfoFor<T>) -> bool {
		let ed = <T as Config>::Currency::minimum_balance();
		let total_balance = <T as Config>::Currency::total_balance(who);
		if total_balance < ed {
			if account.nonce.is_zero() {
				log::info!(
					target: LOG_TARGET,
					"Possible system non-migratable account detected. \
					Account: '{}', info: {:?}",
					who.to_ss58check(),
					account
				);
			} else {
				log::info!(
					target: LOG_TARGET,
					"Non-migratable account detected. \
					Account: '{}', info: {:?}",
					who.to_ss58check(),
					account
				);
			}
			if !total_balance.is_zero() || !account.data.frozen.is_zero() {
				log::warn!(
					target: LOG_TARGET,
					"Non-migratable account has non-zero balance. \
					Account: '{}', info: {:?}",
					who.to_ss58check(),
					account
				);
			}
			return false;
		}
		true
	}

	/// Get the weight for importing a single account on Asset Hub.
	///
	/// The base weight is only included for the first imported account.
	pub fn weight_ah_receive_account(batch_len: u32, account: &AccountFor<T>) -> Weight {
		let weight_of = if account.is_liquid() {
			T::AhWeightInfo::receive_liquid_accounts
		} else {
			// TODO: use `T::AhWeightInfo::receive_accounts` with xcm v5, where
			// `require_weight_at_most` not required
			T::AhWeightInfo::receive_liquid_accounts
		};
		item_weight_of(weight_of, batch_len)
	}

	/// Consumer ref count of migrating to Asset Hub pallets except a reference for `reserved` and
	/// `frozen` balance.
	///
	/// Since the `reserved` and `frozen` balances will be known on a receiving side (AH) they will
	/// be calculated there.
	///
	/// Check accounts.md for more details.
	pub fn get_consumer_count(_who: &T::AccountId, _info: &AccountInfoFor<T>) -> u8 {
		0
	}

	/// Provider ref count of migrating to Asset Hub pallets except the reference for existential
	/// deposit.
	///
	/// Since the `free` balance will be known on a receiving side (AH) the ref count will be
	/// calculated there.
	///
	/// Check accounts.md for more details.
	pub fn get_provider_count(
		_who: &T::AccountId,
		_info: &AccountInfoFor<T>,
		freezes: &Vec<IdAmount<<T as Config>::RuntimeHoldReason, T::Balance>>,
	) -> u8 {
		if freezes.iter().any(|freeze| freeze.id == T::StakingDelegationReason::get()) {
			// one extra provider for accounts with staking delegation
			1
		} else {
			0
		}
	}

	/// The part of the balance of the `who` that must stay on the Relay Chain.
	pub fn get_rc_state(who: &T::AccountId) -> AccountStateFor<T> {
		if let Some(state) = RcAccounts::<T>::get(who) {
			return state;
		}
		AccountStateFor::<T>::Migrate
	}

	fn update_migrated_balance(
		who: &T::AccountId,
		teleported_balance: T::Balance,
	) -> Result<(), Error<T>> {
		RcMigratedBalance::<T>::mutate(|tracker| {
			tracker.migrated =
				tracker.migrated.checked_add(teleported_balance).ok_or_else(|| {
					log::error!(
						target: LOG_TARGET,
						"Balance overflow when adding balance of {}, balance {:?}, to total migrated {:?}",
						who.to_ss58check(), teleported_balance, tracker.migrated,
					);
					Error::<T>::BalanceOverflow
				})?;
			tracker.kept = tracker.kept.checked_sub(teleported_balance).ok_or_else(|| {
				log::error!(
					target: LOG_TARGET,
					"Balance underflow when subtracting balance of {}, balance {:?}, from total kept {:?}",
					who.to_ss58check(), teleported_balance, tracker.kept,
				);
				Error::<T>::BalanceUnderflow
			})?;
			Ok::<_, Error<T>>(())
		})
	}

	/// Obtain all known accounts that must stay on RC and persist it to the [`RcAccounts`] storage
	/// item.
	///
	/// Should be executed once before the migration starts.
	pub fn obtain_rc_accounts() -> Weight {
		let mut weight = Weight::zero();
		let mut reserves = sp_std::collections::btree_map::BTreeMap::new();
		let mut update_reserves = |id, deposit| {
			if deposit == 0 {
				return;
			}
			reserves.entry(id).and_modify(|e| *e += deposit).or_insert(deposit);
		};

		for (channel_id, info) in hrmp::HrmpChannels::<T>::iter() {
			weight += T::DbWeight::get().reads(1);
			// source: https://github.com/paritytech/polkadot-sdk/blob/3dc3a11cd68762c2e5feb0beba0b61f448c4fc92/polkadot/runtime/parachains/src/hrmp.rs#L1475
			let sender: T::AccountId = channel_id.sender.into_account_truncating();
			update_reserves(sender, info.sender_deposit);

			let recipient: T::AccountId = channel_id.recipient.into_account_truncating();
			// source: https://github.com/paritytech/polkadot-sdk/blob/3dc3a11cd68762c2e5feb0beba0b61f448c4fc92/polkadot/runtime/parachains/src/hrmp.rs#L1539
			update_reserves(recipient, info.recipient_deposit);
		}

		for (channel_id, info) in hrmp::HrmpOpenChannelRequests::<T>::iter() {
			weight += T::DbWeight::get().reads(1);
			// source: https://github.com/paritytech/polkadot-sdk/blob/3dc3a11cd68762c2e5feb0beba0b61f448c4fc92/polkadot/runtime/parachains/src/hrmp.rs#L1475
			let sender: T::AccountId = channel_id.sender.into_account_truncating();
			update_reserves(sender, info.sender_deposit);
		}

		for (_, info) in Paras::<T>::iter() {
			weight += T::DbWeight::get().reads(1);
			update_reserves(info.manager, info.deposit);
		}

		for (id, rc_reserved) in reserves {
			weight += T::DbWeight::get().reads(4);
			let account_entry = SystemAccount::<T>::get(&id);
			let free = <T as Config>::Currency::balance(&id);
			let total_frozen = account_entry.data.frozen;
			let total_reserved = <T as Config>::Currency::reserved_balance(&id);
			let total_hold = pallet_balances::Holds::<T>::get(&id)
				.into_iter()
				// we do not expect more holds
				.take(5)
				.map(|h| h.amount)
				.sum::<T::Balance>();

			let rc_ed = <T as Config>::Currency::minimum_balance();
			let ah_ed = T::AhExistentialDeposit::get();

			// we prioritize the named holds over the unnamed reserve. if any named holds we will
			// send them to the AH and keep up to the `rc_reserved` on the RC.
			let rc_reserved = rc_reserved.min(total_reserved.saturating_sub(total_hold));
			let ah_free = free.saturating_sub(rc_ed);

			if rc_reserved == 0 {
				log::debug!(
					target: LOG_TARGET,
					"Account doesn't have enough reserved balance to keep on RC. account: {:?}.",
					id.to_ss58check(),
				);
				continue;
			}

			if ah_free < ah_ed && rc_reserved >= total_reserved && total_frozen.is_zero() {
				weight += T::DbWeight::get().writes(1);
				// when there is no much free balance and the account is used only for reserves
				// for parachains registering or hrmp channels we will keep the entire account on
				// the RC.
				log::debug!(
					target: LOG_TARGET,
					"Preserve account on Relay Chain: '{:?}'",
					id.to_ss58check()
				);
				RcAccounts::<T>::insert(&id, AccountState::Preserve);
			} else {
				weight += T::DbWeight::get().writes(1);
				log::debug!(
					target: LOG_TARGET,
					"Keep part of account: {:?} reserve: {:?} on the RC",
					id.to_ss58check(),
					rc_reserved
				);
				RcAccounts::<T>::insert(&id, AccountState::Part { reserved: rc_reserved });
			}
		}

		// Keep the on-demand pallet account on the RC.
		weight += T::DbWeight::get().writes(1);
		let on_demand_pallet_account: T::AccountId =
			T::OnDemandPalletId::get().into_account_truncating();
		log::debug!(
			target: LOG_TARGET,
			"Preserve on-demand pallet account on Relay Chain: '{:?}'",
			on_demand_pallet_account.to_ss58check()
		);
		RcAccounts::<T>::insert(&on_demand_pallet_account, AccountState::Preserve);

		weight
	}

	/// Try to translate a Parachain sovereign account to the Parachain AH sovereign account.
	///
	/// Returns:
	/// - `Ok(None)` if the account is not a Parachain sovereign account
	/// - `Ok(Some((ah_account, para_id)))` with the translated account and the para id
	/// - `Err(())` otherwise
	///
	/// The way that this normally works is through the configured `SiblingParachainConvertsVia`:
	/// https://github.com/polkadot-fellows/runtimes/blob/7b096c14c2b16cc81ca4e2188eea9103f120b7a4/system-parachains/asset-hubs/asset-hub-polkadot/src/xcm_config.rs#L93-L94
	/// it passes the `Sibling` type into it which has type-ID `sibl`:
	/// https://github.com/paritytech/polkadot-sdk/blob/c10e25aaa8b8afd8665b53f0a0b02e4ea44caa77/polkadot/parachain/src/primitives.rs#L272-L274.
	/// This type-ID gets used by the converter here:
	/// https://github.com/paritytech/polkadot-sdk/blob/7ecf3f757a5d6f622309cea7f788e8a547a5dce8/polkadot/xcm/xcm-builder/src/location_conversion.rs#L314
	/// and eventually ends up in the encoding here
	/// https://github.com/paritytech/polkadot-sdk/blob/cdf107de700388a52a17b2fb852c98420c78278e/substrate/primitives/runtime/src/traits/mod.rs#L1997-L1999
	/// The `para` conversion is likewise with `ChildParachainConvertsVia` and the `para` type-ID
	/// https://github.com/paritytech/polkadot-sdk/blob/c10e25aaa8b8afd8665b53f0a0b02e4ea44caa77/polkadot/parachain/src/primitives.rs#L162-L164
	pub fn try_translate_rc_sovereign_to_ah(
		acc: T::AccountId,
	) -> Result<Option<(T::AccountId, u16)>, ()> {
		let raw = acc.to_raw_vec();

		// Must start with "para"
		let Some(raw) = raw.strip_prefix(b"para") else {
			return Ok(None);
		};
		// Must end with 26 zero bytes
		let Some(raw) = raw.strip_suffix(&[0u8; 26]) else {
			return Ok(None);
		};
		let para_id = u16::decode_all(&mut &raw[..]).map_err(|_| ())?;

		// Translate to AH sibling account
		let mut ah_raw = [0u8; 32];
		ah_raw[0..4].copy_from_slice(b"sibl");
		ah_raw[4..6].copy_from_slice(&para_id.encode());
		let ah_acc = ah_raw.try_into().map_err(|_| ()).defensive()?;

		Ok(Some((ah_acc, para_id)))
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::RcMigrationCheck for AccountsMigrator<T> {
	// rc_total_issuance_before
	type RcPrePayload = BalanceOf<T>;

	fn pre_check() -> Self::RcPrePayload {
		// Store total issuance and checking account balance before migration
		<T as Config>::Currency::total_issuance()
	}

	fn post_check(rc_total_issuance_before: Self::RcPrePayload) {
		// Check that all accounts have been processed correctly
		let mut kept = 0;
		for (who, acc_state) in RcAccounts::<T>::iter() {
			match acc_state {
				AccountState::Migrate => {
					// Account should be fully migrated
					// Assert storage "Balances::Account::rc_post::empty"
					let total_balance = <T as Config>::Currency::total_balance(&who);
					assert_eq!(
						total_balance,
						0,
						"Account {:?} should have no balance on the relay chain after migration",
						who.to_ss58check()
					);

					// Assert storage "Balances::Locks::rc_post::empty"
					let locks = pallet_balances::Locks::<T>::get(&who);
					assert!(
						locks.is_empty(),
						"Account {:?} should have no locks on the relay chain after migration",
						who.to_ss58check()
					);

					// Assert storage "Balances::Holds::rc_post::empty"
					let holds = pallet_balances::Holds::<T>::get(&who);
					assert!(
						holds.is_empty(),
						"Account {:?} should have no holds on the relay chain after migration",
						who.to_ss58check()
					);

					// Assert storage "Balances::Freezes::rc_post::empty"
					let freezes = pallet_balances::Freezes::<T>::get(&who);
					assert!(
						freezes.is_empty(),
						"Account {:?} should have no freezes on the relay chain after migration",
						who.to_ss58check()
					);

					// Assert storage "Balances::Reserves::rc_post::empty"
					let reserved = <T as Config>::Currency::reserved_balance(&who);
					assert_eq!(
						reserved,
						0,
						"Account {:?} should have no reserves on the relay chain after migration",
						who.to_ss58check()
					);
				},
				AccountState::Preserve => {
					// Account should be fully preserved
					let total_balance = <T as Config>::Currency::total_balance(&who);
					kept += total_balance;
				},
				AccountState::Part { reserved } => {
					// Account should have only the reserved amount
					let total_balance = <T as Config>::Currency::total_balance(&who);
					let free_balance = <T as Config>::Currency::reducible_balance(
						&who,
						Preservation::Expendable,
						Fortitude::Polite,
					);
					let reserved_balance = reserved + <T as Config>::Currency::minimum_balance();
					assert_eq!(
						free_balance, 0,
						"Account {:?} should have no free balance on the relay chain after migration",
						who.to_ss58check()
					);

					// Assert storage "Balances::Account::rc_post::empty"
					assert_eq!(
						total_balance, reserved_balance,
						"Account {:?} should have only reserved balance + min existential deposit on the relay chain after migration",
						who.to_ss58check()
					);

					// Assert storage "Balances::Locks::rc_post::empty"
					let locks = pallet_balances::Locks::<T>::get(&who);
					assert!(
						locks.is_empty(),
						"Account {:?} should have no locks on the relay chain after migration",
						who.to_ss58check()
					);

					// Assert storage "Balances::Holds::rc_post::empty"
					let holds = pallet_balances::Holds::<T>::get(&who);
					assert!(
						holds.is_empty(),
						"Account {:?} should have no holds on the relay chain after migration",
						who.to_ss58check()
					);

					// Assert storage "Balances::Freezes::rc_post::empty"
					let freezes = pallet_balances::Freezes::<T>::get(&who);
					assert!(
						freezes.is_empty(),
						"Account {:?} should have no freezes on the relay chain after migration",
						who.to_ss58check()
					);

					kept += reserved;
				},
			}
		}

		// Check that checking account has no balance (fully migrated)
		let check_account = T::CheckingAccount::get();
		let checking_balance = <T as Config>::Currency::total_balance(&check_account);
		assert_eq!(
			checking_balance, 0,
			"Checking account should have no balance on the relay chain after migration"
		);
		let total_issuance = <T as Config>::Currency::total_issuance();
		let tracker = RcMigratedBalance::<T>::get();
		// Check that total kept balance matches the one computed before the migration
		// TODO: Giuseppe @re-gius
		// assert_eq!(
		// 	kept, tracker.kept,
		// 	"Mismatch for total balance kept on the relay chain: after migration ({}) != computed
		// before migration ({})", 	kept, tracker.kept,
		// );
		// verify total issuance hasn't changed for any other reason than the migrated funds
		assert_eq!(total_issuance, rc_total_issuance_before - tracker.migrated);
		assert_eq!(total_issuance, tracker.kept);
	}
}
