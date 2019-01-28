// Copyright 2017-2018 Parity Technologies (UK) Ltd.
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
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Staking manager: Periodically determines the best set of validators.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate serde;

#[macro_use]
extern crate srml_support as runtime_support;

extern crate sr_std as rstd;

#[macro_use]
extern crate parity_codec_derive;

extern crate parity_codec as codec;
extern crate sr_primitives as primitives;
extern crate srml_balances as balances;
extern crate srml_consensus as consensus;
extern crate srml_session as session;
extern crate srml_system as system;

#[cfg(test)]
extern crate substrate_primitives;
#[cfg(test)]
extern crate sr_io as runtime_io;
#[cfg(test)]
extern crate srml_timestamp as timestamp;

use rstd::{prelude::*, cmp};
use codec::HasCompact;
use runtime_support::{Parameter, StorageValue, StorageMap, dispatch::Result};
use session::OnSessionChange;
use primitives::{Perbill, traits::{Zero, One, Bounded, As, StaticLookup}};
use balances::OnDilution;
use system::ensure_signed;

mod mock;

mod tests;

const DEFAULT_MINIMUM_VALIDATOR_COUNT: u32 = 4;

#[derive(PartialEq, Clone)]
#[cfg_attr(test, derive(Debug))]
pub enum LockStatus<BlockNumber: Parameter> {
	Liquid,
	LockedUntil(BlockNumber),
	Bonded,
}

/// Preference of what happens on a slash event.
#[derive(PartialEq, Eq, Clone, Encode, Decode)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct ValidatorPrefs<Balance: HasCompact> {
	/// Validator should ensure this many more slashes than is necessary before being unstaked.
	#[codec(compact)]
	pub unstake_threshold: u32,
	// Reward that validator takes up-front; only the rest is split between themselves and nominators.
	#[codec(compact)]
	pub validator_payment: Balance,
}

impl<B: Default + HasCompact + Copy> Default for ValidatorPrefs<B> {
	fn default() -> Self {
		ValidatorPrefs {
			unstake_threshold: 3,
			validator_payment: Default::default(),
		}
	}
}

pub trait Trait: balances::Trait + session::Trait {
	/// Some tokens minted.
	type OnRewardMinted: OnDilution<<Self as balances::Trait>::Balance>;

	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn deposit_event<T>() = default;

		/// Declare the desire to stake for the transactor.
		///
		/// Effects will be felt at the beginning of the next era.
		fn stake(origin) {
			let who = ensure_signed(origin)?;
			ensure!(Self::nominating(&who).is_none(), "Cannot stake if already nominating.");
			let mut intentions = <Intentions<T>>::get();
			// can't be in the list twice.
			ensure!(intentions.iter().find(|&t| t == &who).is_none(), "Cannot stake if already staked.");

			<Bondage<T>>::insert(&who, T::BlockNumber::max_value());
			intentions.push(who);
			<Intentions<T>>::put(intentions);
		}

		/// Retract the desire to stake for the transactor.
		///
		/// Effects will be felt at the beginning of the next era.
		fn unstake(origin, #[compact] intentions_index: u32) -> Result {
			let who = ensure_signed(origin)?;
			// unstake fails in degenerate case of having too few existing staked parties
			if Self::intentions().len() <= Self::minimum_validator_count() as usize {
				return Err("cannot unstake when there are too few staked participants")
			}
			Self::apply_unstake(&who, intentions_index as usize)
		}

		fn nominate(origin, target: <T::Lookup as StaticLookup>::Source) {
			let who = ensure_signed(origin)?;
			let target = T::Lookup::lookup(target)?;

			ensure!(Self::nominating(&who).is_none(), "Cannot nominate if already nominating.");
			ensure!(Self::intentions().iter().find(|&t| t == &who).is_none(), "Cannot nominate if already staked.");

			// update nominators_for
			let mut t = Self::nominators_for(&target);
			t.push(who.clone());
			<NominatorsFor<T>>::insert(&target, t);

			// update nominating
			<Nominating<T>>::insert(&who, &target);

			// Update bondage
			<Bondage<T>>::insert(&who, T::BlockNumber::max_value());
		}

