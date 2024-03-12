// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Declaration of all bridges between Rococo Bulletin Chain and Rococo Bridge Hub.

use crate::cli::CliChain;

use bp_messages::MessageNonce;
use bp_runtime::{
	AccountIdOf, BalanceOf, BlockNumberOf, ChainId, HashOf, HasherOf, HeaderOf, NonceOf,
	SignatureOf,
};
use frame_support::pallet_prelude::Weight;
use relay_substrate_client::{
	Error as SubstrateError, SignParam, SimpleRuntimeVersion, UnsignedTransaction,
};
use sp_core::storage::StorageKey;
use std::time::Duration;

pub mod bridge_hub_rococo_messages_to_rococo_bulletin;
pub mod rococo_bulletin_headers_to_bridge_hub_rococo;
pub mod rococo_bulletin_messages_to_bridge_hub_rococo;
pub mod rococo_headers_to_rococo_bulletin;
pub mod rococo_parachains_to_rococo_bulletin;

/// Base `Chain` implementation of Rococo, pretending to be Polkadot.
pub struct RococoBaseAsPolkadot;

impl bp_runtime::Chain for RococoBaseAsPolkadot {
	const ID: ChainId = relay_rococo_client::Rococo::ID;

	type BlockNumber = BlockNumberOf<bp_rococo::Rococo>;
	type Hash = HashOf<bp_rococo::Rococo>;
	type Hasher = HasherOf<bp_rococo::Rococo>;
	type Header = HeaderOf<bp_rococo::Rococo>;

	type AccountId = AccountIdOf<bp_rococo::Rococo>;
	type Balance = BalanceOf<bp_rococo::Rococo>;
	type Nonce = NonceOf<bp_rococo::Rococo>;
	type Signature = SignatureOf<bp_rococo::Rococo>;

	fn max_extrinsic_size() -> u32 {
		bp_rococo::Rococo::max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		bp_rococo::Rococo::max_extrinsic_weight()
	}
}

impl bp_header_chain::ChainWithGrandpa for RococoBaseAsPolkadot {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str =
		bp_polkadot::Polkadot::WITH_CHAIN_GRANDPA_PALLET_NAME;
	const MAX_AUTHORITIES_COUNT: u32 = bp_rococo::Rococo::MAX_AUTHORITIES_COUNT;
	const REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY: u32 =
		bp_rococo::Rococo::REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY;
	const MAX_MANDATORY_HEADER_SIZE: u32 = bp_rococo::Rococo::MAX_MANDATORY_HEADER_SIZE;
	const AVERAGE_HEADER_SIZE: u32 = bp_rococo::Rococo::AVERAGE_HEADER_SIZE;
}

/// Relay `Chain` implementation of Rococo, pretending to be Polkadot.
#[derive(Debug, Clone, Copy)]
pub struct RococoAsPolkadot;

impl bp_runtime::UnderlyingChainProvider for RococoAsPolkadot {
	type Chain = RococoBaseAsPolkadot;
}

impl relay_substrate_client::Chain for RococoAsPolkadot {
	const NAME: &'static str = relay_rococo_client::Rococo::NAME;
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		relay_polkadot_client::Polkadot::BEST_FINALIZED_HEADER_ID_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration = relay_rococo_client::Rococo::AVERAGE_BLOCK_INTERVAL;

	type SignedBlock = <relay_rococo_client::Rococo as relay_substrate_client::Chain>::SignedBlock;
	type Call = <relay_rococo_client::Rococo as relay_substrate_client::Chain>::Call;
}

impl relay_substrate_client::ChainWithGrandpa for RococoAsPolkadot {
	const SYNCED_HEADERS_GRANDPA_INFO_METHOD: &'static str =
		relay_polkadot_client::Polkadot::SYNCED_HEADERS_GRANDPA_INFO_METHOD;

	type KeyOwnerProof =
		<relay_rococo_client::Rococo as relay_substrate_client::ChainWithGrandpa>::KeyOwnerProof;
}

impl relay_substrate_client::ChainWithBalances for RococoAsPolkadot {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		relay_rococo_client::Rococo::account_info_storage_key(account_id)
	}
}

impl relay_substrate_client::RelayChain for RococoAsPolkadot {
	const PARAS_PALLET_NAME: &'static str = relay_rococo_client::Rococo::PARAS_PALLET_NAME;
}

impl relay_substrate_client::ChainWithTransactions for RococoAsPolkadot {
	type AccountKeyPair = <relay_rococo_client::Rococo as relay_substrate_client::ChainWithTransactions>::AccountKeyPair;
	type SignedTransaction = <relay_rococo_client::Rococo as relay_substrate_client::ChainWithTransactions>::SignedTransaction;

	fn sign_transaction(
		param: SignParam<Self>,
		unsigned: UnsignedTransaction<Self>,
	) -> Result<Self::SignedTransaction, SubstrateError> {
		relay_rococo_client::Rococo::sign_transaction(
			SignParam {
				spec_version: param.spec_version,
				transaction_version: param.transaction_version,
				genesis_hash: param.genesis_hash,
				signer: param.signer,
			},
			unsigned.switch_chain(),
		)
	}
}

impl CliChain for RococoAsPolkadot {
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion> = None;
}

/// Base `Chain` implementation of Rococo Bridge Hub, pretending to be a Polkadot Bridge Hub.
pub struct BaseBridgeHubRococoAsBridgeHubPolkadot;

