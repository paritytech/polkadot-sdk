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

use crate::{Config, SecurityStatus, LOG_TARGET};
use futures::join;
use std::{fmt, path::Path};

const SECURE_MODE_ANNOUNCEMENT: &'static str =
	"In the next release this will be a hard error by default.
     \nMore information: https://wiki.polkadot.network/docs/maintain-guides-secure-validator#secure-validator-mode";

/// Run checks for supported security features.
///
/// # Returns
///
/// Returns the set of security features that we were able to enable. If an error occurs while
/// enabling a security feature we set the corresponding status to `false`.
pub async fn check_security_status(config: &Config) -> SecurityStatus {
	let Config { prepare_worker_program_path, cache_path, .. } = config;

	let (landlock, seccomp, change_root) = join!(
		check_landlock(prepare_worker_program_path),
		check_seccomp(prepare_worker_program_path),
		check_can_unshare_user_namespace_and_change_root(prepare_worker_program_path, cache_path)
	);

	let security_status = SecurityStatus {
		can_enable_landlock: landlock.is_ok(),
		can_enable_seccomp: seccomp.is_ok(),
		can_unshare_user_namespace_and_change_root: change_root.is_ok(),
	};

	let errs: Vec<SecureModeError> = [landlock, seccomp, change_root]
		.into_iter()
		.filter_map(|result| result.err())
		.collect();
	let err_occurred = print_secure_mode_message(errs);
	if err_occurred {
		gum::error!(
			target: LOG_TARGET,
			"{}",
			SECURE_MODE_ANNOUNCEMENT,
		);
	}

	security_status
}

type SecureModeResult = std::result::Result<(), SecureModeError>;

/// Errors related to enabling Secure Validator Mode.
#[derive(Debug)]
enum SecureModeError {
	CannotEnableLandlock(String),
	CannotEnableSeccomp(String),
	CannotUnshareUserNamespaceAndChangeRoot(String),
}

impl SecureModeError {
	/// Whether this error is allowed with Secure Validator Mode enabled.
	fn is_allowed_in_secure_mode(&self) -> bool {
		use SecureModeError::*;
		match self {
			CannotEnableLandlock(_) => true,
			CannotEnableSeccomp(_) => false,
			CannotUnshareUserNamespaceAndChangeRoot(_) => false,
		}
	}
}

impl fmt::Display for SecureModeError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		use SecureModeError::*;
		match self {
			CannotEnableLandlock(err) => write!(f, "Cannot enable landlock, a Linux 5.13+ kernel security feature: {err}"),
			CannotEnableSeccomp(err) => write!(f, "Cannot enable seccomp, a Linux-specific kernel security feature: {err}"),
			CannotUnshareUserNamespaceAndChangeRoot(err) => write!(f, "Cannot unshare user namespace and change root, which are Linux-specific kernel security features: {err}"),
		}
	}
}

/// Errors if Secure Validator Mode and some mandatory errors occurred, warn otherwise.
///
/// # Returns
///
/// `true` if an error was printed, `false` otherwise.
fn print_secure_mode_message(errs: Vec<SecureModeError>) -> bool {
	// Trying to run securely and some mandatory errors occurred.
	const SECURE_MODE_ERROR: &'static str = "🚨 Your system cannot securely run a validator. \
		 \nRunning validation of malicious PVF code has a higher risk of compromising this machine.";
	// Some errors occurred when running insecurely, or some optional errors occurred when running
	// securely.
	const SECURE_MODE_WARNING: &'static str = "🚨 Some security issues have been detected. \
		 \nRunning validation of malicious PVF code has a higher risk of compromising this machine.";

	if errs.is_empty() {
		return false
	}

	let errs_allowed = errs.iter().all(|err| err.is_allowed_in_secure_mode());
	let errs_string: String = errs
		.iter()
		.map(|err| {
			format!(
				"\n  - {}{}",
				if err.is_allowed_in_secure_mode() { "Optional: " } else { "" },
				err
			)
		})
		.collect();

	if errs_allowed {
		gum::warn!(
			target: LOG_TARGET,
			"{}{}",
			SECURE_MODE_WARNING,
			errs_string,
		);
		false
	} else {
		gum::error!(
			target: LOG_TARGET,
			"{}{}",
			SECURE_MODE_ERROR,
			errs_string,
		);
		true
	}
}

