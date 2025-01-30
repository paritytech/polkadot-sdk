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

pub mod block_weights;
pub mod cumulus_pallet_parachain_system;
pub mod cumulus_pallet_weight_reclaim;
pub mod cumulus_pallet_xcmp_queue;
pub mod extrinsic_weights;
pub mod frame_system;
pub mod frame_system_extensions;
pub mod pallet_asset_conversion;
pub mod pallet_asset_conversion_ops;
pub mod pallet_asset_conversion_tx_payment;
pub mod pallet_asset_rewards;
pub mod pallet_assets_foreign;
pub mod pallet_assets_local;
pub mod pallet_assets_pool;
pub mod pallet_balances;
pub mod pallet_bridge_messages;
pub mod pallet_bridge_relayers;
pub mod pallet_collator_selection;
pub mod pallet_message_queue;
pub mod pallet_multisig;
pub mod pallet_nft_fractionalization;
pub mod pallet_nfts;
pub mod pallet_proxy;
pub mod pallet_session;
pub mod pallet_timestamp;
pub mod pallet_transaction_payment;
pub mod pallet_uniques;
pub mod pallet_utility;
pub mod pallet_xcm;
pub mod pallet_xcm_bridge;
pub mod pallet_xcm_bridge_router_to_westend_over_asset_hub_westend;
pub mod pallet_xcm_bridge_router_to_westend_over_bridge_hub;
pub mod paritydb_weights;
pub mod rocksdb_weights;
pub mod xcm;

pub use block_weights::constants::BlockExecutionWeight;
pub use extrinsic_weights::constants::ExtrinsicBaseWeight;
pub use rocksdb_weights::constants::RocksDbWeight;

use ::pallet_bridge_messages::WeightInfoExt as MessagesWeightInfoExt;
use ::pallet_bridge_relayers::WeightInfoExt as _;

use crate::{Runtime, Weight};

impl MessagesWeightInfoExt for pallet_bridge_messages::WeightInfo<Runtime> {
	fn expected_extra_storage_proof_size() -> u32 {
		bp_bridge_hub_westend::EXTRA_STORAGE_PROOF_SIZE
	}

	fn receive_messages_proof_overhead_from_runtime() -> Weight {
		pallet_bridge_relayers::WeightInfo::<Runtime>::receive_messages_proof_overhead_from_runtime(
		)
	}

	fn receive_messages_delivery_proof_overhead_from_runtime() -> Weight {
		pallet_bridge_relayers::WeightInfo::<Runtime>::receive_messages_delivery_proof_overhead_from_runtime()
	}
}
