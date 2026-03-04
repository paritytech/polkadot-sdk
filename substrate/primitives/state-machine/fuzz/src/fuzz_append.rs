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

//! Differential fuzzer for storage append operations.
//!
//! Compares a simple reference overlay implementation against the real
//! `OverlayedChanges`/`TrieBackend` stack to verify correctness of storage append, insert,
//! remove, and nested transaction operations.
//!
//! ## Running
//! Run with `cargo ziggy fuzz -j 4 --no-honggfuzz -G 128`.
//!
//! ## Coverage
//! Generate coverage reports with `cargo ziggy cover -s ..`.

use sp_runtime::traits::BlakeTwo256;
use sp_state_machine::fuzzing::{fuzz_append, FuzzAppendPayload};

fn main() {
	ziggy::fuzz!(|data: FuzzAppendPayload| {
		fuzz_append::<BlakeTwo256>(data);
	});
}
