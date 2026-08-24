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

//! Native PolkaVM/JAM implementations of the `logging` interface.

use crate::*;
use sp_core::{LogLevelFilter, RuntimeInterfaceLogLevel};
/// Native PolkaVM/JAM implementation of `log`.
pub fn log(_level: RuntimeInterfaceLogLevel, _target: &str, _message: &[u8]) {}

/// Native PolkaVM/JAM implementation of `max_level`.
pub fn max_level() -> LogLevelFilter {
	LogLevelFilter::Off
}
