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
pub struct TraceTxInputPayloadV1<Block> {
	pub block: Block,
	pub tx_index: u32,
	pub config: TracerTypeV1,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct TraceTxInputPayloadV2<Block> {
	pub block: Block,
	pub tx_index: u32,
	pub config: TracerTypeV1,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto)]
pub enum TraceTxVersionedInputPayload<Block> {
	V1(TraceTxInputPayloadV1<Block>),
	V2(TraceTxInputPayloadV2<Block>),
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct TraceTxOutputPayloadV1 {
	pub trace: Option<TraceV1>,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct TraceTxOutputPayloadV2 {
	pub trace: Option<TraceV2>,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto)]
pub enum TraceTxVersionedOutputPayload {
	V1(TraceTxOutputPayloadV1),
	V2(TraceTxOutputPayloadV2),
}
