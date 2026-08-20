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
pub struct UploadCodeInputPayloadV1<AccountId, Balance> {
	pub origin: AccountId,
	pub code: Vec<u8>,
	pub storage_deposit_limit: Option<Balance>,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto)]
pub enum UploadCodeVersionedInputPayload<AccountId, Balance> {
	V1(UploadCodeInputPayloadV1<AccountId, Balance>),
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct UploadCodeOutputPayloadV1<Balance> {
	pub code_upload_return_value: CodeUploadReturnValueV1<Balance>,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto)]
pub enum UploadCodeVersionedOutputPayload<Balance> {
	V1(UploadCodeOutputPayloadV1<Balance>),
}
