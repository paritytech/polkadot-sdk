// Copyright 2023 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Track configurations for Fellowship.

use crate::{Balance, BlockNumber, RuntimeOrigin, DAYS, DOLLARS, MINUTES};
use sp_runtime::Perbill;

/// Referendum `TrackId` type.
pub type TrackId = u16;

/// Referendum track IDs.
pub mod constants {
	use super::TrackId;

	pub const CANDIDATES: TrackId = 0;
	pub const MEMBERS: TrackId = 1;
	pub const PROFICIENTS: TrackId = 2;
	pub const FELLOWS: TrackId = 3;
	pub const SENIOR_FELLOWS: TrackId = 4;
	pub const EXPERTS: TrackId = 5;
	pub const SENIOR_EXPERTS: TrackId = 6;
	pub const MASTERS: TrackId = 7;
	pub const SENIOR_MASTERS: TrackId = 8;
	pub const GRAND_MASTERS: TrackId = 9;
}

pub struct TracksInfo;
impl pallet_referenda::TracksInfo<Balance, BlockNumber> for TracksInfo {
	type Id = TrackId;
	type RuntimeOrigin = <RuntimeOrigin as frame_support::traits::OriginTrait>::PalletsOrigin;
	fn tracks() -> &'static [(Self::Id, pallet_referenda::TrackInfo<Balance, BlockNumber>)] {
		use constants as tracks;
		static DATA: [(TrackId, pallet_referenda::TrackInfo<Balance, BlockNumber>); 10] = [
			(
				tracks::CANDIDATES,
				pallet_referenda::TrackInfo {
					name: "candidates",
					max_deciding: 10,
					decision_deposit: 100 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 7 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 1 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(50),
					},
				},
			),
			(
				tracks::MEMBERS,
				pallet_referenda::TrackInfo {
					name: "members",
					max_deciding: 10,
					decision_deposit: 10 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 7 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 1 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(50),
					},
				},
			),
			(
				tracks::PROFICIENTS,
				pallet_referenda::TrackInfo {
					name: "proficients",
					max_deciding: 10,
					decision_deposit: 10 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 7 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 1 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(50),
					},
				},
			),
			(
				tracks::FELLOWS,
				pallet_referenda::TrackInfo {
					name: "fellows",
					max_deciding: 10,
					decision_deposit: 10 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 7 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 1 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(50),
					},
				},
			),
			(
				tracks::SENIOR_FELLOWS,
				pallet_referenda::TrackInfo {
					name: "senior fellows",
					max_deciding: 10,
					decision_deposit: 10 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 7 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 1 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(50),
					},
				},
			),
			(
				tracks::EXPERTS,
				pallet_referenda::TrackInfo {
					name: "experts",
					max_deciding: 10,
					decision_deposit: 1 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 7 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 1 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(50),
					},
				},
			),
			(
				tracks::SENIOR_EXPERTS,
				pallet_referenda::TrackInfo {
					name: "senior experts",
					max_deciding: 10,
					decision_deposit: 1 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 7 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 1 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(50),
					},
				},
			),
			(
				tracks::MASTERS,
				pallet_referenda::TrackInfo {
					name: "masters",
					max_deciding: 10,
					decision_deposit: 1 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 7 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 1 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(50),
					},
				},
			),
			(
				tracks::SENIOR_MASTERS,
				pallet_referenda::TrackInfo {
					name: "senior masters",
					max_deciding: 10,
					decision_deposit: 1 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 7 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 1 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(50),
					},
				},
			),
			(
				tracks::GRAND_MASTERS,
				pallet_referenda::TrackInfo {
					name: "grand masters",
					max_deciding: 10,
					decision_deposit: 1 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 7 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 1 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(50),
					},
				},
			),
		];
		&DATA[..]
	}
	fn track_for(id: &Self::RuntimeOrigin) -> Result<Self::Id, ()> {
		use super::origins::Origin;
		use constants as tracks;

		#[cfg(feature = "runtime-benchmarks")]
		{
			// For benchmarks, we enable a root origin.
			// It is important that this is not available in production!
			let root: Self::RuntimeOrigin = frame_system::RawOrigin::Root.into();
			if &root == id {
				return Ok(tracks::GRAND_MASTERS)
			}
		}

		match Origin::try_from(id.clone()) {
			Ok(Origin::FellowshipCandidates) => Ok(tracks::CANDIDATES),
			Ok(Origin::Fellowship1Dan) => Ok(tracks::MEMBERS),
			Ok(Origin::Fellowship2Dan) => Ok(tracks::PROFICIENTS),
			Ok(Origin::Fellowship3Dan) | Ok(Origin::Fellows) => Ok(tracks::FELLOWS),
			Ok(Origin::Fellowship4Dan) => Ok(tracks::SENIOR_FELLOWS),
			Ok(Origin::Fellowship5Dan) | Ok(Origin::FellowshipExperts) => Ok(tracks::EXPERTS),
			Ok(Origin::Fellowship6Dan) => Ok(tracks::SENIOR_EXPERTS),
			Ok(Origin::Fellowship7Dan | Origin::FellowshipMasters) => Ok(tracks::MASTERS),
			Ok(Origin::Fellowship8Dan) => Ok(tracks::SENIOR_MASTERS),
			Ok(Origin::Fellowship9Dan) => Ok(tracks::GRAND_MASTERS),
			_ => Err(()),
		}
	}
}
pallet_referenda::impl_tracksinfo_get!(TracksInfo, Balance, BlockNumber);
