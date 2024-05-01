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

//! Staking inflation pallet.
//!
//! This pallet provides the means for a chain to configure its inflation logic in a simple
//! script-like manner.
//!
//! This pallet processes inflation in the following steps:

#[frame_support::pallet]
pub mod pallet_inflation {
	use frame::{
		arithmetic::*,
		prelude::*,
		traits::{
			fungible::{self as fung, Inspect, Mutate},
			AtLeast32BitUnsigned, Saturating, UnixTime,
		},
	};

	type BalanceOf<T> = <T as Config>::CurrencyBalance;

	// Milliseconds per year for the Julian year (365.25 days).
	pub const MILLISECONDS_PER_YEAR: u64 = 1000 * 60 * 60 * 24 * 365_25 / 100;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The payout action to be taken in each inflation step.
	#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
	pub enum PayoutAction<AccountId> {
		/// Pay the amount to the given account.
		Pay(AccountId),
		/// Split the equally between the given accounts.
		///
		/// This can always be implemented by a combination of [`Self::Pay`], but it is easier to
		/// express things like "split the amount between A, B, and C".
		SplitEqual(Vec<AccountId>),
		/// Burn the full amount.
		Burn,
	}

	/// A descriptor of what needs to be done with the inflation amount.
	pub trait InflationFnTrait<AccountId, Balance>: Sized + Clone {
		/// Calculates the next payout action that needs to be made as a part of the inflation
		/// recipients.
		///
		/// The inputs are:
		///
		/// * `max_inflation`: the total amount that is available at this step to be inflated.
		/// * `staked_ratio`: the proportion of the tokens that are staked from the perspective of
		///   this pallet.
		///
		/// Return types are:
		///
		/// * `balance`: a subset of the input balance that should be paid out.
		/// * [`InflationPayout`]: an action to be made.
		fn next_payout(
			max_inflation: Balance,
			staked_ratio: Perquintill,
		) -> (Balance, PayoutAction<AccountId>);
	}

	pub type InflationFnOf<T> =
		Box<dyn InflationFnTrait<<T as frame_system::Config>::AccountId, BalanceOf<T>>>;

	/// A function that calculates the next payout that needs to be made as a part of the inflation
	/// recipients.
	///
	/// The inputs are:
	///
	/// * `balance`: the total amount that is available at this step to be inflated.
	/// * `perquintill`: the proportion of the tokens that are staked from the perspective of this
	///   pallet.
	///
	/// Return types are:
	///
	/// * `balance`: a subset of the input balance that should be paid out.
	/// * [`InflationPayout`]: an action to be made.
	pub type InflationFn<T> = Box<
		dyn Fn(
			BalanceOf<T>,
			Perquintill,
		) -> (BalanceOf<T>, PayoutAction<<T as frame_system::Config>::AccountId>),
	>;

	#[pallet::config(with_default)]
	pub trait Config: frame_system::Config {
		/// Runtime event type.
		#[pallet::no_default_bounds]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Something that provides a notion of the unix-time.
		#[pallet::no_default]
		type UnixTime: frame_support::traits::UnixTime;

		/// The currency type of the runtime.
		#[pallet::no_default]
		type Currency: fung::Mutate<Self::AccountId>
			+ fung::Inspect<Self::AccountId, Balance = Self::CurrencyBalance>;

		/// Same as the balance type of [`Config::Currency`], only provided to further bound it to
		/// `From<u64>`.
		#[pallet::no_default]
		type CurrencyBalance: frame_support::traits::tokens::Balance + From<u64>;

		/// Maximum fixed amount by which we inflate, before passing it down to [`Recipients`]
		type MaxInflation: Get<Perquintill>;

		/// The recipients of the inflation, as a sequence of items described by [`InflationFn`]
		#[pallet::no_default]
		type Recipients: Get<Vec<InflationFn<Self>>>;

		/// An origin that can trigger an inflation at any point in time via
		/// [`Call::force_inflate`].
		type InflationOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Customize how this pallet reads the total issuance, if need be.
		///
		/// NOTE: This is mainly here to cater for Nis in Kusama.
		#[pallet::no_default]
		fn adjusted_total_issuance() -> BalanceOf<Self> {
			Self::Currency::total_issuance()
		}

