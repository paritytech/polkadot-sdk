// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::{Balance, BlockNumber, RuntimeOrigin, DAYS, DOLLARS, HOURS, MINUTES};
use pallet_ranked_collective::Rank;
use sp_runtime::{traits::Convert, Perbill};

/// Referendum `TrackId` type.
pub type TrackId = u16;

/// Referendum track IDs.
pub mod constants {
	use super::TrackId;

	// Regular tracks (7 days) used for general operations. The required rank for voting is the
	// same as that which is named (and also the track ID).
	pub const MEMBERS: TrackId = 1;
	pub const PROFICIENTS: TrackId = 2;
	pub const FELLOWS: TrackId = 3;
	pub const ARCHITECTS: TrackId = 4;
	pub const ARCHITECTS_ADEPT: TrackId = 5;
	pub const GRAND_ARCHITECTS: TrackId = 6;
	pub const MASTERS: TrackId = 7;
	pub const MASTERS_CONSTANT: TrackId = 8;
	pub const GRAND_MASTERS: TrackId = 9;

	// Longer tracks (14 days) used for rank retention. These require a rank of two more than the
	// grade at which they retain (as per the whitepaper). This works out as the track ID minus 8.
	pub const RETAIN_AT_1DAN: TrackId = 11;
	pub const RETAIN_AT_2DAN: TrackId = 12;
	pub const RETAIN_AT_3DAN: TrackId = 13;
	pub const RETAIN_AT_4DAN: TrackId = 14;
	pub const RETAIN_AT_5DAN: TrackId = 15;
	pub const RETAIN_AT_6DAN: TrackId = 16;

	// Longest tracks (30 days) used for promotions. These require a rank of two more than the
	// grade to which they promote (as per the whitepaper). This works out as the track ID minus 18.
	pub const PROMOTE_TO_1DAN: TrackId = 21;
	pub const PROMOTE_TO_2DAN: TrackId = 22;
	pub const PROMOTE_TO_3DAN: TrackId = 23;
	pub const PROMOTE_TO_4DAN: TrackId = 24;
	pub const PROMOTE_TO_5DAN: TrackId = 25;
	pub const PROMOTE_TO_6DAN: TrackId = 26;
}

/// Convert the track ID (defined above) into the minimum rank (i.e. fellowship Dan grade) required
/// to vote on the track.
pub struct MinRankOfClass;
impl Convert<TrackId, Rank> for MinRankOfClass {
	fn convert(a: TrackId) -> Rank {
		match a {
			// Just a regular vote: the track ID is conveniently the same as the minimum rank.
			regular @ 1..=9 => regular,
			// A retention vote; the track ID turns out to be 8 more than the minimum required rank.
			retention @ 11..=16 => retention - 8,
			// A promotion vote; the track ID turns out to be 18 more than the minimum required
			// rank.
			promotion @ 21..=26 => promotion - 18,
			_ => Rank::max_value(),
		}
	}
}

const RETAIN_MAX_DECIDING: u32 = 25;
const RETAIN_DECISION_DEPOSIT: Balance = 5 * DOLLARS;
const RETAIN_PREPARE_PERIOD: BlockNumber = 0;
const RETAIN_DECISION_PERIOD: BlockNumber = 14 * DAYS;
const RETAIN_CONFIRM_PERIOD: BlockNumber = 1 * HOURS;
const RETAIN_MIN_ENACTMENT_PERIOD: BlockNumber = 0;
const RETAIN_MIN_APPROVAL: pallet_referenda::Curve = pallet_referenda::Curve::LinearDecreasing {
	length: Perbill::from_percent(100),
	floor: Perbill::from_percent(60),
	ceil: Perbill::from_percent(100),
};
const RETAIN_MIN_SUPPORT: pallet_referenda::Curve = pallet_referenda::Curve::LinearDecreasing {
	length: Perbill::from_percent(100),
	floor: Perbill::from_percent(10),
	ceil: Perbill::from_percent(100),
};

