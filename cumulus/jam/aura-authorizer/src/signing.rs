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

use alloc::vec::Vec;
use jam_types::{Encode as JamEncode, WorkPackage};
use primitive_types::H256;
use sp_crypto_hashing::blake2_256;

/// Domain separator for the token-free work-package hash signed by AURA collators.
pub const WORK_PACKAGE_SIGN_CTX: &[u8] = b"jam:parachain-service:aura:work-package:v1";

/// Hash of a work-package that can be signed by AURA collators.
///
/// JAM-encodes the code host, authorizer code hash, refinement context, authorizer config,
/// and work items after [`WORK_PACKAGE_SIGN_CTX`], then hashes them with Blake2b-256.
/// The authorization token is excluded because it contains the signature.
pub fn signable_work_package_hash(package: &WorkPackage) -> H256 {
	let payload = JamEncode::encode(&(
		&package.auth_code_host,
		&package.authorizer.code_hash,
		&package.context,
		&package.authorizer.config,
		&package.items,
	));
	hash_signing_payload(&payload)
}

/// Hash the JAM-encoded tuple of code host, authorizer code hash, refinement context,
/// authorizer config, and work items, with the AURA domain separator.
///
/// Adapters for other JAM versions must preserve their version's complete context encoding
/// and exclude the authorization token, just as [`signable_work_package_hash`] does.
pub fn hash_signing_payload(payload: &[u8]) -> H256 {
	let mut signable = Vec::with_capacity(WORK_PACKAGE_SIGN_CTX.len() + payload.len());
	signable.extend_from_slice(WORK_PACKAGE_SIGN_CTX);
	signable.extend_from_slice(payload);

	H256::from(blake2_256(&signable))
}
