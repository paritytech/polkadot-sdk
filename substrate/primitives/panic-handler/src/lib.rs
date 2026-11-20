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

//! Custom panic hook with bug report link
//!
//! This crate provides the [`set`] function, which wraps around [`std::panic::set_hook`] and
//! sets up a panic hook that prints a backtrace and invites the user to open an issue to the
//! given URL.
//!
//! By default, the panic handler aborts the process by calling [`std::process::exit`]. This can
//! temporarily be disabled by using an [`AbortGuard`] or [`abort_on_unwind`].

use backtrace::Backtrace;
use regex::Regex;
use std::{
	any::Any,
	cell::Cell,
	io::{self, Write},
	marker::PhantomData,
	panic::{self, PanicHookInfo, UnwindSafe},
	sync::{Arc, LazyLock, Mutex, PoisonError},
	thread,
	time::{Duration, Instant},
};

/// Summaries of possibly caught panics printed per [`SUMMARY_WINDOW`].
const SUMMARIES_PER_WINDOW: u64 = 10;

/// Window of the summary rate limit.
const SUMMARY_WINDOW: Duration = Duration::from_secs(60);

thread_local! {
	static ON_PANIC: Cell<OnPanic> = Cell::new(OnPanic::Abort);
	static DEFERRED_REPORT: Cell<Option<Box<PanicReport>>> = const { Cell::new(None) };
}

/// Panic action.
#[derive(Debug, Clone, Copy, PartialEq)]
enum OnPanic {
	/// Abort when panic occurs.
	Abort,
	/// Unwind when panic occurs.
	Unwind,
	/// Unwind, printing only a summary and deferring the full panic report.
	DeferReport,
	/// Always unwind even if someone changes strategy to Abort afterwards.
	NeverAbort,
}

/// Set the panic hook.
///
/// Calls [`std::panic::set_hook`] to set up the panic hook.
///
/// The `bug_url` parameter is an invitation for users to visit that URL to submit a bug report
/// in the case where a panic happens.
pub fn set(bug_url: &str, version: &str) {
	let context = Arc::new(ReportContext { url: bug_url.to_owned(), version: version.to_owned() });
	panic::set_hook(Box::new(move |info| panic_hook(info, &context)));
}

macro_rules! ABOUT_PANIC {
	() => {
		"
This is a bug. Please report it at:

	{}
"
	};
}

/// Set aborting flag. Returns previous value of the flag.
fn set_abort(on_panic: OnPanic) -> OnPanic {
	ON_PANIC.with(|val| {
		let prev = val.get();
		match prev {
			OnPanic::Abort | OnPanic::Unwind | OnPanic::DeferReport => val.set(on_panic),
			OnPanic::NeverAbort => (),
		}
		prev
	})
}

/// RAII guard for whether panics in the current thread should unwind or abort.
///
/// Sets a thread-local abort flag on construction and reverts to the previous setting when dropped.
/// Does not implement `Send` on purpose.
///
/// > **Note**: Because we restore the previous value when dropped, you are encouraged to leave
/// > the `AbortGuard` on the stack and let it destroy itself naturally.
pub struct AbortGuard {
	/// Value that was in `ABORT` before we created this guard.
	previous_val: OnPanic,
	/// Marker so that `AbortGuard` doesn't implement `Send`.
	_not_send: PhantomData<std::rc::Rc<()>>,
}

impl AbortGuard {
	fn new(action: OnPanic) -> Self {
		Self { previous_val: set_abort(action), _not_send: PhantomData }
	}

	/// Create a new guard. While the guard is alive, panics that happen in the current thread will
	/// unwind the stack (unless another guard is created afterwards).
	pub fn force_unwind() -> AbortGuard {
		Self::new(OnPanic::Unwind)
	}

	/// Create a new guard. While the guard is alive, panics that happen in the current thread will
	/// abort the process (unless another guard is created afterwards).
	pub fn force_abort() -> AbortGuard {
		Self::new(OnPanic::Abort)
	}

	/// Create a new guard. While the guard is alive, panics that happen in the current thread will
	/// **never** abort the process (even if `AbortGuard::force_abort()` guard will be created
	/// afterwards).
	pub fn never_abort() -> AbortGuard {
		Self::new(OnPanic::NeverAbort)
	}
}

impl Drop for AbortGuard {
	fn drop(&mut self) {
		set_abort(self.previous_val);
	}
}

struct DeferredPanicReport {
	previous_report: Option<Box<PanicReport>>,
	_abort: AbortGuard,
}

impl DeferredPanicReport {
	fn enter() -> Self {
		Self {
			previous_report: DEFERRED_REPORT.take(),
			_abort: AbortGuard::new(OnPanic::DeferReport),
		}
	}
}

