// The Licensed Work is (c) 2022 Sygma
// SPDX-License-Identifier: LGPL-3.0-only

#![cfg_attr(not(feature = "std"), no_std)]

pub use self::pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::*;

#[cfg(test)]
mod mock;

#[allow(unused_variables)]
#[allow(clippy::large_enum_variant)]
#[frame_support::pallet]
pub mod pallet {
	use frame_support::{pallet_prelude::*, traits::StorageVersion};
	use frame_system::pallet_prelude::*;
	use sp_std::boxed::Box;
	use sygma_traits::{DomainID, FeeHandler};
	use xcm::latest::{AssetId, MultiAsset};

	#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug, MaxEncodedLen)]
	pub enum FeeHandlerType {
		BasicFeeHandler,
		PercentageFeeHandler,
		DynamicFeeHandler,
	}

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	pub trait WeightInfo {
		fn set_fee_handler() -> Weight;
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config + sygma_basic_feehandler::Config + sygma_percentage_feehandler::Config
	{
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Fee handlers
		type BasicFeeHandler: FeeHandler;
		type DynamicFeeHandler: FeeHandler;
		type PercentageFeeHandler: FeeHandler;

		/// Current pallet index defined in runtime
		type PalletIndex: Get<u8>;

		/// Type representing the weight of this pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// When fee handler was set for a specific (domain, asset) pair
		/// args: [dest_domain_id, asset_id, handler_type]
		FeeHandlerSet { domain: DomainID, asset: AssetId, handler_type: FeeHandlerType },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Account has not gained access permission
		AccessDenied,
		/// Function unimplemented
		Unimplemented,
	}

	/// Return the Fee handler type based on domainID and assetID
	#[pallet::storage]
	#[pallet::getter(fn handler_type)]
	pub type HandlerType<T: Config> =
		StorageMap<_, Twox64Concat, (DomainID, AssetId), FeeHandlerType>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set fee handler specific (domain, asset) pair
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_fee_handler())]
		pub fn set_fee_handler(
			origin: OriginFor<T>,
			domain: DomainID,
			asset: Box<AssetId>,
			handler_type: FeeHandlerType,
		) -> DispatchResult {
			let asset: AssetId = *asset;
			ensure!(
				<sygma_access_segregator::pallet::Pallet<T>>::has_access(
					<T as Config>::PalletIndex::get(),
					b"set_fee_handler".to_vec(),
					origin
				),
				Error::<T>::AccessDenied
			);

			// Update fee handler
			HandlerType::<T>::insert((domain, &asset), &handler_type);

			// Emit FeeSet event
			Self::deposit_event(Event::FeeHandlerSet { domain, asset, handler_type });
			Ok(())
		}
	}

	impl<T: Config> FeeHandler for Pallet<T> {
		fn get_fee(domain: DomainID, asset: MultiAsset) -> Option<u128> {
			if let Some(handler_type) = HandlerType::<T>::get((&domain, asset.id)) {
				match handler_type {
					FeeHandlerType::BasicFeeHandler => {
						sygma_basic_feehandler::Pallet::<T>::get_fee(domain, asset)
					},
					FeeHandlerType::PercentageFeeHandler => {
						sygma_percentage_feehandler::Pallet::<T>::get_fee(domain, asset)
					},
					FeeHandlerType::DynamicFeeHandler => {
						// TODO: Support dynamic fee handler
						None
					},
				}
			} else {
				None
			}
		}
	}

	#[cfg(test)]
	mod test {
		use super::*;
		use crate as fee_router;
		use fee_router::mock::{
			assert_events, new_test_ext, AccessSegregator, EthereumDomainID, FeeHandlerRouter,
			FeeHandlerRouterPalletIndex, MoonbeamDomainID, PhaLocation, RuntimeEvent,
			RuntimeOrigin as Origin, SygmaBasicFeeHandler, Test, ALICE,
		};
		use frame_support::{assert_noop, assert_ok};
		use sp_std::boxed::Box;
		use sygma_traits::FeeHandler;
		use xcm::latest::prelude::*;

		#[test]
		fn access_control() {
			new_test_ext().execute_with(|| {
				let asset_id = Concrete(PhaLocation::get());

				assert_ok!(FeeHandlerRouter::set_fee_handler(
					Origin::root(),
					EthereumDomainID::get(),
					Box::new(asset_id),
					FeeHandlerType::BasicFeeHandler,
				));
				assert_noop!(
					FeeHandlerRouter::set_fee_handler(
						Some(ALICE).into(),
						EthereumDomainID::get(),
						Box::new(asset_id),
						FeeHandlerType::BasicFeeHandler,
					),
					fee_router::Error::<Test>::AccessDenied
				);
				// (FeeHandlerRouterPalletIndex:get(), b"set_fee_handler") indicates extrinsic:
				// `set_fee` of this pallet
				assert!(!AccessSegregator::has_access(
					FeeHandlerRouterPalletIndex::get(),
					b"set_fee_handler".to_vec(),
					Some(ALICE).into()
				));
				assert_ok!(AccessSegregator::grant_access(
					Origin::root(),
					FeeHandlerRouterPalletIndex::get(),
					b"set_fee_handler".to_vec(),
					ALICE
				));
				assert!(AccessSegregator::has_access(
					FeeHandlerRouterPalletIndex::get(),
					b"set_fee_handler".to_vec(),
					Some(ALICE).into()
				));
				assert_ok!(FeeHandlerRouter::set_fee_handler(
					Some(ALICE).into(),
					MoonbeamDomainID::get(),
					Box::new(asset_id),
					FeeHandlerType::DynamicFeeHandler,
				),);
				assert_eq!(
					HandlerType::<Test>::get((MoonbeamDomainID::get(), asset_id)).unwrap(),
					FeeHandlerType::DynamicFeeHandler
				);
			})
		}

		#[test]
		fn fee_router_should_work() {
			new_test_ext().execute_with(|| {
				// config dest of (ethereum, PHA) use basic fee handler
				assert_ok!(FeeHandlerRouter::set_fee_handler(
					Origin::root(),
					EthereumDomainID::get(),
					Box::new(PhaLocation::get().into()),
					FeeHandlerType::BasicFeeHandler,
				));
				// config dest of (moonbeam, PHA) use dyncmic fee handler
				assert_ok!(FeeHandlerRouter::set_fee_handler(
					Origin::root(),
					MoonbeamDomainID::get(),
					Box::new(PhaLocation::get().into()),
					FeeHandlerType::DynamicFeeHandler,
				));

				// set fee 10000 with PHA for Ethereum
				assert_ok!(SygmaBasicFeeHandler::set_fee(
					Origin::root(),
					EthereumDomainID::get(),
					Box::new(PhaLocation::get().into()),
					10000
				));

				assert_eq!(
					FeeHandlerRouter::get_fee(
						EthereumDomainID::get(),
						(PhaLocation::get(), 10000u128).into()
					)
					.unwrap(),
					10000
				);
				// We don't support dynamic fee handler, return None
				assert_eq!(
					FeeHandlerRouter::get_fee(
						MoonbeamDomainID::get(),
						(PhaLocation::get(), 10000u128).into()
					),
					None
				);
				assert_events(vec![
					RuntimeEvent::FeeHandlerRouter(fee_router::Event::FeeHandlerSet {
						domain: EthereumDomainID::get(),
						asset: PhaLocation::get().into(),
						handler_type: FeeHandlerType::BasicFeeHandler,
					}),
					RuntimeEvent::FeeHandlerRouter(fee_router::Event::FeeHandlerSet {
						domain: MoonbeamDomainID::get(),
						asset: PhaLocation::get().into(),
						handler_type: FeeHandlerType::DynamicFeeHandler,
					}),
					RuntimeEvent::SygmaBasicFeeHandler(sygma_basic_feehandler::Event::FeeSet {
						domain: EthereumDomainID::get(),
						asset: PhaLocation::get().into(),
						amount: 10000,
					}),
				]);
			})
		}
	}
}
