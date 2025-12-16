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

//! Runtime API definition for the transaction storage proof processing.

use sp_runtime::traits::NumberFor;

sp_api::decl_runtime_apis! {
	/// Runtime API trait for transaction storage support.
	pub trait TransactionStorageApi {
		/// Get the actual value of a storage period in blocks.
		fn storage_period() -> NumberFor<Block>;
	}
}

#[cfg(feature = "std")]
pub mod client {
	use codec::Decode;
	use sp_api::{ApiError, CallApiAt, CallApiAtParams};
	use sp_core::traits::CallContext;
	use sp_runtime::traits::{Block as BlockT, NumberFor};

	/// An expected state call key.
	pub const TRANSACTION_STORAGE_API_STORAGE_PERIOD: &'static str =
		"TransactionStorageApi_storage_period";

	/// Fetches the storage period value for a specific block from the runtime API.
	///
	/// This function interacts with the `TRANSACTION_STORAGE_API_STORAGE_PERIOD` API
	/// provided by the runtime.
	///
	/// # Arguments
	/// - `client`: A reference to an object implementing the `CallApiAt` trait, used to interact
	///   with the runtime API.
	/// - `at_block`: The hash of the specific block for which the storage period is queried.
	pub fn retrieve_storage_period<B, C>(
		client: &C,
		at_block: B::Hash,
	) -> Result<NumberFor<B>, ApiError>
	where
		B: BlockT,
		C: CallApiAt<B>,
	{
		// Call the expected runtime API.
		let result = client.call_api_at(CallApiAtParams {
			at: at_block,
			function: TRANSACTION_STORAGE_API_STORAGE_PERIOD,
			arguments: vec![],
			overlayed_changes: &Default::default(),
			call_context: CallContext::Onchain,
			recorder: &None,
			extensions: &Default::default(),
		})?;

		// Decode to `NumberFor<B>`.
		Decode::decode(&mut &result[..]).map_err(|e| ApiError::FailedToDecodeReturnValue {
			function: TRANSACTION_STORAGE_API_STORAGE_PERIOD,
			error: e,
			raw: result,
		})
	}
}
