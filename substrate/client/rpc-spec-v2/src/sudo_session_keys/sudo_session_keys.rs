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

//! API implementation for `sudo_session_keys`.

use jsonrpsee::core::{async_trait, RpcResult};
use sc_rpc_api::DenyUnsafe;
use sp_blockchain::HeaderBackend;
use sp_keystore::{KeystoreExt, KeystorePtr};
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc};

use crate::{hex_string, sudo_session_keys::api::SudoSessionKeysServer, MethodResult};

use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_session::SessionKeys;

/// An API for `SudoSessionKeys` RPC calls.
pub struct SudoSessionKeys<Client, Block: BlockT> {
	/// Substrate client.
	client: Arc<Client>,
	/// The key store.
	keystore: KeystorePtr,
	/// Whether to deny unsafe calls
	deny_unsafe: DenyUnsafe,
	/// Phantom data to hold the block type.
	_phantom: PhantomData<Block>,
}

impl<Client, Block: BlockT> SudoSessionKeys<Client, Block> {
	/// Create a new [`SudoSessionKeys`].
	pub fn new(client: Arc<Client>, keystore: KeystorePtr, deny_unsafe: DenyUnsafe) -> Self {
		Self { client, keystore, deny_unsafe, _phantom: PhantomData }
	}
}

#[async_trait]
impl<Client, Block> SudoSessionKeysServer for SudoSessionKeys<Client, Block>
where
	Block: BlockT + 'static,
	Client: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	Client::Api: SessionKeys<Block>,
{
	fn sudo_session_keys_unstable_generate(&self, seed: Option<String>) -> RpcResult<MethodResult> {
		// Deny potentially unsafe calls.
		if let Err(err) = self.deny_unsafe.check_if_safe() {
			return Ok(MethodResult::err(err.to_string()));
		}

		// Call into the runtime of the best block hash.
		let best_block_hash = self.client.info().best_hash;
		let mut runtime_api = self.client.runtime_api();

		runtime_api.register_extension(KeystoreExt::from(self.keystore.clone()));

		let response = runtime_api
			.generate_session_keys(best_block_hash, seed.map(|seed| seed.into_bytes()))
			.map(|bytes| MethodResult::ok(hex_string(&bytes.as_slice())))
			.unwrap_or_else(|api_err| MethodResult::err(api_err.to_string()));

		Ok(response)
	}
}
