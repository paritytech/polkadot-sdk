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

//! An implementation of [`ElectionProvider`] that uses an `NposSolver` to do the election. As the
//! name suggests, this is meant to be used onchain. Given how heavy the calculations are, please be
//! careful when using it onchain.

use crate::{
	bounds::{DataProviderBounds, ElectionBounds, ElectionBoundsBuilder},
	BoundedSupportsOf, Debug, ElectionDataProvider, ElectionProvider, InstantElectionProvider,
	NposSolver, PageIndex, TryIntoBoundedSupports, WeightInfo, Zero,
};
use alloc::collections::btree_map::BTreeMap;
use core::marker::PhantomData;
use frame_support::{dispatch::DispatchClass, traits::Get};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_npos_elections::{
	assignment_ratio_to_staked_normalized, to_supports, ElectionResult, VoteWeight,
};

/// Errors of the on-chain election.
#[derive(Eq, PartialEq, Debug)]
pub enum Error {
	/// An internal error in the NPoS elections crate.
	NposElections(sp_npos_elections::Error),
	/// Errors from the data provider.
	DataProvider(&'static str),
	/// Configurational error caused by `desired_targets` requested by data provider exceeding
	/// `MaxWinners`.
	TooManyWinners,
	/// Single page election called with multi-page configs.
	SinglePageExpected,
}

impl From<sp_npos_elections::Error> for Error {
	fn from(e: sp_npos_elections::Error) -> Self {
		Error::NposElections(e)
	}
}

/// A simple on-chain implementation of the election provider trait.
///
/// This implements both `ElectionProvider` and `InstantElectionProvider`.
///
/// This type has some utilities to make it safe. Nonetheless, it should be used with utmost care. A
/// thoughtful value must be set as [`Config::Bounds`] to ensure the size of the input is sensible.
pub struct OnChainExecution<T: Config>(PhantomData<T>);

#[deprecated(note = "use OnChainExecution, which is bounded by default")]
pub type BoundedExecution<T> = OnChainExecution<T>;

/// Configuration trait for an onchain election execution.
pub trait Config {
	/// Needed for weight registration.
	type System: frame_system::Config;

	/// `NposSolver` that should be used, an example would be `PhragMMS`.
	type Solver: NposSolver<
		AccountId = <Self::System as frame_system::Config>::AccountId,
		Error = sp_npos_elections::Error,
	>;

	/// Maximum number of backers allowed per target.
	///
	/// If the bounds are exceeded due to the data returned by the data provider, the election will
	/// fail.
	type MaxBackersPerWinner: Get<u32>;

	/// Maximum number of winners in an election.
	///
	/// If the bounds are exceeded due to the data returned by the data provider, the election will
	/// fail.
	type MaxWinnersPerPage: Get<u32>;

	/// Something that provides the data for election.
	type DataProvider: ElectionDataProvider<
		AccountId = <Self::System as frame_system::Config>::AccountId,
		BlockNumber = frame_system::pallet_prelude::BlockNumberFor<Self::System>,
	>;

	/// Weight information for extrinsics in this pallet.
	type WeightInfo: WeightInfo;

