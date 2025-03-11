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
TODO: remove this dec comment when not needed

Sources of account references

provider refs:
- crowdloans: fundraising system account / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/polkadot/runtime/common/src/crowdloan/mod.rs#L416
- parachains_assigner_on_demand / on_demand: pallet's account https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/polkadot/runtime/parachains/src/on_demand/mod.rs#L407
- balances: user account / existential deposit
- session: initial validator set on Genesis / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/session/src/lib.rs#L466
- delegated-staking: delegators and agents (users)

consumer refs:
- balances:
-- might hold on account mutation / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/balances/src/lib.rs#L1007
-- on migration to new logic for every migrating account / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/balances/src/lib.rs#L877
- session:
-- for user setting the keys / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/session/src/lib.rs#L812
-- initial validator set on Genesis / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/session/src/lib.rs#L461
- recovery: user on recovery claim / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/recovery/src/lib.rs#L610
- staking:
-- for user bonding / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/staking/src/pallet/mod.rs#L1036
-- virtual bond / agent key / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/staking/src/pallet/impls.rs#L1948

sufficient refs:
- must be zero since only assets pallet might hold such reference
*/

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
use sp_runtime::traits::Zero;

/// Account type meant to transfer data between RC and AH.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
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
	pub holds: Vec<IdAmount<HoldReason, Balance>>,
	/// Account freezes from Relay Chain.
	///
	/// Expected freeze reasons:
	/// - NominationPools: PoolMinBalance
	pub freezes: Vec<IdAmount<FreezeReason, Balance>>,
	/// Account locks from Relay Chain.
	///
	/// Expected lock ids:
	/// - "staking " - should be transformed to hold with https://github.com/paritytech/polkadot-sdk/pull/5501
	/// - "vesting "
	/// - "pyconvot"
	pub locks: Vec<BalanceLock<Balance>>,
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

