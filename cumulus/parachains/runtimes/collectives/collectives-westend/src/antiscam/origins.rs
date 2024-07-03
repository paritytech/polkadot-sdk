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

//! The Ambassador Program's origins.

#[frame_support::pallet]
pub mod pallet_origins {
	use crate::ambassador::ranks;
	use frame_support::pallet_prelude::*;
	use pallet_ranked_collective::Rank;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	/// The pallet configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[derive(PartialEq, Eq, Clone, MaxEncodedLen, Encode, Decode, TypeInfo, RuntimeDebug)]
	#[pallet::origin]
	pub enum Origin {
		/// Plurality voice of the [ranks::AMBASSADOR_TIER_1] members or above given via
		/// referendum.
		Ambassadors,
		/// Plurality voice of the [ranks::AMBASSADOR_TIER_2] members or above given via
		/// referendum.
		AmbassadorsTier2,
		/// Plurality voice of the [ranks::SENIOR_AMBASSADOR_TIER_3] members or above given via
		/// referendum.
		SeniorAmbassadors,
		/// Plurality voice of the [ranks::SENIOR_AMBASSADOR_TIER_4] members or above given via
		/// referendum.
		SeniorAmbassadorsTier4,
		/// Plurality voice of the [ranks::HEAD_AMBASSADOR_TIER_5] members or above given via
		/// referendum.
		HeadAmbassadors,
		/// Plurality voice of the [ranks::HEAD_AMBASSADOR_TIER_6] members or above given via
		/// referendum.
		HeadAmbassadorsTier6,
		/// Plurality voice of the [ranks::HEAD_AMBASSADOR_TIER_7] members or above given via
		/// referendum.
		HeadAmbassadorsTier7,
		/// Plurality voice of the [ranks::MASTER_AMBASSADOR_TIER_8] members or above given via
		/// referendum.
		MasterAmbassadors,
		/// Plurality voice of the [ranks::MASTER_AMBASSADOR_TIER_9] members or above given via
		/// referendum.
		MasterAmbassadorsTier9,
	}

	impl Origin {
		/// Returns the rank that the origin `self` speaks for, or `None` if it doesn't speak for
		/// any.
		pub fn as_voice(&self) -> Option<Rank> {
			Some(match &self {
				Origin::Ambassadors => ranks::AMBASSADOR_TIER_1,
				Origin::AmbassadorsTier2 => ranks::AMBASSADOR_TIER_2,
				Origin::SeniorAmbassadors => ranks::SENIOR_AMBASSADOR_TIER_3,
				Origin::SeniorAmbassadorsTier4 => ranks::SENIOR_AMBASSADOR_TIER_4,
				Origin::HeadAmbassadors => ranks::HEAD_AMBASSADOR_TIER_5,
				Origin::HeadAmbassadorsTier6 => ranks::HEAD_AMBASSADOR_TIER_6,
				Origin::HeadAmbassadorsTier7 => ranks::HEAD_AMBASSADOR_TIER_7,
				Origin::MasterAmbassadors => ranks::MASTER_AMBASSADOR_TIER_8,
				Origin::MasterAmbassadorsTier9 => ranks::MASTER_AMBASSADOR_TIER_9,
			})
		}
	}

	/// Implementation of the [EnsureOrigin] trait for the [Origin::HeadAmbassadors] origin.
	pub struct EnsureHeadAmbassadorsVoice;
	impl<O: Into<Result<Origin, O>> + From<Origin>> EnsureOrigin<O> for EnsureHeadAmbassadorsVoice {
		type Success = ();
		fn try_origin(o: O) -> Result<Self::Success, O> {
			o.into().and_then(|o| match o {
				Origin::HeadAmbassadors => Ok(()),
				r => Err(O::from(r)),
			})
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<O, ()> {
			Ok(O::from(Origin::HeadAmbassadors))
		}
	}

	/// Implementation of the [EnsureOrigin] trait for the plurality voice [Origin]s
	/// from a given rank `R` with the success result of the corresponding [Rank].
	pub struct EnsureAmbassadorsVoiceFrom<R>(PhantomData<R>);
	impl<R: Get<Rank>, O: Into<Result<Origin, O>> + From<Origin>> EnsureOrigin<O>
		for EnsureAmbassadorsVoiceFrom<R>
	{
		type Success = Rank;
		fn try_origin(o: O) -> Result<Self::Success, O> {
			o.into().and_then(|o| match Origin::as_voice(&o) {
				Some(r) if r >= R::get() => Ok(r),
				_ => Err(O::from(o)),
			})
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<O, ()> {
			ranks::MASTER_AMBASSADOR_TIER_9
				.ge(&R::get())
				.then(|| O::from(Origin::MasterAmbassadorsTier9))
				.ok_or(())
		}
	}

	/// Implementation of the [EnsureOrigin] trait for the plurality voice [Origin]s with the
	/// success result of the corresponding [Rank].
	pub struct EnsureAmbassadorsVoice;
	impl<O: Into<Result<Origin, O>> + From<Origin>> EnsureOrigin<O> for EnsureAmbassadorsVoice {
		type Success = Rank;
		fn try_origin(o: O) -> Result<Self::Success, O> {
			o.into().and_then(|o| Origin::as_voice(&o).ok_or(O::from(o)))
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<O, ()> {
			Ok(O::from(Origin::MasterAmbassadorsTier9))
		}
	}
}
