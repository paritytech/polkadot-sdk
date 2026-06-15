// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

use alloc::vec::Vec;
use codec::Encode;
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_core::Get;
use sp_io::hashing::blake2_256;
use sp_runtime::BoundedVec;

// Domain Tags to ensure that the same message structure used in different
// contexts (e.g. leaf vs inner node) do not collide on the same hash.

/// Tag for an empty MMR.
pub const EMPTY_TAG: u8 = 0x1;
/// Tag for a leaf node.
pub const LEAF_TAG: u8 = 0x2;
/// Tag for an inner node.
pub const INNER_TAG: u8 = 0x3;
/// Tag for a peak.
pub const PEAK_TAG: u8 = 0x4;

// Leaf versioning to allow for future changes to the leaf structure without
// breaking compatibility with old messages.

/// Leaf Version.
pub const LEAF_VERSION: u8 = 0x0;

/// Outgoing message structure.
#[derive(
	Clone,
	codec::Encode,
	codec::MaxEncodedLen,
	codec::Decode,
	codec::DecodeWithMemTracking,
	Debug,
	Default,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct OutgoingMessage<MaxMsgLen: Get<u32>> {
	pub source: ParaId,
	pub destination: ParaId,
	pub position: u64,
	pub payload: BoundedVec<u8, MaxMsgLen>,
}

impl<MaxMsgLen: Get<u32>> OutgoingMessage<MaxMsgLen> {
	pub fn new(
		source: ParaId,
		destination: ParaId,
		position: u64,
		payload: BoundedVec<u8, MaxMsgLen>,
	) -> Self {
		Self { source, destination, position, payload }
	}

	pub fn hash_leaf(&self) -> Hash {
		let mut preimage = Vec::new();
		preimage.extend_from_slice(&LEAF_TAG.to_le_bytes());
		preimage.extend_from_slice(&LEAF_VERSION.to_le_bytes());
		preimage.extend_from_slice(&self.source.encode());
		preimage.extend_from_slice(&self.destination.encode());
		preimage.extend_from_slice(&self.position.to_le_bytes());
		preimage.extend_from_slice(&self.payload.len().to_le_bytes());
		preimage.extend_from_slice(&self.payload);
		blake2_256(&preimage).into()
	}
}
