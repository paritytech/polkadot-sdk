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

//! Tests for `sp_block_builder::check_inherents` on a block that is built by a runtime that is
//! enacted with the block.

use codec::Encode;
use futures::executor::block_on;
use sc_block_builder::BlockBuilderBuilder;
use sc_client_api::HeaderBackend;
use sp_api::{ApiExt, Core, ProvideRuntimeApi};
use sp_core::{storage::well_known_keys, traits::CallContext};
use sp_inherents::{InherentData, InherentDataProvider, InherentIdentifier};
use std::sync::Arc;
use substrate_test_runtime_client::{
	runtime::{
		inherent_upgrade_pallet, wasm_binary_unwrap, wasm_binary_without_inherent_upgrade_pallet,
		Block, Extrinsic, Runtime, RuntimeCall, VERSION,
	},
	DefaultTestClientBuilderExt, TestClient, TestClientBuilder, TestClientBuilderExt,
};

/// Provides the inherent data of `inherent_upgrade_pallet`.
struct InherentUpgradeDataProvider {
	requires_migration: bool,
}

#[async_trait::async_trait]
impl InherentDataProvider for InherentUpgradeDataProvider {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data
			.put_data(inherent_upgrade_pallet::INHERENT_IDENTIFIER, &self.requires_migration)
	}

	async fn try_handle_error(
		&self,
		_: &InherentIdentifier,
		_: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		None
	}
}

/// Creates a client whose genesis state is written by the runtime without
/// `inherent_upgrade_pallet` and has the default runtime as `:pending_code`.
///
/// The default runtime is enacted with block `1`.
fn client_with_pending_upgrade() -> Arc<TestClient> {
	let old_code = wasm_binary_without_inherent_upgrade_pallet::WASM_BINARY
		.expect("You need to build the WASM binaries to run the tests!")
		.to_vec();

	// The genesis storage is built natively and thus, contains the version of the new runtime.
	let old_version = frame_system::LastRuntimeUpgradeInfo {
		spec_version: 1.into(),
		spec_name: VERSION.spec_name,
	};

	let mut builder = TestClientBuilder::new();
	builder.genesis_init_mut().set_wasm_code(old_code);
	let client = builder
		.add_extra_storage(well_known_keys::PENDING_CODE.to_vec(), wasm_binary_unwrap().to_vec())
		.add_extra_storage(
			frame_system::LastRuntimeUpgrade::<Runtime>::hashed_key().to_vec(),
			old_version.encode(),
		)
		.build();

	let genesis_hash = client.info().genesis_hash;

	// The new runtime is only used in the on-chain context.
	let offchain_version = client.runtime_api().version(genesis_hash).unwrap();
	let mut runtime_api = client.runtime_api();
	runtime_api.set_call_context(CallContext::Onchain { import: true });
	let onchain_version = runtime_api.version(genesis_hash).unwrap();
	assert_eq!(offchain_version.spec_version, 1);
	assert_eq!(onchain_version.spec_version, VERSION.spec_version);

	Arc::new(client)
}

/// Builds block `1` that includes the inherent of `inherent_upgrade_pallet`.
fn build_block_with_new_inherent(client: &TestClient) -> Block {
	let mut block_builder = BlockBuilderBuilder::new(client)
		.on_parent_block(client.info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap();

	block_builder
		.push(Extrinsic::new_bare(RuntimeCall::InherentUpgrade(
			inherent_upgrade_pallet::Call::inherent {},
		)))
		.unwrap();

	block_builder.build().unwrap().block
}

/// The old runtime can not decode the inherent of the new runtime. So, the inherents need to be
/// checked by the new runtime.
#[test]
fn check_inherents_uses_runtime_that_built_the_block() {
	let client = client_with_pending_upgrade();
	let block = build_block_with_new_inherent(&client);
	let genesis_hash = client.info().genesis_hash;

	block_on(sp_block_builder::check_inherents(
		client,
		genesis_hash,
		block,
		&InherentUpgradeDataProvider { requires_migration: false },
	))
	.unwrap();
}

/// The inherent check of the new runtime requires that its migrations were applied.
#[test]
fn check_inherents_applies_runtime_migrations() {
	let client = client_with_pending_upgrade();
	let block = build_block_with_new_inherent(&client);
	let genesis_hash = client.info().genesis_hash;

	block_on(sp_block_builder::check_inherents(
		client,
		genesis_hash,
		block,
		&InherentUpgradeDataProvider { requires_migration: true },
	))
	.unwrap();
}
