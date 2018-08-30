// Copyright 2017 Parity Technologies (UK) Ltd.
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

#[cfg(feature = "std")]
#[macro_use]
extern crate serde_derive;

#[cfg(test)]
extern crate wabt;

#[macro_use]
extern crate substrate_runtime_support as runtime_support;

#[cfg_attr(feature = "std", macro_use)]
extern crate substrate_runtime_std as rstd;

#[macro_use]
extern crate substrate_codec_derive;

extern crate substrate_codec as codec;
extern crate substrate_primitives;
extern crate substrate_runtime_io as runtime_io;
extern crate substrate_runtime_primitives as primitives;
extern crate substrate_runtime_balances as balances;
extern crate substrate_runtime_consensus as consensus;
extern crate substrate_runtime_sandbox as sandbox;
extern crate substrate_runtime_session as session;
extern crate substrate_runtime_system as system;
extern crate substrate_runtime_timestamp as timestamp;

use rstd::prelude::*;
use runtime_support::{Parameter, StorageValue, StorageMap};
use runtime_support::dispatch::Result;
use session::OnSessionChange;
use primitives::traits::{Zero, One, Bounded, RefInto, Executable,
	As, AuxLookup};
use balances::address::Address;

mod mock;

mod tests;
mod genesis_config;

#[cfg(feature = "std")]
pub use genesis_config::GenesisConfig;

const DEFAULT_MINIMUM_VALIDATOR_COUNT: usize = 4;

pub type Event<T> = RawEvent<
	<T as balances::Trait>::Balance,
	<T as system::Trait>::AccountId
>;

#[derive(PartialEq, Clone)]
#[cfg_attr(test, derive(Debug))]
pub enum LockStatus<BlockNumber: Parameter> {
	Liquid,
	LockedUntil(BlockNumber),
	Bonded,
}

/// Preference of what happens on a slash event.
#[cfg_attr(feature = "std", derive(Debug, Serialize, Deserialize))]
#[derive(Encode, Decode, Eq, PartialEq, Clone, Copy)]
pub struct SlashPreference {
	/// Validator should ensure this many more slashes than is necessary before being unstaked.
	pub unstake_threshold: u32,
}

impl Default for SlashPreference {
	fn default() -> Self {
		SlashPreference {
			unstake_threshold: 3,
		}
	}
}

