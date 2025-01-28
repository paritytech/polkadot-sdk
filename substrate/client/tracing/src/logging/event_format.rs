// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::logging::fast_local_time::FastLocalTime;
use console::style;
use std::fmt;
use tracing::{Event, Level, Subscriber};
use tracing_log::NormalizeEvent;
use tracing_subscriber::{
	fmt::{format, time::FormatTime, FmtContext, FormatEvent, FormatFields},
	registry::LookupSpan,
};

/// A pre-configured event formatter.
pub struct EventFormat<T = FastLocalTime> {
	/// Use the given timer for log message timestamps.
	pub timer: T,
	/// Sets whether or not an event's target is displayed.
	pub display_target: bool,
	/// Sets whether or not an event's level is displayed.
	pub display_level: bool,
	/// Sets whether or not the name of the current thread is displayed when formatting events.
	pub display_thread_name: bool,
	/// Duplicate INFO, WARN and ERROR messages to stdout.
	pub dup_to_stdout: bool,
}

impl<T> EventFormat<T>
where
	T: FormatTime,
{
	// NOTE: the following code took inspiration from tracing-subscriber
	//
	//       https://github.com/tokio-rs/tracing/blob/2f59b32/tracing-subscriber/src/fmt/format/mod.rs#L449
	pub(crate) fn format_event_custom<'b, 'w, S, N>(
		&self,
		ctx: &FmtContext<'b, S, N>,
		mut writer: format::Writer<'w>,
		event: &Event,
	) -> fmt::Result
	where
		S: Subscriber + for<'a> LookupSpan<'a>,
		N: for<'a> FormatFields<'a> + 'static,
	{
		let normalized_meta = event.normalized_metadata();
		let meta = normalized_meta.as_ref().unwrap_or_else(|| event.metadata());
		time::write(&self.timer, &mut format::Writer::new(&mut writer))?;

		if self.display_level {
			let fmt_level = FmtLevel::new(meta.level());
			write!(writer, "{} ", fmt_level)?;
		}

		if self.display_thread_name {
			let current_thread = std::thread::current();
			match current_thread.name() {
				Some(name) => {
					write!(&mut writer, "{} ", FmtThreadName::new(name))?;
				},
				// fall-back to thread id when name is absent and ids are not enabled
				None => {
					write!(&mut writer, "{:0>2?} ", current_thread.id())?;
				},
			}
		}

		if self.display_target {
			write!(&mut writer, "{}: ", meta.target())?;
		}

		// Custom code to display node name
		if let Some(span) = ctx.lookup_current() {
			for span in span.scope() {
				let exts = span.extensions();
				if let Some(prefix) = exts.get::<super::layers::Prefix>() {
					write!(&mut writer, "{}", prefix.as_str())?;
					break
				}
			}
		}

		ctx.format_fields(format::Writer::new(&mut writer), event)?;
		writeln!(&mut writer)?;

		Ok(())
	}
}

// NOTE: the following code took inspiration from tracing-subscriber
//
//       https://github.com/tokio-rs/tracing/blob/2f59b32/tracing-subscriber/src/fmt/format/mod.rs#L449
impl<S, N, T> FormatEvent<S, N> for EventFormat<T>
where
	S: Subscriber + for<'a> LookupSpan<'a>,
	N: for<'a> FormatFields<'a> + 'static,
	T: FormatTime,
{
	fn format_event(
		&self,
		ctx: &FmtContext<S, N>,
		mut writer: format::Writer<'_>,
		event: &Event,
	) -> fmt::Result {
		if self.dup_to_stdout &&
			(event.metadata().level() == &Level::INFO ||
				event.metadata().level() == &Level::WARN ||
				event.metadata().level() == &Level::ERROR)
		{
			let mut out = String::new();
			let buf_writer = format::Writer::new(&mut out);
			self.format_event_custom(ctx, buf_writer, event)?;
			writer.write_str(&out)?;
			print!("{}", out);
			Ok(())
		} else {
			self.format_event_custom(ctx, writer, event)
		}
	}
}

struct FmtLevel<'a> {
	level: &'a Level,
}

impl<'a> FmtLevel<'a> {
	pub(crate) fn new(level: &'a Level) -> Self {
		Self { level }
	}
}

const TRACE_STR: &str = "TRACE";
const DEBUG_STR: &str = "DEBUG";
const INFO_STR: &str = " INFO";
const WARN_STR: &str = " WARN";
const ERROR_STR: &str = "ERROR";

impl<'a> fmt::Display for FmtLevel<'a> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match *self.level {
			Level::TRACE => write!(f, "{}", style(TRACE_STR).magenta()),
			Level::DEBUG => write!(f, "{}", style(DEBUG_STR).blue()),
			Level::INFO => write!(f, "{}", style(INFO_STR).green()),
			Level::WARN => write!(f, "{}", style(WARN_STR).yellow()),
			Level::ERROR => write!(f, "{}", style(ERROR_STR).red()),
		}
	}
}

struct FmtThreadName<'a> {
	name: &'a str,
}

impl<'a> FmtThreadName<'a> {
	pub(crate) fn new(name: &'a str) -> Self {
		Self { name }
	}
}

// NOTE: the following code has been duplicated from tracing-subscriber
//
//       https://github.com/tokio-rs/tracing/blob/2f59b32/tracing-subscriber/src/fmt/format/mod.rs#L845
impl<'a> fmt::Display for FmtThreadName<'a> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		use std::sync::atomic::{
			AtomicUsize,
			Ordering::{AcqRel, Acquire, Relaxed},
		};

		// Track the longest thread name length we've seen so far in an atomic,
		// so that it can be updated by any thread.
		static MAX_LEN: AtomicUsize = AtomicUsize::new(0);
		let len = self.name.len();
		// Snapshot the current max thread name length.
		let mut max_len = MAX_LEN.load(Relaxed);

		while len > max_len {
			// Try to set a new max length, if it is still the value we took a
			// snapshot of.
			match MAX_LEN.compare_exchange(max_len, len, AcqRel, Acquire) {
				// We successfully set the new max value
				Ok(_) => break,
				// Another thread set a new max value since we last observed
				// it! It's possible that the new length is actually longer than
				// ours, so we'll loop again and check whether our length is
				// still the longest. If not, we'll just use the newer value.
				Err(actual) => max_len = actual,
			}
		}

		// pad thread name using `max_len`
		write!(f, "{:>width$}", self.name, width = max_len)
	}
}

// NOTE: the following code has been duplicated from tracing-subscriber
//
//       https://github.com/tokio-rs/tracing/blob/2f59b32/tracing-subscriber/src/fmt/time/mod.rs#L252
mod time {
	use std::fmt;
	use tracing_subscriber::fmt::{format, time::FormatTime};

	pub(crate) fn write<T>(timer: T, writer: &mut format::Writer<'_>) -> fmt::Result
	where
		T: FormatTime,
	{
		if console::colors_enabled() {
			write!(writer, "\x1B[2m")?;
			timer.format_time(writer)?;
			write!(writer, "\x1B[0m")?;
		} else {
			timer.format_time(writer)?;
		}

		writer.write_char(' ')?;
		Ok(())
	}
}
