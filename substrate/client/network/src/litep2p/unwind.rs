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

//! Makes `catch_unwind` inside `litep2p` effective.
//!
//! The panic hook of [`sp_panic_handler`] terminates the process before unwinding starts, so
//! `litep2p` can't contain the panics of its dependencies on its own.

use std::{
	future::Future,
	panic::AssertUnwindSafe,
	pin::Pin,
	task::{Context, Poll},
};

/// Future polled with the panic hook not terminating the process.
///
/// A panic that escapes the future still terminates the process, even if the hook isn't installed.
#[pin_project::pin_project]
pub(super) struct UnwindScope<F>(#[pin] pub F);

impl<F: Future> Future for UnwindScope<F> {
	type Output = F::Output;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let future = self.project().0;
		sp_panic_handler::abort_on_unwind(AssertUnwindSafe(|| future.poll(cx)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::executor::block_on;
	use std::{
		panic::catch_unwind,
		process::{Command, Output},
	};

	/// Run with the process-wide panic hook in a child process.
	fn run_in_another_process(test_name: &str, test_body: impl FnOnce()) -> Option<Output> {
		if std::env::var("RUN_FORKED_TEST").is_ok() {
			sp_panic_handler::set("test", "1.2.3");
			test_body();
			None
		} else {
			let (_, module) = module_path!().split_once("::").expect("test module path");
			let output = Command::new(std::env::current_exe().expect("test executable path"))
				.args(["--exact", &format!("{module}::{test_name}")])
				.env("RUN_FORKED_TEST", "1")
				.output()
				.expect("start child test process");

			Some(output)
		}
	}

	#[test]
	fn caught_panic_does_not_terminate_process() {
		let output = run_in_another_process("caught_panic_does_not_terminate_process", || {
			block_on(UnwindScope(async {
				assert!(catch_unwind::<_, ()>(|| panic!("caught")).is_err());
			}));
		});

		if let Some(output) = output {
			assert!(output.status.success());
			let stderr = String::from_utf8_lossy(&output.stderr);
			assert!(stderr.contains("panicked at 'caught'"));
			// Only present in the full report.
			assert!(!stderr.contains("Version: 1.2.3"));
		}
	}

	#[test]
	fn escaped_panic_terminates_process() {
		let output = run_in_another_process("escaped_panic_terminates_process", || {
			block_on(UnwindScope(async { panic!("escaped") }));
		});

		if let Some(output) = output {
			assert_eq!(output.status.code(), Some(1));
			let stderr = String::from_utf8_lossy(&output.stderr);
			assert!(stderr.contains("Version: 1.2.3"));
			assert!(stderr.contains("panicked at 'escaped'"));
			assert!(stderr.contains("unwind.rs:"));
		}
	}

	#[test]
	fn pending_poll_restores_panic_hook() {
		let output = run_in_another_process("pending_poll_restores_panic_hook", || {
			let mut future = std::pin::pin!(UnwindScope(std::future::pending::<()>()));
			let mut cx = Context::from_waker(futures::task::noop_waker_ref());
			assert!(future.as_mut().poll(&mut cx).is_pending());
			panic!("outside unwind scope");
		});

		if let Some(output) = output {
			assert_eq!(output.status.code(), Some(1));
			let stderr = String::from_utf8_lossy(&output.stderr);
			assert!(stderr.contains("panicked at 'outside unwind scope'"));
		}
	}
}
