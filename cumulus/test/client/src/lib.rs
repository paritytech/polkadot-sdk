// Copyright 2019 Parity Technologies (UK) Ltd.
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
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! A Cumulus test client.

pub use substrate_test_client::*;

mod local_executor {
	#![allow(missing_docs)]
	use substrate_test_client::runtime;
	use substrate_test_client::executor::native_executor_instance;
	native_executor_instance!(
		pub LocalExecutor,
		runtime::api::dispatch,
		runtime::native_version,
		include_bytes!("../../runtime/wasm/target/wasm32-unknown-unknown/release/cumulus_test_runtime.compact.wasm")
	);
}

/// Native executor used for tests.
pub use local_executor::LocalExecutor;

/// Test client executor.
pub type Executor = client::LocalCallExecutor<
	Backend,
	executor::NativeExecutor<LocalExecutor>,
>;

/// Test client type.
pub type TestClient = client::Client<
	Backend, Executor, runtime::Block, runtime::RuntimeApi
>;

/// An extension to the `TestClientBuilder` for building a cumulus test-client.
pub trait TestClientBuilderExt {
	fn build_cumulus(self) -> TestClient;
}

impl TestClientBuilderExt for TestClientBuilder {
	fn build_cumulus(self) -> TestClient {
		self.build_with_native_executor(NativeExecutor::<LocalExecutor>::new(None))
	}
}