impl bp_runtime::Chain for BaseBridgeHubRococoAsBridgeHubPolkadot {
	const ID: ChainId = relay_bridge_hub_rococo_client::BridgeHubRococo::ID;

	type BlockNumber = BlockNumberOf<bp_bridge_hub_rococo::BridgeHubRococo>;
	type Hash = HashOf<bp_bridge_hub_rococo::BridgeHubRococo>;
	type Hasher = HasherOf<bp_bridge_hub_rococo::BridgeHubRococo>;
	type Header = HeaderOf<bp_bridge_hub_rococo::BridgeHubRococo>;

	type AccountId = AccountIdOf<bp_bridge_hub_rococo::BridgeHubRococo>;
	type Balance = BalanceOf<bp_bridge_hub_rococo::BridgeHubRococo>;
	type Nonce = NonceOf<bp_bridge_hub_rococo::BridgeHubRococo>;
	type Signature = SignatureOf<bp_bridge_hub_rococo::BridgeHubRococo>;

	fn max_extrinsic_size() -> u32 {
		bp_bridge_hub_rococo::BridgeHubRococo::max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		bp_bridge_hub_rococo::BridgeHubRococo::max_extrinsic_weight()
	}
}

impl bp_runtime::Parachain for BaseBridgeHubRococoAsBridgeHubPolkadot {
	const PARACHAIN_ID: u32 = bp_bridge_hub_rococo::BridgeHubRococo::PARACHAIN_ID;
}

impl bp_messages::ChainWithMessages for BaseBridgeHubRococoAsBridgeHubPolkadot {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str =
		relay_bridge_hub_polkadot_client::BridgeHubPolkadot::WITH_CHAIN_MESSAGES_PALLET_NAME;

	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce =
		relay_bridge_hub_rococo_client::BridgeHubRococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce =
		relay_bridge_hub_rococo_client::BridgeHubRococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
}

/// Relay `Chain` implementation of Rococo Bridge Hub, pretending to be a Polkadot Bridge Hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeHubRococoAsBridgeHubPolkadot;

impl bp_runtime::UnderlyingChainProvider for BridgeHubRococoAsBridgeHubPolkadot {
	type Chain = BaseBridgeHubRococoAsBridgeHubPolkadot;
}

impl relay_substrate_client::Chain for BridgeHubRococoAsBridgeHubPolkadot {
	const NAME: &'static str = relay_bridge_hub_rococo_client::BridgeHubRococo::NAME;
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		relay_bridge_hub_polkadot_client::BridgeHubPolkadot::BEST_FINALIZED_HEADER_ID_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration =
		relay_bridge_hub_rococo_client::BridgeHubRococo::AVERAGE_BLOCK_INTERVAL;

	type SignedBlock = <relay_bridge_hub_rococo_client::BridgeHubRococo as relay_substrate_client::Chain>::SignedBlock;
	type Call =
		<relay_bridge_hub_rococo_client::BridgeHubRococo as relay_substrate_client::Chain>::Call;
}

impl relay_substrate_client::ChainWithBalances for BridgeHubRococoAsBridgeHubPolkadot {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		relay_bridge_hub_rococo_client::BridgeHubRococo::account_info_storage_key(account_id)
	}
}

impl relay_substrate_client::ChainWithUtilityPallet for BridgeHubRococoAsBridgeHubPolkadot {
	type UtilityPallet = relay_substrate_client::MockedRuntimeUtilityPallet<
		relay_bridge_hub_rococo_client::RuntimeCall,
	>;
}

impl relay_substrate_client::ChainWithTransactions for BridgeHubRococoAsBridgeHubPolkadot {
	type AccountKeyPair = <relay_bridge_hub_rococo_client::BridgeHubRococo as relay_substrate_client::ChainWithTransactions>::AccountKeyPair;
	type SignedTransaction = <relay_bridge_hub_rococo_client::BridgeHubRococo as relay_substrate_client::ChainWithTransactions>::SignedTransaction;

	fn sign_transaction(
		param: SignParam<Self>,
		unsigned: UnsignedTransaction<Self>,
	) -> Result<Self::SignedTransaction, SubstrateError> {
		relay_bridge_hub_rococo_client::BridgeHubRococo::sign_transaction(
			SignParam {
				spec_version: param.spec_version,
				transaction_version: param.transaction_version,
				genesis_hash: param.genesis_hash,
				signer: param.signer,
			},
			unsigned.switch_chain(),
		)
	}
}

impl relay_substrate_client::ChainWithMessages for BridgeHubRococoAsBridgeHubPolkadot {
	const WITH_CHAIN_RELAYERS_PALLET_NAME: Option<&'static str> =
		relay_bridge_hub_polkadot_client::BridgeHubPolkadot::WITH_CHAIN_RELAYERS_PALLET_NAME;

	const TO_CHAIN_MESSAGE_DETAILS_METHOD: &'static str =
		relay_bridge_hub_polkadot_client::BridgeHubPolkadot::TO_CHAIN_MESSAGE_DETAILS_METHOD;
	const FROM_CHAIN_MESSAGE_DETAILS_METHOD: &'static str =
		relay_bridge_hub_polkadot_client::BridgeHubPolkadot::FROM_CHAIN_MESSAGE_DETAILS_METHOD;
}

impl CliChain for BridgeHubRococoAsBridgeHubPolkadot {
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion> =
		Some(SimpleRuntimeVersion { spec_version: 1_003_000, transaction_version: 3 });
}
