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

//! Types used to connect to the Kusama chain.

use bp_kusama::AccountInfoStorageMapKeyProvider;
use bp_runtime::ChainId;
use relay_substrate_client::{Chain, ChainWithBalances, RelayChain, UnderlyingChainProvider};
use sp_core::storage::StorageKey;
use std::time::Duration;

/// Kusama header id.
pub type HeaderId = relay_utils::HeaderId<bp_kusama::Hash, bp_kusama::BlockNumber>;

/// Kusama header type used in headers sync.
pub type SyncHeader = relay_substrate_client::SyncHeader<bp_kusama::Header>;

/// Kusama chain definition
#[derive(Debug, Clone, Copy)]
pub struct Kusama;

impl UnderlyingChainProvider for Kusama {
	type Chain = bp_kusama::Kusama;
}

impl Chain for Kusama {
	const ID: ChainId = bp_runtime::KUSAMA_CHAIN_ID;
	const NAME: &'static str = "Kusama";
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		bp_kusama::BEST_FINALIZED_KUSAMA_HEADER_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_secs(6);

	type SignedBlock = bp_kusama::SignedBlock;
	type Call = ();
}

impl ChainWithBalances for Kusama {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		AccountInfoStorageMapKeyProvider::final_key(account_id)
	}
}

impl RelayChain for Kusama {
	const PARAS_PALLET_NAME: &'static str = bp_kusama::PARAS_PALLET_NAME;
	const PARACHAINS_FINALITY_PALLET_NAME: &'static str = "BridgeKusamaParachain";
}
