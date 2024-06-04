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

//! > Made with *Polkadot-Sdk*
//!
//! [![github]](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/inflation) -
//! [![polkadot]](https://polkadot.network)
//!
//! [polkadot]:
//!     https://img.shields.io/badge/polkadot-E6007A?style=for-the-badge&logo=polkadot&logoColor=white
//! [github]:
//!     https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//!
//! # Inflation Pallet
//!
//! A pallet designed to handle varying types of inflation systems.
//!
//! While designed to be used in the Polkadot relay chain, it can be used in other contexts as well.
//!
//! ## Overview
//!
//! The pallet performs inflation in two steps, abstracted by [`Config::InflationSource`] and
//! [`Config::Distribution`] respectively.
//!
//! * At arbitrary points in time, this pallet is instructed to inflate. At the moment, this is
//!   exposed as a standalone pallet function [`Pallet::inflate`] and a dispatchable with custom
//!   origin, [`Call::force_inflate`].
//! * To determine the amount that the pallet should consider for inflating,
//!   [`Config::InflationSource`] is used. Two example implementations are [`FixedAnnualInflation`]
//!   and [`FixedRatioAnnualInflation`]. The latter is what Polkadot relay chain uses today. This
//!   function yields an amount, for example `i`. At this stage, `i` is determined, but is not fully
//!   minted yet, as it is not clear "_where_" it should be minted. This is determined in the next
//!   step.
//! * Then, the pallet allows the runtime parameterize what should happen with `i` through a
//!   sequence of "steps". Each step takes an amount out of the aforementioned `i`, and possibly
//!   mints it into an account through [`Action`]. The set of actions provided at the moment are
//!   fairly simple. The `steps` has access to the maximum amount that it can consume.
//!
//! ### Example
//!
//! See the following self-explanatory example, used in the tests of this pallet, which implements a
//! fixed 10% rate inflation system, which is partially distributed and is partially burnt.
#![doc = docify::embed!("./src/lib.rs", configs)]
//!
//! These configurations are then fed into the pallet as such. This using the pre-existing set of
//! actions from [`inflation_actions`].
#![doc = docify::embed!("./src/lib.rs", fns)]
//!
//! A fixed rate inflation system would look the same, except it would use [`FixedAnnualInflation`].
//!
//! The current polkadot inflation system is implemented as a part of
//! [`inflation_actions::polkadot_staking_income`]
//!
//! ## Implementation Notes
//!
//! Given that most inflation schemes work on an annual basis, this pallet always keeps track of
//! when was the last time that it has inflated, and provides this timestamp, next to the current
//! timestamp, to the inflation API.
//!
//! In an somewhat of an opinionated design, but with a similar intent, given that some inflation
//! systems inflate as a function of the staking rate, this pallet also keeps track of a single
//! value as the amount of tokens at stake, and provides this value to some APIs as well. This
//! amount can be updated by any other system in the runtime via [`Config::update_total_stake`].
//!
//! The latter is a sub-optimal design, but allows us to easily use this pallet in Polkadot. A user
//! that wishes to not use this value can entirely ignore it, and the only extra cost is one storage
//! lookup per inflation. This can easily be improved by moving this value to the runtime, but for
//! now it is kept here.
//!
//! Finally, this pallet also provides a means to use an alternative function as the definition of
//! total issuance, in case part of the issuance need not be counted towards inflation. Similarly,
//! this is intended to expand the usage of this pallet to Polkadot/Kusama, and is of zero extra
//! cost to users who wish to simply use [`Config::Currency`].

#![cfg_attr(not(feature = "std"), no_std)]

/// Re-export all of the pallet stuff.
pub use pallet::*;

