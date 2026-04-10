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

//! Substrate core types and inherents for node version tracking.
//!
//! # Backwards Compatibility
//!
//! The node-side `InherentDataProvider` always provides the version hash data via the
//! `ndvrsn00` inherent identifier. If the runtime has not been upgraded to include
//! `pallet-node-version`, the unknown inherent identifier is silently ignored:
//!
//! - **Block authoring**: `create_inherents` only produces extrinsics for pallets that
//!   claim the identifier via `ProvideInherent::INHERENT_IDENTIFIER`. Since no pallet
//!   claims `ndvrsn00`, no extrinsic is created — the inherent data is simply unused.
//!
//! - **Block import / `check_inherents`**: Only inherent identifiers claimed by a pallet
//!   are validated. Unknown identifiers are skipped, so the extra data does not cause
//!   block rejection.
//!
//! This means a **node upgrade can safely happen before the runtime upgrade**. The node
//! will start providing the inherent data immediately, but it will only take effect once
//! the runtime is upgraded to include `pallet-node-version`.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use sp_core::H256;
use sp_inherents::{InherentData, InherentIdentifier, IsFatalError};

/// The identifier for the `node-version` inherent.
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"ndvrsn00";

/// The type of the inherent: a blake2_256 hash of the node version string.
pub type InherentType = H256;

/// Errors that can occur while checking the node version inherent.
#[derive(Encode, Debug)]
#[cfg_attr(feature = "std", derive(Decode, thiserror::Error))]
pub enum InherentError {
	/// The version hash in the inherent does not match what we expected.
	#[cfg_attr(feature = "std", error("Version hash mismatch."))]
	VersionMismatch,
}

impl IsFatalError for InherentError {
	fn is_fatal_error(&self) -> bool {
		false
	}
}

impl InherentError {
	/// Try to create an instance out of the given identifier and data.
	#[cfg(feature = "std")]
	pub fn try_from(id: &InherentIdentifier, mut data: &[u8]) -> Option<Self> {
		if id == &INHERENT_IDENTIFIER {
			<InherentError as codec::Decode>::decode(&mut data).ok()
		} else {
			None
		}
	}
}

/// Auxiliary trait to extract node version inherent data.
pub trait NodeVersionInherentData {
	/// Get node version inherent data.
	fn node_version_inherent_data(&self) -> Result<Option<InherentType>, sp_inherents::Error>;
}

impl NodeVersionInherentData for InherentData {
	fn node_version_inherent_data(&self) -> Result<Option<InherentType>, sp_inherents::Error> {
		self.get_data(&INHERENT_IDENTIFIER)
	}
}

/// Provide the node version hash as inherent data.
#[cfg(feature = "std")]
#[derive(Clone)]
pub struct InherentDataProvider {
	version_hash: InherentType,
}

#[cfg(feature = "std")]
impl InherentDataProvider {
	/// Create a new provider from a version string.
	///
	/// The string is hashed with blake2_256 to produce the version hash.
	pub fn new(version_string: &str) -> Self {
		Self { version_hash: sp_core::blake2_256(version_string.as_bytes()).into() }
	}

	/// Create a new provider from a pre-computed hash.
	pub fn from_hash(version_hash: InherentType) -> Self {
		Self { version_hash }
	}

	/// Returns the version hash.
	pub fn version_hash(&self) -> InherentType {
		self.version_hash
	}
}

#[cfg(feature = "std")]
#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for InherentDataProvider {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data.put_data(INHERENT_IDENTIFIER, &self.version_hash)
	}

	async fn try_handle_error(
		&self,
		identifier: &InherentIdentifier,
		error: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		Some(Err(sp_inherents::Error::Application(Box::from(InherentError::try_from(
			identifier, error,
		)?))))
	}
}