	/// Elections bounds, to use when calling into [`Config::DataProvider`]. It might be overwritten
	/// in the `InstantElectionProvider` impl.
	type Bounds: Get<ElectionBounds>;
}

impl<T: Config> OnChainExecution<T> {
	fn elect_with(
		bounds: ElectionBounds,
		page: PageIndex,
	) -> Result<BoundedSupportsOf<Self>, Error> {
		let (voters, targets) = T::DataProvider::electing_voters(bounds.voters, page)
			.and_then(|voters| {
				Ok((voters, T::DataProvider::electable_targets(bounds.targets, page)?))
			})
			.map_err(Error::DataProvider)?;

		let desired_targets = T::DataProvider::desired_targets().map_err(Error::DataProvider)?;

		if desired_targets > T::MaxWinnersPerPage::get() {
			// early exit
			return Err(Error::TooManyWinners)
		}

		let voters_len = voters.len() as u32;
		let targets_len = targets.len() as u32;

		let stake_map: BTreeMap<_, _> = voters
			.iter()
			.map(|(validator, vote_weight, _)| (validator.clone(), *vote_weight))
			.collect();

		let stake_of = |w: &<T::System as frame_system::Config>::AccountId| -> VoteWeight {
			stake_map.get(w).cloned().unwrap_or_default()
		};

		let ElectionResult { winners: _, assignments } =
			T::Solver::solve(desired_targets as usize, targets, voters).map_err(Error::from)?;

		let staked = assignment_ratio_to_staked_normalized(assignments, &stake_of)?;

		let weight = T::Solver::weight::<T::WeightInfo>(
			voters_len,
			targets_len,
			<T::DataProvider as ElectionDataProvider>::MaxVotesPerVoter::get(),
		);
		frame_system::Pallet::<T::System>::register_extra_weight_unchecked(
			weight,
			DispatchClass::Mandatory,
		);

		// defensive: Since npos solver returns a result always bounded by `desired_targets`, this
		// is never expected to happen as long as npos solver does what is expected for it to do.
		let supports: BoundedSupportsOf<Self> = to_supports(&staked)
			.try_into_bounded_supports()
			.map_err(|_| Error::TooManyWinners)?;

		Ok(supports)
	}
}

impl<T: Config> InstantElectionProvider for OnChainExecution<T> {
	fn instant_elect(
		forced_input_voters_bounds: DataProviderBounds,
		forced_input_targets_bounds: DataProviderBounds,
	) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		let elections_bounds = ElectionBoundsBuilder::from(T::Bounds::get())
			.voters_or_lower(forced_input_voters_bounds)
			.targets_or_lower(forced_input_targets_bounds)
			.build();

		// NOTE: instant provider is *always* single page.
		Self::elect_with(elections_bounds, Zero::zero())
	}
}

impl<T: Config> ElectionProvider for OnChainExecution<T> {
	type AccountId = <T::System as frame_system::Config>::AccountId;
	type BlockNumber = BlockNumberFor<T::System>;
	type Error = Error;
	type MaxWinnersPerPage = T::MaxWinnersPerPage;
	type MaxBackersPerWinner = T::MaxBackersPerWinner;
	type Pages = sp_core::ConstU32<1>;
	type DataProvider = T::DataProvider;

	fn elect(page: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		if page > 0 {
			return Err(Error::SinglePageExpected)
		}

		let election_bounds = ElectionBoundsBuilder::from(T::Bounds::get()).build();
		Self::elect_with(election_bounds, Zero::zero())
	}