		/// A simple and possibly short terms means for updating the total stake, esp. so long as
		/// this pallet is in the same runtime as with `pallet-staking`.
		///
		/// Once multi-chain, we should expect an extrinsic, gated by the origin of the staking
		/// parachain that can update this value. This can be `Transact`-ed via XCM.
		#[pallet::no_default] // TODO @gupnik this should be taken care of better? the fn already has a default.
		fn update_total_stake(stake: BalanceOf<Self>, valid_until: Option<BlockNumberFor<Self>>) {
			LastKnownStakedStorage::<Self>::put(LastKnownStake { stake, valid_until });
		}
	}

	// TODO: needs a migration that sets the initial value.
	#[pallet::storage]
	pub type LastInflated<T> = StorageValue<Value = u64, QueryKind = OptionQuery>;

	/// A record of the last amount of tokens staked from the perspective of this pallet.
	#[derive(Clone, Eq, PartialEq, DebugNoBound, Encode, Decode, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	#[codec(mel_bound())]
	pub struct LastKnownStake<T: Config> {
		/// The staked amount.
		pub(crate) stake: BalanceOf<T>,
		/// Until which future block number is this amount valid for?
		pub(crate) valid_until: Option<BlockNumberFor<T>>,
	}

	// SHOULD ONLY BE READ BY [`Pallet::last_known_stake`]
	#[pallet::storage]
	type LastKnownStakedStorage<T: Config> =
		StorageValue<Value = LastKnownStake<T>, QueryKind = OptionQuery>;

