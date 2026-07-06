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

use alloc::vec::Vec;
use codec::{Decode, Encode};
use derive_more::{From, TryInto};
use scale_info::TypeInfo;

use crate::runtime_api::*;

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct TraceBlockInputPayloadV1<Block> {
	pub block: Block,
	pub config: TracerTypeV1,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct TraceBlockInputPayloadV2<Block> {
	pub block: Block,
	pub config: TracerTypeV1,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto)]
pub enum TraceBlockVersionedInputPayload<Block> {
	V1(TraceBlockInputPayloadV1<Block>),
	V2(TraceBlockInputPayloadV2<Block>),
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct TraceBlockOutputPayloadV1 {
	pub traces: Vec<(u32, TraceV1)>,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct TraceBlockOutputPayloadV2 {
	pub traces: Vec<(u32, TraceV2)>,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto)]
pub enum TraceBlockVersionedOutputPayload {
	V1(TraceBlockOutputPayloadV1),
	V2(TraceBlockOutputPayloadV2),
}
