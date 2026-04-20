// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

//! Custom origins for governance interventions.

pub use pallet_custom_origins::*;

#[frame_support::pallet]
pub mod pallet_custom_origins {
	use crate::{Balance, CENTS, GRAND};
	use frame_support::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[derive(
		PartialEq, Eq, Clone, MaxEncodedLen, Encode, Decode, DecodeWithMemTracking, TypeInfo, Debug,
	)]
	#[pallet::origin]
	pub enum Origin {
		/// Origin for cancelling slashes.
		#[codec(index = 0)]
		StakingAdmin,
		// index 1 was Treasurer, removed by #11705.
		/// Origin for managing the composition of the fellowship.
		#[codec(index = 2)]
		FellowshipAdmin,
		/// Origin for managing the registrar.
		#[codec(index = 3)]
		GeneralAdmin,
		/// Origin for starting auctions.
		#[codec(index = 4)]
		AuctionAdmin,
		/// Origin able to force slot leases.
		#[codec(index = 5)]
		LeaseAdmin,
		/// Origin able to cancel referenda.
		#[codec(index = 6)]
		ReferendumCanceller,
		/// Origin able to kill referenda.
		#[codec(index = 7)]
		ReferendumKiller,
		/// Origin able to spend up to 1 KSM from the treasury at once.
		#[codec(index = 8)]
		SmallTipper,
		/// Origin able to spend up to 5 KSM from the treasury at once.
		#[codec(index = 9)]
		BigTipper,
		/// Origin able to spend up to 50 KSM from the treasury at once.
		#[codec(index = 10)]
		SmallSpender,
		/// Origin able to spend up to 500 KSM from the treasury at once.
		#[codec(index = 11)]
		MediumSpender,
		/// Origin able to spend up to 5,000 KSM from the treasury at once.
		#[codec(index = 12)]
		BigSpender,
		/// Origin able to dispatch a whitelisted call.
		#[codec(index = 13)]
		WhitelistedCaller,
		/// Origin commanded by any members of the Polkadot Fellowship (no Dan grade needed).
		#[codec(index = 14)]
		FellowshipInitiates,
		/// Origin commanded by Polkadot Fellows (3rd Dan fellows or greater).
		#[codec(index = 15)]
		Fellows,
		/// Origin commanded by Polkadot Experts (5th Dan fellows or greater).
		#[codec(index = 16)]
		FellowshipExperts,
		/// Origin commanded by Polkadot Masters (7th Dan fellows of greater).
		#[codec(index = 17)]
		FellowshipMasters,
		/// Origin commanded by rank 1 of the Polkadot Fellowship and with a success of 1.
		#[codec(index = 18)]
		Fellowship1Dan,
		/// Origin commanded by rank 2 of the Polkadot Fellowship and with a success of 2.
		#[codec(index = 19)]
		Fellowship2Dan,
		/// Origin commanded by rank 3 of the Polkadot Fellowship and with a success of 3.
		#[codec(index = 20)]
		Fellowship3Dan,
		/// Origin commanded by rank 4 of the Polkadot Fellowship and with a success of 4.
		#[codec(index = 21)]
		Fellowship4Dan,
		/// Origin commanded by rank 5 of the Polkadot Fellowship and with a success of 5.
		#[codec(index = 22)]
		Fellowship5Dan,
		/// Origin commanded by rank 6 of the Polkadot Fellowship and with a success of 6.
		#[codec(index = 23)]
		Fellowship6Dan,
		/// Origin commanded by rank 7 of the Polkadot Fellowship and with a success of 7.
		#[codec(index = 24)]
		Fellowship7Dan,
		/// Origin commanded by rank 8 of the Polkadot Fellowship and with a success of 8.
		#[codec(index = 25)]
		Fellowship8Dan,
		/// Origin commanded by rank 9 of the Polkadot Fellowship and with a success of 9.
		#[codec(index = 26)]
		Fellowship9Dan,
	}

	macro_rules! decl_unit_ensures {
		( $name:ident: $success_type:ty = $success:expr ) => {
			pub struct $name;
			impl<O: OriginTrait + From<Origin>> EnsureOrigin<O> for $name
			where
				for <'a> &'a O::PalletsOrigin: TryInto<&'a Origin>,
			{
				type Success = $success_type;
				fn try_origin(o: O) -> Result<Self::Success, O> {
					match o.caller().try_into() {
						Ok(Origin::$name) => return Ok($success),
						_ => (),
					}

					Err(o)
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
		StakingAdmin,
		FellowshipAdmin,
		GeneralAdmin,
		AuctionAdmin,
		LeaseAdmin,
		ReferendumCanceller,
		ReferendumKiller,
		WhitelistedCaller,
		FellowshipInitiates: u16 = 0,
		Fellows: u16 = 3,
		FellowshipExperts: u16 = 5,
		FellowshipMasters: u16 = 7,
	);

	macro_rules! decl_ensure {
		(
			$vis:vis type $name:ident: EnsureOrigin<Success = $success_type:ty> {
				$( $item:ident = $success:expr, )*
			}
		) => {
			$vis struct $name;
			impl<O: OriginTrait + From<Origin>> EnsureOrigin<O> for $name
			where
				for <'a> &'a O::PalletsOrigin: TryInto<&'a Origin>,
			{
				type Success = $success_type;
				fn try_origin(o: O) -> Result<Self::Success, O> {
					match o.caller().try_into() {
						$(
							Ok(Origin::$item) => return Ok($success),
						)*
						_ => (),
					}

					Err(o)
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
		pub type Spender: EnsureOrigin<Success = Balance> {
			SmallTipper = 250 * 3 * CENTS,
			BigTipper = 1 * GRAND,
			SmallSpender = 10 * GRAND,
			MediumSpender = 100 * GRAND,
			BigSpender = 1_000 * GRAND,
		}
	}

	decl_ensure! {
		pub type EnsureFellowship: EnsureOrigin<Success = u16> {
			Fellowship1Dan = 1,
			Fellowship2Dan = 2,
			Fellowship3Dan = 3,
			Fellowship4Dan = 4,
			Fellowship5Dan = 5,
			Fellowship6Dan = 6,
			Fellowship7Dan = 7,
			Fellowship8Dan = 8,
			Fellowship9Dan = 9,
		}
	}
}
