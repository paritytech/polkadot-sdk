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

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto)]
pub enum BlockVersionedInputPayload {
	V1(BlockInputPayloadV1),
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct BlockOutputPayloadV1 {
	pub block: BlockV1,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto)]
pub enum BlockVersionedOutputPayload {
	V1(BlockOutputPayloadV1),
}
