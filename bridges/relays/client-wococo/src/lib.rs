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

//! Types used to connect to the Wococo-Substrate chain.

use bp_runtime::ChainId;
use relay_substrate_client::{Chain, ChainWithBalances, RelayChain, UnderlyingChainProvider};
use sp_core::storage::StorageKey;
use std::time::Duration;

/// Wococo header id.
pub type HeaderId = relay_utils::HeaderId<bp_wococo::Hash, bp_wococo::BlockNumber>;

/// Wococo header type used in headers sync.
pub type SyncHeader = relay_substrate_client::SyncHeader<bp_wococo::Header>;

/// Wococo chain definition
#[derive(Debug, Clone, Copy)]
pub struct Wococo;

impl UnderlyingChainProvider for Wococo {
	type Chain = bp_wococo::Wococo;
}

impl Chain for Wococo {
	const ID: ChainId = bp_runtime::WOCOCO_CHAIN_ID;
	const NAME: &'static str = "Wococo";
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		bp_wococo::BEST_FINALIZED_WOCOCO_HEADER_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_secs(6);

	type SignedBlock = bp_wococo::SignedBlock;
	type Call = ();
}

impl ChainWithBalances for Wococo {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		bp_wococo::AccountInfoStorageMapKeyProvider::final_key(account_id)
	}
}

impl RelayChain for Wococo {
	const PARAS_PALLET_NAME: &'static str = bp_wococo::PARAS_PALLET_NAME;
	const PARACHAINS_FINALITY_PALLET_NAME: &'static str = "BridgeWococoParachain";
}