	impl<T: Config> Pallet<T> {
		fn last_known_stake() -> Option<BalanceOf<T>> {
			LastKnownStakedStorage::<T>::get().and_then(|LastKnownStake { stake, valid_until }| {
				if valid_until.map_or(false, |valid_until| {
					valid_until < frame_system::Pallet::<T>::block_number()
				}) {
					None
				} else {
					Some(stake)
				}
			})
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A total of `amount` is proposed for inflation. All of this may or may not be used.
		PossiblyInflated { amount: BalanceOf<T> },
		/// `amount` has been processed with the given `payout` action.
		InflationDistributed { amount: BalanceOf<T>, payout: PayoutAction<T::AccountId> },
		/// `amount` has not be used and is therefore not minted.
		InflationUnused { amount: BalanceOf<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The last stake amount is not set.
		UnknownLastStake,
		/// The last inflation amount is not set.
		UnknownLastInflated,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Force inflation to happen.
		///
		/// The origin of this call must be [`Config::InflationOrigin`]
		#[pallet::weight(0)]
		#[pallet::call_index(0)]
		pub fn force_inflate(origin: OriginFor<T>) -> DispatchResult {
			let _ = T::InflationOrigin::ensure_origin(origin)?;
			Self::inflate()
		}
	}

	impl<T: Config> Pallet<T> {
		/// Perform inflation.
		///
		/// This is the main entry point function of this pallet, and can be called form
		/// [`Call::force_inflate`] or other places in the runtime.
		pub fn inflate() -> DispatchResult {
			let last_inflated = LastInflated::<T>::get().ok_or(Error::<T>::UnknownLastInflated)?;
			let now = T::UnixTime::now().as_millis().saturated_into::<u64>();
			let since_last_inflation = now.saturating_sub(last_inflated);
			let adjusted_total_issuance = T::adjusted_total_issuance();

			// what percentage of a year has passed since last inflation?
			let annual_proportion =
				Perquintill::from_rational(since_last_inflation, MILLISECONDS_PER_YEAR);

			// staking rate.
			let total_staked = Self::last_known_stake().ok_or(Error::<T>::UnknownLastStake)?;
			let staked_ratio = Perquintill::from_rational(total_staked, adjusted_total_issuance);

			let max_annual_inflation = T::MaxInflation::get();
			let mut max_payout = annual_proportion * max_annual_inflation * adjusted_total_issuance;

			if max_payout.is_zero() {
				Self::deposit_event(Event::PossiblyInflated { amount: Zero::zero() });
				LastInflated::<T>::put(T::UnixTime::now().as_millis().saturated_into::<u64>());
				return Ok(());
			}

			crate::log!(
				info,
				"inflating at {:?}, annual proportion {:?}, issuance {:?}, last inflated {:?}, max inflation {:?}, distributing among {} recipients",
				now,
				annual_proportion,
				adjusted_total_issuance,
				last_inflated,
				max_payout,
				T::Recipients::get().len()
			);
			Self::deposit_event(Event::PossiblyInflated { amount: max_payout });

			for payout_fn in T::Recipients::get() {
				let (amount, payout) = payout_fn(max_payout, staked_ratio);
				debug_assert!(amount <= max_payout, "payout exceeds max");
				let amount = amount.min(max_payout);

				crate::log!(
					info,
					"amount {:?} out of {:?} being paid out to to {:?}",
					amount,
					max_payout,
					payout,
				);
				match &payout {
					PayoutAction::Pay(who) => {
						T::Currency::mint_into(who, amount).defensive();
						max_payout -= amount;
					},
					PayoutAction::SplitEqual(whos) => {
						let amount_split = amount / (whos.len() as u32).into();
						for who in whos {
							T::Currency::mint_into(&who, amount_split).defensive();
							max_payout -= amount_split;
						}
					},
					PayoutAction::Burn => {
						// no burn needed, we haven't even minted anything.
						max_payout -= amount;
					},
				}
				Self::deposit_event(Event::InflationDistributed { payout, amount });
			}

			if !max_payout.is_zero() {
				Self::deposit_event(Event::InflationUnused { amount: max_payout });
			}

			LastInflated::<T>::put(T::UnixTime::now().as_millis().saturated_into::<u64>());

			Ok(())
		}

		#[cfg(test)]
		pub(crate) fn kill_last_known_stake() {
			LastKnownStakedStorage::<T>::kill();
		}

		#[cfg(test)]
		pub(crate) fn last_known_stake_storage() -> Option<LastKnownStake<T>> {
			LastKnownStakedStorage::<T>::get()
		}
	}

	/// A set of inflation functions provided by this pallet.
	pub mod inflation_fns {
		use super::*;
		pub fn polkadot_staking_income<
			T: Config,
			IdealStakingRate: Get<Perquintill>,
			Falloff: Get<Perquintill>,
			StakingPayoutAccount: Get<T::AccountId>,
		>(
			max_inflation: BalanceOf<T>,
			staked_ratio: Perquintill,
		) -> (BalanceOf<T>, PayoutAction<T::AccountId>) {
			let ideal_stake = IdealStakingRate::get();
			let falloff = Falloff::get();

			// TODO: notion of min-inflation is now gone, will this be an issue?
			let adjustment =
				pallet_staking_reward_fn::compute_inflation(staked_ratio, ideal_stake, falloff);
			let staking_income = adjustment * max_inflation;
			(staking_income, PayoutAction::Pay(StakingPayoutAccount::get()))
		}

		pub fn burn_ratio<T: Config, BurnRate: Get<Percent>>(
			max_inflation: BalanceOf<T>,
			_staking_ratio: Perquintill,
		) -> (BalanceOf<T>, PayoutAction<T::AccountId>) {
			let burn = BurnRate::get() * max_inflation;
			(burn, PayoutAction::Burn)
		}

		pub fn pay<T: Config, To: Get<T::AccountId>, Ratio: Get<Percent>>(
			max_inflation: BalanceOf<T>,
			_staking_ratio: Perquintill,
		) -> (BalanceOf<T>, PayoutAction<T::AccountId>) {
			let payout = Ratio::get() * max_inflation;
			(payout, PayoutAction::Pay(To::get()))
		}

		pub fn split_equal<T: Config, To: Get<Vec<T::AccountId>>, Ratio: Get<Percent>>(
			max_inflation: BalanceOf<T>,
			_staking_ratio: Perquintill,
		) -> (BalanceOf<T>, PayoutAction<T::AccountId>) {
			let payout = Ratio::get() * max_inflation;
			(payout, PayoutAction::SplitEqual(To::get()))
		}
	}
}

#[cfg(test)]
mod mock {
	use self::pallet_inflation::{LastInflated, LastKnownStake};
	use super::*;
	use core::{borrow::BorrowMut, cell::RefCell};
	use frame::{arithmetic::*, prelude::*, testing_prelude::*, traits::fungible::Mutate};
	use pallet_inflation::{inflation_fns, InflationFn, PayoutAction, MILLISECONDS_PER_YEAR};
	use std::sync::Arc;

