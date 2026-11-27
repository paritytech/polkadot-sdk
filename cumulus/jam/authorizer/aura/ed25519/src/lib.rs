// This file is part of Cumulus.

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

//! Ed25519 AURA authorizer for JAM parachains.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub mod sign;

use cumulus_aura_authorizer_utils::{
	AuthConfig, AuthToken, AuthTrace, AuthorizationError, CollatorKey, CollatorSignature,
	SignatureVerifier,
};
use jam_types::{Slot, WorkPackage};

/// Ed25519 signature verifier for AURA collators, matching `sp_core::ed25519`.
pub struct Ed25519;

impl SignatureVerifier for Ed25519 {
	/// Uses `verify_strict` (not `verify`) to reject cofactored/non-canonical signatures and
	/// low-order public keys for deterministic validator agreement.
	fn verify(key: &CollatorKey, signature: &CollatorSignature, payload: &[u8]) -> bool {
		let Ok(key) = ed25519_dalek::VerifyingKey::from_bytes(key) else { return false };
		key.verify_strict(payload, &ed25519_dalek::Signature::from_bytes(signature))
			.is_ok()
	}
}

/// Validate a work package and its AURA token using [`Ed25519`].
pub fn authorize(
	config: &AuthConfig,
	token: &AuthToken,
	package: &WorkPackage,
	slot: Slot,
) -> Result<AuthTrace, AuthorizationError> {
	AuthTrace::try_from_token::<Ed25519>(config, token, package, slot)
}