pub struct AccountsMigrator<T: Config> {
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
	/// - Some(maybe_last_key) - the last migrated account from RC to AH if
	fn migrate_many(
		last_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Error<T>> {
		// we should not send more than we allocated on AH for the migration.
		let mut ah_weight = WeightMeter::with_limit(T::MaxAhWeight::get());
		// accounts batch for the current iteration.
		let mut batch = Vec::new();

		// TODO transport weight. probably we need to leave some buffer since we do not know how
		// many send batches the one migrate_many will require.
		let xcm_weight = Weight::from_all(1);
		if weight_counter.try_consume(xcm_weight).is_err() {
			return Err(Error::OutOfWeight);
		}

		let mut iter = if let Some(ref last_key) = last_key {
			SystemAccount::<T>::iter_from_key(last_key)
		} else {
			SystemAccount::<T>::iter()
		};

		let mut maybe_last_key = last_key;
		loop {
			let Some((who, account_info)) = iter.next() else {
				maybe_last_key = None;
				break;
			};

			let withdraw_res =
				with_transaction_opaque_err::<Option<AccountFor<T>>, Error<T>, _>(|| {
					match Self::withdraw_account(
						who.clone(),
						account_info.clone(),
						weight_counter,
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
			Pallet::<T>::send_chunked_xcm(
				batch,
				|batch| types::AhMigratorCall::<T>::ReceiveAccounts { accounts: batch },
				|_| ah_weight.consumed(),
			)?;
		}

		Ok(maybe_last_key)
	}
}

impl<T: Config> AccountsMigrator<T> {
	// TODO: Currently, we use `debug_assert!` for basic test checks against a production snapshot.

	/// Migrate a single account out of the Relay chain and return it.
	///
	/// The account on the relay chain is modified as part of this operation.
	fn withdraw_account(
		who: T::AccountId,
		account_info: AccountInfoFor<T>,
		rc_weight: &mut WeightMeter,
		ah_weight: &mut WeightMeter,
		batch_len: u32,
	) -> Result<Option<AccountFor<T>>, Error<T>> {
		// account for `get_rc_state` read below
		if rc_weight.try_consume(T::DbWeight::get().reads(1)).is_err() {
			return Err(Error::OutOfWeight);
		}

		let rc_state = Self::get_rc_state(&who);

		let (rc_reserve, rc_free_min) = match rc_state {
			AccountState::Preserve => {
				log::debug!(
					target: LOG_TARGET,
					"Preserve account '{:?}' on Relay Chain",
					who.to_ss58check(),
				);
				return Ok(None);
			},
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
		};

		log::debug!(
			target: LOG_TARGET,
			"Migrating account '{}'",
			who.to_ss58check(),
		);

		// migrate the target account:
		// - keep `balance`, `holds`, `freezes`, .. in memory
		// - check if there is anything to migrate
		// - release all `holds`, `freezes`, ...
		// - teleport all balance from RC to AH:
		// -- mint into XCM `checking` account
		// -- burn from target account
		// - add `balance`, `holds`, `freezes`, .. to the accounts package to be sent via XCM

		let account_data: AccountData<T::Balance> = account_info.data.clone();

		if account_data.free.is_zero() &&
			account_data.reserved.is_zero() &&
			account_data.frozen.is_zero()
		{
			if account_info.nonce.is_zero() {
				log::warn!(
					target: LOG_TARGET,
					"Possible system account detected. \
					Consumer ref: {}, Provider ref: {}, Account: '{}'",
					account_info.consumers,
					account_info.providers,
					who.to_ss58check()
				);
			} else {
				log::warn!(target: LOG_TARGET, "Weird account detected '{}'", who.to_ss58check());
			}
			return Ok(None);
		}

		// account the weight for migrating a single account on Relay Chain.
		if rc_weight.try_consume(T::RcWeightInfo::migrate_account()).is_err() {
			return Err(Error::OutOfWeight);
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

		let holds: Vec<IdAmount<T::RuntimeHoldReason, T::Balance>> =
			pallet_balances::Holds::<T>::get(&who).into();

		for hold in &holds {
			if let Err(e) =
				<T as Config>::Currency::release(&hold.id, &who, hold.amount, Precision::Exact)
			{
				log::error!(target: LOG_TARGET,
					"Failed to release hold: {:?} \
					for account: {:?} \
					with error: {:?}",
					hold.id,
					who.to_ss58check(),
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

		// unreserve the unnamed reserve but keep some reserve on RC if needed.
		let unnamed_reserve = <T as Config>::Currency::reserved_balance(&who)
			.checked_sub(rc_reserve)
			.defensive_unwrap_or_default();
		let _ = <T as Config>::Currency::unreserve(&who, unnamed_reserve);

		// TODO: ensuring the account can be fully withdrawn from RC to AH requires force-updating
		// the references here. After inspecting the state, it's clear that fully correcting the
		// reference counts would be nearly impossible. Instead, for accounts meant to be fully
		// migrated to the AH, we will calculate the actual reference counts based on the
		// migrating pallets and transfer the counts to AH. This is done using the
		// `Self::get_consumer_count` and `Self::get_provider_count` functions.
		//
		// accounts fully migrating to AH will have a consumer count of `0` since all holds and
		// freezes are removed. Accounts keeping some reserve on RC will have a consumer count of
		// `1` as they have consumers of the reserve. The provider count is set to `1` to allow
		// reaping accounts that provided the ED at the `burn_from` below.
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

		let minted =
			match <T as Config>::Currency::mint_into(&T::CheckingAccount::get(), teleport_total) {
				Ok(minted) => minted,
				Err(e) => {
					log::error!(
						target: LOG_TARGET,
						"Failed to mint balance into checking account: {}, error: {:?}",
						who.to_ss58check(),
						e
					);
					return Err(Error::FailedToWithdrawAccount);
				},
			};

		debug_assert!(teleport_total == minted);

		let withdrawn_account = Account {
			who: who.clone(),
			free: teleport_free,
			reserved: teleport_reserved,
			frozen: account_data.frozen,
			holds,
			freezes,
			locks,
			unnamed_reserve,
			consumers: Self::get_consumer_count(&who, &account_info),
			providers: Self::get_provider_count(&who, &account_info),
		};

		// account the weight for receiving a single account on Asset Hub.
		let an_receive_weight = Self::get_ah_receive_account_weight(batch_len, &withdrawn_account);
		if ah_weight.try_consume(an_receive_weight).is_err() {
			log::debug!(
				target: LOG_TARGET,
				"Out of weight for receiving account. weight meter: {:?}, weight required: {:?}",
				ah_weight,
				an_receive_weight
			);
			return Err(Error::OutOfWeight);
		}

		Ok(Some(withdrawn_account))
	}

	/// Get the weight for importing a single account on Asset Hub.
	///
	/// The base weight is only included for the first imported account.
	pub fn get_ah_receive_account_weight(batch_len: u32, account: &AccountFor<T>) -> Weight {
		let weight_of = if account.is_liquid() {
			T::AhWeightInfo::receive_liquid_accounts
		} else {
			T::AhWeightInfo::receive_accounts
		};
		if batch_len == 0 {
			weight_of(1)
		} else {
			weight_of(1).saturating_sub(weight_of(0))
		}
	}

	/// Consumer ref count of migrating to Asset Hub pallets except a reference for `reserved` and
	/// `frozen` balance.
	///
	/// Since the `reserved` and `frozen` balances will be known on a receiving side (AH) they will
	/// be calculated there.
	pub fn get_consumer_count(_who: &T::AccountId, _info: &AccountInfoFor<T>) -> u8 {
		// TODO: check the pallets for consumer references on Relay Chain.

		// The following pallets increase consumers and are deployed on (Polkadot, Kusama, Westend):
		// - `balances`: (P/K/W)
		// - `recovery`: (/K/W)
		// - `assets`: (//)
		// - `contracts`: (//)
		// - `nfts`: (//)
		// - `uniques`: (//)
		// - `revive`: (//)
		// Staking stuff:
		// - `session`: (P/K/W)
		// - `staking`: (P/K/W)

		0
	}

	/// Provider ref count of migrating to Asset Hub pallets except the reference for existential
	/// deposit.
	///
	/// Since the `free` balance will be known on a receiving side (AH) the ref count will be
	/// calculated there.
	pub fn get_provider_count(_who: &T::AccountId, _info: &AccountInfoFor<T>) -> u8 {
		// TODO: check the pallets for provider references on Relay Chain.

		// The following pallets increase provider and are deployed on (Polkadot, Kusama, Westend):
		// - `crowdloan`: (P/K/W) https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/runtime/common/src/crowdloan/mod.rs#L416
		// - `parachains_on_demand`: (P/K/W) https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/runtime/parachains/src/on_demand/mod.rs#L407
		// - `balances`: (P/K/W) https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame/balances/src/lib.rs#L1026
		// - `broker`: (_/_/_)
		// - `delegate_staking`: (P/K/W)
		// - `session`: (P/K/W) <- Don't count this one (see https://github.com/paritytech/polkadot-sdk/blob/8d4138f77106a6af49920ad84f3283f696f3f905/substrate/frame/session/src/lib.rs#L462-L465)

		0
	}

	/// The part of the balance of the `who` that must stay on the Relay Chain.
	pub fn get_rc_state(who: &T::AccountId) -> AccountStateFor<T> {
		// TODO: static list of System Accounts that must stay on RC
		// e.g. XCM teleport checking account

		if let Some(state) = RcAccounts::<T>::get(who) {
			return state;
		}
		AccountStateFor::<T>::Migrate
	}

	/// Obtain all known accounts that must stay on RC and persist it to the [`RcAccounts`] storage
	/// item.
	///
	/// Should be executed once before the migration starts.
	pub fn obtain_rc_accounts() -> Weight {
		let mut reserves = sp_std::collections::btree_map::BTreeMap::new();
		let mut update_reserves = |id, deposit| {
			if deposit == 0 {
				return;
			}
			reserves.entry(id).and_modify(|e| *e += deposit).or_insert(deposit);
		};

		for (channel_id, info) in hrmp::HrmpChannels::<T>::iter() {
			// source: https://github.com/paritytech/polkadot-sdk/blob/3dc3a11cd68762c2e5feb0beba0b61f448c4fc92/polkadot/runtime/parachains/src/hrmp.rs#L1475
			let sender: T::AccountId = channel_id.sender.into_account_truncating();
			update_reserves(sender, info.sender_deposit);

			let recipient: T::AccountId = channel_id.recipient.into_account_truncating();
			// source: https://github.com/paritytech/polkadot-sdk/blob/3dc3a11cd68762c2e5feb0beba0b61f448c4fc92/polkadot/runtime/parachains/src/hrmp.rs#L1539
			update_reserves(recipient, info.recipient_deposit);
		}

		for (channel_id, info) in hrmp::HrmpOpenChannelRequests::<T>::iter() {
			// source: https://github.com/paritytech/polkadot-sdk/blob/3dc3a11cd68762c2e5feb0beba0b61f448c4fc92/polkadot/runtime/parachains/src/hrmp.rs#L1475
			let sender: T::AccountId = channel_id.sender.into_account_truncating();
			update_reserves(sender, info.sender_deposit);
		}

		for (_, info) in Paras::<T>::iter() {
			update_reserves(info.manager, info.deposit);
		}

		for (id, rc_reserved) in reserves {
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
					"Account has no enough reserved balance to keep on RC. account: {:?}.",
					id.to_ss58check(),
				);
				continue;
			}

			if ah_free < ah_ed && rc_reserved >= total_reserved && total_frozen.is_zero() {
				// when there is no much free balance and the account is used only for reserves
				// for parachains registering or hrmp channels we will keep the entire account on
				// the RC.
				log::debug!(
					target: LOG_TARGET,
					"Preserve account: {:?} on the RC",
					id.to_ss58check()
				);
				RcAccounts::<T>::insert(&id, AccountState::Preserve);
			} else {
				log::debug!(
					target: LOG_TARGET,
					"Keep part of account: {:?} reserve: {:?} on the RC",
					id.to_ss58check(),
					rc_reserved
				);
				RcAccounts::<T>::insert(&id, AccountState::Part { reserved: rc_reserved });
			}
		}

		// TODO: define actual weight
		Weight::from_all(1)
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
