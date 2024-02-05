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
//! <https://github.com/polkadot-fellows/RFCs/blob/main/text/0005-coretime-interface.md>

use sp_std::{prelude::*, result};

use frame_support::{pallet_prelude::*, traits::Currency};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use pallet_broker::{CoreAssignment, CoreIndex as BrokerCoreIndex};
use primitives::{CoreIndex, Id as ParaId};
use sp_arithmetic::traits::SaturatedConversion;
use xcm::v4::{send_xcm, Instruction, Junction, Location, OriginKind, SendXcm, WeightLimit, Xcm};

use crate::{
	assigner_coretime::{self, PartsOf57600},
	initializer::{OnNewSession, SessionChangeNotification},
	origin::{ensure_parachain, Origin},
};

mod benchmarking;
pub mod migration;

pub trait WeightInfo {
	fn request_core_count() -> Weight;
	//fn request_revenue_info_at() -> Weight;
	//fn credit_account() -> Weight;
	fn assign_core(s: u32) -> Weight;
}

/// A weight info that is only suitable for testing.
pub struct TestWeightInfo;

impl WeightInfo for TestWeightInfo {
	fn request_core_count() -> Weight {
		Weight::MAX
	}
	// TODO: Add real benchmarking functionality for each of these to
	// benchmarking.rs, then uncomment here and in trait definition.
	/*fn request_revenue_info_at() -> Weight {
		Weight::MAX
	}
	fn credit_account() -> Weight {
		Weight::MAX
	}*/
	fn assign_core(_s: u32) -> Weight {
		Weight::MAX
	}
}

/// Broker pallet index on the coretime chain. Used to
///
/// construct remote calls. The codec index must correspond to the index of `Broker` in the
/// `construct_runtime` of the coretime chain.
#[derive(Encode, Decode)]
enum BrokerRuntimePallets {
	#[codec(index = 50)]
	Broker(CoretimeCalls),
}

/// Call encoding for the calls needed from the Broker pallet.
#[derive(Encode, Decode)]
enum CoretimeCalls {
	#[codec(index = 1)]
	Reserve(pallet_broker::Schedule),
	#[codec(index = 3)]
	SetLease(pallet_broker::TaskId, pallet_broker::Timeslice),
	#[codec(index = 19)]
	NotifyCoreCount(u16),
}

#[frame_support::pallet]
pub mod pallet {
	use crate::configuration;

	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + assigner_coretime::Config {
		type RuntimeOrigin: From<<Self as frame_system::Config>::RuntimeOrigin>
			+ Into<result::Result<Origin, <Self as Config>::RuntimeOrigin>>;
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The runtime's definition of a Currency.
		type Currency: Currency<Self::AccountId>;
		/// The ParaId of the broker system parachain.
		#[pallet::constant]
		type BrokerId: Get<u32>;
		/// Something that provides the weight of this pallet.
		type WeightInfo: WeightInfo;
		type SendXcm: SendXcm;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The broker chain has asked for revenue information for a specific block.
		RevenueInfoRequested { when: BlockNumberFor<T> },
		/// A core has received a new assignment from the broker chain.
		CoreAssigned { core: CoreIndex },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The paraid making the call is not the coretime brokerage system parachain.
		NotBroker,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(<T as Config>::WeightInfo::request_core_count())]
		#[pallet::call_index(1)]
		pub fn request_core_count(origin: OriginFor<T>, count: u16) -> DispatchResult {
			// Ignore requests not coming from the broker parachain or root.
			Self::ensure_root_or_para(origin, <T as Config>::BrokerId::get().into())?;

			configuration::Pallet::<T>::set_coretime_cores_unchecked(u32::from(count))
		}

		//// TODO Impl me!
		////#[pallet::weight(<T as Config>::WeightInfo::request_revenue_info_at())]
		//#[pallet::call_index(2)]
		//pub fn request_revenue_info_at(
		//	origin: OriginFor<T>,
		//	_when: BlockNumberFor<T>,
		//) -> DispatchResult {
		//	// Ignore requests not coming from the broker parachain or root.
		//	Self::ensure_root_or_para(origin, <T as Config>::BrokerId::get().into())?;
		//	Ok(())
		//}

