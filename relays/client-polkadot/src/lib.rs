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

use frame_support::weights::Weight;
use relay_substrate_client::{Chain, ChainBase, ChainWithBalances, ChainWithGrandpa};
use sp_core::storage::StorageKey;
use std::time::Duration;

/// Polkadot header id.
pub type HeaderId = relay_utils::HeaderId<bp_polkadot::Hash, bp_polkadot::BlockNumber>;

/// Polkadot chain definition
#[derive(Debug, Clone, Copy)]
pub struct Polkadot;

impl ChainBase for Polkadot {
	type BlockNumber = bp_polkadot::BlockNumber;
	type Hash = bp_polkadot::Hash;
	type Hasher = bp_polkadot::Hasher;
	type Header = bp_polkadot::Header;

	type AccountId = bp_polkadot::AccountId;
	type Balance = bp_polkadot::Balance;
	type Index = bp_polkadot::Nonce;
	type Signature = bp_polkadot::Signature;

	fn max_extrinsic_size() -> u32 {
		bp_polkadot::Polkadot::max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		bp_polkadot::Polkadot::max_extrinsic_weight()
	}
}

impl Chain for Polkadot {
	const NAME: &'static str = "Polkadot";
	const TOKEN_ID: Option<&'static str> = Some("polkadot");
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		bp_polkadot::BEST_FINALIZED_POLKADOT_HEADER_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_secs(6);
	const STORAGE_PROOF_OVERHEAD: u32 = bp_polkadot::EXTRA_STORAGE_PROOF_SIZE;

	type SignedBlock = bp_polkadot::SignedBlock;
	type Call = ();
}

impl ChainWithGrandpa for Polkadot {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str =
		bp_polkadot::WITH_POLKADOT_GRANDPA_PALLET_NAME;
}

impl ChainWithBalances for Polkadot {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		StorageKey(bp_polkadot::account_info_storage_key(account_id))
	}
}

/// Polkadot header type used in headers sync.
pub type SyncHeader = relay_substrate_client::SyncHeader<bp_polkadot::Header>;
