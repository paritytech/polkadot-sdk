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
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Extrinsics implementing the relay chain side of the Coretime interface.
//!
//! https://github.com/polkadot-fellows/RFCs/blob/main/text/0005-coretime-interface.md

#[cfg(test)]
mod tests;

use frame_support::{pallet_prelude::*, traits::Currency};
use frame_system::pallet_prelude::*;
use pallet_broker::CoreAssignment;
use primitives::CoreIndex;
use runtime_parachains::assigner_bulk::{self, PartsOf57600};

const LOG_TARGET: &str = "runtime::common::coretime";

pub use pallet::*;

pub trait WeightInfo {
	fn request_core_count() -> Weight;
	fn request_revenue_info_at() -> Weight;
	fn credit_account() -> Weight;
	fn assign_core() -> Weight;
}

/// A weight info that is only suitable for testing.
pub struct TestWeightInfo;

impl WeightInfo for TestWeightInfo {
	fn request_core_count() -> Weight {
		Weight::MAX
	}
	fn request_revenue_info_at() -> Weight {
		Weight::MAX
	}
	fn credit_account() -> Weight {
		Weight::MAX
	}
	fn assign_core() -> Weight {
		Weight::MAX
	}
}

/// Shorthand for the Balance type the runtime is using.
type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet(dev_mode)]
pub mod pallet {

	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + assigner_bulk::Config {
		/// The runtime's definition of a Currency.
		type Currency: Currency<Self::AccountId>;
		/// Something that provides the weight of this pallet.
		//type WeightInfo: WeightInfo;
		/// The external origin allowed to enact coretime extrinsics. Usually the broker system
		/// parachain.
		type ExternalBrokerOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		// TODO Impl me!
		//#[pallet::weight(<T as Config>::WeightInfo::request_core_count())]
		#[pallet::call_index(0)]
		pub fn request_core_count(origin: OriginFor<T>, count: u16) -> DispatchResult {
			// Ignore requests not coming from the External Broker parachain.
			let _multi_location = <T as Config>::ExternalBrokerOrigin::ensure_origin(origin)?;
			Ok(())
		}

		// TODO Impl me!
		//#[pallet::weight(<T as Config>::WeightInfo::request_revenue_info_at())]
		#[pallet::call_index(1)]
		pub fn request_revenue_info_at(
			origin: OriginFor<T>,
			when: BlockNumberFor<T>,
		) -> DispatchResult {
			// Ignore requests not coming from the External Broker parachain.
			let _multi_location = <T as Config>::ExternalBrokerOrigin::ensure_origin(origin)?;
			Ok(())
		}

		// TODO Impl me!
		//#[pallet::weight(<T as Config>::WeightInfo::credit_account())]
		#[pallet::call_index(2)]
		pub fn credit_account(
			origin: OriginFor<T>,
			who: T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			// Ignore requests not coming from the External Broker parachain.
			let _multi_location = <T as Config>::ExternalBrokerOrigin::ensure_origin(origin)?;
			Ok(())
		}

		/// Receive instructions from the `ExternalBrokerOrigin`, detailing how a specific core is
		/// to be used.
		///
		/// Parameters:
		/// -`origin`: The `ExternalBrokerOrigin`, assumed to be the Broker system parachain.
		/// -`core`: The core that should be scheduled.
		/// -`begin`: The starting blockheight of the instruction.
		/// -`assignment`: How the blockspace should be utilised.
		/// -`end_hint`: An optional hint as to when this particular set of instructions will end.
		// TODO: Weights!
		//#[pallet::weight(<T as Config>::WeightInfo::assign_core())]
		#[pallet::call_index(3)]
		pub fn assign_core(
			origin: OriginFor<T>,
			core: CoreIndex,
			begin: BlockNumberFor<T>,
			assignment: Vec<(CoreAssignment, PartsOf57600)>,
			end_hint: Option<BlockNumberFor<T>>,
		) -> DispatchResult {
			// Ignore requests not coming from the External Broker parachain.
			let _multi_location = <T as Config>::ExternalBrokerOrigin::ensure_origin(origin)?;

			<assigner_bulk::Pallet<T>>::assign_core(core, begin, assignment, end_hint)
		}
	}
}