		//// TODO Impl me!
		////#[pallet::weight(<T as Config>::WeightInfo::credit_account())]
		//#[pallet::call_index(3)]
		//pub fn credit_account(
		//	origin: OriginFor<T>,
		//	_who: T::AccountId,
		//	_amount: BalanceOf<T>,
		//) -> DispatchResult {
		//	// Ignore requests not coming from the broker parachain or root.
		//	Self::ensure_root_or_para(origin, <T as Config>::BrokerId::get().into())?;
		//	Ok(())
		//}

		/// Receive instructions from the `ExternalBrokerOrigin`, detailing how a specific core is
		/// to be used.
		///
		/// Parameters:
		/// -`origin`: The `ExternalBrokerOrigin`, assumed to be the Broker system parachain.
		/// -`core`: The core that should be scheduled.
		/// -`begin`: The starting blockheight of the instruction.
		/// -`assignment`: How the blockspace should be utilised.
		/// -`end_hint`: An optional hint as to when this particular set of instructions will end.
		// The broker pallet's `CoreIndex` definition is `u16` but on the relay chain it's `struct
		// CoreIndex(u32)`
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::assign_core(assignment.len() as u32))]
		pub fn assign_core(
			origin: OriginFor<T>,
			core: BrokerCoreIndex,
			begin: BlockNumberFor<T>,
			assignment: Vec<(CoreAssignment, PartsOf57600)>,
			end_hint: Option<BlockNumberFor<T>>,
		) -> DispatchResult {
			// Ignore requests not coming from the broker parachain or root.
			Self::ensure_root_or_para(origin, T::BrokerId::get().into())?;

			let core = u32::from(core).into();

			<assigner_coretime::Pallet<T>>::assign_core(core, begin, assignment, end_hint)?;
			Self::deposit_event(Event::<T>::CoreAssigned { core });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Ensure the origin is one of Root or the `para` itself.
	fn ensure_root_or_para(
		origin: <T as frame_system::Config>::RuntimeOrigin,
		id: ParaId,
	) -> DispatchResult {
		if let Ok(caller_id) = ensure_parachain(<T as Config>::RuntimeOrigin::from(origin.clone()))
		{
			// Check if matching para id...
			ensure!(caller_id == id, Error::<T>::NotBroker);
		} else {
			// Check if root...
			ensure_root(origin.clone())?;
		}
		Ok(())
	}

	pub fn initializer_on_new_session(notification: &SessionChangeNotification<BlockNumberFor<T>>) {
		let old_core_count = notification.prev_config.coretime_cores;
		let new_core_count = notification.new_config.coretime_cores;
		if new_core_count != old_core_count {
			let core_count: u16 = new_core_count.saturated_into();
			let message = Xcm(vec![
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				mk_coretime_call(crate::coretime::CoretimeCalls::NotifyCoreCount(core_count)),
			]);
			if let Err(err) = send_xcm::<T::SendXcm>(
				Location::new(0, [Junction::Parachain(T::BrokerId::get())]),
				message,
			) {
				log::error!("Sending `NotifyCoreCount` to coretime chain failed: {:?}", err);
			}
		}
	}
}

impl<T: Config> OnNewSession<BlockNumberFor<T>> for Pallet<T> {
	fn on_new_session(notification: &SessionChangeNotification<BlockNumberFor<T>>) {
		Self::initializer_on_new_session(notification);
	}
}

fn mk_coretime_call(call: crate::coretime::CoretimeCalls) -> Instruction<()> {
	Instruction::Transact {
		origin_kind: OriginKind::Superuser,
		// Largest call is set_lease with 1526 byte:
		// Longest call is reserve() with 31_000_000
		require_weight_at_most: Weight::from_parts(170_000_000, 20_000),
		call: BrokerRuntimePallets::Broker(call).encode().into(),
	}
}