impl Drop for DeferredPanicReport {
	fn drop(&mut self) {
		DEFERRED_REPORT.set(self.previous_report.take());
	}
}

/// Run `f`, reporting panics caught inside it with a single rate-limited line instead of the full
/// report.
///
/// A panic that escapes `f` prints the full report and exits the process, even if the hook isn't
/// [`set`]. A panic that aborts the process without unwinding, like one in a destructor during
/// unwinding, is only reported with that line.
///
/// When built with `panic = "abort"`, `f` is called with the hook left as is.
pub fn abort_on_unwind<T>(f: impl FnOnce() -> T + UnwindSafe) -> T {
	// Nothing can be caught, so the hook must print the full report.
	if cfg!(panic = "abort") {
		return f();
	}

	// The payload is kept until the exit: its destructor may panic.
	let (payload, report) = {
		let _scope = DeferredPanicReport::enter();
		match panic::catch_unwind(f) {
			Ok(value) => return value,
			Err(payload) => (payload, DEFERRED_REPORT.take()),
		}
	};

	// `None` if the hook has printed the report itself or isn't installed.
	let reported = report.map(|mut report| {
		report.print();
		report.message
	});
	// A payload re-raised with `resume_unwind` skips the hook, leaving the report of another panic.
	let escaped = panic_message(&*payload);
	if reported.as_deref() != Some(escaped) {
		let _ = writeln!(io::stderr(), "Escaped panic payload: '{}'", strip_control_codes(escaped));
	}
	std::process::exit(1)
}

// NOTE: When making any changes here make sure to also change this function in `sc-tracing`.
fn strip_control_codes(input: &str) -> std::borrow::Cow<'_, str> {
	static RE: LazyLock<Regex> = LazyLock::new(|| {
		Regex::new(
			r#"(?x)
			\x1b\[[^m]+m|        # VT100 escape codes
			[
			  \x00-\x09\x0B-\x1F # ASCII control codes / Unicode C0 control codes, except \n
			  \x7F               # ASCII delete
			  \u{80}-\u{9F}      # Unicode C1 control codes
			  \u{202A}-\u{202E}  # Unicode left-to-right / right-to-left control characters
			  \u{2066}-\u{2069}  # Same as above
			]
		"#,
		)
		.expect("regex parsing doesn't fail; qed")
	});

	RE.replace_all(input, "")
}

fn panic_message(payload: &(dyn Any + Send)) -> &str {
	payload
		.downcast_ref::<&str>()
		.copied()
		.or_else(|| payload.downcast_ref::<String>().map(String::as_str))
		.unwrap_or("Box<Any>")
}

/// Report of a panic that may still be caught.
struct PanicReport {
	context: Arc<ReportContext>,
	backtrace: Backtrace,
	message: String,
	file: String,
	line: u32,
}

impl PanicReport {
	fn print(&mut self) {
		self.backtrace.resolve();
		self.context.print(&self.backtrace, &self.message, &self.file, self.line);
	}
}

struct ReportContext {
	url: String,
	version: String,
}

impl ReportContext {
	fn print(&self, backtrace: &Backtrace, message: &str, file: &str, line: u32) {
		let mut stderr = io::stderr();

		let _ = writeln!(stderr);
		let _ = writeln!(stderr, "====================");
		let _ = writeln!(stderr);
		let _ = writeln!(stderr, "Version: {}", self.version);
		let _ = writeln!(stderr);
		let _ = writeln!(stderr, "{:?}", backtrace);
		let _ = writeln!(stderr);
		print_summary(&mut stderr, message, file, line);
		let _ = writeln!(stderr, ABOUT_PANIC!(), self.url);
	}
}

/// Print the one-line panic summary.
fn print_summary(stderr: &mut io::Stderr, message: &str, file: &str, line: u32) {
	let message = strip_control_codes(message);
	let thread = thread::current();
	let name = thread.name().unwrap_or("<unnamed>");

	let _ = writeln!(stderr, "Thread '{}' panicked at '{}', {}:{}", name, message, file, line);
}

/// Rate limit of the summaries of possibly caught panics, which may be frequent.
struct SummaryLimiter {
	/// Start of the current window.
	window_start: Option<Instant>,
	/// Panics in the current window.
	panics: u64,
}

/// Summary let through by [`SummaryLimiter`].
#[derive(Debug, PartialEq)]
struct AllowedSummary {
	/// Summaries suppressed in the previous window.
	suppressed: u64,
	/// Whether the rest of the window is suppressed.
	last: bool,
}

impl SummaryLimiter {
	const fn new() -> Self {
		Self { window_start: None, panics: 0 }
	}

