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
	panic::{catch_unwind, AssertUnwindSafe},
	pin::Pin,
	task::{Context, Poll},
};

/// Future polled with the panic hook not terminating the process.
///
/// A panic that escapes the future still terminates the process.
#[pin_project::pin_project]
pub(super) struct UnwindScope<F>(#[pin] pub F);

impl<F: Future> Future for UnwindScope<F> {
	type Output = F::Output;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let future = self.project().0;
		let result = {
			let _guard = sp_panic_handler::AbortGuard::force_unwind();
			catch_unwind(AssertUnwindSafe(|| future.poll(cx)))
		};

		// Already reported by the panic hook.
		result.unwrap_or_else(|_| std::process::exit(1))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::executor::block_on;

	/// Run `test_body` with the panic hook set in a child process, returning its exit code.
	///
	/// The hook is process-wide and would terminate the test binary on any other panic.
	fn exit_code_in_another_process(test_name: &str, test_body: impl FnOnce()) -> Option<i32> {
		if std::env::var("RUN_FORKED_TEST").is_ok() {
			sp_panic_handler::set("test", "1.2.3");
			test_body();
			None
		} else {
			let output = std::process::Command::new(std::env::current_exe().unwrap())
				.arg(test_name)
				.env("RUN_FORKED_TEST", "1")
				.output()
				.unwrap();

			Some(output.status.code().expect("child not to be killed by a signal"))
		}
	}

	#[test]
	fn caught_panic_does_not_terminate_process() {
		let exit_code =
			exit_code_in_another_process("caught_panic_does_not_terminate_process", || {
				block_on(UnwindScope(async {
					assert!(catch_unwind::<_, ()>(|| panic!("caught")).is_err());
				}));
			});

		if let Some(exit_code) = exit_code {
			assert_eq!(exit_code, 0);
		}
	}

	#[test]
	fn escaped_panic_terminates_process() {
		let exit_code = exit_code_in_another_process("escaped_panic_terminates_process", || {
			block_on(UnwindScope(async { panic!("escaped") }));
		});

		// A failed test exits with 101.
		if let Some(exit_code) = exit_code {
			assert_eq!(exit_code, 1);
		}
	}
}
