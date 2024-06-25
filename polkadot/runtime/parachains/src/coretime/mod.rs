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

use frame_support::{
	pallet_prelude::*,
	traits::{defensive_prelude::*, Currency},
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use pallet_broker::{CoreAssignment, CoreIndex as BrokerCoreIndex};
use polkadot_primitives::{Balance, BlockNumber, CoreIndex, Id as ParaId};
use sp_arithmetic::traits::SaturatedConversion;
use sp_runtime::traits::TryConvert;
use sp_std::{prelude::*, result};
use xcm::{
	prelude::{send_xcm, Instruction, Junction, Location, OriginKind, SendXcm, WeightLimit, Xcm},
	v4::{
		Asset,
		AssetFilter::Wild,
		AssetId, Assets, Error as XcmError,
		Fungibility::Fungible,
		Instruction::{DepositAsset, ReceiveTeleportedAsset},
		Junctions::Here,
		Reanchorable,
		WildAsset::AllCounted,
		XcmContext,
	},
};
use xcm_executor::traits::TransactAsset;

use crate::{
	assigner_coretime::{self, PartsOf57600},
	assigner_on_demand,
	initializer::{OnNewSession, SessionChangeNotification},
	origin::{ensure_parachain, Origin},
};

mod benchmarking;
pub mod migration;

const LOG_TARGET: &str = "runtime::parachains::coretime";

pub trait WeightInfo {
	fn request_core_count() -> Weight;
	fn request_revenue_at() -> Weight;
	//fn credit_account() -> Weight;
	fn assign_core(s: u32) -> Weight;
}

/// A weight info that is only suitable for testing.
pub struct TestWeightInfo;

impl WeightInfo for TestWeightInfo {
	fn request_core_count() -> Weight {
		Weight::MAX
	}
	fn request_revenue_at() -> Weight {
		Weight::MAX
	}
	// TODO: Add real benchmarking functionality for each of these to
	// benchmarking.rs, then uncomment here and in trait definition.
	//fn credit_account() -> Weight {
	//	Weight::MAX
	//}
	fn assign_core(_s: u32) -> Weight {
		Weight::MAX
	}
}

/// Shorthand for the Balance type the runtime is using.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

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
	#[codec(index = 20)]
	NotifyRevenue((BlockNumber, Balance)),
	#[codec(index = 99)]
	SwapLeases(ParaId, ParaId),
}

