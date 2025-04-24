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

//! Substrate tracing primitives and macros.
//!
//! To trace functions or individual code in Substrate, this crate provides [`within_span`]
//! and [`enter_span`]. See the individual docs for how to use these macros.
//!
//! Note that to allow traces from wasm execution environment there are
//! 2 reserved identifiers for tracing `Field` recording, stored in the consts:
//! `WASM_TARGET_KEY` and `WASM_NAME_KEY` - if you choose to record fields, you
//! must ensure that your identifiers do not clash with either of these.
//!
//! Additionally, we have a const: `WASM_TRACE_IDENTIFIER`, which holds a span name used
//! to signal that the 'actual' span name and target should be retrieved instead from
//! the associated Fields mentioned above.
//!
//! Note: The `tracing` crate requires trace metadata to be static. This does not work
//! for wasm code in substrate, as it is regularly updated with new code from on-chain
//! events. The workaround for this is for the wasm tracing wrappers to put the
//! `name` and `target` data in the `values` map (normally they would be in the static
//! metadata assembled at compile time).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "std")]
pub use tracing;
pub use tracing::{
	debug, debug_span, error, error_span, event, info, info_span, span, trace, trace_span, warn,
	warn_span, Level, Span,
};

#[cfg(feature = "std")]
pub use tracing_subscriber;

pub use crate::types::{
	WasmEntryAttributes, WasmFieldName, WasmFields, WasmLevel, WasmMetadata, WasmValue,
	WasmValuesSet,
};
#[cfg(not(substrate_runtime))]
pub use crate::types::{WASM_NAME_KEY, WASM_TARGET_KEY, WASM_TRACE_IDENTIFIER};

/// Tracing facilities and helpers.
///
/// This is modeled after the `tracing`/`tracing-core` interface and uses that more or
/// less directly for the native side. Because of certain optimisations the these crates
/// have done, the wasm implementation diverges slightly and is optimised for that use
/// case (like being able to cross the wasm/native boundary via scale codecs).
///
/// One of said optimisations is that all macros will yield to a `noop` in non-std unless
/// the `with-tracing` feature is explicitly activated. This allows you to just use the
/// tracing wherever you deem fit and without any performance impact by default. Only if
/// the specific `with-tracing`-feature is activated on this crate will it actually include
/// the tracing code in the non-std environment.
///
/// Because of that optimisation, you should not use the `span!` and `span_*!` macros
/// directly as they yield nothing without the feature present. Instead you should use
/// `enter_span!` and `within_span!` – which would strip away even any parameter conversion
/// you do within the span-definition (and thus optimise your performance). For your
/// convenience you directly specify the `Level` and name of the span or use the full
/// feature set of `span!`/`span_*!` on it:
///
/// # Example
///
/// ```rust
/// sp_tracing::enter_span!(sp_tracing::Level::TRACE, "fn wide span");
/// {
/// 		sp_tracing::enter_span!(sp_tracing::trace_span!("outer-span"));
/// 		{
/// 			sp_tracing::enter_span!(sp_tracing::Level::TRACE, "inner-span");
/// 			// ..
/// 		}  // inner span exists here
/// 	} // outer span exists here
///
/// sp_tracing::within_span! {
/// 		sp_tracing::debug_span!("debug-span", you_can_pass="any params");
///     1 + 1;
///     // some other complex code
/// } // debug span ends here
/// ```
///
///
/// # Setup
///
/// This project only provides the macros and facilities to manage tracing
/// it doesn't implement the tracing subscriber or backend directly – that is
/// up to the developer integrating it into a specific environment. In native
/// this can and must be done through the regular `tracing`-facilities, please
/// see their documentation for details.
///
/// On the wasm-side we've adopted a similar approach of having a global
/// `TracingSubscriber` that the macros call and that does the actual work
/// of tracking. To provide your tracking, you must implement `TracingSubscriber`
/// and call `set_tracing_subscriber` at the very beginning of your execution –
/// the default subscriber is doing nothing, so any spans or events happening before
/// will not be recorded!
mod types;

