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

//! Functionality for securing the job processes spawned by the workers using `clone`. If
//! unsupported, falls back to `fork`.

use crate::{worker::WorkerInfo, LOG_TARGET};
use nix::{
	errno::Errno,
	sched::{CloneCb, CloneFlags},
	unistd::Pid,
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("could not clone, errno: {0}")]
	Clone(Errno),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Try to run clone(2) on the current worker.
///
/// SAFETY: new process should be either spawned within a single threaded process, or use only
/// async-signal-safe functions.
pub unsafe fn clone_on_worker(
	worker_info: &WorkerInfo,
	have_unshare_newuser: bool,
	cb: CloneCb,
) -> Result<Pid> {
	let flags = clone_flags(have_unshare_newuser);

	gum::trace!(
		target: LOG_TARGET,
		?worker_info,
		"calling clone with flags: {:?}",
		flags
	);

	try_clone(cb, flags)
}

/// Runs a check for clone(2) with all sandboxing flags and returns an error indicating whether it
/// can be fully enabled on the current Linux environment.
///
/// SAFETY: new process should be either spawned within a single threaded process, or use only
/// async-signal-safe functions.
pub unsafe fn check_can_fully_clone() -> Result<()> {
	try_clone(Box::new(|| 0), clone_flags(false)).map(|_pid| ())
}

/// Runs clone(2) with all sandboxing flags.
///
/// SAFETY: new process should be either spawned within a single threaded process, or use only
/// async-signal-safe functions.
unsafe fn try_clone(cb: CloneCb, flags: CloneFlags) -> Result<Pid> {
	let mut stack = [0u8; 2 * 1024 * 1024];

	nix::sched::clone(cb, stack.as_mut_slice(), flags, None).map_err(|errno| Error::Clone(errno))
}

/// Returns flags for `clone(2)`, including all the sandbox-related ones.
fn clone_flags(have_unshare_newuser: bool) -> CloneFlags {
	// NOTE: CLONE_NEWUSER does not work in `clone` if we previously called `unshare` with this
	// flag. On the other hand, if we did not call `unshare` we need this flag for the CAP_SYS_ADMIN
	// capability.
	let maybe_clone_newuser =
		if have_unshare_newuser { CloneFlags::empty() } else { CloneFlags::CLONE_NEWUSER };
	// SIGCHLD flag is used to inform clone that the parent process is
	// expecting a child termination signal, without this flag `waitpid` function
	// return `ECHILD` error.
	maybe_clone_newuser |
		CloneFlags::CLONE_NEWCGROUP |
		CloneFlags::CLONE_NEWIPC |
		CloneFlags::CLONE_NEWNET |
		CloneFlags::CLONE_NEWNS |
		CloneFlags::CLONE_NEWPID |
		CloneFlags::CLONE_NEWUTS |
		CloneFlags::from_bits_retain(libc::SIGCHLD)
}