	construct_runtime!(
		pub struct Runtime {
			System: frame_system,
			Balances: pallet_balances,
			Inflation: pallet_inflation,
			Timestamp: pallet_timestamp,
		}
	);

	pub type AccountId = <Runtime as frame_system::Config>::AccountId;
	pub type Balance = <Runtime as pallet_balances::Config>::Balance;
	pub type Moment = <Runtime as pallet_timestamp::Config>::Moment;

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = frame::testing_prelude::MockBlock<Runtime>;
		type AccountData = pallet_balances::AccountData<Balance>;
	}

	#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
	impl pallet_balances::Config for Runtime {
		type AccountStore = System;
	}

	#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
	impl pallet_timestamp::Config for Runtime {}

	parameter_types! {
		pub static BurnRatio: Percent = Percent::from_percent(20);
		pub static OneRecipient: AccountId = 1;
		pub static OneRatio: Percent = Percent::from_percent(50);
		pub static DividedRecipients: Vec<AccountId> = vec![2, 3];
		pub static DividedRatio: Percent = Percent::from_percent(100);

		pub static MaxInflation: Perquintill = Perquintill::from_percent(10);
	}

	thread_local! {
		static RECIPIENTS: RefCell<Vec<InflationFn<Runtime>>> = RefCell::new(vec![
			Box::new(inflation_fns::burn_ratio::<Runtime, BurnRatio>),
			Box::new(inflation_fns::pay::<Runtime, OneRecipient, OneRatio>),
			Box::new(inflation_fns::split_equal::<Runtime, DividedRecipients, DividedRatio>),
		]);
	}

	pub struct Recipients;
	impl Get<Vec<InflationFn<Runtime>>> for Recipients {
		fn get() -> Vec<InflationFn<Runtime>> {
			RECIPIENTS.with(|v| {
				let v_borrowed = v.borrow();
				let mut cloned = Vec::with_capacity(v_borrowed.len());
				for fn_box in &*v_borrowed {
					let fn_clone: InflationFn<Runtime> = unsafe { core::ptr::read(fn_box) };
					cloned.push(fn_clone);
				}
				cloned
			})
		}
	}

	impl Recipients {
		pub(crate) fn add(new_fn: InflationFn<Runtime>) {
			RECIPIENTS.with(|v| v.borrow_mut().push(new_fn));
		}
		pub(crate) fn clear() {
			RECIPIENTS.with(|v| v.borrow_mut().clear());
		}
		pub(crate) fn pop() {
			RECIPIENTS.with(|v| v.borrow_mut().pop());
		}
	}

	impl pallet_inflation::Config for Runtime {
		type RuntimeEvent = RuntimeEvent;
		type Recipients = Recipients;
		type Currency = Balances;
		type CurrencyBalance = Balance;
		type MaxInflation = MaxInflation;
		type UnixTime = Timestamp;
		type InflationOrigin = EnsureRoot<AccountId>;
	}

	pub(crate) fn now() -> Moment {
		pallet_timestamp::Now::<Runtime>::get()
	}

	fn progress_timestamp(by: Moment) {
		Timestamp::set_timestamp(now() + by);
	}

	pub(crate) fn progress_day(days: Moment) {
		let progress = MILLISECONDS_PER_YEAR / 365 * days;
		progress_timestamp(progress)
	}