pub trait Trait: balances::Trait + session::Trait {
	/// The overarching event type. 
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_module! {
	pub struct Module<T: Trait>;

	#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
	pub enum Call where aux: T::PublicAux {
		fn stake(aux) -> Result = 0;
		fn unstake(aux, intentions_index: u32) -> Result = 1;
		fn nominate(aux, target: Address<T::AccountId, T::AccountIndex>) -> Result = 2;
		fn unnominate(aux, target_index: u32) -> Result = 3;
		fn register_slash_preference(aux, intentions_index: u32, p: SlashPreference) -> Result = 4;
	}

	#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
	pub enum PrivCall {
		fn set_sessions_per_era(new: T::BlockNumber) -> Result = 0;
		fn set_bonding_duration(new: T::BlockNumber) -> Result = 1;
		fn set_validator_count(new: u32) -> Result = 2;
		fn force_new_era(apply_rewards: bool) -> Result = 3;
		fn set_offline_slash_grace(new: u32) -> Result = 4;
	}
}

/// An event in this module.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize, Debug))]
#[derive(Encode, Decode, PartialEq, Eq, Clone)]
pub enum RawEvent<Balance, AccountId> {
	/// All validators have been rewarded by the given balance.
	Reward(Balance),
	/// One validator (and their nominators) has been given a offline-warning (they're still within
	/// their grace). The accrued number of slashes is recorded, too.
	OfflineWarning(AccountId, u32),
	/// One validator (and their nominators) has been slashed by the given amount.
	OfflineSlash(AccountId, Balance),
}
impl<B, A> From<RawEvent<B, A>> for () {
	fn from(_: RawEvent<B, A>) -> () { () }
}

decl_storage! {
	trait Store for Module<T: Trait> as Staking {

		// The ideal number of staking participants.
		pub ValidatorCount get(validator_count): required u32;
		// Minimum number of staking participants before emergency conditions are imposed.
		pub MinimumValidatorCount: u32;
		// The length of a staking era in sessions.
		pub SessionsPerEra get(sessions_per_era): required T::BlockNumber;
		// Maximum reward, per validator, that is provided per acceptable session.
		pub SessionReward get(session_reward): required T::Balance;
		// Slash, per validator that is taken per abnormal era end.
		pub EarlyEraSlash get(early_era_slash): required T::Balance;
		// Number of instances of offline reports before slashing begins for validators.
		pub OfflineSlashGrace get(offline_slash_grace): default u32;
		// The length of the bonding duration in blocks.
		pub BondingDuration get(bonding_duration): required T::BlockNumber;

		// The current era index.
		pub CurrentEra get(current_era): required T::BlockNumber;
		// Preference over how many times the validator should get slashed for being offline before they are automatically unstaked.
		pub SlashPreferenceOf get(slash_preference_of): default map [ T::AccountId => SlashPreference ];
		// All the accounts with a desire to stake.
		pub Intentions get(intentions): default Vec<T::AccountId>;
		// All nominator -> nominee relationships.
		pub Nominating get(nominating): map [ T::AccountId => T::AccountId ];
		// Nominators for a particular account.
		pub NominatorsFor get(nominators_for): default map [ T::AccountId => Vec<T::AccountId> ];
		// Nominators for a particular account that is in action right now.
		pub CurrentNominatorsFor get(current_nominators_for): default map [ T::AccountId => Vec<T::AccountId> ];
		// The next value of sessions per era.
		pub NextSessionsPerEra get(next_sessions_per_era): T::BlockNumber;
		// The session index at which the era length last changed.
		pub LastEraLengthChange get(last_era_length_change): default T::BlockNumber;

		// The current era stake threshold - unused at present. Consider for removal.
		pub StakeThreshold get(stake_threshold): required T::Balance;

		// The block at which the `who`'s funds become entirely liquid.
		pub Bondage get(bondage): default map [ T::AccountId => T::BlockNumber ];
		// The number of times a given validator has been reported offline. This gets decremented by one each era that passes.
		pub SlashCount get(slash_count): default map [ T::AccountId => u32 ];

		// We are forcing a new era.
		pub ForcingNewEra get(forcing_new_era): ();
	}
}

impl<T: Trait> Module<T> {

	// PUBLIC IMMUTABLES

	pub fn minimum_validator_count() -> usize {
		<MinimumValidatorCount<T>>::get().map(|v| v as usize).unwrap_or(DEFAULT_MINIMUM_VALIDATOR_COUNT)
	}

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

	// PUBLIC DISPATCH

	/// Declare the desire to stake for the transactor.
	///
	/// Effects will be felt at the beginning of the next era.
	fn stake(aux: &T::PublicAux) -> Result {
		let aux = aux.ref_into();
		ensure!(Self::nominating(aux).is_none(), "Cannot stake if already nominating.");
		let mut intentions = <Intentions<T>>::get();
		// can't be in the list twice.
		ensure!(intentions.iter().find(|&t| t == aux).is_none(), "Cannot stake if already staked.");
		intentions.push(aux.clone());
		<Intentions<T>>::put(intentions);
		<Bondage<T>>::insert(aux, T::BlockNumber::max_value());
		Ok(())
	}

	/// Retract the desire to stake for the transactor.
	///
	/// Effects will be felt at the beginning of the next era.
	fn unstake(aux: &T::PublicAux, intentions_index: u32) -> Result {
		// unstake fails in degenerate case of having too few existing staked parties
		if Self::intentions().len() <= Self::minimum_validator_count() {
			return Err("cannot unstake when there are too few staked participants")
		}
		Self::apply_unstake(aux.ref_into(), intentions_index as usize)
	}

