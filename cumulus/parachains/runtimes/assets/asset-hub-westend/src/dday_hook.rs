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
//! - The total issuance at the current block
//!
//! TODO: FAIL-CI - decision pending
//! Note: This hook and the corresponding XCM would not be necessary if:
//! - A mechanism is implemented to read the custom `RelayChainStateProof` context, allowing us to retrieve the latest AssetHub head directly on Collectives.
//!   - See: https://github.com/paritytech/polkadot-sdk/issues/7445
//!   - See: https://github.com/paritytech/polkadot-sdk/issues/82
//! - The `dday-voting` system is modified from using conviction-based voting to an alternative mechanism that does not require the total issuance of AssetHub.
//!   - Alternatively, we allow the total issuance on Collectives to be updated manually by fellows with rank 3+ when initiating referenda, e.g., via a custom extrinsic.

use crate::{Balance, Balances, PolkadotXcm, Runtime};
use alloc::{vec, vec::Vec};
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
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// Return some weight consumed by `on_finalize`.
			T::DbWeight::get().reads_writes(2, 1).saturating_add(
				<<Runtime as pallet_xcm::Config>::WeightInfo as pallet_xcm::WeightInfo>::send(),
			)
		}

		fn on_finalize(n: BlockNumberFor<T>) {
			// Prepare total issuance call
			let mut data = vec![CollectivesPallets::<T>::prepare_total_issuance_call(
				n,
				Balances::total_issuance(),
			)];
			// Prepare header call
			if let Some((hn, head)) = HeaderToSend::<T>::take() {
				data.push(CollectivesPallets::<T>::prepare_head_call(hn, head));
			}

			// Send data
			Self::send_data(data)
		}
	}

	impl<T: Config> Pallet<T> {
		fn send_data(calls: Vec<CollectivesPallets<T>>) {
			let mut xcm = Xcm::builder_unpaid().unpaid_execution(Unlimited, None);
			for call in &calls {
				xcm = xcm.transact(OriginKind::Xcm, None, call.encode());
			}

			match PolkadotXcm::send_xcm(
				Here,
				Location::new(1, [Parachain(COLLECTIVES_ID)]),
				xcm.build(),
			) {
				Ok(message_id) => {
					log::trace!(
						target: LOG_TARGET,
						"DDay data: {:?} successfully sent with message_id: {:?}!",
						calls.encode(), message_id
					)
				},
				Err(e) => {
					log::warn!(
						target: LOG_TARGET,
						"DDay data: {:?} was not sent, error: {:?}!",
						calls.encode(), e
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
	fn prepare_total_issuance_call(
		block_number: BlockNumberFor<T>,
		value: Balance,
	) -> CollectivesPallets<T> {
		CollectivesPallets::DDayDetection(DDayDetectionCall::Submit {
			key: ProofsKey::AssetHubTotalIssuance(block_number),
			value: ProofsValue::AssetHubTotalIssuance(value),
		})
	}

	fn prepare_head_call(
		block_number: BlockNumberFor<T>,
		value: HeadData,
	) -> CollectivesPallets<T> {
		CollectivesPallets::DDayDetection(DDayDetectionCall::Submit {
			key: ProofsKey::AssetHubHeader(block_number),
			value: ProofsValue::AssetHubHeader(value.0),
		})
	}
}

#[derive(Encode, Decode, DecodeWithMemTracking, Debug)]
enum DDayDetectionCall<T: Config> {
	#[codec(index = 0)]
	Submit { key: ProofsKey<T>, value: ProofsValue },
}

#[derive(Encode, Decode, DecodeWithMemTracking, Debug)]
enum ProofsKey<T: Config> {
	/// AssetHub header key (AssetHub's block number).
	AssetHubHeader(BlockNumberFor<T>),
	/// AssetHub total issuance key.
	AssetHubTotalIssuance(BlockNumberFor<T>),
}

#[derive(Encode, Decode, DecodeWithMemTracking, Debug)]
enum ProofsValue {
	/// AssetHub encoded header from `HeadData`.
	AssetHubHeader(Vec<u8>),
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
