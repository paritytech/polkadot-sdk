// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Functionality for sandboxing workers by restricting their capabilities by blocking certain
//! syscalls with seccomp.
//!
//! For security we block the following:
//!
//! - creation of new sockets - these are unneeded in PVF jobs, and we can safely block them without
//!   affecting consensus.
//!
//! - `io_uring` - allows for networking and needs to be blocked. See below for a discussion on the
//!   safety of doing this.
//!
//! # Safety of blocking io_uring
//!
//! `io_uring` is just a way of issuing system calls in an async manner, and there is nothing
//! stopping wasmtime from legitimately using it. Fortunately, at the moment it does not. Generally,
//! not many applications use `io_uring` in production yet, because of the numerous kernel CVEs
//! discovered. It's still under a lot of development. Android outright banned `io_uring` for these
//! reasons.
//!
//! Considering `io_uring`'s status discussed above, and that it very likely would get detected
//! either by our [static analysis](https://github.com/paritytech/polkadot-sdk/pull/1663) or by
//! testing, we think it is safe to block it.
//!
//! ## Consensus analysis
//!
//! If execution hits an edge case code path unique to a given machine, it's already taken a
//! non-deterministic branch anyway. After all, we just care that the majority of validators reach
//! the same result and preserve consensus. So worst-case scenario, there's a dispute, and we can
//! always admit fault and refund the wrong validator. On the other hand, if all validators take the
//! code path that results in a seccomp violation, then they would all vote against the current
//! candidate, which is also fine. The violation would get logged (in big scary letters) and
//! hopefully some validator reports it to us.
//!
//! Actually, a worst-worse-case scenario is that 50% of validators vote against, so that there is
//! no consensus. But so many things would have to go wrong for that to happen:
//!
//! 1. An update to `wasmtime` is introduced that uses io_uring (unlikely as io_uring is mainly for
//!    IO-heavy applications)
//!
//! 2. The new syscall is not detected by our static analysis
//!
//! 3. It is never triggered in any of our tests
//!
//! 4. It then gets triggered on some super edge case in production on 50% of validators causing a
//!    stall (bad but very unlikely)
//!
//! 5. Or, it triggers on only a few validators causing a dispute (more likely but not as bad)
//!
//! Considering how many things would have to go wrong here, we believe it's safe to block
//! `io_uring`.
//!
//! # Action on syscall violations
//!
//! When a forbidden syscall is attempted we immediately kill the process in order to prevent the
//! attacker from doing anything else. In execution, this will result in voting against the
//! candidate.

use crate::{
	worker::{stringify_panic_payload, WorkerInfo},
	LOG_TARGET,
};
use seccompiler::*;
use std::collections::BTreeMap;

/// The action to take on caught syscalls.
#[cfg(not(test))]
const CAUGHT_ACTION: SeccompAction = SeccompAction::KillProcess;
/// Don't kill the process when testing.
#[cfg(test)]
const CAUGHT_ACTION: SeccompAction = SeccompAction::Errno(libc::EACCES as u32);

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error(transparent)]
	Seccomp(#[from] seccompiler::Error),
	#[error(transparent)]
	Backend(#[from] seccompiler::BackendError),
	#[error("A panic occurred in try_restrict: {0}")]
	Panic(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Try to enable seccomp for the given kind of worker.
pub fn enable_for_worker(worker_info: &WorkerInfo) -> Result<()> {
	gum::trace!(
		target: LOG_TARGET,
		?worker_info,
		"enabling seccomp",
	);

	try_restrict()
}

/// Runs a check for seccomp in its own thread, and returns an error indicating whether seccomp with
/// our rules is fully enabled on the current Linux environment.
pub fn check_can_fully_enable() -> Result<()> {
	match std::thread::spawn(|| try_restrict()).join() {
		Ok(Ok(())) => Ok(()),
		Ok(Err(err)) => Err(err),
		Err(err) => Err(Error::Panic(stringify_panic_payload(err))),
	}
}

/// Applies a `seccomp` filter to disable networking for the PVF threads.
fn try_restrict() -> Result<()> {
	// Build a `seccomp` filter which by default allows all syscalls except those blocked in the
	// blacklist.
	let mut blacklisted_rules = BTreeMap::default();

	// Restrict the creation of sockets.
	blacklisted_rules.insert(libc::SYS_socketpair, vec![]);
	blacklisted_rules.insert(libc::SYS_socket, vec![]);

	// Prevent connecting to sockets for extra safety.
	blacklisted_rules.insert(libc::SYS_connect, vec![]);

	// Restrict io_uring.
	blacklisted_rules.insert(libc::SYS_io_uring_setup, vec![]);
	blacklisted_rules.insert(libc::SYS_io_uring_enter, vec![]);
	blacklisted_rules.insert(libc::SYS_io_uring_register, vec![]);

	let filter = SeccompFilter::new(
		blacklisted_rules,
		// Mismatch action: what to do if not in rule list.
		SeccompAction::Allow,
		// Match action: what to do if in rule list.
		CAUGHT_ACTION,
		TargetArch::x86_64,
	)?;

	let bpf_prog: BpfProgram = filter.try_into()?;

	// Applies filter (runs seccomp) to the calling thread.
	seccompiler::apply_filter(&bpf_prog)?;

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{io::ErrorKind, net::TcpListener, thread};

	#[test]
	fn sandboxed_thread_cannot_use_sockets() {
		// TODO: This would be nice: <https://github.com/rust-lang/rust/issues/68007>.
		if check_can_fully_enable().is_err() {
			return
		}

		let handle = thread::spawn(|| {
			// Open a socket, this should succeed before seccomp is applied.
			TcpListener::bind("127.0.0.1:0").unwrap();

			let status = try_restrict();
			if !matches!(status, Ok(())) {
				panic!("Ruleset should be enforced since we checked if seccomp is enabled");
			}

			// Try to open a socket after seccomp.
			assert!(matches!(
				TcpListener::bind("127.0.0.1:0"),
				Err(err) if matches!(err.kind(), ErrorKind::PermissionDenied)
			));

			// Other syscalls should still work.
			unsafe {
				assert!(libc::getppid() > 0);
			}
		});

		assert!(handle.join().is_ok());
	}
}