	pub(crate) fn new_test_ext(issuance: Balance) -> TestState {
		sp_tracing::try_init_simple();
		let mut state = TestState::new_empty();
		state.execute_with(|| {
			Balances::mint_into(&42, issuance).unwrap();
			<Runtime as pallet_inflation::Config>::update_total_stake(0, None);
			// needed to emit events.
			frame_system::Pallet::<Runtime>::set_block_number(1);
			LastInflated::<Runtime>::put(0);
		});

		state
	}

	pub(crate) fn events() -> Vec<pallet_inflation::Event<Runtime>> {
		System::read_events_for_pallet::<pallet_inflation::Event<Runtime>>()
	}
}
#[cfg(test)]
mod tests {
	use super::{mock::*, pallet_inflation::*};
	use crate::inflation::pallet_inflation;
	use frame::{prelude::*, testing_prelude::*, traits::fungible::Inspect};
	use frame_support::hypothetically;

	const DEFAULT_INITIAL_TI: Balance = 365 * 10 * 100;
	const DEFAULT_DAILY_INFLATION: Balance = 80;

	mod polkadot_staking_income {}

	#[test]
	fn payout_variants_work() {
		// with 10% annual inflation, we mint 100 tokens per day with the given issuance.
		new_test_ext(DEFAULT_INITIAL_TI).execute_with(|| {
			progress_day(1);

			// silly, just for sanity.
			let ed = Balances::total_issuance();
			assert_eq!(ed, DEFAULT_INITIAL_TI);

			// no inflation so far
			assert_eq!(LastInflated::<Runtime>::get().unwrap(), 0);

			// do the inflation.
			assert_ok!(Inflation::inflate());

			assert_eq!(
				events(),
				vec![
					Event::PossiblyInflated { amount: 100 },
					Event::InflationDistributed { payout: PayoutAction::Burn, amount: 20 },
					Event::InflationDistributed { payout: PayoutAction::Pay(1), amount: 40 },
					Event::InflationDistributed {
						payout: PayoutAction::SplitEqual(vec![2, 3]),
						amount: 40
					}
				]
			);

			assert_eq!(Balances::total_balance(&1), 40);
			assert_eq!(Balances::total_balance(&2), 20);
			assert_eq!(Balances::total_balance(&3), 20);
			assert_eq!(Balances::total_issuance(), ed + DEFAULT_DAILY_INFLATION);
			assert_eq!(LastInflated::<Runtime>::get().unwrap(), now());
		})
	}

	#[test]
	fn unused_inflation() {
		new_test_ext(DEFAULT_INITIAL_TI).execute_with(|| {
			progress_day(1);
			let ed = Balances::total_issuance();
			// now the last 40 is un-used.
			Recipients::pop();

			// do the inflation.
			assert_ok!(Inflation::inflate());

			assert_eq!(
				events(),
				vec![
					Event::PossiblyInflated { amount: 100 },
					Event::InflationDistributed { payout: PayoutAction::Burn, amount: 20 },
					Event::InflationDistributed { payout: PayoutAction::Pay(1), amount: 40 },
					Event::InflationUnused { amount: 40 }
				]
			);

			assert_eq!(Balances::total_balance(&1), 40);
			assert_eq!(Balances::total_balance(&2), 0);
			assert_eq!(Balances::total_balance(&3), 0);
			assert_eq!(Balances::total_issuance(), ed + 40);
		})
	}

	#[test]
	fn unused_inflation_2() {
		new_test_ext(DEFAULT_INITIAL_TI).execute_with(|| {
			progress_day(1);
			let ed = Balances::total_issuance();
			// no inflation handler, all is not minted.
			Recipients::clear();

			// do the inflation.
			assert_ok!(Inflation::inflate());

			assert_eq!(
				events(),
				vec![
					Event::PossiblyInflated { amount: 100 },
					Event::InflationUnused { amount: 100 }
				]
			);

			assert_eq!(Balances::total_balance(&1), 0);
			assert_eq!(Balances::total_balance(&2), 0);
			assert_eq!(Balances::total_balance(&3), 0);
			assert_eq!(Balances::total_issuance(), ed);
		})
	}