/// Try to init a simple tracing subscriber with log compatibility layer.
///
/// Ignores any error. Useful for testing. Uses the default filter for logs.
///
/// Related functions:
/// - [`init_for_tests()`]: Enables `TRACE` level.
/// - [`test_log_capture::init_log_capture()`]: Captures logs for assertions and/or outputs logs.
/// - [`capture_test_logs!()`]: A macro for capturing logs within test blocks.
#[cfg(feature = "std")]
pub fn try_init_simple() {
	let _ = tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.with_writer(std::io::stderr)
		.try_init();
}

/// Init a tracing subscriber for logging in tests.
///
/// Be aware that this enables `TRACE` by default. It also ignores any error
/// while setting up the logger.
///
/// The logs are not shown by default, logs are only shown when the test fails
/// or if [`nocapture`](https://doc.rust-lang.org/cargo/commands/cargo-test.html#display-options)
/// is being used.
///
/// Related functions:
/// - [`try_init_simple()`]: Uses the default filter.
/// - [`test_log_capture::init_log_capture()`]: Captures logs for assertions and/or outputs logs.
/// - [`capture_test_logs!()`]: A macro for capturing logs within test blocks.
#[cfg(feature = "std")]
pub fn init_for_tests() {
	let _ = tracing_subscriber::fmt()
		.with_max_level(tracing::Level::TRACE)
		.with_test_writer()
		.try_init();
}

/// Runs given code within a tracing span, measuring it's execution time.
///
/// If tracing is not enabled, the code is still executed. Pass in level and name or
/// use any valid `sp_tracing::Span`followed by `;` and the code to execute,
///
/// # Example
///
/// ```
/// sp_tracing::within_span! {
///     sp_tracing::Level::TRACE,
///     "test-span";
///     1 + 1;
///     // some other complex code
/// }
///
/// sp_tracing::within_span! {
///     sp_tracing::span!(sp_tracing::Level::WARN, "warn-span", you_can_pass="any params");
///     1 + 1;
///     // some other complex code
/// }
///
/// sp_tracing::within_span! {
///     sp_tracing::debug_span!("debug-span", you_can_pass="any params");
///     1 + 1;
///     // some other complex code
/// }
/// ```
#[cfg(any(feature = "std", feature = "with-tracing"))]
#[macro_export]
macro_rules! within_span {
	(
		$span:expr;
		$( $code:tt )*
	) => {
		$span.in_scope(||
			{
				$( $code )*
			}
		)
	};
	(
		$lvl:expr,
		$name:expr;
		$( $code:tt )*
	) => {
		{
			$crate::within_span!($crate::span!($lvl, $name); $( $code )*)
		}
	};
}

#[cfg(all(not(feature = "std"), not(feature = "with-tracing")))]
#[macro_export]
macro_rules! within_span {
	(
		$span:stmt;
		$( $code:tt )*
	) => {
		$( $code )*
	};
	(
		$lvl:expr,
		$name:expr;
		$( $code:tt )*
	) => {
		$( $code )*
	};
}

/// Enter a span - noop for `no_std` without `with-tracing`
#[cfg(all(not(feature = "std"), not(feature = "with-tracing")))]
#[macro_export]
macro_rules! enter_span {
	( $lvl:expr, $name:expr ) => {};
	( $name:expr ) => {}; // no-op
}

/// Enter a span.
///
/// The span will be valid, until the scope is left. Use either level and name
/// or pass in any valid `sp_tracing::Span` for extended usage. The span will
/// be exited on drop – which is at the end of the block or to the next
/// `enter_span!` calls, as this overwrites the local variable. For nested
/// usage or to ensure the span closes at certain time either put it into a block
/// or use `within_span!`
///
/// # Example
///
/// ```
/// sp_tracing::enter_span!(sp_tracing::Level::TRACE, "test-span");
/// // previous will be dropped here
/// sp_tracing::enter_span!(
/// 	sp_tracing::span!(sp_tracing::Level::DEBUG, "debug-span", params="value"));
/// sp_tracing::enter_span!(sp_tracing::info_span!("info-span",  params="value"));
///
/// {
/// 		sp_tracing::enter_span!(sp_tracing::Level::TRACE, "outer-span");
/// 		{
/// 			sp_tracing::enter_span!(sp_tracing::Level::TRACE, "inner-span");
/// 			// ..
/// 		}  // inner span exists here
/// 	} // outer span exists here
/// ```
#[cfg(any(feature = "std", feature = "with-tracing"))]
#[macro_export]
macro_rules! enter_span {
	( $span:expr ) => {
		// Calling this twice in a row will overwrite (and drop) the earlier
		// that is a _documented feature_!
		let __within_span__ = $span;
		let __tracing_guard__ = __within_span__.enter();
	};
	( $lvl:expr, $name:expr ) => {
		$crate::enter_span!($crate::span!($lvl, $name))
	};
}

