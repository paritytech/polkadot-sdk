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

//! The Ambassador Fellowship Program's origins.

use super::ranks;
pub use pallet_origins::*;

#[frame_support::pallet]
pub mod pallet_origins {
	use super::ranks;
	use frame_support::pallet_prelude::*;
	use pallet_ranked_collective::Rank;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[derive(
		PartialEq,
		Eq,
		Clone,
		MaxEncodedLen,
		Encode,
		Decode,
		DecodeWithMemTracking,
		TypeInfo,
		RuntimeDebug,
	)]
	#[pallet::origin]
	pub enum Origin {
		/// Plurality voice of the [ranks::ADVOCATE_AMBASSADOR] members or above given via
		/// referendum.
		AdvocateAmbassadors,
		/// Plurality voice of the [ranks::ASSOCIATE_AMBASSADOR] members or above given via
		/// referendum.
		AssociateAmbassadors,
		/// Plurality voice of the [ranks::LEAD_AMBASSADOR] members or above given via
		/// referendum.
		LeadAmbassadors,
		/// Plurality voice of the [ranks::SENIOR_AMBASSADOR] members or above given via
		/// referendum.
		SeniorAmbassadors,
		/// Plurality voice of the [ranks::PRINCIPAL_AMBASSADOR] members or above given via
		/// referendum.
		PrincipalAmbassadors,
		/// Plurality voice of the [ranks::GLOBAL_AMBASSADOR] members or above given via
		/// referendum.
		GlobalAmbassadors,
		/// Plurality voice of the [ranks::GLOBAL_HEAD_AMBASSADOR] members or above given via
		/// referendum.
		GlobalHeadAmbassadors,

		RetainAt1Rank,
		RetainAt2Rank,
		RetainAt3Rank,
		RetainAt4Rank,
		RetainAt5Rank,
		RetainAt6Rank,
		RetainAt7Rank,

		DemoteTo1Rank,
		DemoteTo2Rank,
		DemoteTo3Rank,
		DemoteTo4Rank,
		DemoteTo5Rank,
		DemoteTo6Rank,

		PromoteTo1Rank,
		PromoteTo2Rank,
		PromoteTo3Rank,
		PromoteTo4Rank,
		PromoteTo5Rank,
		PromoteTo6Rank,
		PromoteTo7Rank,
	}

	impl Origin {
		/// Returns the rank that the origin `self` speaks for, or `None` if it doesn't speak for
		/// any.
		pub fn as_voice(&self) -> Option<Rank> {
			Some(match &self {
				Origin::AdvocateAmbassadors => ranks::ADVOCATE_AMBASSADOR,
				Origin::AssociateAmbassadors => ranks::ASSOCIATE_AMBASSADOR,
				Origin::LeadAmbassadors => ranks::LEAD_AMBASSADOR,
				Origin::SeniorAmbassadors => ranks::SENIOR_AMBASSADOR,
				Origin::PrincipalAmbassadors => ranks::PRINCIPAL_AMBASSADOR,
				Origin::GlobalAmbassadors => ranks::GLOBAL_AMBASSADOR,
				Origin::GlobalHeadAmbassadors => ranks::GLOBAL_HEAD_AMBASSADOR,
				_ => return None,
			})
		}
	}

	/// Implementation of the [EnsureOrigin] trait for the [Origin::GlobalHeadAmbassadors] origin.
	pub struct EnsureGlobalHeadAmbassadorsVoice;
	impl<O: Into<Result<Origin, O>> + From<Origin>> EnsureOrigin<O> for EnsureGlobalHeadAmbassadorsVoice {
		type Success = ();
		fn try_origin(o: O) -> Result<Self::Success, O> {
			o.into().and_then(|o| match o {
				Origin::GlobalHeadAmbassadors => Ok(()),
				r => Err(O::from(r)),
			})
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<O, ()> {
			Ok(O::from(Origin::GlobalHeadAmbassadors))
		}
	}

