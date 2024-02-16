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

//! The [landlock] docs say it best:
//!
//! > "Landlock is a security feature available since Linux 5.13. The goal is to enable to restrict
//! ambient rights (e.g., global filesystem access) for a set of processes by creating safe security
//! sandboxes as new security layers in addition to the existing system-wide access-controls. This
//! kind of sandbox is expected to help mitigate the security impact of bugs, unexpected or
//! malicious behaviors in applications. Landlock empowers any process, including unprivileged ones,
//! to securely restrict themselves."
//!
//! [landlock]: https://docs.rs/landlock/latest/landlock/index.html

pub use landlock::RulesetStatus;

use crate::{
	worker::{stringify_panic_payload, WorkerInfo, WorkerKind},
	LOG_TARGET,
};
use landlock::*;
use std::path::{Path, PathBuf};

/// Landlock ABI version. We use ABI V1 because:
///
/// 1. It is supported by our reference kernel version.
/// 2. Later versions do not (yet) provide additional security that would benefit us.
///
/// # Versions (as of October 2023)
///
/// - Polkadot reference kernel version: 5.16+
///
/// - ABI V1: kernel 5.13 - Introduces landlock, including full restrictions on file reads.
///
/// - ABI V2: kernel 5.19 - Adds ability to prevent file renaming. Does not help us. During
///   execution an attacker can only affect the name of a symlinked artifact and not the original
///   one.
///
/// - ABI V3: kernel 6.2 - Adds ability to prevent file truncation. During execution, can
///   prevent attackers from affecting a symlinked artifact. We don't strictly need this as we
///   plan to check for file integrity anyway; see
///   <https://github.com/paritytech/polkadot-sdk/issues/677>.
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

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("Could not fully enable: {0:?}")]
	NotFullyEnabled(RulesetStatus),
	#[error("Invalid exception path: {0:?}")]
	InvalidExceptionPath(PathBuf),
	#[error(transparent)]
	RulesetError(#[from] RulesetError),
	#[error("A panic occurred in try_restrict: {0}")]
	Panic(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Try to enable landlock for the given kind of worker.
pub fn enable_for_worker(worker_info: &WorkerInfo) -> Result<()> {
	let exceptions: Vec<(PathBuf, BitFlags<AccessFs>)> = match worker_info.kind {
		WorkerKind::Prepare => {
			vec![(worker_info.worker_dir_path.to_owned(), AccessFs::WriteFile.into())]
		},
		WorkerKind::Execute => {
			vec![(worker_info.worker_dir_path.to_owned(), AccessFs::ReadFile.into())]
		},
		WorkerKind::CheckPivotRoot =>
			panic!("this should only be passed for checking pivot_root; qed"),
	};

	gum::trace!(
		target: LOG_TARGET,
		?worker_info,
		"enabling landlock with exceptions: {:?}",
		exceptions,
	);

	try_restrict(exceptions)
}

// TODO: <https://github.com/landlock-lsm/rust-landlock/issues/36>
/// Runs a check for landlock in its own thread, and returns an error indicating whether the given
/// landlock ABI is fully enabled on the current Linux environment.
pub fn check_can_fully_enable() -> Result<()> {
	match std::thread::spawn(|| try_restrict(std::iter::empty::<(PathBuf, AccessFs)>())).join() {
		Ok(Ok(())) => Ok(()),
		Ok(Err(err)) => Err(err),
		Err(err) => Err(Error::Panic(stringify_panic_payload(err))),
	}
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
fn try_restrict<I, P, A>(fs_exceptions: I) -> Result<()>
where
	I: IntoIterator<Item = (P, A)>,
	P: AsRef<Path>,
	A: Into<BitFlags<AccessFs>>,
{
	let mut ruleset =
		Ruleset::default().handle_access(AccessFs::from_all(LANDLOCK_ABI))?.create()?;
	for (fs_path, access_bits) in fs_exceptions {
		let paths = &[fs_path.as_ref().to_owned()];
		let mut rules = path_beneath_rules(paths, access_bits).peekable();
		if rules.peek().is_none() {
			// `path_beneath_rules` silently ignores missing paths, so check for it manually.
			return Err(Error::InvalidExceptionPath(fs_path.as_ref().to_owned()))
		}
		ruleset = ruleset.add_rules(rules)?;
	}

	let status = ruleset.restrict_self()?;
	if !matches!(status.ruleset, RulesetStatus::FullyEnforced) {
		return Err(Error::NotFullyEnabled(status.ruleset))
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{fs, io::ErrorKind, thread};

	#[test]
	fn restricted_thread_cannot_read_file() {
		// TODO: This would be nice: <https://github.com/rust-lang/rust/issues/68007>.
		if check_can_fully_enable().is_err() {
			return
		}

		// Restricted thread cannot read from FS.
		let handle = thread::spawn(|| {
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
			if !matches!(status, Ok(())) {
				panic!(
					"Ruleset should be enforced since we checked if landlock is enabled: {:?}",
					status
				);
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
			if !matches!(status, Ok(())) {
				panic!(
					"Ruleset should be enforced since we checked if landlock is enabled: {:?}",
					status
				);
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
		if check_can_fully_enable().is_err() {
			return
		}

		// Restricted thread cannot write to FS.
		let handle = thread::spawn(|| {
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
			if !matches!(status, Ok(())) {
				panic!(
					"Ruleset should be enforced since we checked if landlock is enabled: {:?}",
					status
				);
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
			if !matches!(status, Ok(())) {
				panic!(
					"Ruleset should be enforced since we checked if landlock is enabled: {:?}",
					status
				);
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

	// Test that checks whether landlock under our ABI version is able to truncate files.
	#[test]
	fn restricted_thread_can_truncate_file() {
		// TODO: This would be nice: <https://github.com/rust-lang/rust/issues/68007>.
		if check_can_fully_enable().is_err() {
			return
		}

		// Restricted thread can truncate file.
		let handle = thread::spawn(|| {
			// Create and write a file. This should succeed before any landlock
			// restrictions are applied.
			const TEXT: &str = "foo";
			let tmpfile = tempfile::NamedTempFile::new().unwrap();
			let path = tmpfile.path();

			fs::write(path, TEXT).unwrap();

			// Apply Landlock with all exceptions under the current ABI.
			let status = try_restrict(vec![(path, AccessFs::from_all(LANDLOCK_ABI))]);
			if !matches!(status, Ok(())) {
				panic!(
					"Ruleset should be enforced since we checked if landlock is enabled: {:?}",
					status
				);
			}

			// Try to truncate the file.
			let result = tmpfile.as_file().set_len(0);
			assert!(result.is_ok());
		});

		assert!(handle.join().is_ok());
	}
}
