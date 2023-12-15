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

//! Functionality for securing workers.
//!
//! This is needed because workers are used to compile and execute untrusted code (PVFs).
//!
//! We currently employ the following security measures:
//!
//! - Restrict filesystem
//!   - Use Landlock to remove all unnecessary FS access rights.
//!   - Unshare the user and mount namespaces.
//!   - Change the root directory to a worker-specific temporary directory.
//! - Restrict networking by blocking socket creation and io_uring.
//! - Remove env vars

use crate::{worker::WorkerKind, LOG_TARGET};

#[cfg(target_os = "linux")]
pub mod landlock;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub mod seccomp;

/// Unshare the user namespace and change root to be the artifact directory.
///
/// NOTE: This should not be called in a multi-threaded context. `unshare(2)`:
///       "CLONE_NEWUSER requires that the calling process is not threaded."
#[cfg(target_os = "linux")]
pub fn unshare_user_namespace_and_change_root(
	worker_kind: WorkerKind,
	worker_pid: u32,
	worker_dir_path: &std::path::Path,
) -> Result<(), String> {
	use std::{env, ffi::CString, os::unix::ffi::OsStrExt, path::Path, ptr};

	// TODO: Remove this once this is stable: https://github.com/rust-lang/rust/issues/105723
	macro_rules! cstr_ptr {
		($e:expr) => {
			concat!($e, "\0").as_ptr().cast::<core::ffi::c_char>()
		};
	}

	gum::trace!(
		target: LOG_TARGET,
		%worker_kind,
		%worker_pid,
		?worker_dir_path,
		"unsharing the user namespace and calling pivot_root",
	);

	let worker_dir_path_c = CString::new(worker_dir_path.as_os_str().as_bytes())
		.expect("on unix; the path will never contain 0 bytes; qed");

	// Wrapper around all the work to prevent repetitive error handling.
	//
	// # Errors
	//
	// It's the caller's responsibility to call `Error::last_os_error`. Note that that alone does
	// not give the context of which call failed, so we return a &str error.
	|| -> Result<(), &'static str> {
		// SAFETY: We pass null-terminated C strings and use the APIs as documented. In fact, steps
		//         (2) and (3) are adapted from the example in pivot_root(2), with the additional
		//         change described in the `pivot_root(".", ".")` section.
		unsafe {
			// 1. `unshare` the user and the mount namespaces.
			if libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNS) < 0 {
				return Err("unshare user and mount namespaces")
			}

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
				return Err("mount MS_PRIVATE")
			}
			// Ensure that the new root is a mount point.
			let additional_flags =
				if let WorkerKind::Execute | WorkerKind::CheckPivotRoot = worker_kind {
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
					libc::MS_NOATIME | additional_flags,
				ptr::null(), // ignored when MS_BIND is used
			) < 0
			{
				return Err("mount MS_BIND")
			}

			// 3. `pivot_root` to the artifact directory.
			if libc::chdir(worker_dir_path_c.as_ptr()) < 0 {
				return Err("chdir to worker dir path")
			}
			if libc::syscall(libc::SYS_pivot_root, cstr_ptr!("."), cstr_ptr!(".")) < 0 {
				return Err("pivot_root")
			}
			if libc::umount2(cstr_ptr!("."), libc::MNT_DETACH) < 0 {
				return Err("umount the old root mount point")
			}
		}

		Ok(())
	}()
	.map_err(|err_ctx| {
		let err = std::io::Error::last_os_error();
		format!("{}: {}", err_ctx, err)
	})?;

	// Do some assertions.
	if env::current_dir().map_err(|err| err.to_string())? != Path::new("/") {
		return Err("expected current dir after pivot_root to be `/`".into())
	}
	env::set_current_dir("..").map_err(|err| err.to_string())?;
	if env::current_dir().map_err(|err| err.to_string())? != Path::new("/") {
		return Err("expected not to be able to break out of new root by doing `..`".into())
	}

	Ok(())
}

/// Require env vars to have been removed when spawning the process, to prevent malicious code from
/// accessing them.
pub fn check_env_vars_were_cleared(worker_kind: WorkerKind, worker_pid: u32) -> bool {
	gum::trace!(
		target: LOG_TARGET,
		%worker_kind,
		%worker_pid,
		"clearing env vars in worker",
	);

	let mut ok = true;

	for (key, value) in std::env::vars_os() {
		// TODO: *theoretically* the value (or mere presence) of `RUST_LOG` can be a source of
		// randomness for malicious code. In the future we can remove it also and log in the host;
		// see <https://github.com/paritytech/polkadot/issues/7117>.
		if key == "RUST_LOG" {
			continue
		}
		// An exception for MacOS. This is not a secure platform anyway, so we let it slide.
		#[cfg(target_os = "macos")]
		if key == "__CF_USER_TEXT_ENCODING" {
			continue
		}

		gum::error!(
			target: LOG_TARGET,
			%worker_kind,
			%worker_pid,
			?key,
			?value,
			"env var was present that should have been removed",
		);

		ok = false;
	}

	ok
}
