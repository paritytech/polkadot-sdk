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

//! Simple primitives for proofs and validation.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
// TODO: FAIL-CI - is this a good location? Move it somewhere else? E.g.:
// TODO: FAIL-CI - `sp-runtime/src/proving_trie does not have frame-support
pub mod proving;

/// A trait representing a provider of root hashes.
pub trait ProvideHash {
    /// A key type.
    type Key;
    /// A hash type.
    type Hash;

    /// Returns the proof root `Hash` for the given `key`.
    fn provide_hash_for(key: Self::Key) -> Option<Self::Hash>;
}