		/// Will panic if called when source isn't currently nominating target.
		/// Updates Nominating, NominatorsFor and NominationBalance.
		fn unnominate(origin, #[compact] target_index: u32) {
			let source = ensure_signed(origin)?;
			let target_index = target_index as usize;

			let target = <Nominating<T>>::get(&source).ok_or("Account must be nominating")?;

			let mut t = Self::nominators_for(&target);
			if t.get(target_index) != Some(&source) {
				return Err("Invalid target index")
			}

			// Ok - all valid.

			// update nominators_for
			t.swap_remove(target_index);
			<NominatorsFor<T>>::insert(&target, t);

			// update nominating
			<Nominating<T>>::remove(&source);

			// update bondage
			<Bondage<T>>::insert(
				source,
				<system::Module<T>>::block_number() + Self::bonding_duration()
			);
		}

		/// Set the given account's preference for slashing behaviour should they be a validator.
		///
		/// An error (no-op) if `Self::intentions()[intentions_index] != origin`.
		fn register_preferences(
			origin,
			#[compact] intentions_index: u32,
			prefs: ValidatorPrefs<T::Balance>
		) {
			let who = ensure_signed(origin)?;

			if Self::intentions().get(intentions_index as usize) != Some(&who) {
				return Err("Invalid index")
			}

			<ValidatorPreferences<T>>::insert(who, prefs);
		}

		/// Set the number of sessions in an era.
		fn set_sessions_per_era(#[compact] new: T::BlockNumber) {
			<NextSessionsPerEra<T>>::put(new);
		}

		/// The length of the bonding duration in eras.
		fn set_bonding_duration(#[compact] new: T::BlockNumber) {
			<BondingDuration<T>>::put(new);
		}

		/// The ideal number of validators.
		fn set_validator_count(#[compact] new: u32) {
			<ValidatorCount<T>>::put(new);
		}

		/// Force there to be a new era. This also forces a new session immediately after.
		/// `apply_rewards` should be true for validators to get the session reward.
		fn force_new_era(apply_rewards: bool) -> Result {
			Self::apply_force_new_era(apply_rewards)
		}

		/// Set the offline slash grace period.
		fn set_offline_slash_grace(#[compact] new: u32) {
			<OfflineSlashGrace<T>>::put(new);
		}

		/// Set the validators who cannot be slashed (if any).
		fn set_invulnerables(validators: Vec<T::AccountId>) {
			<Invulerables<T>>::put(validators);
		}
	}
}

/// An event in this module.
decl_event!(
	pub enum Event<T> where <T as balances::Trait>::Balance, <T as system::Trait>::AccountId {
		/// All validators have been rewarded by the given balance.
		Reward(Balance),
		/// One validator (and their nominators) has been given a offline-warning (they're still
		/// within their grace). The accrued number of slashes is recorded, too.
		OfflineWarning(AccountId, u32),
		/// One validator (and their nominators) has been slashed by the given amount.
		OfflineSlash(AccountId, Balance),
	}
);

pub type PairOf<T> = (T, T);

decl_storage! {
	trait Store for Module<T: Trait> as Staking {

		/// The ideal number of staking participants.
		pub ValidatorCount get(validator_count) config(): u32;
		/// Minimum number of staking participants before emergency conditions are imposed.
		pub MinimumValidatorCount get(minimum_validator_count) config(): u32 = DEFAULT_MINIMUM_VALIDATOR_COUNT;
		/// The length of a staking era in sessions.
		pub SessionsPerEra get(sessions_per_era) config(): T::BlockNumber = T::BlockNumber::sa(1000);
		/// Maximum reward, per validator, that is provided per acceptable session.
		pub SessionReward get(session_reward) config(): Perbill = Perbill::from_billionths(60);
		/// Slash, per validator that is taken for the first time they are found to be offline.
		pub OfflineSlash get(offline_slash) config(): Perbill = Perbill::from_millionths(1000); // Perbill::from_fraction() is only for std, so use from_millionths().
		/// Number of instances of offline reports before slashing begins for validators.
		pub OfflineSlashGrace get(offline_slash_grace) config(): u32;
		/// The length of the bonding duration in blocks.
		pub BondingDuration get(bonding_duration) config(): T::BlockNumber = T::BlockNumber::sa(1000);

		/// Any validators that may never be slashed or forcible kicked. It's a Vec since they're easy to initialise
		/// and the performance hit is minimal (we expect no more than four invulnerables) and restricted to testnets.
		pub Invulerables get(invulnerables) config(): Vec<T::AccountId>;

		/// The current era index.
		pub CurrentEra get(current_era) config(): T::BlockNumber;
		/// Preferences that a validator has.
		pub ValidatorPreferences get(validator_preferences): map T::AccountId => ValidatorPrefs<T::Balance>;
		/// All the accounts with a desire to stake.
		pub Intentions get(intentions) config(): Vec<T::AccountId>;
		/// All nominator -> nominee relationships.
		pub Nominating get(nominating): map T::AccountId => Option<T::AccountId>;
		/// Nominators for a particular account.
		pub NominatorsFor get(nominators_for): map T::AccountId => Vec<T::AccountId>;
		/// Nominators for a particular account that is in action right now.
		pub CurrentNominatorsFor get(current_nominators_for): map T::AccountId => Vec<T::AccountId>;

		/// Maximum reward, per validator, that is provided per acceptable session.
		pub CurrentSessionReward get(current_session_reward) config(): T::Balance;
		/// Slash, per validator that is taken for the first time they are found to be offline.
		pub CurrentOfflineSlash get(current_offline_slash) config(): T::Balance;

		/// The next value of sessions per era.
		pub NextSessionsPerEra get(next_sessions_per_era): Option<T::BlockNumber>;
		/// The session index at which the era length last changed.
		pub LastEraLengthChange get(last_era_length_change): T::BlockNumber;

		/// The highest and lowest staked validator slashable balances.
		pub StakeRange get(stake_range): PairOf<T::Balance>;

		/// The block at which the `who`'s funds become entirely liquid.
		pub Bondage get(bondage): map T::AccountId => T::BlockNumber;
		/// The number of times a given validator has been reported offline. This gets decremented by one each era that passes.
		pub SlashCount get(slash_count): map T::AccountId => u32;

		/// We are forcing a new era.
		pub ForcingNewEra get(forcing_new_era): Option<()>;
	}
}

