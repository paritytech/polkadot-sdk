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

//! Native PolkaVM/JAM implementations of the `offchain_index` interface.

use crate::*;
/// Native PolkaVM/JAM implementation of `clear`.
pub fn clear(_key: &[u8]) {
	panic!("`offchain_index::clear` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `set`.
pub fn set(_key: &[u8], _value: &[u8]) {
	panic!("`offchain_index::set` needs node-side state and has no in-blob implementation")
}