	fn ongoing() -> bool {
		false
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{ElectionProvider, PhragMMS, SequentialPhragmen};
	use frame_support::{assert_noop, derive_impl, parameter_types};
	use sp_npos_elections::Support;
	use sp_runtime::Perbill;
	type AccountId = u64;
	type Nonce = u64;
	type BlockNumber = u64;

	pub type Header = sp_runtime::generic::Header<BlockNumber, sp_runtime::traits::BlakeTwo256>;
	pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<AccountId, (), (), ()>;
	pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;

	frame_support::construct_runtime!(
		pub enum Runtime {
			System: frame_system,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type SS58Prefix = ();
		type BaseCallFilter = frame_support::traits::Everything;
		type RuntimeOrigin = RuntimeOrigin;
		type Nonce = Nonce;
		type RuntimeCall = RuntimeCall;
		type Hash = sp_core::H256;
		type Hashing = sp_runtime::traits::BlakeTwo256;
		type AccountId = AccountId;
		type Lookup = sp_runtime::traits::IdentityLookup<Self::AccountId>;
		type Block = Block;
		type RuntimeEvent = ();
		type BlockHashCount = ();
		type DbWeight = ();
		type BlockLength = ();
		type BlockWeights = ();
		type Version = ();
		type PalletInfo = PalletInfo;
		type AccountData = ();
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type SystemWeightInfo = ();
		type OnSetCode = ();
		type MaxConsumers = frame_support::traits::ConstU32<16>;
	}

	struct PhragmenParams;
	struct PhragMMSParams;

	parameter_types! {
		pub static MaxWinnersPerPage: u32 = 10;
		pub static MaxBackersPerWinner: u32 = 20;
		pub static DesiredTargets: u32 = 2;
		pub static Bounds: ElectionBounds = ElectionBoundsBuilder::default().voters_count(600.into()).targets_count(400.into()).build();
	}

	impl Config for PhragmenParams {
		type System = Runtime;
		type Solver = SequentialPhragmen<AccountId, Perbill>;
		type DataProvider = mock_data_provider::DataProvider;
		type MaxWinnersPerPage = MaxWinnersPerPage;
		type MaxBackersPerWinner = MaxBackersPerWinner;
		type Bounds = Bounds;
		type WeightInfo = ();
	}

	impl Config for PhragMMSParams {
		type System = Runtime;
		type Solver = PhragMMS<AccountId, Perbill>;
		type DataProvider = mock_data_provider::DataProvider;
		type MaxWinnersPerPage = MaxWinnersPerPage;
		type MaxBackersPerWinner = MaxBackersPerWinner;
		type WeightInfo = ();
		type Bounds = Bounds;
	}

	mod mock_data_provider {
		use frame_support::traits::ConstU32;
		use sp_runtime::bounded_vec;

		use super::*;
		use crate::{data_provider, PageIndex, VoterOf};

		pub struct DataProvider;
		impl ElectionDataProvider for DataProvider {
			type AccountId = AccountId;
			type BlockNumber = BlockNumber;
			type MaxVotesPerVoter = ConstU32<2>;
			fn electing_voters(
				_: DataProviderBounds,
				_page: PageIndex,
			) -> data_provider::Result<Vec<VoterOf<Self>>> {
				Ok(vec![
					(1, 10, bounded_vec![10, 20]),
					(2, 20, bounded_vec![30, 20]),
					(3, 30, bounded_vec![10, 30]),
				])
			}

			fn electable_targets(
				_: DataProviderBounds,
				_page: PageIndex,
			) -> data_provider::Result<Vec<AccountId>> {
				Ok(vec![10, 20, 30])
			}

			fn desired_targets() -> data_provider::Result<u32> {
				Ok(DesiredTargets::get())
			}

			fn next_election_prediction(_: BlockNumber) -> BlockNumber {
				0
			}
		}
	}

	#[test]
	fn onchain_seq_phragmen_works() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			let expected_suports = vec![
				(
					10 as AccountId,
					Support { total: 25, voters: vec![(1 as AccountId, 10), (3, 15)] },
				),
				(30, Support { total: 35, voters: vec![(2, 20), (3, 15)] }),
			]
			.try_into_bounded_supports()
			.unwrap();

			assert_eq!(
				<OnChainExecution::<PhragmenParams> as ElectionProvider>::elect(0).unwrap(),
				expected_suports,
			);
		})
	}

	#[test]
	fn too_many_winners_when_desired_targets_exceed_max_winners() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			// given desired targets larger than max winners
			DesiredTargets::set(10);
			MaxWinnersPerPage::set(9);

			assert_noop!(
				<OnChainExecution::<PhragmenParams> as ElectionProvider>::elect(0),
				Error::TooManyWinners,
			);
		})
	}

	#[test]
	fn onchain_phragmms_works() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			assert_eq!(
				<OnChainExecution::<PhragMMSParams> as ElectionProvider>::elect(0).unwrap(),
				vec![
					(
						10 as AccountId,
						Support { total: 25, voters: vec![(1 as AccountId, 10), (3, 15)] }
					),
					(30, Support { total: 35, voters: vec![(2, 20), (3, 15)] })
				]
				.try_into_bounded_supports()
				.unwrap()
			);
		})
	}
}