	fn nominate(aux: &T::PublicAux, target: Address<T::AccountId, T::AccountIndex>) -> Result {
		let target = <balances::Module<T>>::lookup(target)?;
		let aux = aux.ref_into();

		ensure!(Self::nominating(aux).is_none(), "Cannot nominate if already nominating.");
		ensure!(Self::intentions().iter().find(|&t| t == aux.ref_into()).is_none(), "Cannot nominate if already staked.");

		// update nominators_for
		let mut t = Self::nominators_for(&target);
		t.push(aux.clone());
		<NominatorsFor<T>>::insert(&target, t);

		// update nominating
		<Nominating<T>>::insert(aux, &target);

		// Update bondage
		<Bondage<T>>::insert(aux.ref_into(), T::BlockNumber::max_value());

		Ok(())
	}

	/// Will panic if called when source isn't currently nominating target.
	/// Updates Nominating, NominatorsFor and NominationBalance.
	fn unnominate(aux: &T::PublicAux, target_index: u32) -> Result {
		let source = aux.ref_into();
		let target_index = target_index as usize;

		let target = <Nominating<T>>::get(source).ok_or("Account must be nominating")?;

		let mut t = Self::nominators_for(&target);
		if t.get(target_index) != Some(source) {
			return Err("Invalid target index")
		}

		// Ok - all valid.

		// update nominators_for
		t.swap_remove(target_index);
		<NominatorsFor<T>>::insert(&target, t);

		// update nominating
		<Nominating<T>>::remove(source);

		// update bondage
		<Bondage<T>>::insert(aux.ref_into(), <system::Module<T>>::block_number() + Self::bonding_duration());
		Ok(())
	}

	/// Set the given account's preference for slashing behaviour should they be a validator. 
	/// 
	/// An error (no-op) if `Self::intentions()[intentions_index] != aux`.
	fn register_slash_preference(
		aux: &T::PublicAux,
		intentions_index: u32,
		p: SlashPreference
	) -> Result {
		let aux = aux.ref_into();

		if Self::intentions().get(intentions_index as usize) != Some(aux) {
			return Err("Invalid index")
		}
		
		<SlashPreferenceOf<T>>::insert(aux, p);

		Ok(())
	}

	// PRIV DISPATCH

	/// Deposit one of this module's events.
	fn deposit_event(event: Event<T>) {
		<system::Module<T>>::deposit_event(<T as Trait>::Event::from(event).into());
	}

	/// Set the number of sessions in an era.
	fn set_sessions_per_era(new: T::BlockNumber) -> Result {
		<NextSessionsPerEra<T>>::put(&new);
		Ok(())
	}

	/// The length of the bonding duration in eras.
	fn set_bonding_duration(new: T::BlockNumber) -> Result {
		<BondingDuration<T>>::put(&new);
		Ok(())
	}

	/// The length of a staking era in sessions.
	fn set_validator_count(new: u32) -> Result {
		<ValidatorCount<T>>::put(&new);
		Ok(())
	}

	/// Force there to be a new era. This also forces a new session immediately after.
	/// `apply_rewards` should be true for validators to get the session reward.
	fn force_new_era(apply_rewards: bool) -> Result {
		<ForcingNewEra<T>>::put(());
		<session::Module<T>>::force_new_session(apply_rewards)
	}

	/// Set the offline slash grace period.
	fn set_offline_slash_grace(new: u32) -> Result {
		<OfflineSlashGrace<T>>::put(&new);
		Ok(())
	}

	// PUBLIC MUTABLES (DANGEROUS)