	#[test]
	fn unused_inflation_3() {
		new_test_ext(DEFAULT_INITIAL_TI).execute_with(|| {
			progress_day(1);
			let ed = Balances::total_issuance();
			// one inflation handler that burns it all. Equal to `unused_inflation_2`.
			Recipients::clear();
			Recipients::add(Box::new(|x, _| (x, PayoutAction::Burn)));

			// do the inflation.
			assert_ok!(Inflation::inflate());

			assert_eq!(
				events(),
				vec![
					Event::PossiblyInflated { amount: 100 },
					Event::InflationDistributed { amount: 100, payout: PayoutAction::Burn },
				]
			);

			assert_eq!(Balances::total_balance(&1), 0);
			assert_eq!(Balances::total_balance(&2), 0);
			assert_eq!(Balances::total_balance(&3), 0);
			assert_eq!(Balances::total_issuance(), ed);
		})
	}

	#[test]
	fn unset_last_known_total_stake() {
		new_test_ext(DEFAULT_INITIAL_TI).execute_with(|| {
			// some money is there to be inflated..
			progress_day(1);

			// remove last known stake.
			Inflation::kill_last_known_stake();

			assert_noop!(Inflation::inflate(), Error::<Runtime>::UnknownLastStake);
		})
	}

	#[test]
	fn expired_last_known_total_stake() {
		new_test_ext(DEFAULT_INITIAL_TI).execute_with(|| {
			// some money is there to be inflated..
			progress_day(1);
			let ed = Balances::total_issuance();

			// and as of now it can only be inflated until block 10
			<Runtime as pallet_inflation::Config>::update_total_stake(0, Some(10));

			// if it is claimed before block 10.
			hypothetically!({
				frame_system::Pallet::<Runtime>::set_block_number(5);
				assert_ok!(Inflation::inflate());
				assert_eq!(Balances::total_issuance(), ed + DEFAULT_DAILY_INFLATION);
			});

			// but not if claimed after block 10.
			hypothetically!({
				frame_system::Pallet::<Runtime>::set_block_number(11);
				assert_noop!(Inflation::inflate(), Error::<Runtime>::UnknownLastStake);
			});
		})
	}

	#[test]
	fn unknown_last_inflated() {
		new_test_ext(DEFAULT_INITIAL_TI).execute_with(|| {
			// some money is there to be inflated..
			progress_day(1);
			let ed = Balances::total_issuance();
			LastInflated::<Runtime>::kill();

			assert_noop!(Inflation::inflate(), Error::<Runtime>::UnknownLastInflated);
		})
	}

	#[test]
	fn inflation_is_time_independent() {
		new_test_ext(DEFAULT_INITIAL_TI).execute_with(|| {
			let ed = Balances::total_issuance();

			hypothetically!({
				for _ in 0..10 {
					progress_day(1);
					assert_ok!(Inflation::inflate());
				}
				assert_eq!(Balances::total_issuance(), 365800);
			});

			hypothetically!({
				for _ in 0..10 {
					progress_day(1);
				}
				assert_ok!(Inflation::inflate());
				assert_eq!(Balances::total_issuance(), 365800);
			});
			// what matters is that the final total issuance is the same in both cases.
		})
	}

	mod ideas {
		use super::*;

		fn capped_inflation
	}
}

mod deprecated {
	use super::*;
	use sp_runtime::{curve::PiecewiseLinear, Perbill};