/// Check if we can change root to a new, sandboxed root and return an error if not.
///
/// We do this check by spawning a new process and trying to sandbox it. To get as close as possible
/// to running the check in a worker, we try it... in a worker. The expected return status is 0 on
/// success and -1 on failure.
async fn check_can_unshare_user_namespace_and_change_root(
	#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
	prepare_worker_program_path: &Path,
	#[cfg_attr(not(target_os = "linux"), allow(unused_variables))] cache_path: &Path,
) -> SecureModeResult {
	cfg_if::cfg_if! {
		if #[cfg(target_os = "linux")] {
			let cache_dir_tempdir = tempfile::Builder::new()
				.prefix("check-can-unshare-")
				.tempdir_in(cache_path)
				.map_err(|err| SecureModeError::CannotUnshareUserNamespaceAndChangeRoot(
					format!("could not create a temporary directory in {:?}: {}", cache_path, err)
				))?;
			match tokio::process::Command::new(prepare_worker_program_path)
				.arg("--check-can-unshare-user-namespace-and-change-root")
				.arg(cache_dir_tempdir.path())
				.output()
				.await
			{
				Ok(output) if output.status.success() => Ok(()),
				Ok(output) => {
					let stderr = std::str::from_utf8(&output.stderr)
						.expect("child process writes a UTF-8 string to stderr; qed")
						.trim();
					if stderr.is_empty() {
						Err(SecureModeError::CannotUnshareUserNamespaceAndChangeRoot(
							"not available".into()
						))
					} else {
						Err(SecureModeError::CannotUnshareUserNamespaceAndChangeRoot(
							format!("not available: {}", stderr)
						))
					}
				},
				Err(err) =>
					Err(SecureModeError::CannotUnshareUserNamespaceAndChangeRoot(
						format!("could not start child process: {}", err)
					)),
			}
		} else {
			Err(SecureModeError::CannotUnshareUserNamespaceAndChangeRoot(
				"only available on Linux".into()
			))
		}
	}
}

/// Check if landlock is supported and return an error if not.
///
/// We do this check by spawning a new process and trying to sandbox it. To get as close as possible
/// to running the check in a worker, we try it... in a worker. The expected return status is 0 on
/// success and -1 on failure.
async fn check_landlock(
	#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
	prepare_worker_program_path: &Path,
) -> SecureModeResult {
	cfg_if::cfg_if! {
		if #[cfg(target_os = "linux")] {
			match tokio::process::Command::new(prepare_worker_program_path)
				.arg("--check-can-enable-landlock")
				.output()
				.await
			{
				Ok(output) if output.status.success() => Ok(()),
				Ok(output) => {
					let abi =
						polkadot_node_core_pvf_common::worker::security::landlock::LANDLOCK_ABI as u8;
					let stderr = std::str::from_utf8(&output.stderr)
						.expect("child process writes a UTF-8 string to stderr; qed")
						.trim();
					if stderr.is_empty() {
						Err(SecureModeError::CannotEnableLandlock(
							format!("landlock ABI {} not available", abi)
						))
					} else {
						Err(SecureModeError::CannotEnableLandlock(
							format!("not available: {}", stderr)
						))
					}
				},
				Err(err) =>
					Err(SecureModeError::CannotEnableLandlock(
						format!("could not start child process: {}", err)
					)),
			}
		} else {
			Err(SecureModeError::CannotEnableLandlock(
				"only available on Linux".into()
			))
		}
	}
}

/// Check if seccomp is supported and return an error if not.
///
/// We do this check by spawning a new process and trying to sandbox it. To get as close as possible
/// to running the check in a worker, we try it... in a worker. The expected return status is 0 on
/// success and -1 on failure.
async fn check_seccomp(
	#[cfg_attr(not(all(target_os = "linux", target_arch = "x86_64")), allow(unused_variables))]
	prepare_worker_program_path: &Path,
) -> SecureModeResult {
	cfg_if::cfg_if! {
		if #[cfg(target_os = "linux")] {
			cfg_if::cfg_if! {
				if #[cfg(target_arch = "x86_64")] {
					match tokio::process::Command::new(prepare_worker_program_path)
						.arg("--check-can-enable-seccomp")
						.output()
						.await
					{
						Ok(output) if output.status.success() => Ok(()),
						Ok(output) => {
							let stderr = std::str::from_utf8(&output.stderr)
								.expect("child process writes a UTF-8 string to stderr; qed")
								.trim();
							if stderr.is_empty() {
								Err(SecureModeError::CannotEnableSeccomp(
									"not available".into()
								))
							} else {
								Err(SecureModeError::CannotEnableSeccomp(
									format!("not available: {}", stderr)
								))
							}
						},
						Err(err) =>
							Err(SecureModeError::CannotEnableSeccomp(
								format!("could not start child process: {}", err)
							)),
					}
				} else {
					Err(SecureModeError::CannotEnableSeccomp(
						"only supported on CPUs from the x86_64 family (usually Intel or AMD)".into()
					))
				}
			}
		} else {
			cfg_if::cfg_if! {
				if #[cfg(target_arch = "x86_64")] {
					Err(SecureModeError::CannotEnableSeccomp(
						"only supported on Linux".into()
					))
				} else {
					Err(SecureModeError::CannotEnableSeccomp(
						"only supported on Linux and on CPUs from the x86_64 family (usually Intel or AMD).".into()
					))
				}
			}
		}
	}
}
