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

//! Fellowship custom origins.

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
	pub struct Pallet<T>(_);

	#[derive(PartialEq, Eq, Clone, MaxEncodedLen, Encode, Decode, TypeInfo, RuntimeDebug)]
	#[pallet::origin]
	pub enum Origin {
		/// Origin aggregated through weighted votes of those with rank 1 or above; `Success` is 1.
		/// Aka the "voice" of all Members.
		Members,
		/// Origin aggregated through weighted votes of those with rank 2 or above; `Success` is 2.
		/// Aka the "voice" of members at least II Dan.
		Fellowship2Dan,
		/// Origin aggregated through weighted votes of those with rank 3 or above; `Success` is 3.
		/// Aka the "voice" of all Fellows.
		Fellows,
		/// Origin aggregated through weighted votes of those with rank 4 or above; `Success` is 4.
		/// Aka the "voice" of members at least IV Dan.
		Architects,
		/// Origin aggregated through weighted votes of those with rank 5 or above; `Success` is 5.
		/// Aka the "voice" of members at least V Dan.
		Fellowship5Dan,
		/// Origin aggregated through weighted votes of those with rank 6 or above; `Success` is 6.
		/// Aka the "voice" of members at least VI Dan.
		Fellowship6Dan,
		/// Origin aggregated through weighted votes of those with rank 7 or above; `Success` is 7.
		/// Aka the "voice" of all Masters.
		Masters,
		/// Origin aggregated through weighted votes of those with rank 8 or above; `Success` is 8.
		/// Aka the "voice" of members at least VIII Dan.
		Fellowship8Dan,
		/// Origin aggregated through weighted votes of those with rank 9 or above; `Success` is 9.
		/// Aka the "voice" of members at least IX Dan.
		Fellowship9Dan,

		/// Origin aggregated through weighted votes of those with rank 3 or above when voting on
		/// a fortnight-long track; `Success` is 1.
		RetainAt1Dan,
		/// Origin aggregated through weighted votes of those with rank 4 or above when voting on
		/// a fortnight-long track; `Success` is 2.
		RetainAt2Dan,
		/// Origin aggregated through weighted votes of those with rank 5 or above when voting on
		/// a fortnight-long track; `Success` is 3.
		RetainAt3Dan,
		/// Origin aggregated through weighted votes of those with rank 6 or above when voting on
		/// a fortnight-long track; `Success` is 4.
		RetainAt4Dan,
		/// Origin aggregated through weighted votes of those with rank 7 or above when voting on
		/// a fortnight-long track; `Success` is 5.
		RetainAt5Dan,
		/// Origin aggregated through weighted votes of those with rank 8 or above when voting on
		/// a fortnight-long track; `Success` is 6.
		RetainAt6Dan,

		/// Origin aggregated through weighted votes of those with rank 3 or above when voting on
		/// a month-long track; `Success` is 1.
		PromoteTo1Dan,
		/// Origin aggregated through weighted votes of those with rank 4 or above when voting on
		/// a month-long track; `Success` is 2.
		PromoteTo2Dan,
		/// Origin aggregated through weighted votes of those with rank 5 or above when voting on
		/// a month-long track; `Success` is 3.
		PromoteTo3Dan,
		/// Origin aggregated through weighted votes of those with rank 6 or above when voting on
		/// a month-long track; `Success` is 4.
		PromoteTo4Dan,
		/// Origin aggregated through weighted votes of those with rank 7 or above when voting on
		/// a month-long track; `Success` is 5.
		PromoteTo5Dan,
		/// Origin aggregated through weighted votes of those with rank 8 or above when voting on
		/// a month-long track; `Success` is 6.
		PromoteTo6Dan,
	}

	impl Origin {
		/// Returns the rank that the origin `self` speaks for, or `None` if it doesn't speak for
		/// any.
		///
		/// `Some` will be returned only for the first 9 elements of [Origin].
		pub fn as_voice(&self) -> Option<pallet_ranked_collective::Rank> {
			Some(match &self {
				Origin::Members => ranks::DAN_1,
				Origin::Fellowship2Dan => ranks::DAN_2,
				Origin::Fellows => ranks::DAN_3,
				Origin::Architects => ranks::DAN_4,
				Origin::Fellowship5Dan => ranks::DAN_5,
				Origin::Fellowship6Dan => ranks::DAN_6,
				Origin::Masters => ranks::DAN_7,
				Origin::Fellowship8Dan => ranks::DAN_8,
				Origin::Fellowship9Dan => ranks::DAN_9,
				_ => return None,
			})
		}
	}

	/// A `TryMorph` implementation which is designed to convert an aggregate `RuntimeOrigin`
	/// value into the Fellowship voice it represents if it is a Fellowship pallet origin an
	/// appropriate variant. See also [Origin::as_voice].
	pub struct ToVoice;
	impl<'a, O: 'a + TryInto<&'a Origin>> sp_runtime::traits::TryMorph<O> for ToVoice {
		type Outcome = pallet_ranked_collective::Rank;
		fn try_morph(o: O) -> Result<pallet_ranked_collective::Rank, ()> {
			o.try_into().ok().and_then(Origin::as_voice).ok_or(())
		}
	}

	macro_rules! decl_unit_ensures {
		( $name:ident: $success_type:ty = $success:expr ) => {
			pub struct $name;
			impl<O: Into<Result<Origin, O>> + From<Origin>>
				EnsureOrigin<O> for $name
			{
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
	decl_unit_ensures!(
		Members: Rank = ranks::DAN_1,
		Fellows: Rank = ranks::DAN_3,
		Architects: Rank = ranks::DAN_4,
		Masters: Rank = ranks::DAN_7,
	);

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
		}
	}

	// Fellowship origin indicating weighted voting from at least the rank of `Success` on a
	// week-long track.
	decl_ensure! {
		pub type EnsureFellowship: EnsureOrigin<Success = Rank> {
			Members = ranks::DAN_1,
			Fellowship2Dan = ranks::DAN_2,
			Fellows = ranks::DAN_3,
			Architects = ranks::DAN_4,
			Fellowship5Dan = ranks::DAN_5,
			Fellowship6Dan = ranks::DAN_6,
			Masters = ranks::DAN_7,
			Fellowship8Dan = ranks::DAN_8,
			Fellowship9Dan = ranks::DAN_9,
		}
	}

	// Fellowship origin indicating weighted voting from at least the rank of `Success + 2` on
	// a fortnight-long track; needed for Fellowship retention voting.
	decl_ensure! {
		pub type EnsureCanRetainAt: EnsureOrigin<Success = Rank> {
			RetainAt1Dan = ranks::DAN_1,
			RetainAt2Dan = ranks::DAN_2,
			RetainAt3Dan = ranks::DAN_3,
			RetainAt4Dan = ranks::DAN_4,
			RetainAt5Dan = ranks::DAN_5,
			RetainAt6Dan = ranks::DAN_6,
		}
	}

	// Fellowship origin indicating weighted voting from at least the rank of `Success + 2` on
	// a month-long track; needed for Fellowship promotion voting.
	decl_ensure! {
		pub type EnsureCanPromoteTo: EnsureOrigin<Success = Rank> {
			PromoteTo1Dan = ranks::DAN_1,
			PromoteTo2Dan = ranks::DAN_2,
			PromoteTo3Dan = ranks::DAN_3,
			PromoteTo4Dan = ranks::DAN_4,
			PromoteTo5Dan = ranks::DAN_5,
			PromoteTo6Dan = ranks::DAN_6,
		}
	}
}
