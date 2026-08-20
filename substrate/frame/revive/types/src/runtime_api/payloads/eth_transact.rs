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
pub struct TransactInputPayloadV1<Moment> {
	pub tx: GenericTransactionV1,
	pub timestamp_override: Option<Moment>,
	pub perform_balance_checks: bool,
	pub state_overrides: Option<StateOverrideSetV1>,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto)]
pub enum TransactVersionedInputPayload<Moment> {
	V1(TransactInputPayloadV1<Moment>),
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct TransactOutputPayloadV1<Balance> {
	pub transact_info: EthTransactInfoV1<Balance>,
}

#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto)]
pub enum TransactVersionedOutputPayload<Balance> {
	V1(TransactOutputPayloadV1<Balance>),
}
