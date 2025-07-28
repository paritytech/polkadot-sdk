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

use codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;
use std::ops::Deref;

/// Wrapper for BlockV3 that implements `MaxEncodedLen`.
#[derive(Debug, Clone, PartialEq, Encode, Decode, TypeInfo)]
pub struct BlockV3(ethereum::BlockV3);

impl codec::MaxEncodedLen for BlockV3 {
	fn max_encoded_len() -> usize {
		usize::MAX
	}
}

// Passthrough implementations for Encode and Decode
impl From<ethereum::BlockV3> for BlockV3 {
	fn from(block: ethereum::BlockV3) -> Self {
		BlockV3(block)
	}
}

impl From<BlockV3> for ethereum::BlockV3 {
	fn from(wrapper: BlockV3) -> Self {
		wrapper.0
	}
}

impl Deref for BlockV3 {
	type Target = ethereum::BlockV3;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

/// Wrapper for ReceiptV4 that implements `MaxEncodedLen`.
#[derive(Debug, Clone, PartialEq, Encode, Decode, TypeInfo)]
pub struct ReceiptV4(ethereum::ReceiptV4);

impl codec::MaxEncodedLen for ReceiptV4 {
	fn max_encoded_len() -> usize {
		usize::MAX
	}
}

// Passthrough implementations for Encode and Decode
impl From<ethereum::ReceiptV4> for ReceiptV4 {
	fn from(block: ethereum::ReceiptV4) -> Self {
		ReceiptV4(block)
	}
}

impl From<ReceiptV4> for ethereum::ReceiptV4 {
	fn from(wrapper: ReceiptV4) -> Self {
		wrapper.0
	}
}

impl Deref for ReceiptV4 {
	type Target = ethereum::ReceiptV4;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
