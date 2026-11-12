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

use sp_api::{ApiExt, CallContext, Core};
use sp_inherents::{InherentData, InherentDataProvider, InherentIdentifier};
use sp_runtime::traits::Block as BlockT;

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
pub async fn check_inherents<Block: BlockT, Client: sp_api::ProvideRuntimeApi<Block>>(
	client: std::sync::Arc<Client>,
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
pub async fn check_inherents_with_data<Block: BlockT, Client: sp_api::ProvideRuntimeApi<Block>>(
	client: std::sync::Arc<Client>,
	at_hash: Block::Hash,
	block: Block,
	inherent_data_provider: &impl InherentDataProvider,
	inherent_data: InherentData,
) -> Result<(), CheckInherentsError>
where
	Client::Api: BlockBuilder<Block>,
{
	let mut runtime_api = client.runtime_api();
	runtime_api.set_call_context(CallContext::Onchain { import: true });

	// Initialize the block first to run any pending runtime migrations, as the block may have been
	// built by a new runtime.
	if runtime_api
		.has_api_with::<dyn Core<Block>, _>(at_hash, |v| v >= 5)
		.map_err(CheckInherentsError::Client)?
	{
		runtime_api
			.initialize_block(at_hash, block.header())
			.map_err(CheckInherentsError::Client)?;
	} else {
		#[allow(deprecated)]
		runtime_api
			.initialize_block_before_version_5(at_hash, block.header())
			.map_err(CheckInherentsError::Client)?;
	}

	let res = runtime_api
		.check_inherents(at_hash, block.into(), inherent_data)
		.map_err(CheckInherentsError::Client)?;

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