#[cfg(feature = "test-utils")]
pub mod test_log_capture {
	use std::{
		io::Write,
		sync::{Arc, Mutex},
	};
	use tracing::level_filters::LevelFilter;
	use tracing_subscriber::{fmt, fmt::MakeWriter, layer::SubscriberExt, Layer, Registry};

	/// A reusable log capturing struct for unit tests.
	/// Captures logs written during test execution for assertions.
	///
	/// # Examples
	/// ```
	/// use sp_tracing::test_log_capture::LogCapture;
	/// use std::io::Write;
	///
	/// let mut log_capture = LogCapture::new();
	/// writeln!(log_capture, "Test log message").unwrap();
	/// assert!(log_capture.contains("Test log message"));
	/// ```
	pub struct LogCapture {
		buffer: Arc<Mutex<Vec<u8>>>,
	}

	impl LogCapture {
		/// Creates a new `LogCapture` instance with an internal buffer.
		///
		/// # Examples
		/// ```
		/// use sp_tracing::test_log_capture::LogCapture;
		///
		/// let log_capture = LogCapture::new();
		/// assert!(log_capture.get_logs().is_empty());
		/// ```
		pub fn new() -> Self {
			LogCapture { buffer: Arc::new(Mutex::new(Vec::new())) }
		}

		/// Checks if the captured logs contain a specific substring.
		///
		/// # Examples
		/// ```
		/// use sp_tracing::test_log_capture::LogCapture;
		/// use std::io::Write;
		///
		/// let mut log_capture = LogCapture::new();
		/// writeln!(log_capture, "Hello, world!").unwrap();
		/// assert!(log_capture.contains("Hello"));
		/// assert!(!log_capture.contains("Goodbye"));
		/// ```
		pub fn contains(&self, expected: &str) -> bool {
			let logs = self.get_logs();
			logs.contains(expected)
		}

		/// Retrieves the captured logs as a `String`.
		///
		/// # Examples
		/// ```
		/// use sp_tracing::test_log_capture::LogCapture;
		/// use std::io::Write;
		///
		/// let mut log_capture = LogCapture::new();
		/// writeln!(log_capture, "Log entry").unwrap();
		/// assert_eq!(log_capture.get_logs().trim(), "Log entry");
		/// ```
		pub fn get_logs(&self) -> String {
			let raw_logs = String::from_utf8(self.buffer.lock().unwrap().clone()).unwrap();
			let ansi_escape = regex::Regex::new(r"\x1B\[[0-9;]*[mK]").unwrap(); // Regex to match ANSI codes
			ansi_escape.replace_all(&raw_logs, "").to_string() // Remove ANSI codes
		}

		/// Returns a clone of the internal buffer for use in `MakeWriter`.
		pub fn writer(&self) -> Self {
			LogCapture { buffer: Arc::clone(&self.buffer) }
		}
	}

	impl Write for LogCapture {
		/// Writes log data into the internal buffer.
		fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
			let mut logs = self.buffer.lock().unwrap();
			logs.extend_from_slice(buf);
			Ok(buf.len())
		}

