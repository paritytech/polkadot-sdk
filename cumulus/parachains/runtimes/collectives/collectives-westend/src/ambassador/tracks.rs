// Copyright (C) 2022 Parity Technologies (UK) Ltd.
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

//! The Ambassador Fellowship's referenda voting tracks.

use crate::{Balance, BlockNumber, RuntimeOrigin, DAYS, DOLLARS, HOURS};
use sp_runtime::{str_array as s, Perbill};
use sp_std::borrow::Cow;
use frame_support::traits::{CallerTrait};

/// Referendum `TrackId` type.
pub type TrackId = u16;

/// Referendum track IDs.
pub mod constants {
	use super::TrackId;

	// Tier A: Learners
	pub const ASSOCIATE_AMBASSADOR: TrackId = 1;
	pub const LEAD_AMBASSADOR: TrackId = 2;

	// Tier B: Engagers
	pub const SENIOR_AMBASSADOR: TrackId = 3;
	pub const PRINCIPAL_AMBASSADOR: TrackId = 4;

	// Tier C: Drivers
	pub const GLOBAL_AMBASSADOR: TrackId = 5;
	pub const GLOBAL_HEAD_AMBASSADOR: TrackId = 6;
}

/// The type implementing the [`pallet_referenda::TracksInfo`] trait for referenda pallet.
pub struct TracksInfo;

/// Information on the voting tracks.
impl pallet_referenda::TracksInfo<Balance, BlockNumber> for TracksInfo {
	type Id = TrackId;

	type RuntimeOrigin = <RuntimeOrigin as frame_support::traits::OriginTrait>::PalletsOrigin;

	/// Return the list of available tracks and their information.
	fn tracks(
	) -> impl Iterator<Item = Cow<'static, pallet_referenda::Track<Self::Id, Balance, BlockNumber>>>
	{
		static DATA: [pallet_referenda::Track<TrackId, Balance, BlockNumber>; 6] = [
			pallet_referenda::Track {
				id: constants::ASSOCIATE_AMBASSADOR,
				info: pallet_referenda::TrackInfo {
					name: s("associate ambassador"),
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 24 * HOURS,
					decision_period: 1 * DAYS,
					confirm_period: 24 * HOURS,
					min_enactment_period: 1 * HOURS,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(10),
						ceil: Perbill::from_percent(50),
					},
				},
			},
			pallet_referenda::Track {
				id: constants::LEAD_AMBASSADOR,
				info: pallet_referenda::TrackInfo {
					name: s("lead ambassador"),
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 24 * HOURS,
					decision_period: 1 * DAYS,
					confirm_period: 24 * HOURS,
					min_enactment_period: 1 * HOURS,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(10),
						ceil: Perbill::from_percent(50),
					},
				},
			},
			pallet_referenda::Track {
				id: constants::SENIOR_AMBASSADOR,
				info: pallet_referenda::TrackInfo {
					name: s("senior ambassador"),
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 24 * HOURS,
					decision_period: 1 * DAYS,
					confirm_period: 24 * HOURS,
					min_enactment_period: 1 * HOURS,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(10),
						ceil: Perbill::from_percent(50),
					},
				},
			},
			pallet_referenda::Track {
				id: constants::PRINCIPAL_AMBASSADOR,
				info: pallet_referenda::TrackInfo {
					name: s("principal ambassador"),
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 24 * HOURS,
					decision_period: 1 * DAYS,
					confirm_period: 24 * HOURS,
					min_enactment_period: 1 * HOURS,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(10),
						ceil: Perbill::from_percent(50),
					},
				},
			},
			pallet_referenda::Track {
				id: constants::GLOBAL_AMBASSADOR,
				info: pallet_referenda::TrackInfo {
					name: s("global ambassador"),
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 24 * HOURS,
					decision_period: 1 * DAYS,
					confirm_period: 24 * HOURS,
					min_enactment_period: 1 * HOURS,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(10),
						ceil: Perbill::from_percent(50),
					},
				},
			},
			pallet_referenda::Track {
				id: constants::GLOBAL_HEAD_AMBASSADOR,
				info: pallet_referenda::TrackInfo {
					name: s("global head ambassador"),
					max_deciding: 10,
					decision_deposit: 5 * DOLLARS,
					prepare_period: 24 * HOURS,
					decision_period: 1 * DAYS,
					confirm_period: 24 * HOURS,
					min_enactment_period: 1 * HOURS,
					min_approval: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: pallet_referenda::Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(10),
						ceil: Perbill::from_percent(50),
					},
				},
			},
		];
		DATA.iter().map(|x| Cow::Borrowed(x))
	}

	/// Determine the voting track for the given `origin`.
	fn track_for(origin: &Self::RuntimeOrigin) -> Result<Self::Id, ()> {
		use constants::*;
		use scale_info::prelude::format;

		// Check if it's a system origin
		if origin.is_root() || origin.is_none() {
			return Err(());
		}

		let origin_str = format!("{:?}", origin);

		// Check for each ambassador origin by looking at the debug string
		if origin_str.contains("GlobalHeadAmbassadors") {
			return Ok(GLOBAL_HEAD_AMBASSADOR);
		} else if origin_str.contains("GlobalAmbassadors") {
			return Ok(GLOBAL_AMBASSADOR);
		} else if origin_str.contains("PrincipalAmbassadors") {
			return Ok(PRINCIPAL_AMBASSADOR);
		} else if origin_str.contains("SeniorAmbassadors") {
			return Ok(SENIOR_AMBASSADOR);
		} else if origin_str.contains("LeadAmbassadors") {
			return Ok(LEAD_AMBASSADOR);
		} else if origin_str.contains("AssociateAmbassadors") {
			return Ok(ASSOCIATE_AMBASSADOR);
		}

		Err(())
	}
}
