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

use super::LOG_TARGET;
use sp_core::{LogLevelFilter, RuntimeInterfaceLogLevel};
use sp_runtime_interface::{
	pass_by::{PassAs, PassFatPointerAndRead, ReturnAs},
	runtime_interface,
};
use std::cell::OnceCell;

thread_local! {
	/// Log level filter that the runtime will use.
	///
	/// Must be initialized by the host before invoking the runtime executor. You may use `init` for
	/// this or set it manually. The that can be set are either levels directly or filter like
	// `warn,runtime=info`.
	pub static RUNTIME_LOG: OnceCell<env_filter::Filter> = OnceCell::new();
}

/// Init runtime logger with the following priority (high to low):
/// - CLI argument
/// - Environment variable
/// - Default logger settings
pub fn init(arg: Option<String>) {
	let filter_str = arg.unwrap_or_else(|| {
		if let Ok(env) = std::env::var("RUNTIME_LOG") {
			env
		} else {
			log::max_level().to_string()
		}
	});

	let filter = env_filter::Builder::new()
		.try_parse(&filter_str)
		.expect("Invalid runtime log filter")
		.build();

	RUNTIME_LOG.with(|cell| {
		cell.set(filter).expect("Can be set by host");
		log::info!(target: LOG_TARGET, "Initialized runtime log filter to '{}'", filter_str);
	});
}

/// Alternative implementation to `sp_runtime_interface::logging::HostFunctions` for benchmarking.
#[runtime_interface]
pub trait Logging {
	#[allow(dead_code)]
	fn log(
		level: PassAs<RuntimeInterfaceLogLevel, u8>,
		target: PassFatPointerAndRead<&str>,
		message: PassFatPointerAndRead<&[u8]>,
	) {
		let Ok(message) = core::str::from_utf8(message) else {
			log::error!(target: LOG_TARGET, "Runtime tried to log invalid UTF-8 data");
			return;
		};

		let level = log::Level::from(level);
		let metadata = log::MetadataBuilder::new().level(level).target(target).build();

		if RUNTIME_LOG.with(|filter| filter.get().expect("Must be set by host").enabled(&metadata))
		{
			log::log!(target: target, level, "{}", message);
		}
	}

	#[allow(dead_code)]
	fn max_level() -> ReturnAs<LogLevelFilter, u8> {
		RUNTIME_LOG
			// .filter() gives us the max level of this filter
			.with(|filter| filter.get().expect("Must be set by host").filter())
			.into()
	}
}
