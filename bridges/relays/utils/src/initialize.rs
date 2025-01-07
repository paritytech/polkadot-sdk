// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Relayer initialization functions.

use parking_lot::Mutex;
use sp_tracing::{
	tracing::Level,
	tracing_subscriber::{
		fmt::{time::OffsetTime, SubscriberBuilder},
		EnvFilter,
	},
};
use std::cell::RefCell;

/// Relayer version that is provided as metric. Must be set by a binary
/// (get it with `option_env!("CARGO_PKG_VERSION")` from a binary package code).
pub static RELAYER_VERSION: Mutex<Option<String>> = Mutex::new(None);

async_std::task_local! {
	pub(crate) static LOOP_NAME: RefCell<String> = RefCell::new(String::default());
}

/// Initialize relay environment.
pub fn initialize_relay() {
	initialize_logger(true);
}

/// Initialize Relay logger instance.
pub fn initialize_logger(with_timestamp: bool) {
	let format = time::format_description::parse(
		"[year]-[month]-[day] \
		[hour repr:24]:[minute]:[second] [offset_hour sign:mandatory]",
	)
	.expect("static format string is valid");

	let local_time = OffsetTime::new(
		time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC),
		format,
	);

	let env_filter = EnvFilter::builder()
		.with_default_directive(Level::WARN.into())
		.with_default_directive("bridge=info".parse().expect("static filter string is valid"))
		.from_env_lossy();

	let builder = SubscriberBuilder::default().with_env_filter(env_filter);

	if with_timestamp {
		builder.with_timer(local_time).init();
	} else {
		builder.without_time().init();
	}
}

/// Initialize relay loop. Must only be called once per every loop task.
pub(crate) fn initialize_loop(loop_name: String) {
	LOOP_NAME.with(|g_loop_name| *g_loop_name.borrow_mut() = loop_name);
}