#[frame_support::pallet]
pub mod pallet {
	use crate::configuration;
	use sp_runtime::traits::TryConvert;
	use xcm::v4::InteriorLocation;
	use xcm_executor::traits::TransactAsset;

	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config + assigner_coretime::Config + assigner_on_demand::Config
	{
		type RuntimeOrigin: From<<Self as frame_system::Config>::RuntimeOrigin>
			+ Into<result::Result<Origin, <Self as Config>::RuntimeOrigin>>;
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The runtime's definition of a Currency.
		type Currency: Currency<Self::AccountId>;
		/// The ParaId of the coretime chain.
		#[pallet::constant]
		type BrokerId: Get<u32>;
		/// The coretime chain pot location.
		#[pallet::constant]
		type BrokerPotLocation: Get<InteriorLocation>;
		/// Something that provides the weight of this pallet.
		type WeightInfo: WeightInfo;
		/// The XCM sender.
		type SendXcm: SendXcm;
		/// The asset transactor.
		type AssetTransactor: TransactAsset;
		/// AccountId to Location converter
		type AccountToLocation: for<'a> TryConvert<&'a Self::AccountId, Location>;

		/// Maximum weight for any XCM transact call that should be executed on the coretime chain.
		///
		/// Basically should be `max_weight(set_leases, reserve, notify_core_count)`.
		type MaxXcmTransactWeight: Get<Weight>;
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
		/// Requested revenue information `when` parameter was in the future from the current
		/// block height.
		RequestedFutureRevenue,
		/// Failed to transfer assets to the coretime chain
		AssetTransferFailed,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Request the configuration to be updated with the specified number of cores. Warning:
		/// Since this only schedules a configuration update, it takes two sessions to come into
		/// effect.
		///
		/// - `origin`: Root or the Coretime Chain
		/// - `count`: total number of cores
		#[pallet::weight(<T as Config>::WeightInfo::request_core_count())]
		#[pallet::call_index(1)]
		pub fn request_core_count(origin: OriginFor<T>, count: u16) -> DispatchResult {
			// Ignore requests not coming from the coretime chain or root.
			Self::ensure_root_or_para(origin, <T as Config>::BrokerId::get().into())?;

			configuration::Pallet::<T>::set_coretime_cores_unchecked(u32::from(count))
		}

		/// Request to claim the instantaneous coretime sales revenue starting from the block it was
		/// last claimed until and up to the block specified. The claimed amount value is sent back
		/// to the Coretime chain in a `notify_revenue` message. At the same time, the amount is
		/// teleported to the Coretime chain.
		#[pallet::weight(<T as Config>::WeightInfo::request_revenue_at())]
		#[pallet::call_index(2)]
		pub fn request_revenue_at(origin: OriginFor<T>, when: BlockNumber) -> DispatchResult {
			// Ignore requests not coming from the Coretime Chain or Root.
			Self::ensure_root_or_para(origin, <T as Config>::BrokerId::get().into())?;
			Self::notify_revenue(when)
		}

		//// TODO Impl me!
		////#[pallet::weight(<T as Config>::WeightInfo::credit_account())]
		//#[pallet::call_index(3)]
		//pub fn credit_account(
		//	origin: OriginFor<T>,
		//	_who: T::AccountId,
		//	_amount: BalanceOf<T>,
		//) -> DispatchResult {
		//	// Ignore requests not coming from the coretime chain or root.
		//	Self::ensure_root_or_para(origin, <T as Config>::BrokerId::get().into())?;
		//	Ok(())
		//}

		/// Receive instructions from the `ExternalBrokerOrigin`, detailing how a specific core is
		/// to be used.
		///
		/// Parameters:
		/// -`origin`: The `ExternalBrokerOrigin`, assumed to be the coretime chain.
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
			// Ignore requests not coming from the coretime chain or root.
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
		let old_core_count = notification.prev_config.scheduler_params.num_cores;
		let new_core_count = notification.new_config.scheduler_params.num_cores;
		if new_core_count != old_core_count {
			let core_count: u16 = new_core_count.saturated_into();
			let message = Xcm(vec![
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				mk_coretime_call::<T>(crate::coretime::CoretimeCalls::NotifyCoreCount(core_count)),
			]);
			if let Err(err) = send_xcm::<T::SendXcm>(
				Location::new(0, [Junction::Parachain(T::BrokerId::get())]),
				message,
			) {
				log::error!(target: LOG_TARGET, "Sending `NotifyCoreCount` to coretime chain failed: {:?}", err);
			}
		}
	}

	/// Provide the amount of revenue accumulated from Instantaneous Coretime Sales from Relay-chain
	/// block number last_until to until, not including until itself. last_until is defined as being
	/// the until argument of the last notify_revenue message sent, or zero for the first call. If
	/// revenue is None, this indicates that the information is no longer available. This explicitly
	/// disregards the possibility of multiple parachains requesting and being notified of revenue
	/// information.
	///
	/// The Relay-chain must be configured to ensure that only a single revenue information
	/// destination exists.
	pub fn notify_revenue(until: BlockNumber) -> DispatchResult {
		let now = <frame_system::Pallet<T>>::block_number();
		let until_bnf: BlockNumberFor<T> = until.into();

		// When cannot be in the future.
		ensure!(until_bnf <= now, Error::<T>::RequestedFutureRevenue);

		let amount = <assigner_on_demand::Pallet<T>>::claim_revenue_until(until_bnf);
		log::debug!(target: LOG_TARGET, "Revenue info requested: {:?}", amount);

		let raw_revenue: Balance = amount.try_into().map_err(|_| {
			log::error!(target: LOG_TARGET, "Converting on demand revenue for `NotifyRevenue` failed");
			Error::<T>::AssetTransferFailed
		})?;

		do_notify_revenue::<T>(until, raw_revenue).map_err(|err| {
			log::error!(target: LOG_TARGET, "notify_revenue failed: {err:?}");
			Error::<T>::AssetTransferFailed
		})?;

		Ok(())
	}

