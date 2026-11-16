// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use codec::{Decode, Encode};
use derive_more::{From, TryInto};
use scale_info::TypeInfo;

use crate::runtime_api::*;

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct BlockInputPayloadV1;

/// The input type used when calling the `eth_block_versioned` runtime API function. This function
/// replaces the unversioned `eth_block` runtime API function.
#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto)]
pub enum BlockVersionedInputPayload {
	/// The arguments provided when calling the `eth_block_versioned` runtime API function.
	///
	/// When this version is provided, the function behaves identically to and returns the same
	/// output as the unversioned `eth_block` runtime API function.
	V1(BlockInputPayloadV1),
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct BlockOutputPayloadV1 {
	pub block: BlockV1,
}

/// The output type returned when calling the `eth_block_versioned` runtime API function. This
/// function replaces the unversioned `eth_block` runtime API function.
#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto)]
pub enum BlockVersionedOutputPayload {
	/// The output returned when calling the `eth_block_versioned` runtime API function with `V1`
	/// arguments.
	///
	/// This output is identical to the output returned by the unversioned `eth_block` runtime API
	/// function. `transactions` is the list the block's `transactions_root` commits to. On a
	/// runtime that mirrors substrate-native balance changes as EVM logs, a block carrying such
	/// logs ends with one synthetic transaction for them; `V1` of `eth_receipt_data_versioned` has
	/// no entry for it, `V2` reports it apart.
	V1(BlockOutputPayloadV1),
}
