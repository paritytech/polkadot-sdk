// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use jsonrpsee::RpcModule;
use runtime::interface::{AccountId, Block, Nonce};
use sc_transaction_pool_api::TransactionPool;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use std::sync::Arc;
use substrate_frame_rpc_system::{System, SystemApiServer};

pub use sc_rpc_api::DenyUnsafe;

/// Full client dependencies.
pub struct FullDeps<C, P> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// Whether to deny unsafe calls
	pub deny_unsafe: DenyUnsafe,
}

pub type OpaqueBlock1 = frame::deps::sp_runtime::generic::Block<
	frame::deps::sp_runtime::generic::Header<u32, frame::primitives::BlakeTwo256>,
	frame::deps::sp_runtime::OpaqueExtrinsic,
>;
pub type OpaqueBlock2 = runtime::interface::OpaqueBlock;

trait AssertSameType<A, B> {}
impl<T> AssertSameType<T, T> for Tester<T, T> {}

struct Tester<A, B>(std::marker::PhantomData<(A, B)>);

impl<A, B> Tester<A, B> {
	fn is_equal()
	where
		Self: AssertSameType<A, B>,
	{
	}
}

// HELP: Works with OpaqueBlock1, but not with, even though they are really really the same damn
// type, see `Tester`. Or try printing the `type_id()` inside `fn main()`. Yes, they even have the
// same type-id.
type OpaqueBlock = OpaqueBlock1;

/// Instantiate all full RPC extensions.
pub fn create_full<C, P>(
	deps: FullDeps<C, P>,
) -> Result<RpcModule<()>, Box<dyn std::error::Error + Send + Sync>>
where
	C: Send
		+ Sync
		+ 'static
		+ sp_api::ProvideRuntimeApi<OpaqueBlock>
		+ HeaderBackend<OpaqueBlock>
		+ HeaderMetadata<OpaqueBlock, Error = BlockChainError>
		+ 'static,
	C::Api: sp_block_builder::BlockBuilder<OpaqueBlock>,
	C::Api: substrate_frame_rpc_system::AccountNonceApi<OpaqueBlock, AccountId, Nonce>,
	P: TransactionPool + 'static,
{
	// Tester::<u32, u64>::is_equal(); // will fail to compile.
	Tester::<u32, u32>::is_equal();
	Tester::<OpaqueBlock1, OpaqueBlock2>::is_equal();

	let mut module = RpcModule::new(());
	let FullDeps { client, pool, deny_unsafe } = deps;

	module.merge(System::new(client.clone(), pool.clone(), deny_unsafe).into_rpc())?;

	Ok(module)
}
