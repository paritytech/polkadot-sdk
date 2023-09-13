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

//! API implementation for `archive`.

use crate::{chain_head::hex_string, SubscriptionTaskExecutor};
use sc_client_api::Backend;
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc};

/// An API for archive RPC calls.
pub struct Archive<BE: Backend<Block>, Block: BlockT, Client> {
	/// Substrate client.
	client: Arc<Client>,
	/// Backend of the chain.
	backend: Arc<BE>,
	/// Executor to spawn subscriptions.
	executor: SubscriptionTaskExecutor,
	/// The hexadecimal encoded hash of the genesis block.
	genesis_hash: String,
	/// Phantom member to pin the block type.
	_phantom: PhantomData<Block>,
}

impl<BE: Backend<Block>, Block: BlockT, Client> Archive<BE, Block, Client> {
	/// Create a new [`Archive`].
	pub fn new<GenesisHash: AsRef<[u8]>>(
		client: Arc<Client>,
		backend: Arc<BE>,
		executor: SubscriptionTaskExecutor,
		genesis_hash: GenesisHash,
	) -> Self {
		let genesis_hash = hex_string(&genesis_hash.as_ref());
		Self { client, backend: backend.clone(), executor, genesis_hash, _phantom: PhantomData }
	}
}
