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

//! Types used to connect to the Polkadot chain.

use bp_polkadot::AccountInfoStorageMapKeyProvider;
use relay_substrate_client::{Chain, ChainWithBalances, ChainWithGrandpa, UnderlyingChainProvider};
use sp_core::storage::StorageKey;
use std::time::Duration;

/// Polkadot header id.
pub type HeaderId = relay_utils::HeaderId<bp_polkadot::Hash, bp_polkadot::BlockNumber>;

/// Polkadot chain definition
#[derive(Debug, Clone, Copy)]
pub struct Polkadot;

impl UnderlyingChainProvider for Polkadot {
	type Chain = bp_polkadot::Polkadot;
}

impl Chain for Polkadot {
	const NAME: &'static str = "Polkadot";
	const TOKEN_ID: Option<&'static str> = Some("polkadot");
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		bp_polkadot::BEST_FINALIZED_POLKADOT_HEADER_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_secs(6);

	type SignedBlock = bp_polkadot::SignedBlock;
	type Call = ();
}

impl ChainWithGrandpa for Polkadot {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str =
		bp_polkadot::WITH_POLKADOT_GRANDPA_PALLET_NAME;
}

impl ChainWithBalances for Polkadot {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		AccountInfoStorageMapKeyProvider::final_key(account_id)
	}
}

/// Polkadot header type used in headers sync.
pub type SyncHeader = relay_substrate_client::SyncHeader<bp_polkadot::Header>;