	/// Record a panic, returning `None` if its summary must be suppressed.
	fn record(&mut self, now: Instant) -> Option<AllowedSummary> {
		let mut suppressed = 0;
		if self
			.window_start
			.is_none_or(|start| now.duration_since(start) >= SUMMARY_WINDOW)
		{
			suppressed = self.panics.saturating_sub(SUMMARIES_PER_WINDOW);
			self.window_start = Some(now);
			self.panics = 0;
		}
		self.panics += 1;

		(self.panics <= SUMMARIES_PER_WINDOW)
			.then_some(AllowedSummary { suppressed, last: self.panics == SUMMARIES_PER_WINDOW })
	}
}

/// Print the rate-limited summary of a possibly caught panic.
fn print_limited_summary(message: &str, file: &str, line: u32) {
	static LIMITER: Mutex<SummaryLimiter> = Mutex::new(SummaryLimiter::new());

	let allowed = LIMITER.lock().unwrap_or_else(PoisonError::into_inner).record(Instant::now());
	let Some(AllowedSummary { suppressed, last }) = allowed else { return };
	let mut stderr = io::stderr();

	if suppressed > 0 {
		let _ = writeln!(stderr, "{} panic summaries were suppressed", suppressed);
	}
	print_summary(&mut stderr, message, file, line);
	if last {
		let _ = writeln!(
			stderr,
			"Further panic summaries are suppressed for up to {} seconds",
			SUMMARY_WINDOW.as_secs(),
		);
	}
}

