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

use crate::{ApiError, ApiExt, ApiRef, Core};

use sp_runtime::traits::Block as BlockT;

#[derive(Debug)]
pub enum ExecuteBlockError {
	/// Runtime Api error.
	ApiError(ApiError),
	/// Failed to get runtime version
	VersionInvalid,
}

pub fn execute_block<Block: BlockT, Api: ApiExt<Block> + Core<Block>>(
	api: &ApiRef<Api>,
	at: Block::Hash,
	block: Block,
) -> Result<(), ExecuteBlockError> {
	let core_version = api
		.api_version::<dyn Core<Block>>(at)
		.map_err(ExecuteBlockError::ApiError)?
		.ok_or(ExecuteBlockError::VersionInvalid)?;

	match core_version {
		..6 => {
			// Until version 5, `execute_block` didn't have to receive a lazy block.
			#[allow(deprecated)]
			api.execute_block_before_version_6(at, block)
				.map_err(ExecuteBlockError::ApiError)
		},
		6.. => api
			.execute_block(at, block.into_lazy_block())
			.map_err(ExecuteBlockError::ApiError),
	}
}
