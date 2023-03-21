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
		/// Origin commanded by any members of the Polkadot Fellowship (no Dan grade needed).
		FellowshipCandidates,
		/// Origin commanded by Polkadot Fellows (3rd Dan fellows or greater).
		Fellows,
		/// Origin commanded by Polkadot Experts (5th Dan fellows or greater).
		FellowshipExperts,
		/// Origin commanded by Polkadot Masters (7th Dan fellows of greater).
		FellowshipMasters,
		/// Origin commanded by rank 1 of the Polkadot Fellowship and with a success of 1.
		Fellowship1Dan,
		/// Origin commanded by rank 2 of the Polkadot Fellowship and with a success of 2.
		Fellowship2Dan,
		/// Origin commanded by rank 3 of the Polkadot Fellowship and with a success of 3.
		Fellowship3Dan,
		/// Origin commanded by rank 4 of the Polkadot Fellowship and with a success of 4.
		Fellowship4Dan,
		/// Origin commanded by rank 5 of the Polkadot Fellowship and with a success of 5.
		Fellowship5Dan,
		/// Origin commanded by rank 6 of the Polkadot Fellowship and with a success of 6.
		Fellowship6Dan,
		/// Origin commanded by rank 7 of the Polkadot Fellowship and with a success of 7.
		Fellowship7Dan,
		/// Origin commanded by rank 8 of the Polkadot Fellowship and with a success of 8.
		Fellowship8Dan,
		/// Origin commanded by rank 9 of the Polkadot Fellowship and with a success of 9.
		Fellowship9Dan,
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
		FellowshipCandidates: Rank = ranks::CANDIDATES,
		Fellows: Rank = ranks::DAN_3,
		FellowshipExperts: Rank = ranks::DAN_5,
		FellowshipMasters: Rank = ranks::DAN_7,
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

	decl_ensure! {
		pub type EnsureFellowship: EnsureOrigin<Success = Rank> {
			Fellowship1Dan = ranks::DAN_1,
			Fellowship2Dan = ranks::DAN_2,
			Fellowship3Dan = ranks::DAN_3,
			Fellowship4Dan = ranks::DAN_4,
			Fellowship5Dan = ranks::DAN_5,
			Fellowship6Dan = ranks::DAN_6,
			Fellowship7Dan = ranks::DAN_7,
			Fellowship8Dan = ranks::DAN_8,
			Fellowship9Dan = ranks::DAN_9,
		}
	}
}
