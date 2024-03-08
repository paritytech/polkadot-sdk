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
	use frame_support::{dispatch::DispatchResult, pallet_prelude::*, traits::StorageVersion};
	use frame_system::pallet_prelude::*;
	use sp_std::boxed::Box;
	use sygma_traits::{DomainID, FeeHandler};
	use xcm::latest::{AssetId, Fungibility::Fungible, MultiAsset};

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	/// Mapping fungible asset id with domain id to fee rate and its lower bound, upperbound
	#[pallet::storage]
	pub type AssetFeeRate<T: Config> =
		StorageMap<_, Twox64Concat, (DomainID, AssetId), (u32, u128, u128)>;

	pub trait WeightInfo {
		fn set_fee_rate() -> Weight;
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + sygma_access_segregator::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Current pallet index defined in runtime
		type PalletIndex: Get<u8>;

		/// Type representing the weight of this pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Fee set rate for a specific asset and domain
		/// args: [domain, asset, rate_basis_point, fee_lower_bound, fee_upper_bound]
		FeeRateSet {
			domain: DomainID,
			asset: AssetId,
			rate_basis_point: u32,
			fee_lower_bound: u128,
			fee_upper_bound: u128,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Function unimplemented
		Unimplemented,
		/// Account has not gained access permission
		AccessDenied,
		/// Fee rate is out of range [0, 10000)
		FeeRateOutOfRange,

		/// Percentage fee bound is invalid
		InvalidFeeBound,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set bridge fee rate for a specific asset and domain. Note the fee rate is in Basis Point
		/// representation, and the valid fee rate is [0, 10000)
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_fee_rate())]
		pub fn set_fee_rate(
			origin: OriginFor<T>,
			domain: DomainID,
			asset: Box<AssetId>,
			fee_rate_basis_point: u32,
			fee_lower_bound: u128,
			fee_upper_bound: u128,
		) -> DispatchResult {
			let asset: AssetId = *asset;
			ensure!(
				<sygma_access_segregator::pallet::Pallet<T>>::has_access(
					<T as Config>::PalletIndex::get(),
					b"set_fee_rate".to_vec(),
					origin
				),
				Error::<T>::AccessDenied
			);

			// Make sure fee rate is valid
			ensure!(fee_rate_basis_point < 10_000u32, Error::<T>::FeeRateOutOfRange);

			// Make sure fee bound is valid
			ensure!(fee_lower_bound < fee_upper_bound, Error::<T>::InvalidFeeBound);

			// Update asset fee rate with fee bound
			AssetFeeRate::<T>::insert(
				(domain, &asset),
				(fee_rate_basis_point, fee_lower_bound, fee_upper_bound),
			);

			// Emit FeeRateSet event
			Self::deposit_event(Event::FeeRateSet {
				domain,
				asset,
				rate_basis_point: fee_rate_basis_point,
				fee_lower_bound,
				fee_upper_bound,
			});
			Ok(())
		}
	}

	impl<T: Config> FeeHandler for Pallet<T> {
		fn get_fee(domain: DomainID, asset: MultiAsset) -> Option<u128> {
			match (asset.fun, asset.id) {
				(Fungible(amount), _) => {
					let (fee_rate_basis_point, fee_lower_bound, fee_upper_bound) =
						AssetFeeRate::<T>::get((domain, asset.id))?;
					let fee_amount =
						amount.saturating_mul(fee_rate_basis_point as u128).saturating_div(10000);

					if fee_amount > fee_upper_bound {
						return Some(fee_upper_bound);
					} else if fee_amount < fee_lower_bound {
						return Some(fee_lower_bound);
					}
					Some(fee_amount)
				},
				_ => None,
			}
		}
	}

	#[cfg(test)]
	mod test {
		use crate as percentage_fee_handler;
		use crate::{AssetFeeRate, Event as PercentageFeeHandlerEvent};
		use frame_support::{assert_noop, assert_ok};
		use percentage_fee_handler::mock::{
			assert_events, new_test_ext, AccessSegregator, PercentageFeeHandler,
			PercentageFeeHandlerPalletIndex, RuntimeEvent as Event, RuntimeOrigin as Origin, Test,
			ALICE,
		};
		use sp_std::boxed::Box;
		use sygma_traits::DomainID;
		use xcm::latest::{prelude::*, MultiLocation};

		#[test]
		fn set_get_fee() {
			new_test_ext().execute_with(|| {
				let dest_domain_id: DomainID = 0;
				let another_dest_domain_id: DomainID = 1;
				let asset_id_a = Concrete(MultiLocation::new(1, Here));
				let asset_id_b = Concrete(MultiLocation::new(2, Here));
				let asset_a_deposit: MultiAsset = (asset_id_a, 100u128).into();
				let asset_b_deposit: MultiAsset = (asset_id_b, 200u128).into();

				// if not set fee rate, return None
				assert_eq!(AssetFeeRate::<Test>::get((dest_domain_id, asset_id_a)), None);

				// test the min fee rate case: 0u32 => 0%, should work
				// set fee rate as 0 basis point aka 0% with assetId asset_id_a for one domain
				assert_ok!(PercentageFeeHandler::set_fee_rate(
					Origin::root(),
					dest_domain_id,
					Box::new(asset_id_a),
					0u32,
					0u128,
					100u128
				));

				// test the max fee rate case: 10000 => 100%, should raise FeeRateOutOfRange error
				assert_noop!(
					PercentageFeeHandler::set_fee_rate(
						Origin::root(),
						dest_domain_id,
						Box::new(asset_id_a),
						10_000u32,
						0u128,
						100u128
					),
					percentage_fee_handler::Error::<Test>::FeeRateOutOfRange
				);

				// set fee rate as 50 basis point aka 0.5% with assetId asset_id_a for one domain
				assert_ok!(PercentageFeeHandler::set_fee_rate(
					Origin::root(),
					dest_domain_id,
					Box::new(asset_id_a),
					50u32,
					0u128,
					100u128
				));
				// set fee rate as 200 basis point aka 2% with assetId asset_id_a for another domain
				assert_ok!(PercentageFeeHandler::set_fee_rate(
					Origin::root(),
					another_dest_domain_id,
					Box::new(asset_id_b),
					200u32,
					0u128,
					100u128
				));

				assert_eq!(
					AssetFeeRate::<Test>::get((dest_domain_id, asset_id_a)).unwrap(),
					(50, 0, 100)
				);
				assert_eq!(
					AssetFeeRate::<Test>::get((another_dest_domain_id, asset_id_b)).unwrap(),
					(200, 0, 100)
				);

				// permission test: unauthorized account should not be able to set fee
				let unauthorized_account = Origin::from(Some(ALICE));
				assert_noop!(
					PercentageFeeHandler::set_fee_rate(
						unauthorized_account,
						dest_domain_id,
						Box::new(asset_id_a),
						100u32,
						0u128,
						100u128
					),
					percentage_fee_handler::Error::<Test>::AccessDenied
				);

				assert_events(vec![
					Event::PercentageFeeHandler(PercentageFeeHandlerEvent::FeeRateSet {
						domain: dest_domain_id,
						asset: asset_id_a,
						rate_basis_point: 50u32,
						fee_lower_bound: 0u128,
						fee_upper_bound: 100u128,
					}),
					Event::PercentageFeeHandler(PercentageFeeHandlerEvent::FeeRateSet {
						domain: another_dest_domain_id,
						asset: asset_id_b,
						rate_basis_point: 200u32,
						fee_lower_bound: 0u128,
						fee_upper_bound: 100u128,
					}),
				]);
			})
		}

		#[test]
		fn access_control() {
			new_test_ext().execute_with(|| {
				let dest_domain_id: DomainID = 0;
				let asset_id = Concrete(MultiLocation::new(0, Here));

				assert_ok!(PercentageFeeHandler::set_fee_rate(
					Origin::root(),
					dest_domain_id,
					Box::new(asset_id),
					100u32,
					0u128,
					100u128
				),);
				assert_noop!(
					PercentageFeeHandler::set_fee_rate(
						Some(ALICE).into(),
						dest_domain_id,
						Box::new(asset_id),
						200u32,
						0u128,
						100u128
					),
					percentage_fee_handler::Error::<Test>::AccessDenied
				);
				assert!(!AccessSegregator::has_access(
					PercentageFeeHandlerPalletIndex::get(),
					b"set_fee_rate".to_vec(),
					Some(ALICE).into()
				));
				assert_ok!(AccessSegregator::grant_access(
					Origin::root(),
					PercentageFeeHandlerPalletIndex::get(),
					b"set_fee_rate".to_vec(),
					ALICE
				));
				assert!(AccessSegregator::has_access(
					PercentageFeeHandlerPalletIndex::get(),
					b"set_fee_rate".to_vec(),
					Some(ALICE).into()
				));
				assert_ok!(PercentageFeeHandler::set_fee_rate(
					Some(ALICE).into(),
					dest_domain_id,
					Box::new(asset_id),
					200u32,
					0u128,
					100u128
				),);
				assert_eq!(
					AssetFeeRate::<Test>::get((dest_domain_id, asset_id)).unwrap(),
					(200u32, 0u128, 100u128)
				);
			})
		}
	}
}