		/// Flushes the internal buffer (no-op in this implementation).
		fn flush(&mut self) -> std::io::Result<()> {
			Ok(())
		}
	}

	impl<'a> MakeWriter<'a> for LogCapture {
		type Writer = Self;

		/// Provides a `MakeWriter` implementation for `tracing_subscriber`.
		fn make_writer(&'a self) -> Self::Writer {
			self.writer()
		}
	}

	/// Initialises a log capture utility for testing, with optional log printing.
	///
	/// This function sets up a `LogCapture` instance to capture logs during test execution.
	/// It also configures a `tracing_subscriber` with the specified maximum log level
	/// and a writer that directs logs to `LogCapture`. If `print_logs` is enabled, logs
	/// up to `max_level` are also printed to the test output.
	///
	/// # Arguments
	///
	/// * `max_level` - The maximum log level to capture and print, which can be converted into
	///   `LevelFilter`.
	/// * `print_logs` - If `true`, logs up to `max_level` will also be printed to the test output.
	///
	/// # Returns
	///
	/// A tuple containing:
	/// - `LogCapture`: The log capture instance.
	/// - `Subscriber`: A configured `tracing_subscriber` that captures logs.
	///
	/// # Examples
	///
	/// ```
	/// use sp_tracing::{
	///     test_log_capture::init_log_capture,
	///     tracing::{info, subscriber, Level},
	/// };
	///
	/// let (log_capture, subscriber) = init_log_capture(Level::INFO, false);
	/// subscriber::with_default(subscriber, || {
	///     info!("This log will be captured");
	///     assert!(log_capture.contains("This log will be captured"));
	/// });
	/// ```
	///
	/// # Usage Guide
	///
	/// - If you only need to **capture logs for assertions** without printing them, use
	///   `init_log_capture(max_level, false)`.
	/// - If you need both **capturing and printing logs**, use `init_log_capture(max_level, true)`.
	/// - If you only need to **print logs** but not capture them, use
	///   `sp_tracing::init_for_tests()`.
	pub fn init_log_capture(
		max_level: impl Into<LevelFilter>,
		print_logs: bool,
	) -> (LogCapture, impl tracing::Subscriber + Send + Sync) {
		// Create a new log capture instance
		let log_capture = LogCapture::new();

		// Convert the max log level into LevelFilter
		let level_filter = max_level.into();

		// Create a layer for capturing logs into LogCapture
		let capture_layer = fmt::layer()
			.with_writer(log_capture.writer()) // Use LogCapture as the writer
			.with_filter(level_filter); // Set the max log level

		// Base subscriber with log capturing
		let subscriber = Registry::default().with(capture_layer);

		// If `print_logs` is enabled, add a layer that prints logs to test output up to `max_level`
		let test_layer = if print_logs {
			Some(
				fmt::layer()
					.with_test_writer() // Direct logs to test output
					.with_filter(level_filter), // Apply the same max log level filter
			)
		} else {
			None
		};

		// Combine the log capture subscriber with the test layer (if applicable)
		let combined_subscriber = subscriber.with(test_layer);

		(log_capture, combined_subscriber)
	}

	/// Macro for capturing logs during test execution.
	///
	/// This macro sets up a log subscriber with a specified maximum log level
	/// and an option to print logs to the test output while capturing them.
	///
	/// # Arguments
	///
	/// - `$max_level`: The maximum log level to capture.
	/// - `$print_logs`: Whether to also print logs to the test output.
	/// - `$test`: The block of code where logs are captured.
	///
	/// # Examples
	///
	/// ```
	/// use sp_tracing::{
	///     capture_test_logs,
	///     tracing::{info, warn, Level},
	/// };
	///
	/// // Capture logs at WARN level without printing them
	/// let log_capture = capture_test_logs!(Level::WARN, false, {
	///     info!("Captured info message");
	///     warn!("Captured warning");
	/// });
	///
	/// assert!(!log_capture.contains("Captured info message"));
	/// assert!(log_capture.contains("Captured warning"));
	///
	/// // Capture logs at TRACE level and also print them
	/// let log_capture = capture_test_logs!(Level::TRACE, true, {
	///     info!("This will be captured and printed");
	/// });
	///
	/// assert!(log_capture.contains("This will be captured and printed"));
	/// ```
	///
	/// # Related functions:
	/// - [`init_log_capture()`]: Captures logs for assertions.
	/// - `sp_tracing::init_for_tests()`: Outputs logs but does not capture them.
	#[macro_export]
	macro_rules! capture_test_logs {
		// Case when max_level and print_logs are provided
		($max_level:expr, $print_logs:expr, $test:block) => {{
			let (log_capture, subscriber) =
				sp_tracing::test_log_capture::init_log_capture($max_level, $print_logs);

			sp_tracing::tracing::subscriber::with_default(subscriber, || $test);

			log_capture
		}};

		// Case when only max_level is provided (defaults to not printing logs)
		($max_level:expr, $test:block) => {{
			capture_test_logs!($max_level, false, $test)
		}};

		// Case when max_level is omitted (defaults to DEBUG, no printing)
		($test:block) => {{
			capture_test_logs!(sp_tracing::tracing::Level::DEBUG, false, $test)
		}};
	}
}
