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
//!
//! Build the work-package authorizer with `cargo jam-build -p cumulus-aura-authorizer-ed25519`.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub mod sign;

use cumulus_aura_authorizer_utils::{
	AuthConfig, AuthToken, AuthTrace, AuthorizationError, CollatorKey, CollatorSignature,
	SignatureVerifier,
};
use jam_types::{Slot, WorkPackage};

#[cfg(target_env = "polkavm")]
struct Authorizer;

#[cfg(target_env = "polkavm")]
jam_pvm_common::declare_authorizer!(Authorizer);

// Reserve 8 KiB; the Dalek-backed authorizer peaked at 5,984 bytes in the blob test.
#[cfg(target_env = "polkavm")]
polkavm_derive::min_stack_size!(8192);

#[cfg(target_env = "polkavm")]
impl jam_pvm_common::Authorizer for Authorizer {
	fn is_authorized(_: jam_types::CoreIndex) -> jam_types::AuthTrace {
		let package = jam_pvm_common::is_authorized::work_package();
		cumulus_aura_authorizer_utils::is_authorized::<Ed25519>(&package)
	}
}

/// Ed25519 signature verifier for AURA collators, matching `sp_core::ed25519`.
pub struct Ed25519;

impl SignatureVerifier for Ed25519 {
	fn verify(key: &CollatorKey, signature: &CollatorSignature, payload: &[u8]) -> bool {
		let Ok(key) = ed25519_dalek::VerifyingKey::from_bytes(key) else { return false };
		key.verify_strict(payload, &ed25519_dalek::Signature::from_bytes(signature))
			.is_ok()
	}
}

/// Check whether the AURA token authorizes this work package for the given slot using [`Ed25519`].
pub fn authorize(
	config: &AuthConfig,
	token: &AuthToken,
	package: &WorkPackage,
	slot: Slot,
) -> Result<AuthTrace, AuthorizationError> {
	AuthTrace::try_from_token::<Ed25519>(config, token, package, slot)
}
