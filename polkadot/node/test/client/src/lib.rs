// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! A Polkadot test client.
//!
//! This test client is using the Polkadot test runtime.

mod block_builder;

use polkadot_primitives::Block;
use sp_runtime::BuildStorage;
use std::sync::Arc;

pub use block_builder::*;
pub use polkadot_test_runtime as runtime;
pub use polkadot_test_service::{
	construct_extrinsic, construct_transfer_extrinsic, Client, FullBackend,
};
pub use substrate_test_client::*;

/// Test client executor.
pub type Executor = client::LocalCallExecutor<
	Block,
	FullBackend,
	WasmExecutor<(sp_io::SubstrateHostFunctions, frame_benchmarking::benchmarking::HostFunctions)>,
>;

/// Test client builder for Polkadot.
pub type TestClientBuilder =
	substrate_test_client::TestClientBuilder<Block, Executor, FullBackend, GenesisParameters>;

/// `LongestChain` type for the test runtime/client.
pub type LongestChain = sc_consensus::LongestChain<FullBackend, Block>;

/// Parameters of test-client builder with test-runtime.
#[derive(Default)]
pub struct GenesisParameters;

impl substrate_test_client::GenesisInit for GenesisParameters {
	fn genesis_storage(&self) -> Storage {
		polkadot_test_service::chain_spec::polkadot_local_testnet_config()
			.build_storage()
			.expect("Builds test runtime genesis storage")
	}
}

/// A `test-runtime` extensions to `TestClientBuilder`.
pub trait TestClientBuilderExt: Sized {
	/// Build the test client.
	fn build(self) -> Client {
		self.build_with_longest_chain().0
	}

	/// Build the test client and longest chain selector.
	fn build_with_longest_chain(self) -> (Client, LongestChain);
}

impl TestClientBuilderExt for TestClientBuilder {
	fn build_with_longest_chain(self) -> (Client, LongestChain) {
		let executor = WasmExecutor::builder().build();
		let executor = client::LocalCallExecutor::new(
			self.backend().clone(),
			executor.clone(),
			Default::default(),
			ExecutionExtensions::new(Default::default(), Arc::new(executor)),
		)
		.unwrap();

		self.build_with_executor(executor)
	}
}

/// A `TestClientBuilder` with default backend and executor.
pub trait DefaultTestClientBuilderExt: Sized {
	/// Create new `TestClientBuilder`
	fn new() -> Self;
}

