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

//! Native PolkaVM/JAM implementations of the `panic_handler` interface.

use crate::*;
use sp_core::RuntimeInterfaceLogLevel;

/// Native PolkaVM/JAM implementation of `abort_on_panic`.
pub fn abort_on_panic(message: &str) -> ! {
	// Without this the abort is a bare `trap` at some program counter, with nothing to say
	// which assertion fired.
	logging::log(RuntimeInterfaceLogLevel::Error, "runtime", message.as_bytes());
	crate::unreachable()
}
