//! Facade of currency implementation. Useful while migrating from old to new currency system.

use frame_support::traits::{Currency, InspectLockableCurrency, LockableCurrency};

use crate::{BalanceOf, Config, NegativeImbalanceOf, PositiveImbalanceOf};

/// Existential deposit for the chain.
pub fn existential_deposit<T: Config>() -> BalanceOf<T> {
	T::Currency::minimum_balance()
}

/// Total issuance of the chain.
pub fn total_issuance<T: Config>() -> BalanceOf<T> {
	T::Currency::total_issuance()
}

/// Set balance that can be staked for `who`.
///
/// This includes any balance that is already staked.
pub fn set_stakeable_balance<T: Config>(who: &T::AccountId, value: BalanceOf<T>) {
	T::Currency::make_free_balance_be(who, value);
}

/// Burn the amount from the total issuance.
#[cfg(feature = "runtime-benchmarks")]
pub fn burn<T: Config>(amount: BalanceOf<T>) -> PositiveImbalanceOf<T> {
	T::Currency::burn(amount)
}

/// Stakeable balance of `who`.
///
/// This includes balance free to stake along with any balance that is already staked.
pub fn stakeable_balance<T: Config>(who: &T::AccountId) -> BalanceOf<T> {
	T::Currency::free_balance(who)
}

/// Total balance of an account. Includes both, free and reserved.
pub fn total_balance<T: Config>(who: &T::AccountId) -> BalanceOf<T> {
	T::Currency::total_balance(who)
}

/// Balance of `who` that is at stake.
///
/// The staked amount is locked and cannot be transferred out of `who`s account.
pub fn staked<T: Config>(who: &T::AccountId) -> BalanceOf<T> {
	T::Currency::balance_locked(crate::STAKING_ID, who)
}

/// Update `amount` at stake for `who`.
///
/// Overwrites the existing stake amount. If passed amount is lower than the existing stake, the
/// difference is unlocked.
pub fn update_stake<T: Config>(who: &T::AccountId, amount: BalanceOf<T>) {
	T::Currency::set_lock(
		crate::STAKING_ID,
		who,
		amount,
		frame_support::traits::WithdrawReasons::all(),
	);
}

/// Kill the stake of `who`.
///
/// All locked amount is unlocked.
pub fn kill_stake<T: Config>(who: &T::AccountId) {
	T::Currency::remove_lock(crate::STAKING_ID, who);
}

/// Slash the value from `who`.
///
/// A negative imbalance is returned which can be resolved to deposit the slashed value.
pub fn slash<T: Config>(
	who: &T::AccountId,
	value: BalanceOf<T>,
) -> (NegativeImbalanceOf<T>, BalanceOf<T>) {
	T::Currency::slash(who, value)
}

/// Mint reward into an existing account.
///
/// This does not increase the total issuance.
pub fn mint_existing<T: Config>(
	who: &T::AccountId,
	value: BalanceOf<T>,
) -> Option<PositiveImbalanceOf<T>> {
	T::Currency::deposit_into_existing(who, value).ok()
}

/// Mint reward and create account for `who` if it does not exist.
///
/// This does not increase the total issuance.
pub fn mint_creating<T: Config>(who: &T::AccountId, value: BalanceOf<T>) -> PositiveImbalanceOf<T> {
	T::Currency::deposit_creating(who, value)
}

/// Deposit newly issued or slashed `value` into `who`.
pub fn deposit_slashed<T: Config>(who: &T::AccountId, value: NegativeImbalanceOf<T>) {
	T::Currency::resolve_creating(who, value)
}

/// Issue `value` increasing total issuance.
///
/// Creates a negative imbalance.
pub fn issue<T: Config>(value: BalanceOf<T>) -> NegativeImbalanceOf<T> {
	T::Currency::issue(value)
}
