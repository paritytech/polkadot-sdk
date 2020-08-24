// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use sp_runtime::traits::{Block as BlockT, NumberFor, Saturating, One};
use sp_blockchain::HeaderBackend;
use crate::{TFullBackend, TLightBackend};
use std::sync::Arc;
use sp_runtime::generic::BlockId;

/// An error for if this function is being called on a full node.
pub const CHT_ROOT_ERROR: &str =
	"Backend doesn't store CHT roots. Make sure you're calling this on a light client.";

/// Something that might allow access to a `ChtRootStorage`.
pub trait MaybeChtRootStorageProvider<Block> {
	/// Potentially get a reference to a `ChtRootStorage`.
	fn cht_root_storage(&self) -> Option<&dyn sc_client_api::light::ChtRootStorage<Block>>;
}

impl<Block: BlockT> MaybeChtRootStorageProvider<Block> for TFullBackend<Block> {
	fn cht_root_storage(&self) -> Option<&dyn sc_client_api::light::ChtRootStorage<Block>> {
		None
	}
}

impl<Block: BlockT> MaybeChtRootStorageProvider<Block> for TLightBackend<Block> {
	fn cht_root_storage(&self) -> Option<&dyn sc_client_api::light::ChtRootStorage<Block>> {
		Some(self.blockchain().storage())
	}
}

/// Build a `LightSyncState` from the CHT roots stored in a backend.
pub fn build_light_sync_state<TBl, TCl, TBackend>(
	client: Arc<TCl>,
	backend: Arc<TBackend>,
) -> Result<sc_chain_spec::LightSyncState<TBl>, sp_blockchain::Error>
	where
		TBl: BlockT,
		TCl: HeaderBackend<TBl>,
		TBackend: MaybeChtRootStorageProvider<TBl>,
{
	let storage = backend.cht_root_storage().ok_or(CHT_ROOT_ERROR)?;

	let finalized_hash = client.info().finalized_hash;
	let finalized_number = client.info().finalized_number;

	use sc_client_api::cht;

	let mut chts = Vec::new();

	// We can't fetch a CHT root later than `finalized_number - 2 * cht_size`.
	let cht_size_x_2 = cht::size::<NumberFor::<TBl>>() * NumberFor::<TBl>::from(2);

	let mut number = NumberFor::<TBl>::one();

	while number <= finalized_number.saturating_sub(cht_size_x_2) {
		match storage.header_cht_root(cht::size(), number)? {
			Some(cht_root) => chts.push(cht_root),
			None => log::error!("No CHT found for block {}", number),
		}

		number += cht::size();
	}

	Ok(sc_chain_spec::LightSyncState {
		header: client.header(BlockId::Hash(finalized_hash))?.unwrap(),
		chts,
	})
}
