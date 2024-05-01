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
//! This pallet provides inflation related functionality specifically for
//! (`pallet-staking``)[`crate`]. While generalized to a high extent, it is not necessarily written
//! to be reusable outside of the Polkadot relay chain scope.
//!
//! This pallet processes inflation in the following steps:

#[frame_support::pallet]
pub mod polkadot_inflation {
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

	/// Default implementations of [`DefaultConfig`], which can be used to implement [`Config`].
	// pub mod config_preludes {
	// 	use super::*;
	// 	use frame_support::derive_impl;

	// 	type AccountId = <TestDefaultConfig as frame_system::DefaultConfig>::AccountId;

	// 	pub struct TestDefaultConfig;

	// 	#[derive_impl(frame_system::config_preludes::TestDefaultConfig, no_aggregated_types)]
	// 	impl frame_system::DefaultConfig for TestDefaultConfig {}

	// 	frame_support::parameter_types! {
	// 		pub const IdealStakingRate: Perquintill = Perquintill::from_percent(75);
	// 		pub const MaxInflation: Perquintill = Perquintill::from_percent(10);
	// 		pub const MinInflation: Perquintill = Perquintill::from_percent(2);
	// 		pub const Falloff: Perquintill = Perquintill::from_percent(5);
	// 		pub const LeftoverRecipients: Vec<(AccountId, Perquintill)> = vec![];
	// 	}

	// 	use crate::inflation::polkadot_inflation::DefaultConfig;
	// 	#[frame_support::register_default_impl(TestDefaultConfig)]
	// 	impl DefaultConfig for TestDefaultConfig {
	// 		#[inject_runtime_type]
	// 		type RuntimeEvent = ();

	// 		type IdealStakingRate = IdealStakingRate;
	// 		type MaxInflation = MaxInflation;
	// 		type MinInflation = MinInflation;
	// 		type Falloff = Falloff;
	// 		type LeftoverRecipients = LeftoverRecipients;
	// 	}
	// }

	#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
	pub enum InflationPayout<AccountId> {
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

	/// A function that calculates the inflation payout.
	///
	/// Inputs are the total amount that is left from the inflation, and the proportion of the
	/// tokens that are staked from the perspective of this pallet, as [`LastKnownStakedStorage`].
	pub type InflationFn<T> = Box<
		dyn FnOnce(
			BalanceOf<T>,
			Perquintill,
		) -> (BalanceOf<T>, InflationPayout<<T as frame_system::Config>::AccountId>),
	>;

	#[pallet::config(with_default)]
	pub trait Config: frame_system::Config {
		#[pallet::no_default_bounds]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		#[pallet::no_default]
		type UnixTime: frame_support::traits::UnixTime;

		#[pallet::no_default]
		type Currency: fung::Mutate<Self::AccountId>
			+ fung::Inspect<Self::AccountId, Balance = Self::CurrencyBalance>;

		#[pallet::no_default]
		type CurrencyBalance: frame_support::traits::tokens::Balance + From<u64>;

		type MaxInflation: Get<Perquintill>;

		#[pallet::no_default]
		type Recipients: Get<Vec<InflationFn<Self>>>;

		/// Customize how this pallet reads the total issuance, if need be.
		///
		/// This is mainly here to cater for Nis in Kusama.
		///
		/// NOTE: one should not use `T::Currency::total_issuance()` directly within the pallet in
		/// case it has been overwritten here.
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
	// TODO: test if this is not set, that we are still bound to max inflation.
	#[pallet::storage]
	pub type LastInflated<T> = StorageValue<Value = u64, QueryKind = ValueQuery>;

