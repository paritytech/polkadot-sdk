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

use frame_support::weights::Weight;
use relay_substrate_client::{Chain, ChainBase, ChainWithBalances, ChainWithGrandpa, RelayChain};
use sp_core::storage::StorageKey;
use std::time::Duration;

/// Wococo header id.
pub type HeaderId = relay_utils::HeaderId<bp_wococo::Hash, bp_wococo::BlockNumber>;

/// Wococo header type used in headers sync.
pub type SyncHeader = relay_substrate_client::SyncHeader<bp_wococo::Header>;

/// Wococo chain definition
#[derive(Debug, Clone, Copy)]
pub struct Wococo;

impl ChainBase for Wococo {
	type BlockNumber = bp_wococo::BlockNumber;
	type Hash = bp_wococo::Hash;
	type Hasher = bp_wococo::Hashing;
	type Header = bp_wococo::Header;

	type AccountId = bp_wococo::AccountId;
	type Balance = bp_wococo::Balance;
	type Index = bp_wococo::Nonce;
	type Signature = bp_wococo::Signature;

	fn max_extrinsic_size() -> u32 {
		bp_wococo::Wococo::max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		bp_wococo::Wococo::max_extrinsic_weight()
	}
}

impl Chain for Wococo {
	const NAME: &'static str = "Wococo";
	const TOKEN_ID: Option<&'static str> = None;
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		bp_wococo::BEST_FINALIZED_WOCOCO_HEADER_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_secs(6);
	const STORAGE_PROOF_OVERHEAD: u32 = bp_wococo::EXTRA_STORAGE_PROOF_SIZE;

	type SignedBlock = bp_wococo::SignedBlock;
	type Call = ();
}

impl ChainWithGrandpa for Wococo {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = bp_wococo::WITH_WOCOCO_GRANDPA_PALLET_NAME;
}

impl ChainWithBalances for Wococo {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		StorageKey(bp_wococo::account_info_storage_key(account_id))
	}
}

impl RelayChain for Wococo {
	const PARAS_PALLET_NAME: &'static str = bp_wococo::PARAS_PALLET_NAME;
	const PARACHAINS_FINALITY_PALLET_NAME: &'static str = "bridgeWococoParachain";
}
