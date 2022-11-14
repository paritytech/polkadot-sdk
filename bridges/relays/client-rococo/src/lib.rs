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

//! Types used to connect to the Rococo-Substrate chain.

use frame_support::weights::Weight;
use relay_substrate_client::{Chain, ChainBase, ChainWithBalances, ChainWithGrandpa, RelayChain};
use sp_core::storage::StorageKey;
use std::time::Duration;

/// Rococo header id.
pub type HeaderId = relay_utils::HeaderId<bp_rococo::Hash, bp_rococo::BlockNumber>;

/// Rococo header type used in headers sync.
pub type SyncHeader = relay_substrate_client::SyncHeader<bp_rococo::Header>;

/// Rococo chain definition
#[derive(Debug, Clone, Copy)]
pub struct Rococo;

impl ChainBase for Rococo {
	type BlockNumber = bp_rococo::BlockNumber;
	type Hash = bp_rococo::Hash;
	type Hasher = bp_rococo::Hashing;
	type Header = bp_rococo::Header;

	type AccountId = bp_rococo::AccountId;
	type Balance = bp_rococo::Balance;
	type Index = bp_rococo::Nonce;
	type Signature = bp_rococo::Signature;

	fn max_extrinsic_size() -> u32 {
		bp_rococo::Rococo::max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		bp_rococo::Rococo::max_extrinsic_weight()
	}
}

impl Chain for Rococo {
	const NAME: &'static str = "Rococo";
	const TOKEN_ID: Option<&'static str> = None;
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		bp_rococo::BEST_FINALIZED_ROCOCO_HEADER_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_secs(6);
	const STORAGE_PROOF_OVERHEAD: u32 = bp_rococo::EXTRA_STORAGE_PROOF_SIZE;

	type SignedBlock = bp_rococo::SignedBlock;
	type Call = ();
}

impl ChainWithGrandpa for Rococo {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = bp_rococo::WITH_ROCOCO_GRANDPA_PALLET_NAME;
}

impl ChainWithBalances for Rococo {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		StorageKey(bp_rococo::account_info_storage_key(account_id))
	}
}

impl RelayChain for Rococo {
	const PARAS_PALLET_NAME: &'static str = bp_rococo::PARAS_PALLET_NAME;
	const PARACHAINS_FINALITY_PALLET_NAME: &'static str = "bridgeRococoParachain";
}
