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

use crate::BlockBuilder;

use sp_api::{ApiExt, CallContext, ProvideRuntimeApi};
use sp_inherents::{InherentData, InherentDataProvider, InherentIdentifier};
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

/// Errors that occur when creating and checking on the client side.
#[derive(Debug)]
pub enum CheckInherentsError {
	/// Create inherents error.
	CreateInherentData(sp_inherents::Error),
	/// Client Error
	Client(sp_api::ApiError),
	/// Check inherents error
	CheckInherents(sp_inherents::Error),
	/// Unknown inherent error for identifier
	CheckInherentsUnknownError(InherentIdentifier),
}

/// Create inherent data and check that the inherents are valid.
///
/// See [`check_inherents_with_data`] for the context the runtime API is called in.
pub async fn check_inherents<Block: BlockT, Client: ProvideRuntimeApi<Block>>(
	client: Arc<Client>,
	at_hash: Block::Hash,
	block: Block,
	inherent_data_providers: &impl InherentDataProvider,
) -> Result<(), CheckInherentsError>
where
	Client::Api: BlockBuilder<Block>,
{
	let inherent_data = inherent_data_providers
		.create_inherent_data()
		.await
		.map_err(CheckInherentsError::CreateInherentData)?;

	check_inherents_with_data(client, at_hash, block, inherent_data_providers, inherent_data).await
}

/// Check that the inherents are valid.
///
/// The runtime API is called in the on-chain call context ([`CallContext::Onchain`]), the context
/// the block builder produced `block` in. The call context decides which runtime executes the call
/// around a delayed runtime upgrade (`system_version >= 3`): the upgrade is staged in
/// `:pending_code` and replaces `:code` only at the end of the block *after* the upgrade block.
/// On-chain calls resolve the staged code, off-chain calls (the default of a runtime API call) do
/// not. The first block after an upgrade is therefore built by the new runtime, and checking it
/// with the old runtime fails for every inherent whose encoding changed with the upgrade, so that
/// every node but the author rejects the block.
///
/// In this context the new runtime checks that block on top of the state the old runtime left
/// behind, before its migrations ran. The runtime's `check_inherents` implementation has to apply
/// the pending upgrade first, see [`BlockBuilder::check_inherents`].
pub async fn check_inherents_with_data<Block: BlockT, Client: ProvideRuntimeApi<Block>>(
	client: Arc<Client>,
	at_hash: Block::Hash,
	block: Block,
	inherent_data_provider: &impl InherentDataProvider,
	inherent_data: InherentData,
) -> Result<(), CheckInherentsError>
where
	Client::Api: BlockBuilder<Block>,
{
	// Scoped so that the api instance, which is not `Send`, is dropped before the `.await` below.
	let res = {
		let mut api = client.runtime_api();
		api.set_call_context(CallContext::Onchain { import: false });
		api.check_inherents(at_hash, block.into(), inherent_data)
			.map_err(CheckInherentsError::Client)?
	};

	if !res.ok() {
		for (id, error) in res.into_errors() {
			match inherent_data_provider.try_handle_error(&id, &error).await {
				Some(res) => res.map_err(CheckInherentsError::CheckInherents)?,
				None => return Err(CheckInherentsError::CheckInherentsUnknownError(id)),
			}
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use sp_api::{ApiError, ApiRef, CallApiAt, CallApiAtParams, ConstructRuntimeApi};
	use sp_inherents::CheckInherentsResult;
	use sp_runtime::traits::HashingFor;
	use sp_state_machine::InMemoryBackend;
	use sp_test_primitives::{Block, Header};
	use std::sync::Mutex;

	/// Stands in for the runtime type so that `impl_runtime_apis!` generates the client side of
	/// the [`BlockBuilder`] api, which is all the tests use.
	struct Runtime;

	sp_api::impl_runtime_apis! {
		impl sp_api::Core<Block> for Runtime {
			fn version() -> sp_version::RuntimeVersion {
				unimplemented!()
			}

			fn execute_block(_: <Block as BlockT>::LazyBlock) {
				unimplemented!()
			}

			fn initialize_block(
				_: &<Block as BlockT>::Header,
			) -> sp_runtime::ExtrinsicInclusionMode {
				unimplemented!()
			}
		}

		impl crate::BlockBuilder<Block> for Runtime {
			fn apply_extrinsic(
				_: <Block as BlockT>::Extrinsic,
			) -> sp_runtime::ApplyExtrinsicResult {
				unimplemented!()
			}

			fn finalize_block() -> <Block as BlockT>::Header {
				unimplemented!()
			}

			fn inherent_extrinsics(_: InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
				unimplemented!()
			}

			fn check_inherents(
				_: <Block as BlockT>::LazyBlock,
				_: InherentData,
			) -> CheckInherentsResult {
				unimplemented!()
			}
		}
	}

	/// Answers every `check_inherents` call with a successful result and records the call context
	/// of each runtime api call.
	#[derive(Default)]
	struct RecordingClient {
		call_contexts: Mutex<Vec<CallContext>>,
	}

	impl ProvideRuntimeApi<Block> for RecordingClient {
		type Api = RuntimeApiImpl<Block, Self>;

		fn runtime_api(&self) -> ApiRef<'_, Self::Api> {
			RuntimeApi::construct_runtime_api(self)
		}
	}

	impl CallApiAt<Block> for RecordingClient {
		type StateBackend = InMemoryBackend<HashingFor<Block>>;

		fn call_api_at(&self, params: CallApiAtParams<Block>) -> Result<Vec<u8>, ApiError> {
			assert_eq!(params.function, "BlockBuilder_check_inherents");
			self.call_contexts.lock().unwrap().push(params.call_context);
			Ok(CheckInherentsResult::new().encode())
		}

		fn runtime_version_at(
			&self,
			_: <Block as BlockT>::Hash,
			_: CallContext,
		) -> Result<sp_version::RuntimeVersion, ApiError> {
			Ok(sp_version::RuntimeVersion { apis: RUNTIME_API_VERSIONS, ..Default::default() })
		}

		fn state_at(&self, _: <Block as BlockT>::Hash) -> Result<Self::StateBackend, ApiError> {
			unimplemented!("not used by the tests")
		}

		fn initialize_extensions(
			&self,
			_: <Block as BlockT>::Hash,
			_: &mut sp_externalities::Extensions,
		) -> Result<(), ApiError> {
			Ok(())
		}
	}

	fn block() -> Block {
		Block::new(Header::new_from_number(1), Vec::new())
	}

	#[test]
	fn check_inherents_calls_the_runtime_in_the_on_chain_context() {
		let client = Arc::new(RecordingClient::default());

		futures::executor::block_on(check_inherents(
			client.clone(),
			Default::default(),
			block(),
			&(),
		))
		.unwrap();

		assert_eq!(
			*client.call_contexts.lock().unwrap(),
			vec![CallContext::Onchain { import: false }],
		);
	}

	#[test]
	fn check_inherents_with_data_calls_the_runtime_in_the_on_chain_context() {
		let client = Arc::new(RecordingClient::default());

		futures::executor::block_on(check_inherents_with_data(
			client.clone(),
			Default::default(),
			block(),
			&(),
			InherentData::new(),
		))
		.unwrap();

		assert_eq!(
			*client.call_contexts.lock().unwrap(),
			vec![CallContext::Onchain { import: false }],
		);
	}
}
