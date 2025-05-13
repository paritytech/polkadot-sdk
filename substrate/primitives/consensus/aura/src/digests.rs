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

//! Aura (Authority-Round) digests
//!
//! This implements the digests for AuRa, to allow the private
//! `CompatibleDigestItem` trait to appear in public interfaces.

use crate::AURA_ENGINE_ID;
use codec::{Codec, Encode};
use sp_consensus_slots::Slot;
use sp_runtime::generic::DigestItem;

/// A digest item which is usable with aura consensus.
pub trait CompatibleDigestItem<Signature>: Sized {
	/// Construct a digest item which contains a signature on the hash.
	fn aura_seal(signature: Signature) -> Self;

	/// If this item is an Aura seal, return the signature.
	fn as_aura_seal(&self) -> Option<Signature>;

	/// Construct a digest item which contains the slot number
	fn aura_pre_digest(slot: Slot) -> Self;

	/// If this item is an AuRa pre-digest, return the slot number
	fn as_aura_pre_digest(&self) -> Option<Slot>;
}

impl<Signature> CompatibleDigestItem<Signature> for DigestItem
where
	Signature: Codec,
{
	fn aura_seal(signature: Signature) -> Self {
		DigestItem::Seal(AURA_ENGINE_ID, signature.encode())
	}

	fn as_aura_seal(&self) -> Option<Signature> {
		self.seal_try_to(&AURA_ENGINE_ID)
	}

	fn aura_pre_digest(slot: Slot) -> Self {
		DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())
	}

	fn as_aura_pre_digest(&self) -> Option<Slot> {
		self.pre_runtime_try_to(&AURA_ENGINE_ID)
	}
}