	/// Implementation of the [EnsureOrigin] trait for the plurality voice [Origin]s
	/// from a given rank `R` with the success result of the corresponding [Rank].
	pub struct EnsureAmbassadorsVoiceFrom<R>(PhantomData<R>);
	impl<R: Get<Rank>, O: OriginTrait + From<Origin>> EnsureOrigin<O> for EnsureAmbassadorsVoiceFrom<R>
	where
		for<'a> &'a O::PalletsOrigin: TryInto<&'a Origin>,
	{
		type Success = Rank;
		fn try_origin(o: O) -> Result<Self::Success, O> {
			match o.caller().try_into().map(|o| Origin::as_voice(o)) {
				Ok(Some(r)) if r >= R::get() => return Ok(r),
				_ => (),
			}

			Err(o)
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<O, ()> {
			ranks::GLOBAL_HEAD_AMBASSADOR
				.ge(&R::get())
				.then(|| O::from(Origin::GlobalHeadAmbassadors))
				.ok_or(())
		}
	}

	/// Implementation of the [EnsureOrigin] trait for the plurality voice [Origin]s with the
	/// success result of the corresponding [Rank].
	pub struct EnsureAmbassadorsVoice;
	impl<O: OriginTrait + From<Origin>> EnsureOrigin<O> for EnsureAmbassadorsVoice
	where
		for<'a> &'a O::PalletsOrigin: TryInto<&'a Origin>,
	{
		type Success = Rank;
		fn try_origin(o: O) -> Result<Self::Success, O> {
			match o.caller().try_into().map(|o| Origin::as_voice(o)) {
				Ok(Some(r)) => return Ok(r),
				_ => (),
			}

			Err(o)
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<O, ()> {
			Ok(O::from(Origin::GlobalHeadAmbassadors))
		}
	}

	// Define a macro for creating EnsureOrigin implementations for specific origins
	macro_rules! decl_unit_ensures {
		( $name:ident: $success_type:ty = $success:expr ) => {
			pub struct $name;
			impl<O: Into<Result<Origin, O>> + From<Origin>> EnsureOrigin<O> for $name {
				type Success = $success_type;
				fn try_origin(o: O) -> Result<Self::Success, O> {
					o.into().and_then(|o| match o {
						Origin::$name => Ok($success),
						r => Err(O::from(r)),
					})
				}
				#[cfg(feature = "runtime-benchmarks")]
				fn try_successful_origin() -> Result<O, ()> {
					Ok(O::from(Origin::$name))
				}
			}
		};
		( $name:ident ) => { decl_unit_ensures! { $name : () = () } };
		( $name:ident: $success_type:ty = $success:expr, $( $rest:tt )* ) => {
			decl_unit_ensures! { $name: $success_type = $success }
			decl_unit_ensures! { $( $rest )* }
		};
		( $name:ident, $( $rest:tt )* ) => {
			decl_unit_ensures! { $name }
			decl_unit_ensures! { $( $rest )* }
		};
		() => {}
	}

	// Implement specific EnsureOrigin types for each ambassador rank
	decl_unit_ensures!(
		AdvocateAmbassadors: Rank = ranks::ADVOCATE_AMBASSADOR,
		AssociateAmbassadors: Rank = ranks::ASSOCIATE_AMBASSADOR,
		LeadAmbassadors: Rank = ranks::LEAD_AMBASSADOR,
		SeniorAmbassadors: Rank = ranks::SENIOR_AMBASSADOR,
		PrincipalAmbassadors: Rank = ranks::PRINCIPAL_AMBASSADOR,
		GlobalAmbassadors: Rank = ranks::GLOBAL_AMBASSADOR,
		GlobalHeadAmbassadors: Rank = ranks::GLOBAL_HEAD_AMBASSADOR,
	);

	// Define the decl_ensure! macro for creating more complex EnsureOrigin implementations
	macro_rules! decl_ensure {
		(
			$vis:vis type $name:ident: EnsureOrigin<Success = $success_type:ty> {
				$( $item:ident = $success:expr, )*
			}
		) => {
			$vis struct $name;
			impl<O: Into<Result<Origin, O>> + From<Origin>>
				EnsureOrigin<O> for $name
			{
				type Success = $success_type;
				fn try_origin(o: O) -> Result<Self::Success, O> {
					o.into().and_then(|o| match o {
						$(
							Origin::$item => Ok($success),
						)*
						r => Err(O::from(r)),
					})
				}
				#[cfg(feature = "runtime-benchmarks")]
				fn try_successful_origin() -> Result<O, ()> {
					// By convention the more privileged origins go later, so for greatest chance
					// of success, we want the last one.
					let _result: Result<O, ()> = Err(());
					$(
						let _result: Result<O, ()> = Ok(O::from(Origin::$item));
					)*
					_result
				}
			}
		};
	}

	// Ambassador origin indicating weighted voting from at least the rank of `Success` on a
	// week-long track.
	decl_ensure! {
		pub type EnsureAmbassador: EnsureOrigin<Success = Rank> {
			AdvocateAmbassadors = ranks::ADVOCATE_AMBASSADOR,
			AssociateAmbassadors = ranks::ASSOCIATE_AMBASSADOR,
			LeadAmbassadors = ranks::LEAD_AMBASSADOR,
			SeniorAmbassadors = ranks::SENIOR_AMBASSADOR,
			PrincipalAmbassadors = ranks::PRINCIPAL_AMBASSADOR,
			GlobalAmbassadors = ranks::GLOBAL_AMBASSADOR,
			GlobalHeadAmbassadors = ranks::GLOBAL_HEAD_AMBASSADOR,
		}
	}

	// Ambassador origin indicating weighted voting from at least the rank of `Success + 2` on
	// a fortnight-long track; needed for Ambassador retention voting.
	decl_ensure! {
		pub type EnsureCanRetainAt: EnsureOrigin<Success = Rank> {
			RetainAt1Rank = ranks::ADVOCATE_AMBASSADOR,
			RetainAt2Rank = ranks::ASSOCIATE_AMBASSADOR,
			RetainAt3Rank = ranks::LEAD_AMBASSADOR,
			RetainAt4Rank = ranks::SENIOR_AMBASSADOR,
			RetainAt5Rank = ranks::PRINCIPAL_AMBASSADOR,
			RetainAt6Rank = ranks::GLOBAL_AMBASSADOR,
			RetainAt7Rank = ranks::GLOBAL_HEAD_AMBASSADOR,
		}
	}

	// Ambassador origin indicating weighted voting from at least the rank of `Success + 2` on
	// a fortnight-long track; needed for Ambassador demotion voting.
	decl_ensure! {
		pub type EnsureCanDemoteTo: EnsureOrigin<Success = Rank> {
			DemoteTo1Rank = ranks::ADVOCATE_AMBASSADOR,
			DemoteTo2Rank = ranks::ASSOCIATE_AMBASSADOR,
			DemoteTo3Rank = ranks::LEAD_AMBASSADOR,
			DemoteTo4Rank = ranks::SENIOR_AMBASSADOR,
			DemoteTo5Rank = ranks::PRINCIPAL_AMBASSADOR,
			DemoteTo6Rank = ranks::GLOBAL_AMBASSADOR,
		}
	}

	// Ambassador origin indicating weighted voting from at least the rank of `Success + 2` on
	// a fortnight-long track; needed for Ambassador promotion voting.
	decl_ensure! {
		pub type EnsureCanPromoteTo: EnsureOrigin<Success = Rank> {
			PromoteTo1Rank = ranks::ADVOCATE_AMBASSADOR,
			PromoteTo2Rank = ranks::ASSOCIATE_AMBASSADOR,
			PromoteTo3Rank = ranks::LEAD_AMBASSADOR,
			PromoteTo4Rank = ranks::SENIOR_AMBASSADOR,
			PromoteTo5Rank = ranks::PRINCIPAL_AMBASSADOR,
			PromoteTo6Rank = ranks::GLOBAL_AMBASSADOR,
			PromoteTo7Rank = ranks::GLOBAL_HEAD_AMBASSADOR,
		}
	}
}
