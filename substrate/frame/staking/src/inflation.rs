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

//! This module expose one function `P_NPoS` (Payout NPoS) or `compute_total_payout` which returns
//! the total payout for the era given the era duration and the staking rate in NPoS.
//! The staking rate in NPoS is the total amount of tokens staked by nominators and validators,
//! divided by the total token supply.

use sp_runtime::{curve::PiecewiseLinear, traits::AtLeast32BitUnsigned, Perbill};

#[frame_support::pallet]
pub mod inflation {
	//! Polkadot inflation pallet.
	use frame_support::{
		pallet_prelude::*,
		traits::{
			fungible::{self as fung, Inspect, Mutate},
			UnixTime,
		},
	};
	use frame_system::pallet_prelude::*;
	use sp_runtime::{traits::Saturating, Perquintill};

	type BalanceOf<T> = <T as Config>::CurrencyBalance;

	const MILLISECONDS_PER_YEAR: u64 = 1000 * 3600 * 24 * 36525 / 100;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type UnixTime: frame_support::traits::UnixTime;

		type IdealStakingRate: Get<Perquintill>;
		type MaxInflation: Get<Perquintill>;
		type MinInflation: Get<Perquintill>;
		type Falloff: Get<Perquintill>;

		type LeftoverRecipients: Get<Vec<(Self::AccountId, Perquintill)>>;
		type StakingRecipient: Get<Self::AccountId>;

		type Currency: fung::Mutate<Self::AccountId>
			+ fung::Inspect<Self::AccountId, Balance = Self::CurrencyBalance>;
		type CurrencyBalance: frame_support::traits::tokens::Balance + From<u64>;

		/// Customize how this pallet reads the total issuance, if need be.
		///
		/// This is mainly here to cater for Nis in Kusama.
		///
		/// NOTE: one should not use `T::Currency::total_issuance()` directly within the pallet in
		/// case it has been overwritten here.
		fn adjusted_total_issuance() -> BalanceOf<Self> {
			Self::Currency::total_issuance()
		}

		/// A simple and possibly short terms means for updating the total stake, esp. so long as
		/// this pallet is in the same runtime as with `pallet-staking`.
		///
		/// Once multi-chain, we should expect an extrinsic, gated by the origin of the staking
		/// parachain that can update this value. This can be `Transact`-ed via XCM.
		fn update_total_stake(new_total_stake: BalanceOf<Self>) {
			LastKnownStaked::<Self>::put(new_total_stake);
		}
	}

	// TODO: needs a migration that sets the initial value.
	// TODO: test if this is not set, that we are still bound to max inflation.
	#[pallet::storage]
	pub type LastInflated<T> = StorageValue<Value = u64, QueryKind = ValueQuery>;

	#[pallet::storage]
	pub type LastKnownStaked<T: Config> =
		StorageValue<Value = BalanceOf<T>, QueryKind = ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Inflated { staking: BalanceOf<T>, leftovers: BalanceOf<T> },
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// `force_inflate`
		#[pallet::weight(0)]
		#[pallet::call_index(0)]
		pub fn force_inflate(origin: OriginFor<T>) -> DispatchResult {
			ensure_root(origin)?;
			Self::inflate_with_bookkeeping();
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Trigger an inflation,
		pub fn inflate_with_duration(since_last_inflation: u64) {
			let adjusted_total_issuance = T::adjusted_total_issuance();

			// what percentage of a year has passed since last inflation?
			let annual_proportion =
				Perquintill::from_rational(since_last_inflation, MILLISECONDS_PER_YEAR);

			let total_staked = LastKnownStaked::<T>::get();

			let min_annual_inflation = T::MinInflation::get();
			let max_annual_inflation = T::MaxInflation::get();
			let delta_annual_inflation = max_annual_inflation.saturating_sub(min_annual_inflation);
			let ideal_stake = T::IdealStakingRate::get();

			let staked_ratio = Perquintill::from_rational(total_staked, adjusted_total_issuance);
			let falloff = T::Falloff::get();

			let adjustment =
				pallet_staking_reward_fn::compute_inflation(staked_ratio, ideal_stake, falloff);
			let staking_annual_inflation: Perquintill =
				min_annual_inflation.saturating_add(delta_annual_inflation * adjustment);

			// final inflation formula.
			let payout_with_annual_inflation = |i| annual_proportion * i * adjusted_total_issuance;

			// ideal amount that we want to payout.
			let max_payout = payout_with_annual_inflation(max_annual_inflation);
			let staking_payout = payout_with_annual_inflation(staking_annual_inflation);
			let leftover_inflation = max_payout.saturating_sub(staking_payout);

			T::LeftoverRecipients::get().into_iter().for_each(|(who, proportion)| {
				let amount = proportion * leftover_inflation;
				// not much we can do about errors here.
				let _ = T::Currency::mint_into(&who, amount).defensive();
			});

			Self::deposit_event(Event::Inflated { staking: staking_payout, leftovers: max_payout });
		}

		pub fn inflate_with_bookkeeping() {
			let last_inflated = LastInflated::<T>::get();
			let now = T::UnixTime::now().as_millis().saturated_into::<u64>();
			let since_last_inflation = now.saturating_sub(last_inflated);
			Self::inflate_with_duration(since_last_inflation);
			LastInflated::<T>::put(T::UnixTime::now().as_millis().saturated_into::<u64>());
		}
	}
}

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
	N: AtLeast32BitUnsigned + Clone,
{
	// Milliseconds per year for the Julian year (365.25 days).
	const MILLISECONDS_PER_YEAR: u64 = 1000 * 3600 * 24 * 36525 / 100;

	let portion = Perbill::from_rational(era_duration as u64, MILLISECONDS_PER_YEAR);
	let payout = portion *
		yearly_inflation
			.calculate_for_fraction_times_denominator(npos_token_staked, total_tokens.clone());
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
