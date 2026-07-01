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

//! Functionality for securing workers by unsharing some namespaces from other processes and
//! changing the root.

use crate::{
	worker::{WorkerInfo, WorkerKind},
	LOG_TARGET,
};
use std::{
	env,
	ffi::CString,
	io::{self, Write},
	os::unix::ffi::OsStrExt,
	path::Path,
	ptr,
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("{0}")]
	OsErrWithContext(String),
	#[error(transparent)]
	Io(#[from] io::Error),
	#[error("assertion failed: {0}")]
	AssertionFailed(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Try to enable for the given kind of worker.
///
/// NOTE: This should not be called in a multi-threaded context. `unshare(2)`:
///       "CLONE_NEWUSER requires that the calling process is not threaded."
pub fn enable_for_worker(worker_info: &WorkerInfo) -> Result<()> {
	gum::trace!(
		target: LOG_TARGET,
		?worker_info,
		"enabling change-root",
	);

	try_restrict(worker_info)
}

/// Runs a check for unshare-and-change-root and returns an error indicating whether it can be fully
/// enabled on the current Linux environment.
///
/// NOTE: This should not be called in a multi-threaded context. `unshare(2)`:
///       "CLONE_NEWUSER requires that the calling process is not threaded."
pub fn check_can_fully_enable(tempdir: &Path) -> Result<()> {
	let worker_dir_path = tempdir.to_owned();
	try_restrict(&WorkerInfo {
		pid: std::process::id(),
		kind: WorkerKind::CheckPivotRoot,
		version: None,
		worker_dir_path,
	})
}

/// Unshare the user namespace and change root to be the worker directory.
///
/// NOTE: This should not be called in a multi-threaded context. `unshare(2)`:
///       "CLONE_NEWUSER requires that the calling process is not threaded."
fn try_restrict(worker_info: &WorkerInfo) -> Result<()> {
	// TODO: Remove this once this is stable: https://github.com/rust-lang/rust/issues/105723
	macro_rules! cstr_ptr {
		($e:expr) => {
			concat!($e, "\0").as_ptr().cast::<core::ffi::c_char>()
		};
	}

	let worker_dir_path_c = CString::new(worker_info.worker_dir_path.as_os_str().as_bytes())
		.expect("on unix; the path will never contain 0 bytes; qed");

	// Wrapper around all the work to prevent repetitive error handling.
	//
	// # Errors
	//
	// It's the caller's responsibility to call `Error::last_os_error`. Note that that alone does
	// not give the context of which call failed, so we return a &str error.
	|| -> std::result::Result<(), &'static str> {
		// Read the uid/gid *before* unsharing. Once in the new user namespace they read
		// as the overflow id (65534) until a map is written, and the unprivileged
		// single-line `uid_map` write requires the outside id to equal the writer's id
		// in the parent namespace — writing the overflow id is rejected with EPERM.
		// SAFETY: getuid is documented as never failing.
		let uid = unsafe { libc::getuid() };
		// SAFETY: getgid is documented as never failing.
		let gid = unsafe { libc::getgid() };

		// 1. `unshare` the user and the mount namespaces.
		// SAFETY: no preconditions beyond running single-threaded (caller's responsibility).
		if unsafe { libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNS) } < 0 {
			return Err("unshare user and mount namespaces");
		}

		// Establish a 1-line identity uid/gid map so the worker's id is mapped in the
		// new user namespace. Without it, a nested `CLONE_NEWUSER` — which PolkaVM's
		// JIT sandbox performs — is denied with EPERM (see clone(2) / `create_user_ns`).
		// `setgroups=deny` must come first, per the CVE-2014-8989 mitigation.
		//
		// Open each O_WRONLY and write once. `std::fs::write` would add `O_TRUNC`, whose
		// effect on these procfs files is unspecified (open(2)); the kernel also requires
		// `uid_map` to arrive in a single `write`, which `write_all` satisfies for this
		// tiny buffer. Mirrors how runc / polkavm write these files.
		let write_proc = |path: &str, data: &str| -> io::Result<()> {
			std::fs::OpenOptions::new().write(true).open(path)?.write_all(data.as_bytes())
		};
		if write_proc("/proc/self/setgroups", "deny").is_err() {
			return Err("write /proc/self/setgroups");
		}
		if write_proc("/proc/self/uid_map", &format!("{uid} {uid} 1")).is_err() {
			return Err("write /proc/self/uid_map");
		}
		if write_proc("/proc/self/gid_map", &format!("{gid} {gid} 1")).is_err() {
			return Err("write /proc/self/gid_map");
		}

		// SAFETY: We pass null-terminated C strings and use the APIs as documented. In fact, steps
		//         (2) and (3) are adapted from the example in pivot_root(2), with the additional
		//         change described in the `pivot_root(".", ".")` section.
		unsafe {
			// 2. Setup mounts.
			//
			// Ensure that new root and its parent mount don't have shared propagation (which would
			// cause pivot_root() to return an error), and prevent propagation of mount events to
			// the initial mount namespace.
			if libc::mount(
				ptr::null(),
				cstr_ptr!("/"),
				ptr::null(),
				libc::MS_REC | libc::MS_PRIVATE,
				ptr::null(),
			) < 0
			{
				return Err("mount MS_PRIVATE");
			}
			// Ensure that the new root is a mount point.
			let additional_flags =
				if let WorkerKind::Execute | WorkerKind::CheckPivotRoot = worker_info.kind {
					libc::MS_RDONLY
				} else {
					0
				};
			if libc::mount(
				worker_dir_path_c.as_ptr(),
				worker_dir_path_c.as_ptr(),
				ptr::null(), // ignored when MS_BIND is used
				libc::MS_BIND |
					libc::MS_REC | libc::MS_NOEXEC |
					libc::MS_NODEV | libc::MS_NOSUID |
					libc::MS_NOATIME |
					additional_flags,
				ptr::null(), // ignored when MS_BIND is used
			) < 0
			{
				return Err("mount MS_BIND");
			}

			// 3. `pivot_root` to the artifact directory.
			if libc::chdir(worker_dir_path_c.as_ptr()) < 0 {
				return Err("chdir to worker dir path");
			}
			if libc::syscall(libc::SYS_pivot_root, cstr_ptr!("."), cstr_ptr!(".")) < 0 {
				return Err("pivot_root");
			}
			if libc::umount2(cstr_ptr!("."), libc::MNT_DETACH) < 0 {
				return Err("umount the old root mount point");
			}
		}

		Ok(())
	}()
	.map_err(|err_ctx| {
		let err = io::Error::last_os_error();
		Error::OsErrWithContext(format!("{}: {}", err_ctx, err))
	})?;

	// Do some assertions.
	if env::current_dir()? != Path::new("/") {
		return Err(Error::AssertionFailed(
			"expected current dir after pivot_root to be `/`".into(),
		));
	}
	env::set_current_dir("..")?;
	if env::current_dir()? != Path::new("/") {
		return Err(Error::AssertionFailed(
			"expected not to be able to break out of new root by doing `..`".into(),
		));
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Runs `probe` in a `fork`ed child and returns its exit code, or `None` if it
	/// did not exit normally.
	///
	/// `unshare(CLONE_NEWUSER)` is only permitted in a single-threaded process, but
	/// the test harness is multi-threaded. A `fork`ed child inherits only the calling
	/// thread, so it *is* single-threaded — whereas a spawned thread would leave the
	/// process multi-threaded and the call would fail with EINVAL. Forking also keeps
	/// `try_restrict`'s `pivot_root` from clobbering the test process itself.
	fn exit_code_of_forked(probe: impl FnOnce() -> i32) -> Option<i32> {
		// SAFETY: the child runs `probe` then `_exit`s; it never returns into libtest.
		match unsafe { libc::fork() } {
			-1 => panic!("fork failed: {}", std::io::Error::last_os_error()),
			0 => {
				let code = probe();
				// SAFETY: child path — `_exit` without unwinding back into the harness
				// or running destructors.
				unsafe { libc::_exit(code) }
			},
			child => {
				let mut status: libc::c_int = 0;
				// SAFETY: `status` is a valid out-pointer for the duration of the call.
				if unsafe { libc::waitpid(child, &mut status, 0) } < 0 {
					panic!("waitpid failed: {}", std::io::Error::last_os_error());
				}
				if libc::WIFEXITED(status) {
					Some(libc::WEXITSTATUS(status))
				} else {
					None
				}
			},
		}
	}

	/// A nested `CLONE_NEWUSER` — the user-namespace creation PolkaVM's JIT sandbox
	/// performs — succeeds inside the namespace established by `try_restrict`.
	///
	/// Regression test for the PVF execute worker denying that sandbox: without an
	/// identity uid/gid map in the worker's new user namespace, the worker's uid is
	/// unmapped there and the nested `CLONE_NEWUSER` returns EPERM.
	#[test]
	fn nested_user_namespace_works_after_restrict() {
		// Skip only when the host genuinely cannot create user namespaces at all (e.g.
		// Docker without `--privileged`), never because our own sequence failed — that
		// is the regression under test. Mirrors the `seccomp`/`landlock` skip style.
		let host_supports_userns = exit_code_of_forked(|| {
			// SAFETY: single-threaded (just forked).
			if unsafe { libc::unshare(libc::CLONE_NEWUSER) } == 0 {
				0
			} else {
				1
			}
		});
		if host_supports_userns != Some(0) {
			return;
		}

		let tmp = tempfile::tempdir().unwrap();
		let worker_dir = tmp.path().to_owned();
		let code = exit_code_of_forked(move || {
			let worker_info = WorkerInfo {
				pid: std::process::id(),
				kind: WorkerKind::CheckPivotRoot,
				version: None,
				worker_dir_path: worker_dir,
			};
			if let Err(err) = try_restrict(&worker_info) {
				eprintln!("try_restrict failed: {err}");
				return 1;
			}
			// The nested user namespace PolkaVM's sandbox creates after the worker has
			// already unshared one. SAFETY: single-threaded (just forked).
			if unsafe { libc::unshare(libc::CLONE_NEWUSER) } != 0 {
				eprintln!(
					"nested unshare(CLONE_NEWUSER) after restrict failed: {}",
					std::io::Error::last_os_error()
				);
				return 2;
			}
			0
		});
		assert_eq!(
			code,
			Some(0),
			"creating a nested user namespace after try_restrict failed (see child stderr)",
		);
	}
}