impl<T: Trait> Module<T> {
	// Just force_new_era without origin check.
	fn apply_force_new_era(apply_rewards: bool) -> Result {
		<ForcingNewEra<T>>::put(());
		<session::Module<T>>::apply_force_new_session(apply_rewards)
	}

	// PUBLIC IMMUTABLES

	/// The length of a staking era in blocks.
	pub fn era_length() -> T::BlockNumber {
		Self::sessions_per_era() * <session::Module<T>>::length()
	}

	/// Balance of a (potential) validator that includes all nominators.
	pub fn nomination_balance(who: &T::AccountId) -> T::Balance {
		Self::nominators_for(who).iter()
			.map(<balances::Module<T>>::total_balance)
			.fold(Zero::zero(), |acc, x| acc + x)
	}

	/// The total balance that can be slashed from an account.
	pub fn slashable_balance(who: &T::AccountId) -> T::Balance {
		Self::nominators_for(who).iter()
			.map(<balances::Module<T>>::total_balance)
			.fold(<balances::Module<T>>::total_balance(who), |acc, x| acc + x)
	}

	/// The block at which the `who`'s funds become entirely liquid.
	pub fn unlock_block(who: &T::AccountId) -> LockStatus<T::BlockNumber> {
		match Self::bondage(who) {
			i if i == T::BlockNumber::max_value() => LockStatus::Bonded,
			i if i <= <system::Module<T>>::block_number() => LockStatus::Liquid,
			i => LockStatus::LockedUntil(i),
		}
	}

	/// Get the current validators.
	pub fn validators() -> Vec<T::AccountId> {
		session::Module::<T>::validators()
	}

	// PUBLIC MUTABLES (DANGEROUS)

	/// Slash a given validator by a specific amount. Removes the slash from their balance by preference,
	/// and reduces the nominators' balance if needed.
	fn slash_validator(v: &T::AccountId, slash: T::Balance) {
		// skip the slash in degenerate case of having only 4 staking participants despite having a larger
		// desired number of validators (validator_count).
		if Self::intentions().len() <= Self::minimum_validator_count() as usize {
			return
		}

		if let Some(rem) = <balances::Module<T>>::slash(v, slash) {
			let noms = Self::current_nominators_for(v);
			let total = noms.iter().map(<balances::Module<T>>::total_balance).fold(T::Balance::zero(), |acc, x| acc + x);
			if !total.is_zero() {
				let safe_mul_rational = |b| b * rem / total;// TODO: avoid overflow
				for n in noms.iter() {
					let _ = <balances::Module<T>>::slash(n, safe_mul_rational(<balances::Module<T>>::total_balance(n)));	// best effort - not much that can be done on fail.
				}
			}
		}
	}