	/// Slash a given validator by a specific amount. Removes the slash from their balance by preference,
	/// and reduces the nominators' balance if needed.
	fn slash_validator(v: &T::AccountId, slash: T::Balance) {
		// skip the slash in degenerate case of having only 4 staking participants despite having a larger
		// desired number of validators (validator_count).
		if Self::intentions().len() <= Self::minimum_validator_count() {
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
		let noms = Self::current_nominators_for(who);
		let total = noms.iter().map(<balances::Module<T>>::total_balance).fold(<balances::Module<T>>::total_balance(who), |acc, x| acc + x);
		if !total.is_zero() {
			let safe_mul_rational = |b| b * reward / total;// TODO: avoid overflow
			for n in noms.iter() {
				let _ = <balances::Module<T>>::reward(n, safe_mul_rational(<balances::Module<T>>::total_balance(n)));
			}
			let _ = <balances::Module<T>>::reward(who, safe_mul_rational(<balances::Module<T>>::total_balance(who)));
		}
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
		<SlashPreferenceOf<T>>::remove(who);
		<SlashCount<T>>::remove(who);
		<Bondage<T>>::insert(who, <system::Module<T>>::block_number() + Self::bonding_duration());
		Ok(())
	}

	/// Get the reward for the session, assuming it ends with this block.
	fn this_session_reward(actual_elapsed: T::Moment) -> T::Balance {
		let ideal_elapsed = <session::Module<T>>::ideal_session_duration();
		let per65536: u64 = (T::Moment::sa(65536u64) * ideal_elapsed.clone() / actual_elapsed.max(ideal_elapsed)).as_();
		Self::session_reward() * T::Balance::sa(per65536) / T::Balance::sa(65536u64)
	}

	/// Session has just changed. We need to determine whether we pay a reward, slash and/or
	/// move to a new era.
	fn new_session(actual_elapsed: T::Moment, should_reward: bool) {
		if should_reward {
			// apply good session reward
			let reward = Self::this_session_reward(actual_elapsed);
			for v in <session::Module<T>>::validators().iter() {
				Self::reward_validator(v, reward);
			}
			Self::deposit_event(RawEvent::Reward(reward));
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
		if intentions.len() < Self::minimum_validator_count() {
			return
		}

		intentions.sort_unstable_by(|&(ref b1, _), &(ref b2, _)| b2.cmp(&b1));

		<StakeThreshold<T>>::put(
			if !intentions.is_empty() {
				let i = (<ValidatorCount<T>>::get() as usize).min(intentions.len() - 1);
				intentions[i].0.clone()
			} else { Zero::zero() }
		);
		let vals = &intentions.into_iter()
			.map(|(_, v)| v)
			.take(<ValidatorCount<T>>::get() as usize)
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
	}
}

impl<T: Trait> Executable for Module<T> {
	fn execute() {
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

impl<T: Trait> consensus::OnOfflineValidator for Module<T> {
	fn on_offline_validator(validator_index: usize) {
		let v = <session::Module<T>>::validators()[validator_index].clone();
		let slash_count = Self::slash_count(&v);
		<SlashCount<T>>::insert(v.clone(), slash_count + 1);
		let grace = Self::offline_slash_grace();

		let event = if slash_count >= grace {
			let instances = slash_count - grace;
			let slash = Self::early_era_slash() << instances;
			let next_slash = slash << 1u32;
			let _ = Self::slash_validator(&v, slash);
			if instances >= Self::slash_preference_of(&v).unstake_threshold
				|| Self::slashable_balance(&v) < next_slash
			{
				if let Some(pos) = Self::intentions().into_iter().position(|x| &x == &v) {
					Self::apply_unstake(&v, pos)
						.expect("pos derived correctly from Self::intentions(); \
							apply_unstake can only fail if pos wrong; \
							Self::intentions() doesn't change; qed");
				}
				let _ = Self::force_new_era(false);
			}
			RawEvent::OfflineSlash(v, slash)
		} else {
			RawEvent::OfflineWarning(v, slash_count)
		};
		Self::deposit_event(event);
	}
}
