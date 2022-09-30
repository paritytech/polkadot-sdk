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

use frame_support::weights::Weight;
use relay_substrate_client::{Chain, ChainBase, ChainWithBalances, ChainWithGrandpa};
use sp_core::storage::StorageKey;
use std::time::Duration;

/// Kusama header id.
pub type HeaderId = relay_utils::HeaderId<bp_kusama::Hash, bp_kusama::BlockNumber>;

/// Kusama chain definition
#[derive(Debug, Clone, Copy)]
pub struct Kusama;

impl ChainBase for Kusama {
	type BlockNumber = bp_kusama::BlockNumber;
	type Hash = bp_kusama::Hash;
	type Hasher = bp_kusama::Hasher;
	type Header = bp_kusama::Header;

	type AccountId = bp_kusama::AccountId;
	type Balance = bp_kusama::Balance;
	type Index = bp_kusama::Nonce;
	type Signature = bp_kusama::Signature;

	fn max_extrinsic_size() -> u32 {
		bp_kusama::Kusama::max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		bp_kusama::Kusama::max_extrinsic_weight()
	}
}

impl Chain for Kusama {
	const NAME: &'static str = "Kusama";
	const TOKEN_ID: Option<&'static str> = Some("kusama");
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		bp_kusama::BEST_FINALIZED_KUSAMA_HEADER_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_secs(6);
	const STORAGE_PROOF_OVERHEAD: u32 = bp_kusama::EXTRA_STORAGE_PROOF_SIZE;

	type SignedBlock = bp_kusama::SignedBlock;
	type Call = ();
}

impl ChainWithGrandpa for Kusama {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = bp_kusama::WITH_KUSAMA_GRANDPA_PALLET_NAME;
}

impl ChainWithBalances for Kusama {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		StorageKey(bp_kusama::account_info_storage_key(account_id))
	}
}

/// Kusama header type used in headers sync.
pub type SyncHeader = relay_substrate_client::SyncHeader<bp_kusama::Header>;
