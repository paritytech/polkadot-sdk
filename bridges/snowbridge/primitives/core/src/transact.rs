// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use crate::{AssetMetadata, Vec};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::H160;
use sp_runtime::RuntimeDebug;
use xcm::prelude::Location;

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct TransactInfo {
	pub kind: TransactKind,
	pub params: Vec<u8>,
}

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub enum TransactKind {
	RegisterToken,
	RegisterAgent,
	CallContract,
}

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct RegisterTokenParams {
	pub location: Location,
	pub metadata: AssetMetadata,
}

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct CallContractParams {
	pub target: H160,
	pub data: Vec<u8>,
	pub gas_limit: u64,
	pub value: u128,
}