impl DefaultTestClientBuilderExt for TestClientBuilder {
	fn new() -> Self {
		Self::with_default_backend()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_consensus::BlockOrigin;

	#[test]
	fn ensure_test_client_can_build_and_import_block() {
		let client = TestClientBuilder::new().build();

		let block_builder = client.init_polkadot_block_builder();
		let block = block_builder.build().expect("Finalizes the block").block;

		futures::executor::block_on(client.import(BlockOrigin::Own, block))
			.expect("Imports the block");
	}

	#[test]
	fn ensure_test_client_can_push_extrinsic() {
		let client = TestClientBuilder::new().build();

		let transfer = construct_transfer_extrinsic(
			&client,
			sp_keyring::Sr25519Keyring::Alice,
			sp_keyring::Sr25519Keyring::Bob,
			1000,
		);
		let mut block_builder = client.init_polkadot_block_builder();
		block_builder.push_polkadot_extrinsic(transfer).expect("Pushes extrinsic");

		let block = block_builder.build().expect("Finalizes the block").block;

		futures::executor::block_on(client.import(BlockOrigin::Own, block))
			.expect("Imports the block");
	}

	#[test]
	fn node_version_inherent_included_with_valid_author() {
		let client = TestClientBuilder::new().build();
		let chain_info = client.chain_info();

		let version_hash = sp_core::blake2_256(b"1.0.0-abc123").into();

		// Build a block with authority_index 0 (Alice) and node version data.
		// The block should include the node version inherent because there's a valid
		// author and no prior version recorded.
		let block_builder = client.init_polkadot_block_builder_with_options(
			chain_info.best_hash,
			BlockBuilderOptions { authority_index: 0, node_version_hash: Some(version_hash) },
		);
		let block = block_builder.build().expect("Finalizes the block").block;

		// Build a baseline block without node version data to count baseline extrinsics.
		let baseline_builder = client.init_polkadot_block_builder_with_options(
			chain_info.best_hash,
			BlockBuilderOptions { authority_index: 0, node_version_hash: None },
		);
		let baseline_block = baseline_builder.build().expect("Finalizes the block").block;

		// The block with node version should have one more extrinsic (report_version).
		assert_eq!(
			block.extrinsics().len(),
			baseline_block.extrinsics().len() + 1,
			"Block with node version should have one additional inherent extrinsic"
		);

		futures::executor::block_on(client.import(BlockOrigin::Own, block))
			.expect("Imports the block with node version inherent");
	}

	#[test]
	fn node_version_inherent_skipped_when_unchanged() {
		let client = TestClientBuilder::new().build();
		let chain_info = client.chain_info();

		let version_hash = sp_core::blake2_256(b"1.0.0-abc123").into();

		// Build and import the first block with the version.
		let block_builder = client.init_polkadot_block_builder_with_options(
			chain_info.best_hash,
			BlockBuilderOptions { authority_index: 0, node_version_hash: Some(version_hash) },
		);
		let block1 = block_builder.build().expect("Finalizes block 1").block;
		let block1_ext_count = block1.extrinsics().len();

		futures::executor::block_on(client.import(BlockOrigin::Own, block1))
			.expect("Imports block 1");

		// Build a second block with the SAME version hash.
		// The pallet should detect the version hasn't changed and skip the inherent.
		let chain_info = client.chain_info();
		let block_builder = client.init_polkadot_block_builder_with_options(
			chain_info.best_hash,
			BlockBuilderOptions { authority_index: 0, node_version_hash: Some(version_hash) },
		);
		let block2 = block_builder.build().expect("Finalizes block 2").block;

		assert_eq!(
			block2.extrinsics().len(),
			block1_ext_count - 1,
			"Second block should skip the node version inherent since version is unchanged"
		);

		futures::executor::block_on(client.import(BlockOrigin::Own, block2))
			.expect("Imports block 2");
	}

	#[test]
	fn node_version_inherent_resubmitted_on_version_change() {
		let client = TestClientBuilder::new().build();
		let chain_info = client.chain_info();

		let version_v1 = sp_core::blake2_256(b"1.0.0").into();
		let version_v2 = sp_core::blake2_256(b"2.0.0").into();

		// Build and import block with version v1.
		let bb = client.init_polkadot_block_builder_with_options(
			chain_info.best_hash,
			BlockBuilderOptions { authority_index: 0, node_version_hash: Some(version_v1) },
		);
		let block1 = bb.build().expect("Finalizes block 1").block;
		let block1_ext_count = block1.extrinsics().len();
		futures::executor::block_on(client.import(BlockOrigin::Own, block1))
			.expect("Imports block 1");

		// Build block with version v2 (different) — inherent should be included again.
		let chain_info = client.chain_info();
		let bb = client.init_polkadot_block_builder_with_options(
			chain_info.best_hash,
			BlockBuilderOptions { authority_index: 0, node_version_hash: Some(version_v2) },
		);
		let block2 = bb.build().expect("Finalizes block 2").block;

		assert_eq!(
			block2.extrinsics().len(),
			block1_ext_count,
			"Block with updated version should include the node version inherent again"
		);

		futures::executor::block_on(client.import(BlockOrigin::Own, block2))
			.expect("Imports block 2");
	}

	#[test]
	fn block_builds_without_node_version_data() {
		// This tests backwards compatibility: a block can be built and imported
		// successfully even when no node version inherent data is provided.
		// This is the scenario where a runtime includes pallet-node-version but
		// the node hasn't been upgraded to provide the inherent data yet.
		let client = TestClientBuilder::new().build();
		let chain_info = client.chain_info();

		let block_builder = client.init_polkadot_block_builder_with_options(
			chain_info.best_hash,
			BlockBuilderOptions { authority_index: 0, node_version_hash: None },
		);
		let block = block_builder.build().expect("Finalizes the block").block;

		futures::executor::block_on(client.import(BlockOrigin::Own, block))
			.expect("Imports the block without node version data");
	}
}