	/// Reward a given validator by a specific amount. Add the reward to their, and their nominators'
	/// balance, pro-rata.
	fn reward_validator(who: &T::AccountId, reward: T::Balance) {
		let off_the_table = reward.min(Self::validator_preferences(who).validator_payment);
		let reward = reward - off_the_table;
		let validator_cut = if reward.is_zero() {
			Zero::zero()
		} else {
			let noms = Self::current_nominators_for(who);
			let total = noms.iter()
				.map(<balances::Module<T>>::total_balance)
				.fold(<balances::Module<T>>::total_balance(who), |acc, x| acc + x)
				.max(One::one());
			let safe_mul_rational = |b| b * reward / total;// TODO: avoid overflow
			for n in noms.iter() {
				let _ = <balances::Module<T>>::reward(n, safe_mul_rational(<balances::Module<T>>::total_balance(n)));
			}
			safe_mul_rational(<balances::Module<T>>::total_balance(who))
		};
		let _ = <balances::Module<T>>::reward(who, validator_cut + off_the_table);
	}

	/// Actually carry out the unstake operation.
	/// Assumes `intentions()[intentions_index] == who`.
	fn apply_unstake(who: &T::AccountId, intentions_index: usize) -> Result {
		let mut intentions = Self::intentions();
		if intentions.get(intentions_index) != Some(who) {
			return Err("Invalid index");
		}
		intentions.swap_remove(intentions_index);
		<Intentions<T>>::put(intentions);
		<ValidatorPreferences<T>>::remove(who);
		<SlashCount<T>>::remove(who);
		<Bondage<T>>::insert(who, <system::Module<T>>::block_number() + Self::bonding_duration());
		Ok(())
	}

	/// Get the reward for the session, assuming it ends with this block.
	fn this_session_reward(actual_elapsed: T::Moment) -> T::Balance {
		let ideal_elapsed = <session::Module<T>>::ideal_session_duration();
		if ideal_elapsed.is_zero() {
			return Self::current_session_reward();
		}
		let per65536: u64 = (T::Moment::sa(65536u64) * ideal_elapsed.clone() / actual_elapsed.max(ideal_elapsed)).as_();
		Self::current_session_reward() * T::Balance::sa(per65536) / T::Balance::sa(65536u64)
	}

	/// Session has just changed. We need to determine whether we pay a reward, slash and/or
	/// move to a new era.
	fn new_session(actual_elapsed: T::Moment, should_reward: bool) {
		if should_reward {
			// apply good session reward
			let reward = Self::this_session_reward(actual_elapsed);
			let validators = <session::Module<T>>::validators();
			for v in validators.iter() {
				Self::reward_validator(v, reward);
			}
			Self::deposit_event(RawEvent::Reward(reward));
			let total_minted = reward * <T::Balance as As<usize>>::sa(validators.len());
			let total_rewarded_stake = Self::stake_range().1 * <T::Balance as As<usize>>::sa(validators.len());
			T::OnRewardMinted::on_dilution(total_minted, total_rewarded_stake);
		}

		let session_index = <session::Module<T>>::current_index();
		if <ForcingNewEra<T>>::take().is_some()
			|| ((session_index - Self::last_era_length_change()) % Self::sessions_per_era()).is_zero()
		{
			Self::new_era();
		}
	}

	/// The era has changed - enact new staking set.
	///
	/// NOTE: This always happens immediately before a session change to ensure that new validators
	/// get a chance to set their session keys.
	fn new_era() {
		// Increment current era.
		<CurrentEra<T>>::put(&(<CurrentEra<T>>::get() + One::one()));

		// Enact era length change.
		if let Some(next_spe) = Self::next_sessions_per_era() {
			if next_spe != Self::sessions_per_era() {
				<SessionsPerEra<T>>::put(&next_spe);
				<LastEraLengthChange<T>>::put(&<session::Module<T>>::current_index());
			}
		}

		// evaluate desired staking amounts and nominations and optimise to find the best
		// combination of validators, then use session::internal::set_validators().
		// for now, this just orders would-be stakers by their balances and chooses the top-most
		// <ValidatorCount<T>>::get() of them.
		// TODO: this is not sound. this should be moved to an off-chain solution mechanism.
		let mut intentions = Self::intentions()
			.into_iter()
			.map(|v| (Self::slashable_balance(&v), v))
			.collect::<Vec<_>>();

		// Avoid reevaluate validator set if it would leave us with fewer than the minimum
		// needed validators
		if intentions.len() < Self::minimum_validator_count() as usize {
			return
		}

		intentions.sort_unstable_by(|&(ref b1, _), &(ref b2, _)| b2.cmp(&b1));

		let desired_validator_count = <ValidatorCount<T>>::get() as usize;
		let stake_range = if !intentions.is_empty() {
			let n = cmp::min(desired_validator_count, intentions.len());
			(intentions[0].0, intentions[n - 1].0)
		} else {
			(Zero::zero(), Zero::zero())
		};
		<StakeRange<T>>::put(&stake_range);

		let vals = &intentions.into_iter()
			.map(|(_, v)| v)
			.take(desired_validator_count)
			.collect::<Vec<_>>();
		for v in <session::Module<T>>::validators().iter() {
			<CurrentNominatorsFor<T>>::remove(v);
			let slash_count = <SlashCount<T>>::take(v);
			if slash_count > 1 {
				<SlashCount<T>>::insert(v, slash_count - 1);
			}
		}
		for v in vals.iter() {
			<CurrentNominatorsFor<T>>::insert(v, Self::nominators_for(v));
		}
		<session::Module<T>>::set_validators(vals);

		// Update the balances for slashing/rewarding according to the stakes.
		<CurrentOfflineSlash<T>>::put(Self::offline_slash().times(stake_range.1));
		<CurrentSessionReward<T>>::put(Self::session_reward().times(stake_range.1));
	}

