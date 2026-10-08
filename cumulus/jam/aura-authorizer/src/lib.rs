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

//! Shared AURA authorization types, signing payloads, and collator proofs.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, Encode, MaxEncodedLen};
use jam_types::{Slot, WorkPackage};

mod config;
mod merkle;
mod signing;
mod token;

pub use polkadot_parachain_primitives::primitives::Id as ParaId;

pub use config::{AuthConfig, ConfigError};
pub use merkle::build_collator_tree;
pub use signing::{hash_signing_payload, signable_work_package_hash, WORK_PACKAGE_SIGN_CTX};
pub use token::{AuthToken, CollatorKey, CollatorSignature, SignatureVerifier, TokenError};

/// AURA configuration or token validation failed.
#[derive(Debug)]
pub enum AuthorizationError {
	BadConfig(config::ConfigError),
	BadToken(token::TokenError),
}

/// Select the slot's collator and validate its token.
pub fn authorize<S: token::SignatureVerifier>(
	config: &config::AuthConfig,
	token: &token::AuthToken,
	package: &WorkPackage,
	slot: Slot,
) -> Result<AuthTrace, AuthorizationError> {
	let wp_hash = signable_work_package_hash(package);
	authorize_hash::<S>(config, token, wp_hash, slot)
}

/// Select the slot's collator and validate a token against an AURA signing hash.
///
/// The caller must derive the hash from the complete work package using
/// [`signable_work_package_hash`] or a version-specific [`hash_signing_payload`] adapter.
pub fn authorize_hash<S: token::SignatureVerifier>(
	config: &config::AuthConfig,
	token: &token::AuthToken,
	work_package_hash: primitive_types::H256,
	slot: Slot,
) -> Result<AuthTrace, AuthorizationError> {
	let collator_index =
		config.expected_collator_index(slot).map_err(AuthorizationError::BadConfig)?;
	token
		.check_proof(config, collator_index)
		.map_err(AuthorizationError::BadToken)?;
	token
		.check_signature::<S>(work_package_hash)
		.map_err(AuthorizationError::BadToken)?;
	Ok(AuthTrace { author_key: token.key })
}

/// The collator authenticated by AURA for the work package.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
pub struct AuthTrace {
	pub author_key: token::CollatorKey,
}
