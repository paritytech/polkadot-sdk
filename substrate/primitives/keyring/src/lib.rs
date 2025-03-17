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

//! Support code for the runtime. A set of test accounts.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use alloc::fmt;

/// Test account crypto for sr25519.
pub mod sr25519;

/// Test account crypto for ed25519.
pub mod ed25519;

/// Test account crypto for bandersnatch.
#[cfg(feature = "bandersnatch-experimental")]
pub mod bandersnatch;

#[cfg(feature = "bandersnatch-experimental")]
pub use bandersnatch::Keyring as BandersnatchKeyring;
pub use ed25519::Keyring as Ed25519Keyring;
pub use sr25519::Keyring as Sr25519Keyring;

#[derive(Debug)]
/// Represents an error that occurs when parsing a string into a `KeyRing`.
pub struct ParseKeyringError;

impl fmt::Display for ParseKeyringError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "ParseKeyringError")
	}
}
