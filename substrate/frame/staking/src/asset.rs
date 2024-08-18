//! Facade of currency implementation. Useful while migrating from old to new currency system.

use frame_support::{
	defensive, ensure,
	traits::{Currency, Defensive, InspectLockableCurrency, LockableCurrency},
};
use sp_staking::{StakingAccount, StakingInterface};

use crate::{
	BalanceOf, Bonded, Config, Error, Ledger, NegativeImbalanceOf, Pallet, Payee,
	PositiveImbalanceOf, RewardDestination, StakingLedger, VirtualStakers, STAKING_ID,
};

/// Existential deposit for the chain.
pub fn existential_deposit<T: Config>() -> BalanceOf<T> {
	T::Currency::minimum_balance()
}

pub fn total_issuance<T: Config>() -> BalanceOf<T> {
	T::Currency::total_issuance()
}

pub fn set_balance<T: Config>(who: &T::AccountId, value: BalanceOf<T>) {
	T::Currency::make_free_balance_be(who, value);
}

pub fn burn<T: Config>(amount: BalanceOf<T>) {
	T::Currency::burn(amount);
}

pub fn free_balance<T: Config>(who: &T::AccountId) -> BalanceOf<T> {
	T::Currency::free_balance(who)
}

pub fn total_balance<T: Config>(who: &T::AccountId) -> BalanceOf<T> {
	T::Currency::total_balance(who)
}

/// Balance that is staked and at stake.
pub fn staked<T: Config>(who: &T::AccountId) -> BalanceOf<T> {
	T::Currency::balance_locked(crate::STAKING_ID, who)
}

pub fn update_stake<T: Config>(who: &T::AccountId, amount: BalanceOf<T>) {
	T::Currency::set_lock(
		crate::STAKING_ID,
		who,
		amount,
		frame_support::traits::WithdrawReasons::all(),
	);
}

pub fn kill_stake<T: Config>(who: &T::AccountId) {
	T::Currency::remove_lock(crate::STAKING_ID, who);
}

pub fn slash<T: Config>(
	who: &T::AccountId,
	value: BalanceOf<T>,
) -> (NegativeImbalanceOf<T>, BalanceOf<T>) {
	T::Currency::slash(who, value)
}

/// Mint reward.
pub fn mint_existing<T: Config>(
	who: &T::AccountId,
	value: BalanceOf<T>,
) -> Option<PositiveImbalanceOf<T>> {
	T::Currency::deposit_into_existing(who, value).ok()
}

pub fn mint_creating<T: Config>(who: &T::AccountId, value: BalanceOf<T>) -> PositiveImbalanceOf<T> {
	T::Currency::deposit_creating(who, value)
}

pub fn resolve_creating<T: Config>(who: &T::AccountId, value: NegativeImbalanceOf<T>) {
	T::Currency::resolve_creating(who, value)
}

pub fn issue<T: Config>(value: BalanceOf<T>) -> NegativeImbalanceOf<T> {
	T::Currency::issue(value)
}