	// Handle legacy swaps in coretime. Notifies coretime chain that a lease swap has occurred via
	// XCM message. This function is meant to be used in an implementation of `OnSwap` trait.
	pub fn on_legacy_lease_swap(one: ParaId, other: ParaId) {
		let message = Xcm(vec![
			Instruction::UnpaidExecution {
				weight_limit: WeightLimit::Unlimited,
				check_origin: None,
			},
			mk_coretime_call::<T>(crate::coretime::CoretimeCalls::SwapLeases(one, other)),
		]);
		if let Err(err) = send_xcm::<T::SendXcm>(
			Location::new(0, [Junction::Parachain(T::BrokerId::get())]),
			message,
		) {
			log::error!(target: LOG_TARGET, "Sending `SwapLeases` to coretime chain failed: {:?}", err);
		}
	}
}

impl<T: Config> OnNewSession<BlockNumberFor<T>> for Pallet<T> {
	fn on_new_session(notification: &SessionChangeNotification<BlockNumberFor<T>>) {
		Self::initializer_on_new_session(notification);
	}
}

fn mk_coretime_call<T: Config>(call: crate::coretime::CoretimeCalls) -> Instruction<()> {
	Instruction::Transact {
		origin_kind: OriginKind::Superuser,
		require_weight_at_most: T::MaxXcmTransactWeight::get(),
		call: BrokerRuntimePallets::Broker(call).encode().into(),
	}
}

fn do_notify_revenue<T: Config>(when: BlockNumber, raw_revenue: Balance) -> Result<(), XcmError> {
	let dest = Junction::Parachain(T::BrokerId::get()).into_location();
	let mut message = Vec::new();
	let asset = Asset { id: AssetId(Location::here()), fun: Fungible(raw_revenue) };
	let dummy_xcm_context = XcmContext { origin: None, message_id: [0; 32], topic: None };

	if raw_revenue > 0 {
		let on_demand_pot =
			T::AccountToLocation::try_convert(&<assigner_on_demand::Pallet<T>>::account_id())
				.map_err(|err| {
					log::error!(
						target: LOG_TARGET,
						"Failed to convert on-demand pot account to XCM location: {err:?}",
					);
					XcmError::InvalidLocation
				})?;

		let withdrawn = T::AssetTransactor::withdraw_asset(&asset, &on_demand_pot, None)?;

		T::AssetTransactor::can_check_out(&dest, &asset, &dummy_xcm_context)?;

		let assets_reanchored = Into::<Assets>::into(withdrawn)
			.reanchored(&dest, &Here.into())
			.defensive_map_err(|_| XcmError::ReanchorFailed)?;

		message.extend(
			[
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				ReceiveTeleportedAsset(assets_reanchored),
				DepositAsset {
					assets: Wild(AllCounted(1)),
					beneficiary: T::BrokerPotLocation::get().into_location(),
				},
			]
			.into_iter(),
		);
	}

	message.push(mk_coretime_call::<T>(CoretimeCalls::NotifyRevenue((when, raw_revenue))));

	send_xcm::<T::SendXcm>(dest.clone(), Xcm(message))?;

	if raw_revenue > 0 {
		T::AssetTransactor::check_out(&dest, &asset, &dummy_xcm_context);
	}

	Ok(())
}
