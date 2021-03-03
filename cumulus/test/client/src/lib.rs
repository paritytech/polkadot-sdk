// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! A Polkadot test client.

mod block_builder;
use codec::Encode;
use runtime::{
	Balance, Block, BlockHashCount, Call, GenesisConfig, Runtime, Signature, SignedExtra,
	SignedPayload, UncheckedExtrinsic, VERSION,
};
use sc_service::client;
use sp_blockchain::HeaderBackend;
use sp_core::storage::Storage;
use sp_runtime::{generic::Era, BuildStorage, SaturatedConversion};

pub use block_builder::*;
pub use cumulus_test_runtime as runtime;
pub use substrate_test_client::*;

mod local_executor {
	use substrate_test_client::sc_executor::native_executor_instance;
	native_executor_instance!(
		pub LocalExecutor,
		cumulus_test_runtime::api::dispatch,
		cumulus_test_runtime::native_version,
	);
}

/// Native executor used for tests.
pub use local_executor::LocalExecutor;

/// Test client database backend.
pub type Backend = substrate_test_client::Backend<Block>;

/// Test client executor.
pub type Executor = client::LocalCallExecutor<Backend, sc_executor::NativeExecutor<LocalExecutor>>;

/// Test client builder for Cumulus
pub type TestClientBuilder =
	substrate_test_client::TestClientBuilder<Block, Executor, Backend, GenesisParameters>;

/// LongestChain type for the test runtime/client.
pub type LongestChain = sc_consensus::LongestChain<Backend, Block>;

/// Test client type with `LocalExecutor` and generic Backend.
pub type Client = client::Client<Backend, Executor, Block, runtime::RuntimeApi>;

/// Parameters of test-client builder with test-runtime.
#[derive(Default)]
pub struct GenesisParameters;

impl substrate_test_client::GenesisInit for GenesisParameters {
	fn genesis_storage(&self) -> Storage {
		genesis_config().build_storage().unwrap()
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
		self.build_with_native_executor(None)
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

fn genesis_config() -> GenesisConfig {
	cumulus_test_service::local_testnet_genesis()
}

/// Generate an extrinsic from the provided function call, origin and [`Client`].
pub fn generate_extrinsic(
	client: &Client,
	origin: sp_keyring::AccountKeyring,
	function: Call,
) -> UncheckedExtrinsic {
	let current_block_hash = client.info().best_hash;
	let current_block = client.info().best_number.saturated_into();
	let genesis_block = client.hash(0).unwrap().unwrap();
	let nonce = 0;
	let period = BlockHashCount::get()
		.checked_next_power_of_two()
		.map(|c| c / 2)
		.unwrap_or(2) as u64;
	let tip = 0;
	let extra: SignedExtra = (
		frame_system::CheckSpecVersion::<Runtime>::new(),
		frame_system::CheckGenesis::<Runtime>::new(),
		frame_system::CheckEra::<Runtime>::from(Era::mortal(period, current_block)),
		frame_system::CheckNonce::<Runtime>::from(nonce),
		frame_system::CheckWeight::<Runtime>::new(),
		pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
	);
	let raw_payload = SignedPayload::from_raw(
		function.clone(),
		extra.clone(),
		(
			VERSION.spec_version,
			genesis_block,
			current_block_hash,
			(),
			(),
			(),
		),
	);
	let signature = raw_payload.using_encoded(|e| origin.sign(e));

	UncheckedExtrinsic::new_signed(
		function.clone(),
		origin.public().into(),
		Signature::Sr25519(signature.clone()),
		extra.clone(),
	)
}

/// Transfer some token from one account to another using a provided test [`Client`].
pub fn transfer(
	client: &Client,
	origin: sp_keyring::AccountKeyring,
	dest: sp_keyring::AccountKeyring,
	value: Balance,
) -> UncheckedExtrinsic {
	let function = Call::Balances(pallet_balances::Call::transfer(dest.public().into(), value));

	generate_extrinsic(client, origin, function)
}
