// Copyright (C) Parity Technologies (UK) Ltd.
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

//! This hook pallet is used to send data to the Collectives.
//! It sends:
//! - The parent block
//!
//! Note: This hook and the corresponding XCM would not be necessary if:
//! - A mechanism is implemented to read the custom `RelayChainStateProof` context, allowing us to retrieve the latest AssetHub head directly on Collectives.
//!   - See: https://github.com/paritytech/polkadot-sdk/issues/7445
//!   - See: https://github.com/paritytech/polkadot-sdk/issues/82

use crate::{Balance, PolkadotXcm, Runtime};
use codec::{Decode, DecodeWithMemTracking, Encode};
use cumulus_pallet_parachain_system::OnSystemEvent;
use cumulus_primitives_core::PersistedValidationData;
use frame_system::pallet_prelude::BlockNumberFor;
use polkadot_parachain_primitives::primitives::HeadData;
use sp_runtime::{traits::One, Saturating};
use westend_runtime_constants::system_parachain::COLLECTIVES_ID;

pub use pallet::*;

const LOG_TARGET: &str = "runtime::dday::hook";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_support::weights::WeightMeter;
	use xcm::latest::prelude::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// The pallet configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::storage]
	#[pallet::whitelist_storage]
	pub type HeaderToSend<T: Config> = StorageValue<_, (BlockNumberFor<T>, HeadData)>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_idle(_n: BlockNumberFor<T>, limit: Weight) -> Weight {
			let mut meter = WeightMeter::with_limit(limit);
			if meter.try_consume(Self::on_idle_weight()).is_err() {
				log::debug!(
					target: LOG_TARGET,
					"Not enough weight for on_idle. {} < {}",
					Self::on_idle_weight(), limit
				);
				return meter.consumed();
			}

			// Send header
			if let Some((hn, head)) = HeaderToSend::<T>::take() {
				Self::send_data(CollectivesPallets::<T>::prepare_head_call(hn, head));
			}

			meter.consumed()
		}
	}

	impl<T: Config> Pallet<T> {
		/// The worst-case weight of [`Self::on_idle`].
		fn on_idle_weight() -> Weight {
			T::DbWeight::get().reads_writes(1, 1).saturating_add(
				<<Runtime as pallet_xcm::Config>::WeightInfo as pallet_xcm::WeightInfo>::send(),
			)
		}

		fn send_data(call: CollectivesPallets<T>) {
			match PolkadotXcm::send_xcm(
				Here,
				Location::new(1, [Parachain(COLLECTIVES_ID)]),
				Xcm::builder_unpaid()
					.unpaid_execution(Unlimited, None)
					.transact(OriginKind::Xcm, None, call.encode())
					.build(),
			) {
				Ok(message_id) => {
					log::trace!(
						target: LOG_TARGET,
						"DDay data: {:?} successfully sent with message_id: {:?}!",
						call.encode(), message_id
					)
				},
				Err(e) => {
					log::warn!(
						target: LOG_TARGET,
						"DDay data: {:?} was not sent, error: {:?}!",
						call.encode(), e
					)
				},
			}
		}
	}
}

/// A type containing the encoding of the DDay detection pallet in the Collectives chain runtime. Used to
/// construct any remote calls. The codec index must correspond to the index of `DDayDetection` in the
/// `construct_runtime` of the Collectives chain.
#[derive(Encode, Decode, DecodeWithMemTracking, Debug, scale_info::TypeInfo)]
#[scale_info(skip_type_params(T))]
enum CollectivesPallets<T: Config> {
	#[codec(index = 83)]
	DDayDetection(DDayDetectionCall<T>),
}
impl<T: Config> CollectivesPallets<T> {
	fn prepare_head_call(
		block_number: BlockNumberFor<T>,
		value: HeadData,
	) -> CollectivesPallets<T> {
		CollectivesPallets::DDayDetection(DDayDetectionCall::note_new_head {
			remote_block_number: block_number,
			remote_head: value,
		})
	}
}

#[derive(Encode, Decode, DecodeWithMemTracking, Debug)]
#[allow(non_camel_case_types)]
enum DDayDetectionCall<T: Config> {
	#[codec(index = 0)]
	note_new_head { remote_block_number: BlockNumberFor<T>, remote_head: HeadData },
}

#[derive(Encode, Decode, DecodeWithMemTracking, Debug)]
enum ProofsKey<T: Config> {
	/// AssetHub total issuance key.
	AssetHubTotalIssuance(BlockNumberFor<T>),
}

#[derive(Encode, Decode, DecodeWithMemTracking, Debug)]
enum ProofsValue {
	/// AssetHub total issuance balance.
	AssetHubTotalIssuance(Balance),
}

/// Implementation of `OnSystemEvent` callback where we can get the parent head.
impl<T: Config> OnSystemEvent for Pallet<T> {
	fn on_validation_data(data: &PersistedValidationData) {
		let parent_number = frame_system::Pallet::<T>::block_number().saturating_sub(One::one());
		HeaderToSend::<T>::set(Some((parent_number, data.parent_head.clone())));
	}

	fn on_validation_code_applied() {}
}