pub(crate) const LOG_TARGET: &str = "runtime::inflation";

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		frame::log::$level!(
			target: crate::LOG_TARGET,
			concat!("[{:?}] 💶 ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

#[frame::pallet]
pub mod pallet {
	use frame::{
		arithmetic::*,
		deps::sp_std::marker::PhantomData,
		prelude::*,
		traits::{
			fungible::{self as fung, Inspect, Mutate},
			UnixTime,
		},
	};

	type BalanceOf<T> = <T as Config>::CurrencyBalance;

	// Milliseconds per year for the Julian year (365.25 days).
	pub const MILLISECONDS_PER_YEAR: u64 = 1000 * 60 * 60 * 24 * 365_25 / 100;

	/// A descriptor of how much we should inflate.
	///
	/// This pallet always keeps track of the last instance of time that at which point inflation
	/// happened, and it provides it to this trait as input.
	// TODO: not happy with the name.
	pub trait InflationSource<Balance> {
		/// Pay
		fn inflation_source(current_issuance: Balance, last_inflated: u64, now: u64) -> Balance;
	}

	/// Fixed percentage of the issuance inflation per yer, as specified by `Ratio`.
	pub struct FixedRatioAnnualInflation<T, Ratio>(PhantomData<(T, Ratio)>);
	impl<T: Config, Ratio: Get<Perquintill>> InflationSource<BalanceOf<T>>
		for FixedRatioAnnualInflation<T, Ratio>
	{
		fn inflation_source(
			current_issuance: BalanceOf<T>,
			last_inflated: u64,
			now: u64,
		) -> T::CurrencyBalance {
			let since_last_inflation = now.saturating_sub(last_inflated);

			// what percentage of a year has passed since last inflation?
			// TODO: if this function is called less than once per yer, it will not be accurate. But
			// I suppose that is fine?
			let annual_proportion =
				Perquintill::from_rational(since_last_inflation, MILLISECONDS_PER_YEAR);

			let max_annual_inflation = Ratio::get();
			annual_proportion * max_annual_inflation * current_issuance
		}
	}

	/// Fixed inflation per yer, as specified by `Amount`.
	pub struct FixedAnnualInflation<T, Amount>(PhantomData<(T, Amount)>);
	impl<T: Config, Amount: Get<BalanceOf<T>>> InflationSource<BalanceOf<T>>
		for FixedAnnualInflation<T, Amount>
	{
		fn inflation_source(
			_current_issuance: BalanceOf<T>,
			last_inflated: u64,
			now: u64,
		) -> T::CurrencyBalance {
			let since_last_inflation = now.saturating_sub(last_inflated);

			// what percentage of a year has passed since last inflation?
			let annual_proportion =
				Perquintill::from_rational(since_last_inflation, MILLISECONDS_PER_YEAR);
			annual_proportion * Amount::get()
		}
	}

	/// The payout action to be taken in each inflation step.
	#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
	pub enum Action<AccountId> {
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

	/// A step in the process of [`Config::Distribution`].
	///
	/// The inputs are:
	///
	/// * `balance`: the total amount that is available at this step to be inflated.
	/// * `perquintill`: the proportion of the tokens that are staked from the perspective of this
	///   pallet.
	///
	/// Return types are:
	///
	/// * `balance`: a subset of the input balance that should be paid out. This amount should
	///   always be less than or equal to the input balance.
	/// * [`Action`]: an action to be made.
	pub type DistributionStep<T> = Box<
		dyn Fn(
			BalanceOf<T>,
			Perquintill,
		) -> (BalanceOf<T>, Action<<T as frame_system::Config>::AccountId>),
	>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Something that provides a notion of the unix-time.
		type UnixTime: frame::traits::UnixTime;

		/// The currency type of the runtime.
		type Currency: fung::Mutate<Self::AccountId>
			+ fung::Inspect<Self::AccountId, Balance = Self::CurrencyBalance>;

		/// Same as the balance type of [`Config::Currency`], only provided to further bound it to
		/// `From<u64>`.
		type CurrencyBalance: frame::traits::tokens::Balance + From<u64>;

		/// How the inflation amount, specified by [`Config::InflationSource`] should be
		/// distributed.
		type Distribution: Get<Vec<DistributionStep<Self>>>;

		/// An origin that can trigger an inflation at any point in time via
		/// [`Call::force_inflate`].
		type InflationOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The main input function that determines how this pallet determines the inflation amount.
		type InflationSource: InflationSource<BalanceOf<Self>>;

		/// Customize how this pallet reads the total issuance, if need be. If not, the sensible
		/// value of `Currency::total_issuance()` is used.
		///
		/// NOTE: This is mainly here to cater for Nis in Kusama.
		fn adjusted_total_issuance() -> BalanceOf<Self> {
			Self::Currency::total_issuance()
		}

		/// A simple and possibly short terms means for updating the total stake.
		// Once multi-chain, we should expect an extrinsic, gated by the origin of the staking
		// parachain that can update this value. This can be `Transact`-ed via XCM.
		fn update_total_stake(stake: BalanceOf<Self>, valid_until: Option<BlockNumberFor<Self>>) {
			LastKnownStakedStorage::<Self>::put(LastKnownStake { stake, valid_until });
		}
	}

	// TODO: needs a migration that sets the initial value, and a genesis config that sets it.
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
		InflationDistributed { amount: BalanceOf<T>, payout: Action<T::AccountId> },
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
			let adjusted_total_issuance = T::adjusted_total_issuance();

			let mut max_payout =
				T::InflationSource::inflation_source(adjusted_total_issuance, last_inflated, now);

			if max_payout.is_zero() {
				Self::deposit_event(Event::PossiblyInflated { amount: Zero::zero() });
				LastInflated::<T>::put(T::UnixTime::now().as_millis().saturated_into::<u64>());
				return Ok(());
			}

			// staking rate.
			let total_staked = Self::last_known_stake().ok_or(Error::<T>::UnknownLastStake)?;
			let staked_ratio = Perquintill::from_rational(total_staked, adjusted_total_issuance);

			crate::log!(
				info,
				"inflating at {:?}, annual proportion {:?}, issuance {:?}, last inflated {:?}, max inflation {:?}, distributing among {}",
				now,
				Perquintill::from_rational(now.saturating_sub(last_inflated), MILLISECONDS_PER_YEAR),
				adjusted_total_issuance,
				last_inflated,
				max_payout,
				T::Distribution::get().len()
			);
			Self::deposit_event(Event::PossiblyInflated { amount: max_payout });

			for payout_fn in T::Distribution::get() {
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
					Action::Pay(who) => {
						let _ = T::Currency::mint_into(who, amount).defensive();
						max_payout -= amount;
					},
					Action::SplitEqual(whos) => {
						let amount_split = amount / (whos.len() as u32).into();
						for who in whos {
							let _ = T::Currency::mint_into(&who, amount_split).defensive();
							max_payout -= amount_split;
						}
					},
					Action::Burn => {
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
	pub mod inflation_step_prelude {
		use super::*;

		pub fn polkadot_staking_income<
			T: Config,
			IdealStakingRate: Get<Perquintill>,
			Falloff: Get<Perquintill>,
			StakingPayoutAccount: Get<T::AccountId>,
		>(
			max_payout: BalanceOf<T>,
			staked_ratio: Perquintill,
		) -> (BalanceOf<T>, Action<T::AccountId>) {
			// TODO: this should be runtime parameters.
			let min_payout_part = Perquintill::from_rational(25u64, 100u64);
			let ideal_stake = IdealStakingRate::get();
			let falloff = Falloff::get();

			// TODO: notion of min-inflation is now gone, will this be an issue?
			let adjustment =
				pallet_staking_reward_fn::compute_inflation(staked_ratio, ideal_stake, falloff);
			let staking_inflation =
				min_payout_part.saturating_add(Perquintill::one().saturating_sub(min_payout_part) * adjustment);
			let staking_income = staking_inflation * max_payout;

			crate::log!(
					info,
					"ideal_stake {:?}; falloff {:?}; staked_ratio: {:?}, adjustment {:?}, max_payout {:?}, staking_income {:?}",
					ideal_stake,
					falloff,
					staked_ratio,
					adjustment,
					max_payout,
					staking_income,
				);

			crate::log!(info, "calculated staking inflation is {:?}", staking_inflation);
			(staking_income, Action::Pay(StakingPayoutAccount::get()))
		}

		pub fn burn<T: Config, BurnRate: Get<Perquintill>>(
			max_inflation: BalanceOf<T>,
			_staking_ratio: Perquintill,
		) -> (BalanceOf<T>, Action<T::AccountId>) {
			let burn = BurnRate::get() * max_inflation;
			(burn, Action::Burn)
		}

		pub fn pay<T: Config, To: Get<T::AccountId>, Ratio: Get<Perquintill>>(
			max_inflation: BalanceOf<T>,
			_staking_ratio: Perquintill,
		) -> (BalanceOf<T>, Action<T::AccountId>) {
			let payout = Ratio::get() * max_inflation;
			(payout, Action::Pay(To::get()))
		}

		pub fn split_equal<T: Config, To: Get<Vec<T::AccountId>>, Ratio: Get<Perquintill>>(
			max_inflation: BalanceOf<T>,
			_staking_ratio: Perquintill,
		) -> (BalanceOf<T>, Action<T::AccountId>) {
			let payout = Ratio::get() * max_inflation;
			(payout, Action::SplitEqual(To::get()))
		}
	}
}

#[cfg(test)]
mod mock {
	use super::{pallet as pallet_inflation, *};
	use core::cell::RefCell;
	use frame::{arithmetic::*, prelude::*, testing_prelude::*, traits::fungible::Mutate};

	// TODO: update to `frame::runtime`.
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

	#[docify::export(configs)]
	parameter_types! {
		// With an annual inflation of 10%..
		pub static AnnualInflation: Perquintill = Perquintill::from_percent(10);

		// burn 20% of the inflation..
		pub static BurnRatio: Perquintill = Perquintill::from_percent(20);

		// Then give half of the rest to 1..
		pub static OneRecipient: AccountId = 1;
		pub static OneRatio: Perquintill = Perquintill::from_percent(50);

		// and split all of the rest between 2 and 3.
		pub static DividedRecipients: Vec<AccountId> = vec![2, 3];
		pub static DividedRatio: Perquintill = Perquintill::from_percent(100);
	}

	#[docify::export(fns)]
	thread_local! {
		static RECIPIENTS: RefCell<Vec<DistributionStep<Runtime>>> = RefCell::new(vec![
			Box::new(inflation_step_prelude::burn::<Runtime, BurnRatio>),
			Box::new(inflation_step_prelude::pay::<Runtime, OneRecipient, OneRatio>),
			Box::new(inflation_step_prelude::split_equal::<Runtime, DividedRecipients, DividedRatio>),
		]);
	}

	pub struct Recipients;
	impl Get<Vec<DistributionStep<Runtime>>> for Recipients {
		fn get() -> Vec<DistributionStep<Runtime>> {
			RECIPIENTS.with(|v| {
				let v_borrowed = v.borrow();
				let mut cloned = Vec::with_capacity(v_borrowed.len());
				for fn_box in &*v_borrowed {
					let fn_clone: DistributionStep<Runtime> = unsafe { core::ptr::read(fn_box) };
					cloned.push(fn_clone);
				}
				cloned
			})
		}
	}

	impl Recipients {
		pub(crate) fn add(new_fn: DistributionStep<Runtime>) {
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
		type Distribution = Recipients;
		type Currency = Balances;
		type CurrencyBalance = Balance;
		type InflationSource = FixedRatioAnnualInflation<Runtime, AnnualInflation>;
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
	use super::{
		mock::*,
		pallet::{self as pallet_inflation, *},
	};
	use frame::{prelude::*, testing_prelude::*, traits::fungible::Inspect};

	const DEFAULT_INITIAL_TI: Balance = 365 * 10 * 100;
	const DEFAULT_DAILY_INFLATION: Balance = 80;

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
					Event::InflationDistributed { payout: Action::Burn, amount: 20 },
					Event::InflationDistributed { payout: Action::Pay(1), amount: 40 },
					Event::InflationDistributed {
						payout: Action::SplitEqual(vec![2, 3]),
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
					Event::InflationDistributed { payout: Action::Burn, amount: 20 },
					Event::InflationDistributed { payout: Action::Pay(1), amount: 40 },
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
			Recipients::add(Box::new(|x, _| (x, Action::Burn)));

			// do the inflation.
			assert_ok!(Inflation::inflate());

			assert_eq!(
				events(),
				vec![
					Event::PossiblyInflated { amount: 100 },
					Event::InflationDistributed { amount: 100, payout: Action::Burn },
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
			LastInflated::<Runtime>::kill();

			assert_noop!(Inflation::inflate(), Error::<Runtime>::UnknownLastInflated);
		})
	}

	#[test]
	fn inflation_is_time_independent() {
		new_test_ext(DEFAULT_INITIAL_TI).execute_with(|| {
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
}