	#[derive(Clone, Eq, PartialEq, DebugNoBound, Encode, Decode, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	#[codec(mel_bound())]
	pub struct LastKnownStake<T: Config> {
		pub(crate) stake: BalanceOf<T>,
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
		Inflated { amount: BalanceOf<T> },
		InflationDistributed { payout: InflationPayout<T::AccountId>, amount: BalanceOf<T> },
		InflationUnused { amount: BalanceOf<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		UnknownLastStake,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// `force_inflate`
		#[pallet::weight(0)]
		#[pallet::call_index(0)]
		pub fn force_inflate(origin: OriginFor<T>) -> DispatchResult {
			let _ = ensure_root(origin)?;
			Self::inflate()
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
		) -> (BalanceOf<T>, InflationPayout<T::AccountId>) {
			let ideal_stake = IdealStakingRate::get();
			let falloff = Falloff::get();

			// TODO: notion of min-inflation is now gone, will this be an issue?
			let adjustment =
				pallet_staking_reward_fn::compute_inflation(staked_ratio, ideal_stake, falloff);
			let staking_income = adjustment * max_inflation;
			(staking_income, InflationPayout::Pay(StakingPayoutAccount::get()))
		}

		pub fn fixed_ratio<T: Config, To: Get<T::AccountId>, FixedIncome: Get<Perquintill>>(
			max_inflation: BalanceOf<T>,
			_staking_ratio: Perquintill,
		) -> (BalanceOf<T>, InflationPayout<T::AccountId>) {
			let fixed_income = FixedIncome::get();
			let fixed_income = fixed_income * max_inflation;
			(fixed_income, InflationPayout::Pay(To::get()))
		}

		pub fn burn_ratio<T: Config, BurnRate: Get<Percent>>(
			max_inflation: BalanceOf<T>,
			_staking_ratio: Perquintill,
		) -> (BalanceOf<T>, InflationPayout<T::AccountId>) {
			let burn = BurnRate::get() * max_inflation;
			(burn, InflationPayout::Burn)
		}

		pub fn pay<T: Config, To: Get<T::AccountId>, Ratio: Get<Percent>>(
			max_inflation: BalanceOf<T>,
			_staking_ratio: Perquintill,
		) -> (BalanceOf<T>, InflationPayout<T::AccountId>) {
			let payout = Ratio::get() * max_inflation;
			(payout, InflationPayout::Pay(To::get()))
		}

		pub fn split_equal<T: Config, To: Get<Vec<T::AccountId>>, Ratio: Get<Percent>>(
			max_inflation: BalanceOf<T>,
			_staking_ratio: Perquintill,
		) -> (BalanceOf<T>, InflationPayout<T::AccountId>) {
			let payout = Ratio::get() * max_inflation;
			(payout, InflationPayout::SplitEqual(To::get()))
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn inflate() -> DispatchResult {
			let last_inflated = LastInflated::<T>::get();
			let now = T::UnixTime::now().as_millis().saturated_into::<u64>();
			let since_last_inflation = now.saturating_sub(last_inflated);

			let adjusted_total_issuance = T::adjusted_total_issuance();

			// what percentage of a year has passed since last inflation?
			let annual_proportion =
				Perquintill::from_rational(since_last_inflation, MILLISECONDS_PER_YEAR);
			let max_annual_inflation = T::MaxInflation::get();

			// final inflation formula.
			let total_staked = Self::last_known_stake().ok_or(Error::<T>::UnknownLastStake)?;
			let mut max_payout = annual_proportion * max_annual_inflation * adjusted_total_issuance;
			let staked_ratio = Perquintill::from_rational(total_staked, adjusted_total_issuance);

			if max_payout.is_zero() {
				Self::deposit_event(Event::Inflated { amount: Zero::zero() });
				LastInflated::<T>::put(T::UnixTime::now().as_millis().saturated_into::<u64>());
				return Ok(());
			}

			crate::log!(
				info,
				"inflating at {:?}, last inflated {:?}, max inflation {:?}, distributing among {} recipients",
				now,
				last_inflated,
				max_payout,
				T::Recipients::get().len()
			);
			Self::deposit_event(Event::Inflated { amount: max_payout });

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
					InflationPayout::Pay(who) => {
						T::Currency::mint_into(who, amount).defensive();
						max_payout -= amount;
					},
					InflationPayout::SplitEqual(whos) => {
						let amount_split = amount / (whos.len() as u32).into();
						for who in whos {
							T::Currency::mint_into(&who, amount_split).defensive();
							max_payout -= amount_split;
						}
					},
					InflationPayout::Burn => {
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
}

#[cfg(test)]
mod mock {
	use self::polkadot_inflation::LastKnownStake;
	use super::*;
	use frame::{arithmetic::*, prelude::*, testing_prelude::*, traits::fungible::Mutate};
	use polkadot_inflation::{inflation_fns, InflationFn, InflationPayout, MILLISECONDS_PER_YEAR};

	construct_runtime!(
		pub struct Runtime {
			System: frame_system,
			Balances: pallet_balances,
			Inflation: polkadot_inflation,
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

		pub Recipients: Vec<InflationFn<Runtime>> = vec![
			Box::new(inflation_fns::burn_ratio::<Runtime, BurnRatio>),
			Box::new(inflation_fns::pay::<Runtime, OneRecipient, OneRatio>),
			Box::new(inflation_fns::split_equal::<Runtime, DividedRecipients, DividedRatio>),
		];

		pub static MaxInflation: Perquintill = Perquintill::from_percent(10);
	}

	impl polkadot_inflation::Config for Runtime {
		type RuntimeEvent = RuntimeEvent;
		type Recipients = Recipients;
		type Currency = Balances;
		type CurrencyBalance = Balance;
		type MaxInflation = MaxInflation;
		type UnixTime = Timestamp;
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
			<Runtime as polkadot_inflation::Config>::update_total_stake(0, None);
			// needed to emit events.
			frame_system::Pallet::<Runtime>::set_block_number(1);
		});

		state
	}

	pub(crate) fn events() -> Vec<polkadot_inflation::Event<Runtime>> {
		System::read_events_for_pallet::<polkadot_inflation::Event<Runtime>>()
	}
}
#[cfg(test)]
mod tests {
	use super::{mock::*, polkadot_inflation::*};
	use crate::inflation::polkadot_inflation;
	use frame::{prelude::*, testing_prelude::*, traits::fungible::Inspect};
	use frame_support::hypothetically;

	mod polkadot_staking_income {}

	mod fixed_income {}

	#[test]
	fn payout_variants_work() {
		// with 10% annual inflation, we mint 100 tokens per day with the given issuance.
		new_test_ext(365 * 10 * 100).execute_with(|| {
			progress_day(1);

			// silly, just for sanity.
			let ed = Balances::total_issuance();
			assert_eq!(ed, 365 * 10 * 100);

			// no inflation so far
			assert_eq!(LastInflated::<Runtime>::get(), 0);

			// do the inflation.
			assert_ok!(Inflation::inflate());

			assert_eq!(
				events(),
				vec![
					Event::Inflated { amount: 100 },
					Event::InflationDistributed { payout: InflationPayout::Burn, amount: 20 },
					Event::InflationDistributed { payout: InflationPayout::Pay(1), amount: 40 },
					Event::InflationDistributed {
						payout: InflationPayout::SplitEqual(vec![2, 3]),
						amount: 40
					}
				]
			);

			assert_eq!(Balances::total_balance(&1), 40);
			assert_eq!(Balances::total_balance(&2), 20);
			assert_eq!(Balances::total_balance(&3), 20);
			assert_eq!(Balances::total_issuance(), ed + 80);
			assert_eq!(LastInflated::<Runtime>::get(), now());
		})
	}

	#[test]
	fn unused_inflation() {
		// unused inflation is not minted and is reported as event.
	}

	#[test]
	fn unset_last_known_total_stake() {
		new_test_ext(356 * 10 * 100).execute_with(|| {
			// some money is there to be inflated..
			progress_day(1);
			let ed = Balances::total_issuance();

			// remove last known stake.
			Inflation::kill_last_known_stake();

			assert_noop!(Inflation::inflate(), Error::<Runtime>::UnknownLastStake);
		})
	}

	#[test]
	fn expired_last_known_total_stake() {
		new_test_ext(356 * 10 * 100).execute_with(|| {
			// some money is there to be inflated..
			progress_day(1);
			let ed = Balances::total_issuance();

			// if it is claimed before block 10.
			<Runtime as polkadot_inflation::Config>::update_total_stake(0, Some(10));

			hypothetically!({
				frame_system::Pallet::<Runtime>::set_block_number(5);
				assert_ok!(Inflation::inflate());
				assert_eq!(Balances::total_issuance(), ed + 100 - 10);
			});

			// but not if claimed after block 10.
			hypothetically!({
				frame_system::Pallet::<Runtime>::set_block_number(11);
				assert_noop!(Inflation::inflate(), Error::<Runtime>::UnknownLastStake);
			});
		})
	}

	#[test]
	fn inflation_is_time_independent() {
		// over a fixed period, eg. a day, total amount inflated is the same if we inflate every
		// block or every our or just once, assuming total stake is constant.
	}

	#[test]
	fn staking_inflation_works_with_zero_ed() {
		// inflation for staking, and how the stake is distributed into sub accounts is correct for
		// both zero and non-zero ED.
	}

	#[test]
	fn payouts_are_stored_in_pots() {
		// as we progress eras but no one claims, amounts are stored in pot accounts.
	}

	#[test]
	fn unclaimed_rewards_are_burnt() {
		// upon expiry, unclaimed rewards are burnt.
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
