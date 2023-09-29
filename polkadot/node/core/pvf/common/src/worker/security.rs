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
//! - Remove env vars

use crate::{worker::WorkerKind, LOG_TARGET};

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

	// The following was copied from the `cstr_core` crate.
	//
	// TODO: Remove this once this is stable: https://github.com/rust-lang/rust/issues/105723
	#[inline]
	#[doc(hidden)]
	const fn cstr_is_valid(bytes: &[u8]) -> bool {
		if bytes.is_empty() || bytes[bytes.len() - 1] != 0 {
			return false
		}

		let mut index = 0;
		while index < bytes.len() - 1 {
			if bytes[index] == 0 {
				return false
			}
			index += 1;
		}
		true
	}

	macro_rules! cstr {
		($e:expr) => {{
			const STR: &[u8] = concat!($e, "\0").as_bytes();
			const STR_VALID: bool = cstr_is_valid(STR);
			let _ = [(); 0 - (!(STR_VALID) as usize)];
			#[allow(unused_unsafe)]
			unsafe {
				core::ffi::CStr::from_bytes_with_nul_unchecked(STR)
			}
		}}
	}

	gum::debug!(
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
				cstr!("/").as_ptr(),
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
			if libc::syscall(libc::SYS_pivot_root, cstr!(".").as_ptr(), cstr!(".").as_ptr()) < 0 {
				return Err("pivot_root")
			}
			if libc::umount2(cstr!(".").as_ptr(), libc::MNT_DETACH) < 0 {
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

/// The [landlock] docs say it best:
///
/// > "Landlock is a security feature available since Linux 5.13. The goal is to enable to restrict
/// ambient rights (e.g., global filesystem access) for a set of processes by creating safe security
/// sandboxes as new security layers in addition to the existing system-wide access-controls. This
/// kind of sandbox is expected to help mitigate the security impact of bugs, unexpected or
/// malicious behaviors in applications. Landlock empowers any process, including unprivileged ones,
/// to securely restrict themselves."
///
/// [landlock]: https://docs.rs/landlock/latest/landlock/index.html
#[cfg(target_os = "linux")]
pub mod landlock {
	pub use landlock::RulesetStatus;

	use crate::{worker::WorkerKind, LOG_TARGET};
	use landlock::*;
	use std::{
		fmt,
		path::{Path, PathBuf},
	};

	/// Landlock ABI version. We use ABI V1 because:
	///
	/// 1. It is supported by our reference kernel version.
	/// 2. Later versions do not (yet) provide additional security.
	///
	/// # Versions (as of June 2023)
	///
	/// - Polkadot reference kernel version: 5.16+
	/// - ABI V1: 5.13 - introduces	landlock, including full restrictions on file reads
	/// - ABI V2: 5.19 - adds ability to configure file renaming (not used by us)
	///
	/// # Determinism
	///
	/// You may wonder whether we could always use the latest ABI instead of only the ABI supported
	/// by the reference kernel version. It seems plausible, since landlock provides a best-effort
	/// approach to enabling sandboxing. For example, if the reference version only supported V1 and
	/// we were on V2, then landlock would use V2 if it was supported on the current machine, and
	/// just fall back to V1 if not.
	///
	/// The issue with this is indeterminacy. If half of validators were on V2 and half were on V1,
	/// they may have different semantics on some PVFs. So a malicious PVF now has a new attack
	/// vector: they can exploit this indeterminism between landlock ABIs!
	///
	/// On the other hand we do want validators to be as secure as possible and protect their keys
	/// from attackers. And, the risk with indeterminacy is low and there are other indeterminacy
	/// vectors anyway. So we will only upgrade to a new ABI if either the reference kernel version
	/// supports it or if it introduces some new feature that is beneficial to security.
	pub const LANDLOCK_ABI: ABI = ABI::V1;

	#[derive(Debug)]
	pub enum TryRestrictError {
		InvalidExceptionPath(PathBuf),
		RulesetError(RulesetError),
	}

	impl From<RulesetError> for TryRestrictError {
		fn from(err: RulesetError) -> Self {
			Self::RulesetError(err)
		}
	}

	impl fmt::Display for TryRestrictError {
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			match self {
				Self::InvalidExceptionPath(path) => write!(f, "invalid exception path: {:?}", path),
				Self::RulesetError(err) => write!(f, "ruleset error: {}", err.to_string()),
			}
		}
	}

	impl std::error::Error for TryRestrictError {}

	/// Try to enable landlock for the given kind of worker.
	pub fn enable_for_worker(
		worker_kind: WorkerKind,
		worker_pid: u32,
		worker_dir_path: &Path,
	) -> Result<RulesetStatus, Box<dyn std::error::Error>> {
		let exceptions: Vec<(PathBuf, BitFlags<AccessFs>)> = match worker_kind {
			WorkerKind::Prepare => {
				vec![(worker_dir_path.to_owned(), AccessFs::WriteFile.into())]
			},
			WorkerKind::Execute => {
				vec![(worker_dir_path.to_owned(), AccessFs::ReadFile.into())]
			},
			WorkerKind::CheckPivotRoot =>
				panic!("this should only be passed for checking pivot_root; qed"),
		};

		gum::debug!(
			target: LOG_TARGET,
			%worker_kind,
			%worker_pid,
			?worker_dir_path,
			"enabling landlock with exceptions: {:?}",
			exceptions,
		);

		Ok(try_restrict(exceptions)?)
	}

	// TODO: <https://github.com/landlock-lsm/rust-landlock/issues/36>
	/// Runs a check for landlock and returns a single bool indicating whether the given landlock
	/// ABI is fully enabled on the current Linux environment.
	pub fn check_is_fully_enabled() -> bool {
		let status_from_thread: Result<RulesetStatus, Box<dyn std::error::Error>> =
			match std::thread::spawn(|| try_restrict(std::iter::empty::<(PathBuf, AccessFs)>()))
				.join()
			{
				Ok(Ok(status)) => Ok(status),
				Ok(Err(ruleset_err)) => Err(ruleset_err.into()),
				Err(_err) => Err("a panic occurred in try_restrict".into()),
			};

		matches!(status_from_thread, Ok(RulesetStatus::FullyEnforced))
	}

	/// Tries to restrict the current thread (should only be called in a process' main thread) with
	/// the following landlock access controls:
	///
	/// 1. all global filesystem access restricted, with optional exceptions
	/// 2. ... more sandbox types (e.g. networking) may be supported in the future.
	///
	/// If landlock is not supported in the current environment this is simply a noop.
	///
	/// # Returns
	///
	/// The status of the restriction (whether it was fully, partially, or not-at-all enforced).
	fn try_restrict<I, P, A>(fs_exceptions: I) -> Result<RulesetStatus, TryRestrictError>
	where
		I: IntoIterator<Item = (P, A)>,
		P: AsRef<Path>,
		A: Into<BitFlags<AccessFs>>,
	{
		let mut ruleset =
			Ruleset::new().handle_access(AccessFs::from_all(LANDLOCK_ABI))?.create()?;
		for (fs_path, access_bits) in fs_exceptions {
			let paths = &[fs_path.as_ref().to_owned()];
			let mut rules = path_beneath_rules(paths, access_bits).peekable();
			if rules.peek().is_none() {
				// `path_beneath_rules` silently ignores missing paths, so check for it manually.
				return Err(TryRestrictError::InvalidExceptionPath(fs_path.as_ref().to_owned()))
			}
			ruleset = ruleset.add_rules(rules)?;
		}
		let status = ruleset.restrict_self()?;
		Ok(status.ruleset)
	}

	#[cfg(test)]
	mod tests {
		use super::*;
		use std::{fs, io::ErrorKind, thread};

		#[test]
		fn restricted_thread_cannot_read_file() {
			// TODO: This would be nice: <https://github.com/rust-lang/rust/issues/68007>.
			if !check_is_fully_enabled() {
				return
			}

			// Restricted thread cannot read from FS.
			let handle =
				thread::spawn(|| {
					// Create, write, and read two tmp files. This should succeed before any
					// landlock restrictions are applied.
					const TEXT: &str = "foo";
					let tmpfile1 = tempfile::NamedTempFile::new().unwrap();
					let path1 = tmpfile1.path();
					let tmpfile2 = tempfile::NamedTempFile::new().unwrap();
					let path2 = tmpfile2.path();

					fs::write(path1, TEXT).unwrap();
					let s = fs::read_to_string(path1).unwrap();
					assert_eq!(s, TEXT);
					fs::write(path2, TEXT).unwrap();
					let s = fs::read_to_string(path2).unwrap();
					assert_eq!(s, TEXT);

					// Apply Landlock with a read exception for only one of the files.
					let status = try_restrict(vec![(path1, AccessFs::ReadFile)]);
					if !matches!(status, Ok(RulesetStatus::FullyEnforced)) {
						panic!("Ruleset should be enforced since we checked if landlock is enabled: {:?}", status);
					}

					// Try to read from both files, only tmpfile1 should succeed.
					let result = fs::read_to_string(path1);
					assert!(matches!(
						result,
						Ok(s) if s == TEXT
					));
					let result = fs::read_to_string(path2);
					assert!(matches!(
						result,
						Err(err) if matches!(err.kind(), ErrorKind::PermissionDenied)
					));

					// Apply Landlock for all files.
					let status = try_restrict(std::iter::empty::<(PathBuf, AccessFs)>());
					if !matches!(status, Ok(RulesetStatus::FullyEnforced)) {
						panic!("Ruleset should be enforced since we checked if landlock is enabled: {:?}", status);
					}

					// Try to read from tmpfile1 after landlock, it should fail.
					let result = fs::read_to_string(path1);
					assert!(matches!(
						result,
						Err(err) if matches!(err.kind(), ErrorKind::PermissionDenied)
					));
				});

			assert!(handle.join().is_ok());
		}

		#[test]
		fn restricted_thread_cannot_write_file() {
			// TODO: This would be nice: <https://github.com/rust-lang/rust/issues/68007>.
			if !check_is_fully_enabled() {
				return
			}

			// Restricted thread cannot write to FS.
			let handle =
				thread::spawn(|| {
					// Create and write two tmp files. This should succeed before any landlock
					// restrictions are applied.
					const TEXT: &str = "foo";
					let tmpfile1 = tempfile::NamedTempFile::new().unwrap();
					let path1 = tmpfile1.path();
					let tmpfile2 = tempfile::NamedTempFile::new().unwrap();
					let path2 = tmpfile2.path();

					fs::write(path1, TEXT).unwrap();
					fs::write(path2, TEXT).unwrap();

					// Apply Landlock with a write exception for only one of the files.
					let status = try_restrict(vec![(path1, AccessFs::WriteFile)]);
					if !matches!(status, Ok(RulesetStatus::FullyEnforced)) {
						panic!("Ruleset should be enforced since we checked if landlock is enabled: {:?}", status);
					}

					// Try to write to both files, only tmpfile1 should succeed.
					let result = fs::write(path1, TEXT);
					assert!(matches!(result, Ok(_)));
					let result = fs::write(path2, TEXT);
					assert!(matches!(
						result,
						Err(err) if matches!(err.kind(), ErrorKind::PermissionDenied)
					));

					// Apply Landlock for all files.
					let status = try_restrict(std::iter::empty::<(PathBuf, AccessFs)>());
					if !matches!(status, Ok(RulesetStatus::FullyEnforced)) {
						panic!("Ruleset should be enforced since we checked if landlock is enabled: {:?}", status);
					}

					// Try to write to tmpfile1 after landlock, it should fail.
					let result = fs::write(path1, TEXT);
					assert!(matches!(
						result,
						Err(err) if matches!(err.kind(), ErrorKind::PermissionDenied)
					));
				});

			assert!(handle.join().is_ok());
		}
	}
}