const PROMOTE_MAX_DECIDING: u32 = 10;
const PROMOTE_DECISION_DEPOSIT: Balance = 5 * DOLLARS;
const PROMOTE_PREPARE_PERIOD: BlockNumber = 0;
const PROMOTE_DECISION_PERIOD: BlockNumber = 30 * DAYS;
const PROMOTE_CONFIRM_PERIOD: BlockNumber = 1 * HOURS;
const PROMOTE_MIN_ENACTMENT_PERIOD: BlockNumber = 0;
const PROMOTE_MIN_APPROVAL: pallet_referenda::Curve = pallet_referenda::Curve::LinearDecreasing {
	length: Perbill::from_percent(100),
	floor: Perbill::from_percent(60),
	ceil: Perbill::from_percent(100),
};
const PROMOTE_MIN_SUPPORT: pallet_referenda::Curve = pallet_referenda::Curve::LinearDecreasing {
	length: Perbill::from_percent(100),
	floor: Perbill::from_percent(10),
	ceil: Perbill::from_percent(100),
};

pub struct TracksInfo;
impl pallet_referenda::TracksInfo<Balance, BlockNumber> for TracksInfo {
	type Id = TrackId;
	type RuntimeOrigin = <RuntimeOrigin as frame_support::traits::OriginTrait>::PalletsOrigin;
	fn tracks() -> &'static [(Self::Id, pallet_referenda::TrackInfo<Balance, BlockNumber>)] {
		use constants as tracks;
		static DATA: [(TrackId, pallet_referenda::TrackInfo<Balance, BlockNumber>); 21] = [
			(
				tracks::MEMBERS,
				pallet_referenda::TrackInfo {
					name: "members",
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 1 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 5 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(100),
					},
				},
			),
			(
				tracks::PROFICIENTS,
				pallet_referenda::TrackInfo {
					name: "proficient members",
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 1 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 5 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(100),
					},
				},
			),
			(
				tracks::FELLOWS,
				pallet_referenda::TrackInfo {
					name: "fellows",
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 1 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 5 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(100),
					},
				},
			),
			(
				tracks::ARCHITECTS,
				pallet_referenda::TrackInfo {
					name: "architects",
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 1 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 5 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(100),
					},
				},
			),
			(
				tracks::ARCHITECTS_ADEPT,
				pallet_referenda::TrackInfo {
					name: "architects adept",
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 1 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 5 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(100),
					},
				},
			),
			(
				tracks::GRAND_ARCHITECTS,
				pallet_referenda::TrackInfo {
					name: "grand architects",
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 1 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 5 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(100),
					},
				},
			),
			(
				tracks::MASTERS,
				pallet_referenda::TrackInfo {
					name: "masters",
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 1 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 5 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(100),
					},
				},
			),
			(
				tracks::MASTERS_CONSTANT,
				pallet_referenda::TrackInfo {
					name: "masters constant",
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 1 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 5 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(100),
					},
				},
			),
			(
				tracks::GRAND_MASTERS,
				pallet_referenda::TrackInfo {
					name: "grand masters",
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 30 * MINUTES,
					decision_period: 1 * DAYS,
					confirm_period: 30 * MINUTES,
					min_enactment_period: 5 * MINUTES,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(100),
					},
				},
			),
			(
				tracks::RETAIN_AT_1DAN,
				pallet_referenda::TrackInfo {
					name: "retain at I Dan",
					max_deciding: RETAIN_MAX_DECIDING,
					decision_deposit: RETAIN_DECISION_DEPOSIT,
					prepare_period: RETAIN_PREPARE_PERIOD,
					decision_period: RETAIN_DECISION_PERIOD,
					confirm_period: RETAIN_CONFIRM_PERIOD,
					min_enactment_period: RETAIN_MIN_ENACTMENT_PERIOD,
					min_approval: RETAIN_MIN_APPROVAL,
					min_support: RETAIN_MIN_SUPPORT,
				},
			),
			(
				tracks::RETAIN_AT_2DAN,
				pallet_referenda::TrackInfo {
					name: "retain at II Dan",
					max_deciding: RETAIN_MAX_DECIDING,
					decision_deposit: RETAIN_DECISION_DEPOSIT,
					prepare_period: RETAIN_PREPARE_PERIOD,
					decision_period: RETAIN_DECISION_PERIOD,
					confirm_period: RETAIN_CONFIRM_PERIOD,
					min_enactment_period: RETAIN_MIN_ENACTMENT_PERIOD,
					min_approval: RETAIN_MIN_APPROVAL,
					min_support: RETAIN_MIN_SUPPORT,
				},
			),
			(
				tracks::RETAIN_AT_3DAN,
				pallet_referenda::TrackInfo {
					name: "retain at III Dan",
					max_deciding: RETAIN_MAX_DECIDING,
					decision_deposit: RETAIN_DECISION_DEPOSIT,
					prepare_period: RETAIN_PREPARE_PERIOD,
					decision_period: RETAIN_DECISION_PERIOD,
					confirm_period: RETAIN_CONFIRM_PERIOD,
					min_enactment_period: RETAIN_MIN_ENACTMENT_PERIOD,
					min_approval: RETAIN_MIN_APPROVAL,
					min_support: RETAIN_MIN_SUPPORT,
				},
			),
			(
				tracks::RETAIN_AT_4DAN,
				pallet_referenda::TrackInfo {
					name: "retain at IV Dan",
					max_deciding: RETAIN_MAX_DECIDING,
					decision_deposit: RETAIN_DECISION_DEPOSIT,
					prepare_period: RETAIN_PREPARE_PERIOD,
					decision_period: RETAIN_DECISION_PERIOD,
					confirm_period: RETAIN_CONFIRM_PERIOD,
					min_enactment_period: RETAIN_MIN_ENACTMENT_PERIOD,
					min_approval: RETAIN_MIN_APPROVAL,
					min_support: RETAIN_MIN_SUPPORT,
				},
			),
			(
				tracks::RETAIN_AT_5DAN,
				pallet_referenda::TrackInfo {
					name: "retain at V Dan",
					max_deciding: RETAIN_MAX_DECIDING,
					decision_deposit: RETAIN_DECISION_DEPOSIT,
					prepare_period: RETAIN_PREPARE_PERIOD,
					decision_period: RETAIN_DECISION_PERIOD,
					confirm_period: RETAIN_CONFIRM_PERIOD,
					min_enactment_period: RETAIN_MIN_ENACTMENT_PERIOD,
					min_approval: RETAIN_MIN_APPROVAL,
					min_support: RETAIN_MIN_SUPPORT,
				},
			),
			(
				tracks::RETAIN_AT_6DAN,
				pallet_referenda::TrackInfo {
					name: "retain at VI Dan",
					max_deciding: RETAIN_MAX_DECIDING,
					decision_deposit: RETAIN_DECISION_DEPOSIT,
					prepare_period: RETAIN_PREPARE_PERIOD,
					decision_period: RETAIN_DECISION_PERIOD,
					confirm_period: RETAIN_CONFIRM_PERIOD,
					min_enactment_period: RETAIN_MIN_ENACTMENT_PERIOD,
					min_approval: RETAIN_MIN_APPROVAL,
					min_support: RETAIN_MIN_SUPPORT,
				},
			),
			(
				tracks::PROMOTE_TO_1DAN,
				pallet_referenda::TrackInfo {
					name: "promote to I Dan",
					max_deciding: PROMOTE_MAX_DECIDING,
					decision_deposit: PROMOTE_DECISION_DEPOSIT,
					prepare_period: PROMOTE_PREPARE_PERIOD,
					decision_period: PROMOTE_DECISION_PERIOD,
					confirm_period: PROMOTE_CONFIRM_PERIOD,
					min_enactment_period: PROMOTE_MIN_ENACTMENT_PERIOD,
					min_approval: PROMOTE_MIN_APPROVAL,
					min_support: PROMOTE_MIN_SUPPORT,
				},
			),
			(
				tracks::PROMOTE_TO_2DAN,
				pallet_referenda::TrackInfo {
					name: "promote to II Dan",
					max_deciding: PROMOTE_MAX_DECIDING,
					decision_deposit: PROMOTE_DECISION_DEPOSIT,
					prepare_period: PROMOTE_PREPARE_PERIOD,
					decision_period: PROMOTE_DECISION_PERIOD,
					confirm_period: PROMOTE_CONFIRM_PERIOD,
					min_enactment_period: PROMOTE_MIN_ENACTMENT_PERIOD,
					min_approval: PROMOTE_MIN_APPROVAL,
					min_support: PROMOTE_MIN_SUPPORT,
				},
			),
			(
				tracks::PROMOTE_TO_3DAN,
				pallet_referenda::TrackInfo {
					name: "promote to III Dan",
					max_deciding: PROMOTE_MAX_DECIDING,
					decision_deposit: PROMOTE_DECISION_DEPOSIT,
					prepare_period: PROMOTE_PREPARE_PERIOD,
					decision_period: PROMOTE_DECISION_PERIOD,
					confirm_period: PROMOTE_CONFIRM_PERIOD,
					min_enactment_period: PROMOTE_MIN_ENACTMENT_PERIOD,
					min_approval: PROMOTE_MIN_APPROVAL,
					min_support: PROMOTE_MIN_SUPPORT,
				},
			),
			(
				tracks::PROMOTE_TO_4DAN,
				pallet_referenda::TrackInfo {
					name: "promote to IV Dan",
					max_deciding: PROMOTE_MAX_DECIDING,
					decision_deposit: PROMOTE_DECISION_DEPOSIT,
					prepare_period: PROMOTE_PREPARE_PERIOD,
					decision_period: PROMOTE_DECISION_PERIOD,
					confirm_period: PROMOTE_CONFIRM_PERIOD,
					min_enactment_period: PROMOTE_MIN_ENACTMENT_PERIOD,
					min_approval: PROMOTE_MIN_APPROVAL,
					min_support: PROMOTE_MIN_SUPPORT,
				},
			),
			(
				tracks::PROMOTE_TO_5DAN,
				pallet_referenda::TrackInfo {
					name: "promote to V Dan",
					max_deciding: PROMOTE_MAX_DECIDING,
					decision_deposit: PROMOTE_DECISION_DEPOSIT,
					prepare_period: PROMOTE_PREPARE_PERIOD,
					decision_period: PROMOTE_DECISION_PERIOD,
					confirm_period: PROMOTE_CONFIRM_PERIOD,
					min_enactment_period: PROMOTE_MIN_ENACTMENT_PERIOD,
					min_approval: PROMOTE_MIN_APPROVAL,
					min_support: PROMOTE_MIN_SUPPORT,
				},
			),
			(
				tracks::PROMOTE_TO_6DAN,
				pallet_referenda::TrackInfo {
					name: "promote to VI Dan",
					max_deciding: PROMOTE_MAX_DECIDING,
					decision_deposit: PROMOTE_DECISION_DEPOSIT,
					prepare_period: PROMOTE_PREPARE_PERIOD,
					decision_period: PROMOTE_DECISION_PERIOD,
					confirm_period: PROMOTE_CONFIRM_PERIOD,
					min_enactment_period: PROMOTE_MIN_ENACTMENT_PERIOD,
					min_approval: PROMOTE_MIN_APPROVAL,
					min_support: PROMOTE_MIN_SUPPORT,
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
			Ok(Origin::Members) => Ok(tracks::MEMBERS),
			Ok(Origin::Fellowship2Dan) => Ok(tracks::PROFICIENTS),
			Ok(Origin::Fellows) => Ok(tracks::FELLOWS),
			Ok(Origin::Architects) => Ok(tracks::ARCHITECTS),
			Ok(Origin::Fellowship5Dan) => Ok(tracks::ARCHITECTS_ADEPT),
			Ok(Origin::Fellowship6Dan) => Ok(tracks::GRAND_ARCHITECTS),
			Ok(Origin::Masters) => Ok(tracks::MASTERS),
			Ok(Origin::Fellowship8Dan) => Ok(tracks::MASTERS_CONSTANT),
			Ok(Origin::Fellowship9Dan) => Ok(tracks::GRAND_MASTERS),

			Ok(Origin::RetainAt1Dan) => Ok(tracks::RETAIN_AT_1DAN),
			Ok(Origin::RetainAt2Dan) => Ok(tracks::RETAIN_AT_2DAN),
			Ok(Origin::RetainAt3Dan) => Ok(tracks::RETAIN_AT_3DAN),
			Ok(Origin::RetainAt4Dan) => Ok(tracks::RETAIN_AT_4DAN),
			Ok(Origin::RetainAt5Dan) => Ok(tracks::RETAIN_AT_5DAN),
			Ok(Origin::RetainAt6Dan) => Ok(tracks::RETAIN_AT_6DAN),

			Ok(Origin::PromoteTo1Dan) => Ok(tracks::PROMOTE_TO_1DAN),
			Ok(Origin::PromoteTo2Dan) => Ok(tracks::PROMOTE_TO_2DAN),
			Ok(Origin::PromoteTo3Dan) => Ok(tracks::PROMOTE_TO_3DAN),
			Ok(Origin::PromoteTo4Dan) => Ok(tracks::PROMOTE_TO_4DAN),
			Ok(Origin::PromoteTo5Dan) => Ok(tracks::PROMOTE_TO_5DAN),
			Ok(Origin::PromoteTo6Dan) => Ok(tracks::PROMOTE_TO_6DAN),

			_ => Err(()),
		}
	}
}
pallet_referenda::impl_tracksinfo_get!(TracksInfo, Balance, BlockNumber);
