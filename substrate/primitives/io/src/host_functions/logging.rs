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

use sp_core::{LogLevelFilter, RuntimeInterfaceLogLevel};

use sp_runtime_interface::{
	pass_by::{PassAs, PassFatPointerAndRead, ReturnAs},
	runtime_interface,
};

#[allow(unused_imports)]
use crate::*;

/// Interface that provides functions for logging from within the runtime.
#[runtime_interface]
pub trait Logging {
	/// Request to print a log message on the host.
	///
	/// Note that this will be only displayed if the host is enabled to display log messages with
	/// given level and target.
	///
	/// Instead of using directly, prefer setting up `RuntimeLogger` and using `log` macros.
	fn log(
		level: PassAs<RuntimeInterfaceLogLevel, u8>,
		target: PassFatPointerAndRead<&str>,
		message: PassFatPointerAndRead<&[u8]>,
	) {
		if let Ok(message) = core::str::from_utf8(message) {
			log::log!(target: target, log::Level::from(level), "{}", message)
		}
	}

	/// Returns the max log level used by the host.
	fn max_level() -> ReturnAs<LogLevelFilter, u8> {
		log::max_level().into()
	}
}