	/// The total payout to all validators (and their nominators) per era and maximum payout.
	///
	/// Defined as such:
	/// `staker-payout = yearly_inflation(npos_token_staked / total_tokens) * total_tokens /
	/// era_per_year` `maximum-payout = max_yearly_inflation * total_tokens / era_per_year`
	///
	/// `era_duration` is expressed in millisecond.
	#[deprecated]
	pub fn compute_total_payout<N>(
		yearly_inflation: &PiecewiseLinear<'static>,
		npos_token_staked: N,
		total_tokens: N,
		era_duration: u64,
	) -> (N, N)
	where
		N: sp_runtime::traits::AtLeast32BitUnsigned + Clone,
	{
		// Milliseconds per year for the Julian year (365.25 days).
		const MILLISECONDS_PER_YEAR: u64 = 1000 * 3600 * 24 * 36525 / 100;

		let portion = Perbill::from_rational(era_duration as u64, MILLISECONDS_PER_YEAR);
		let payout = portion *
			yearly_inflation.calculate_for_fraction_times_denominator(
				npos_token_staked,
				total_tokens.clone(),
			);
		let maximum = portion * (yearly_inflation.maximum * total_tokens);
		(payout, maximum)
	}

	#[cfg(test)]
	mod test {
		use sp_runtime::curve::PiecewiseLinear;

		pallet_staking_reward_curve::build! {
			const I_NPOS: PiecewiseLinear<'static> = curve!(
				min_inflation: 0_025_000,
				max_inflation: 0_100_000,
				ideal_stake: 0_500_000,
				falloff: 0_050_000,
				max_piece_count: 40,
				test_precision: 0_005_000,
			);
		}

		#[test]
		fn npos_curve_is_sensible() {
			const YEAR: u64 = 365 * 24 * 60 * 60 * 1000;

			// check maximum inflation.
			// not 10_000 due to rounding error.
			assert_eq!(super::compute_total_payout(&I_NPOS, 0, 100_000u64, YEAR).1, 9_993);

			// super::I_NPOS.calculate_for_fraction_times_denominator(25, 100)
			assert_eq!(super::compute_total_payout(&I_NPOS, 0, 100_000u64, YEAR).0, 2_498);
			assert_eq!(super::compute_total_payout(&I_NPOS, 5_000, 100_000u64, YEAR).0, 3_248);
			assert_eq!(super::compute_total_payout(&I_NPOS, 25_000, 100_000u64, YEAR).0, 6_246);
			assert_eq!(super::compute_total_payout(&I_NPOS, 40_000, 100_000u64, YEAR).0, 8_494);
			assert_eq!(super::compute_total_payout(&I_NPOS, 50_000, 100_000u64, YEAR).0, 9_993);
			assert_eq!(super::compute_total_payout(&I_NPOS, 60_000, 100_000u64, YEAR).0, 4_379);
			assert_eq!(super::compute_total_payout(&I_NPOS, 75_000, 100_000u64, YEAR).0, 2_733);
			assert_eq!(super::compute_total_payout(&I_NPOS, 95_000, 100_000u64, YEAR).0, 2_513);
			assert_eq!(super::compute_total_payout(&I_NPOS, 100_000, 100_000u64, YEAR).0, 2_505);

			const DAY: u64 = 24 * 60 * 60 * 1000;
			assert_eq!(super::compute_total_payout(&I_NPOS, 25_000, 100_000u64, DAY).0, 17);
			assert_eq!(super::compute_total_payout(&I_NPOS, 50_000, 100_000u64, DAY).0, 27);
			assert_eq!(super::compute_total_payout(&I_NPOS, 75_000, 100_000u64, DAY).0, 7);

			const SIX_HOURS: u64 = 6 * 60 * 60 * 1000;
			assert_eq!(super::compute_total_payout(&I_NPOS, 25_000, 100_000u64, SIX_HOURS).0, 4);
			assert_eq!(super::compute_total_payout(&I_NPOS, 50_000, 100_000u64, SIX_HOURS).0, 7);
			assert_eq!(super::compute_total_payout(&I_NPOS, 75_000, 100_000u64, SIX_HOURS).0, 2);

			const HOUR: u64 = 60 * 60 * 1000;
			assert_eq!(
				super::compute_total_payout(
					&I_NPOS,
					2_500_000_000_000_000_000_000_000_000u128,
					5_000_000_000_000_000_000_000_000_000u128,
					HOUR
				)
				.0,
				57_038_500_000_000_000_000_000
			);
		}
	}
}

pub use deprecated::compute_total_payout;