	/// Call when a validator is determined to be offline. `count` is the
	/// number of offences the validator has committed.
	pub fn on_offline_validator(v: T::AccountId, count: usize) {
		use primitives::traits::{CheckedAdd, CheckedShl};

		// Early exit if validator is invulnerable.
		if Self::invulnerables().contains(&v) {
			return
		}

		let slash_count = Self::slash_count(&v);
		let new_slash_count = slash_count + count as u32;
		<SlashCount<T>>::insert(v.clone(), new_slash_count);
		let grace = Self::offline_slash_grace();

		let event = if new_slash_count > grace {
			let slash = {
				let base_slash = Self::current_offline_slash();
				let instances = slash_count - grace;

				let mut total_slash = T::Balance::default();
				for i in instances..(instances + count as u32) {
					if let Some(total) = base_slash.checked_shl(i)
							.and_then(|slash| total_slash.checked_add(&slash)) {
						total_slash = total;
					} else {
						// reset slash count only up to the current
						// instance. the total slash overflows the unit for
						// balance in the system therefore we can slash all
						// the slashable balance for the account
						<SlashCount<T>>::insert(v.clone(), slash_count + i);
						total_slash = Self::slashable_balance(&v);
						break;
					}
				}

				total_slash
			};

			let _ = Self::slash_validator(&v, slash);

			let next_slash = match slash.checked_shl(1) {
				Some(slash) => slash,
				None => Self::slashable_balance(&v),
			};

			let instances = new_slash_count - grace;
			if instances > Self::validator_preferences(&v).unstake_threshold
				|| Self::slashable_balance(&v) < next_slash
				|| next_slash <= slash
			{
				if let Some(pos) = Self::intentions().into_iter().position(|x| &x == &v) {
					Self::apply_unstake(&v, pos)
						.expect("pos derived correctly from Self::intentions(); \
								 apply_unstake can only fail if pos wrong; \
								 Self::intentions() doesn't change; qed");
				}
				let _ = Self::apply_force_new_era(false);
			}
			RawEvent::OfflineSlash(v.clone(), slash)
		} else {
			RawEvent::OfflineWarning(v.clone(), slash_count)
		};

		Self::deposit_event(event);
	}
}

impl<T: Trait> OnSessionChange<T::Moment> for Module<T> {
	fn on_session_change(elapsed: T::Moment, should_reward: bool) {
		Self::new_session(elapsed, should_reward);
	}
}

impl<T: Trait> balances::EnsureAccountLiquid<T::AccountId> for Module<T> {
	fn ensure_account_liquid(who: &T::AccountId) -> Result {
		if Self::bondage(who) <= <system::Module<T>>::block_number() {
			Ok(())
		} else {
			Err("cannot transfer illiquid funds")
		}
	}
}

impl<T: Trait> balances::OnFreeBalanceZero<T::AccountId> for Module<T> {
	fn on_free_balance_zero(who: &T::AccountId) {
		<Bondage<T>>::remove(who);
	}
}

impl<T: Trait> consensus::OnOfflineReport<Vec<u32>> for Module<T> {
	fn handle_report(reported_indices: Vec<u32>) {
		for validator_index in reported_indices {
			let v = <session::Module<T>>::validators()[validator_index as usize].clone();
			Self::on_offline_validator(v, 1);
		}
	}
}