fn panic_hook(info: &PanicHookInfo, context: &Arc<ReportContext>) {
	let (file, line) = info
		.location()
		.map_or(("<unknown>", 0), |location| (location.file(), location.line()));
	let message = panic_message(info.payload());
	let action = ON_PANIC.get();

	if action == OnPanic::DeferReport {
		// The only report of a panic that is caught, or that aborts the process without unwinding.
		print_limited_summary(message, file, line);
		DEFERRED_REPORT.set(Some(Box::new(PanicReport {
			context: Arc::clone(context),
			backtrace: Backtrace::new_unresolved(),
			message: message.to_owned(),
			file: file.to_owned(),
			line,
		})));
		return;
	}

	// Must not be mistaken for the report of this panic.
	let _ = DEFERRED_REPORT.try_with(|report| report.set(None));
	context.print(&Backtrace::new(), message, file, line);
	if action == OnPanic::Abort {
		std::process::exit(1);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn does_not_abort() {
		set("test", "1.2.3");
		let _guard = AbortGuard::force_unwind();
		::std::panic::catch_unwind(|| panic!()).ok();
	}

	#[test]
	fn does_not_abort_after_never_abort() {
		set("test", "1.2.3");
		let _guard = AbortGuard::never_abort();
		let _guard = AbortGuard::force_abort();
		std::panic::catch_unwind(|| panic!()).ok();
	}

	fn run_test_in_another_process(
		test_name: &str,
		test_body: impl FnOnce(),
	) -> Option<std::process::Output> {
		if std::env::var("RUN_FORKED_TEST").is_ok() {
			test_body();
			None
		} else {
			let output = std::process::Command::new(std::env::current_exe().unwrap())
				.arg(test_name)
				.env("RUN_FORKED_TEST", "1")
				.output()
				.unwrap();

			Some(output)
		}
	}

	/// Only present in the full report.
	const FULL_REPORT: &str = "Version: 1.2.3";

	fn stderr(output: &std::process::Output) -> String {
		String::from_utf8_lossy(&output.stderr).into_owned()
	}

	#[test]
	fn control_characters_are_always_stripped_out_from_the_panic_messages() {
		const RAW_LINE: &str = "$$START$$\x1B[1;32mIn\u{202a}\u{202e}\u{2066}\u{2069}ner\n\r\x7ftext!\u{80}\u{9f}\x1B[0m$$END$$";
		const SANITIZED_LINE: &str = "$$START$$Inner\ntext!$$END$$";

		let output = run_test_in_another_process(
			"control_characters_are_always_stripped_out_from_the_panic_messages",
			|| {
				set("test", "1.2.3");
				let _guard = AbortGuard::force_unwind();
				let _ = std::panic::catch_unwind(|| panic!("{}", RAW_LINE));
			},
		);

		if let Some(output) = output {
			assert!(output.status.success());
			let stderr = String::from_utf8(output.stderr).unwrap();
			assert!(!stderr.contains(RAW_LINE));
			assert!(stderr.contains(SANITIZED_LINE));
		}
	}

	#[test]
	fn caught_panic_prints_summary_only() {
		let output = run_test_in_another_process("caught_panic_prints_summary_only", || {
			set("test", "1.2.3");
			abort_on_unwind(|| {
				assert!(panic::catch_unwind::<_, ()>(|| panic!("caught")).is_err());
			});
		});

		if let Some(output) = output {
			assert!(output.status.success());
			let stderr = stderr(&output);
			assert!(stderr.contains("panicked at 'caught'"));
			assert!(stderr.contains("lib.rs:"));
			assert!(!stderr.contains(FULL_REPORT));
		}
	}

	#[test]
	fn escaped_panic_prints_report_and_exits() {
		let output = run_test_in_another_process("escaped_panic_prints_report_and_exits", || {
			set("test", "1.2.3");
			abort_on_unwind::<()>(|| panic!("escaped"));
		});

		if let Some(output) = output {
			assert_eq!(output.status.code(), Some(1));
			let stderr = stderr(&output);
			assert_eq!(stderr.matches(FULL_REPORT).count(), 1);
			assert!(stderr.contains("panicked at 'escaped'"));
			assert!(!stderr.contains("Escaped panic payload"));
		}
	}

	#[test]
	fn escaped_panic_reported_by_hook_is_not_printed_twice() {
		let output = run_test_in_another_process(
			"escaped_panic_reported_by_hook_is_not_printed_twice",
			|| {
				set("test", "1.2.3");
				let _guard = AbortGuard::never_abort();
				abort_on_unwind::<()>(|| panic!("escaped"));
			},
		);

		if let Some(output) = output {
			assert_eq!(output.status.code(), Some(1));
			assert_eq!(stderr(&output).matches(FULL_REPORT).count(), 1);
		}
	}

	#[test]
	fn reraised_payload_is_reported_next_to_last_panic() {
		let output =
			run_test_in_another_process("reraised_payload_is_reported_next_to_last_panic", || {
				set("test", "1.2.3");
				abort_on_unwind::<()>(|| {
					let first = panic::catch_unwind::<_, ()>(|| panic!("first")).unwrap_err();
					let _ = panic::catch_unwind::<_, ()>(|| panic!("second"));
					panic::resume_unwind(first);
				});
			});

		if let Some(output) = output {
			assert_eq!(output.status.code(), Some(1));
			let stderr = stderr(&output);
			// The full report is of the last panic seen by the hook.
			assert_eq!(stderr.matches(FULL_REPORT).count(), 1);
			assert_eq!(stderr.matches("panicked at 'second'").count(), 2);
			assert_eq!(stderr.matches("panicked at 'first'").count(), 1);
			assert!(stderr.contains("Escaped panic payload: 'first'"));
		}
	}

	#[test]
	fn limiter_suppresses_summaries_over_window_limit() {
		let mut limiter = SummaryLimiter::new();
		let start = Instant::now();

		for panics in 1..=SUMMARIES_PER_WINDOW {
			assert_eq!(
				limiter.record(start),
				Some(AllowedSummary { suppressed: 0, last: panics == SUMMARIES_PER_WINDOW }),
			);
		}
		assert_eq!(limiter.record(start), None);
		assert_eq!(limiter.record(start + SUMMARY_WINDOW / 2), None);
		assert_eq!(
			limiter.record(start + SUMMARY_WINDOW),
			Some(AllowedSummary { suppressed: 2, last: false }),
		);
	}

	#[test]
	fn caught_panic_summaries_are_rate_limited() {
		let output = run_test_in_another_process("caught_panic_summaries_are_rate_limited", || {
			set("test", "1.2.3");
			abort_on_unwind(|| {
				for _ in 0..SUMMARIES_PER_WINDOW + 5 {
					assert!(panic::catch_unwind::<_, ()>(|| panic!("caught")).is_err());
				}
			});
		});

		if let Some(output) = output {
			assert!(output.status.success());
			let stderr = stderr(&output);
			assert_eq!(stderr.matches("panicked at 'caught'").count() as u64, SUMMARIES_PER_WINDOW,);
			assert!(stderr.contains("Further panic summaries are suppressed"));
		}
	}

	#[test]
	fn non_unwinding_panic_prints_summary() {
		struct PanicOnDrop;

		impl Drop for PanicOnDrop {
			fn drop(&mut self) {
				panic!("in destructor");
			}
		}

		let output = run_test_in_another_process("non_unwinding_panic_prints_summary", || {
			set("test", "1.2.3");
			abort_on_unwind::<()>(|| {
				let _panic_on_drop = PanicOnDrop;
				panic!("first");
			});
		});

		if let Some(output) = output {
			// Aborted by the runtime: a panic in a destructor during unwinding can't unwind.
			assert!(!output.status.success());
			let stderr = stderr(&output);
			assert!(stderr.contains("panicked at 'first'"));
			assert!(stderr.contains("panicked at 'in destructor'"));
		}
	}
}
