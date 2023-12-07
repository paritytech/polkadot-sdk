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

//! Utilities to build a `TestClient` for `kitchensink-runtime`.

use sp_runtime::BuildStorage;
/// Re-export test-client utilities.
pub use substrate_test_client::*;

/// Call executor for `kitchensink-runtime` `TestClient`.
use node_cli::service::RuntimeExecutor;

/// Default backend type.
pub type Backend = sc_client_db::Backend<node_primitives::Block>;

/// Test client type.
pub type Client = client::Client<
	Backend,
	client::LocalCallExecutor<node_primitives::Block, Backend, RuntimeExecutor>,
	node_primitives::Block,
	kitchensink_runtime::RuntimeApi,
>;

/// Genesis configuration parameters for `TestClient`.
#[derive(Default)]
pub struct GenesisParameters;

impl substrate_test_client::GenesisInit for GenesisParameters {
	fn genesis_storage(&self) -> Storage {
		let mut storage = crate::genesis::config().build_storage().unwrap();
		storage.top.insert(
			sp_core::storage::well_known_keys::CODE.to_vec(),
			kitchensink_runtime::wasm_binary_unwrap().into(),
		);
		storage
	}
}

/// A `test-runtime` extensions to `TestClientBuilder`.
pub trait TestClientBuilderExt: Sized {
	/// Create test client builder.
	fn new() -> Self;

	/// Build the test client.
	fn build(self) -> Client;
}

impl TestClientBuilderExt
	for substrate_test_client::TestClientBuilder<
		node_primitives::Block,
		client::LocalCallExecutor<node_primitives::Block, Backend, RuntimeExecutor>,
		Backend,
		GenesisParameters,
	>
{
	fn new() -> Self {
		Self::default()
	}
	fn build(self) -> Client {
		let executor = RuntimeExecutor::builder().build();
		use sc_service::client::LocalCallExecutor;
		use std::sync::Arc;
		let executor = LocalCallExecutor::new(
			self.backend().clone(),
			executor.clone(),
			Default::default(),
			ExecutionExtensions::new(None, Arc::new(executor)),
		)
		.expect("Creates LocalCallExecutor");
		self.build_with_executor(executor).0
	}
}
