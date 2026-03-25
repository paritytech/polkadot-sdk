// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Implementation of the `bitswap_v1_get` RPC method.

use crate::bitswap::{api::BitswapApiServer, error::Error};
use jsonrpsee::core::RpcResult;
use sc_client_api::BlockBackend;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

/// Bitswap RPC implementation.
pub struct Bitswap<Block, Client> {
	client: Arc<Client>,
	sync_oracle: Arc<dyn sp_consensus::SyncOracle + Send + Sync>,
	_phantom: std::marker::PhantomData<Block>,
}

impl<Block, Client> Bitswap<Block, Client> {
	/// Creates a new [`Bitswap`] instance.
	pub fn new(
		client: Arc<Client>,
		sync_oracle: Arc<dyn sp_consensus::SyncOracle + Send + Sync>,
	) -> Self {
		Self { client, sync_oracle, _phantom: std::marker::PhantomData }
	}
}

impl<Block, Client> BitswapApiServer for Bitswap<Block, Client>
where
	Block: BlockT,
	Client: BlockBackend<Block> + Send + Sync + 'static,
{
	fn bitswap_v1_get(&self, cid_str: String) -> RpcResult<String> {
		let cid = cid::Cid::try_from(cid_str.as_str())
			.map_err(|e| Error::InvalidCid(format!("{e}")))?;

		if cid.version() != cid::Version::V1 {
			return Err(Error::InvalidCid("Only CIDv1 is supported".into()).into());
		}

		let hash = cid.hash();
		if hash.size() != 32 {
			return Err(
				Error::InvalidCid("Hash digest must be 32 bytes".into()).into()
			);
		}

		let digest = H256::from_slice(hash.digest());

		match self.client.indexed_transaction(digest) {
			Ok(Some(data)) => Ok(crate::hex_string(&data)),
			Ok(None) =>
				if self.sync_oracle.is_major_syncing() {
					Err(Error::MajorSyncing.into())
				} else {
					Err(Error::NotFound.into())
				},
			Err(e) => Err(Error::Internal(e.to_string()).into()),
		}
	}
}
