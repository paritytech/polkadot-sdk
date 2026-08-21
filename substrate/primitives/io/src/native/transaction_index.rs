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

//! Native PolkaVM/JAM implementations of the `transaction_index` interface.

use crate::*;
#[cfg(feature = "bandersnatch-experimental")]
use sp_core::bandersnatch;
#[cfg(feature = "bls-experimental")]
use sp_core::{bls381, ecdsa_bls381};
/// Native PolkaVM/JAM implementation of `index`.
pub fn index(_extrinsic: u32, _size: u32, _context_hash: [u8; 32]) {
	panic!("`transaction_index::index` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `renew`.
pub fn renew(_extrinsic: u32, _context_hash: [u8; 32]) {
	panic!("`transaction_index::renew` needs node-side state and has no in-blob implementation")
}
