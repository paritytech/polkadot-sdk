// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Track configurations for DDay.

use crate::{
	fellowship::pallet_fellowship_origins::Origin, Balance, BlockNumber, RuntimeOrigin, DAYS,
	DOLLARS, MINUTES,
};
use sp_runtime::{str_array as s, Perbill};
use sp_std::borrow::Cow;

/// Referendum `TrackId` type.
pub type TrackId = u16;

/// Referendum track IDs.
pub mod constants {
	use super::TrackId;
	pub const DDAY_PARACHAIN_RESCUE: TrackId = 1;
}

pub struct TracksInfo;
impl pallet_referenda::TracksInfo<Balance, BlockNumber> for TracksInfo {
	type Id = TrackId;
	type RuntimeOrigin = <RuntimeOrigin as frame_support::traits::OriginTrait>::PalletsOrigin;

	fn tracks(
	) -> impl Iterator<Item = Cow<'static, pallet_referenda::Track<Self::Id, Balance, BlockNumber>>>
	{
		use constants as tracks;
		static DATA: [pallet_referenda::Track<TrackId, Balance, BlockNumber>; 1] =
			[pallet_referenda::Track {
				id: tracks::DDAY_PARACHAIN_RESCUE,
				// TODO: FAIL-CI - verify constants
				info: pallet_referenda::TrackInfo {
					name: s("dday-parachain-rescue"),
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
			}];
		DATA.iter().map(Cow::Borrowed)
	}
	fn track_for(id: &Self::RuntimeOrigin) -> Result<Self::Id, ()> {
		use constants as tracks;
		#[cfg(feature = "runtime-benchmarks")]
		{
			// For benchmarks, we enable a root origin.
			// It is important that this is not available in production!
			let root: Self::RuntimeOrigin = frame_system::RawOrigin::Root.into();
			if &root == id {
				return Ok(tracks::DDAY_PARACHAIN_RESCUE);
			}
		}

		match Origin::try_from(id.clone()) {
			Ok(Origin::Fellows)
			| Ok(Origin::Architects)
			| Ok(Origin::Fellowship5Dan)
			| Ok(Origin::Fellowship6Dan)
			| Ok(Origin::Masters)
			| Ok(Origin::Fellowship8Dan)
			| Ok(Origin::Fellowship9Dan) => Ok(tracks::DDAY_PARACHAIN_RESCUE),
			_ => Err(()),
		}
	}
}
