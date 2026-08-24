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

//! Native PolkaVM/JAM implementations of the `misc` interface.

use crate::*;
use alloc::{vec, vec::Vec};
/// Native PolkaVM/JAM implementation of `last_cursor`.
pub fn last_cursor(_out: &mut [u8]) -> Option<u32> {
	panic!("`misc::last_cursor` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `print_hex`.
pub fn print_hex(_data: &[u8]) {}

/// Native PolkaVM/JAM implementation of `print_num`.
pub fn print_num(_val: u64) {}

/// Native PolkaVM/JAM implementation of `print_utf8`.
pub fn print_utf8(_utf8: &[u8]) {}

/// Native PolkaVM/JAM implementation of `runtime_version__raw`.
pub fn runtime_version__raw(_wasm: &[u8], _out: &mut [u8]) -> Option<u32> {
	panic!("`misc::runtime_version__raw` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `runtime_version`.
pub fn runtime_version(code: impl AsRef<[u8]>) -> Option<Vec<u8>> {
	let mut version = vec![0u8; 1024];
	let len = runtime_version__raw(code.as_ref(), &mut version[..])?;
	if len as usize > version.len() {
		version.resize(len as usize, 0);
		runtime_version__raw(code.as_ref(), &mut version[..])?;
	}
	version.truncate(len as usize);
	Some(version)
}
