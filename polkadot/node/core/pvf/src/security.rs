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

/// Run checks for supported security features.
///
/// # Returns
///
/// Returns the set of security features that we were able to enable. If an error occurs while
/// enabling a security feature we set the corresponding status to `false`.
///
/// # Errors
///
/// Returns an error only if we could not fully enforce the security level required by the current
/// configuration.
pub async fn check_security_status(config: &Config) -> Result<SecurityStatus, String> {
	let Config { prepare_worker_program_path, secure_validator_mode, cache_path, .. } = config;

	let (landlock, seccomp, change_root) = join!(
		check_landlock(prepare_worker_program_path),
		check_seccomp(prepare_worker_program_path),
		check_can_unshare_user_namespace_and_change_root(prepare_worker_program_path, cache_path)
	);

	let full_security_status =
		FullSecurityStatus::new(*secure_validator_mode, landlock, seccomp, change_root);
	let security_status = full_security_status.as_partial();

	if full_security_status.err_occurred() {
		print_secure_mode_error_or_warning(&full_security_status);
		if !full_security_status.all_errs_allowed() {
			return Err("could not enable Secure Validator Mode; check logs".into())
		}
	}

	if security_status.secure_validator_mode {
		gum::info!(
			target: LOG_TARGET,
			"👮‍♀️ Running in Secure Validator Mode. \
			 It is highly recommended that you operate according to our security guidelines. \
			 \nMore information: https://wiki.polkadot.network/docs/maintain-guides-secure-validator#secure-validator-mode"
		);
	}

	Ok(security_status)
}

/// Contains the full security status including error states.
struct FullSecurityStatus {
	partial: SecurityStatus,
	errs: Vec<SecureModeError>,
}

impl FullSecurityStatus {
	fn new(
		secure_validator_mode: bool,
		landlock: SecureModeResult,
		seccomp: SecureModeResult,
		change_root: SecureModeResult,
	) -> Self {
		Self {
			partial: SecurityStatus {
				secure_validator_mode,
				can_enable_landlock: landlock.is_ok(),
				can_enable_seccomp: seccomp.is_ok(),
				can_unshare_user_namespace_and_change_root: change_root.is_ok(),
			},
			errs: [landlock, seccomp, change_root]
				.into_iter()
				.filter_map(|result| result.err())
				.collect(),
		}
	}

	fn as_partial(&self) -> SecurityStatus {
		self.partial.clone()
	}

	fn err_occurred(&self) -> bool {
		!self.errs.is_empty()
	}

	fn all_errs_allowed(&self) -> bool {
		!self.partial.secure_validator_mode ||
			self.errs.iter().all(|err| err.is_allowed_in_secure_mode(&self.partial))
	}

	fn errs_string(&self) -> String {
		self.errs
			.iter()
			.map(|err| {
				format!(
					"\n  - {}{}",
					if err.is_allowed_in_secure_mode(&self.partial) { "Optional: " } else { "" },
					err
				)
			})
			.collect()
	}
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
	fn is_allowed_in_secure_mode(&self, security_status: &SecurityStatus) -> bool {
		use SecureModeError::*;
		match self {
			// Landlock is present on relatively recent Linuxes. This is optional if the unshare
			// capability is present, providing FS sandboxing a different way.
			CannotEnableLandlock(_) => security_status.can_unshare_user_namespace_and_change_root,
			// seccomp should be present on all modern Linuxes unless it's been disabled.
			CannotEnableSeccomp(_) => false,
			// Should always be present on modern Linuxes. If not, Landlock also provides FS
			// sandboxing, so don't enforce this.
			CannotUnshareUserNamespaceAndChangeRoot(_) => security_status.can_enable_landlock,
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

/// Print an error if Secure Validator Mode and some mandatory errors occurred, warn otherwise.
fn print_secure_mode_error_or_warning(security_status: &FullSecurityStatus) {
	// Trying to run securely and some mandatory errors occurred.
	const SECURE_MODE_ERROR: &'static str = "🚨 Your system cannot securely run a validator. \
		 \nRunning validation of malicious PVF code has a higher risk of compromising this machine.";
	// Some errors occurred when running insecurely, or some optional errors occurred when running
	// securely.
	const SECURE_MODE_WARNING: &'static str = "🚨 Some security issues have been detected. \
		 \nRunning validation of malicious PVF code has a higher risk of compromising this machine.";
	// Message to be printed only when running securely and mandatory errors occurred.
	const IGNORE_SECURE_MODE_TIP: &'static str =
		"\nYou can ignore this error with the `--insecure-validator-i-know-what-i-do` \
		 command line argument if you understand and accept the risks of running insecurely. \
		 With this flag, security features are enabled on a best-effort basis, but not mandatory. \
		 \nMore information: https://wiki.polkadot.network/docs/maintain-guides-secure-validator#secure-validator-mode";

	let all_errs_allowed = security_status.all_errs_allowed();
	let errs_string = security_status.errs_string();

	if all_errs_allowed {
		gum::warn!(
			target: LOG_TARGET,
			"{}{}",
			SECURE_MODE_WARNING,
			errs_string,
		);
	} else {
		gum::error!(
			target: LOG_TARGET,
			"{}{}{}",
			SECURE_MODE_ERROR,
			errs_string,
			IGNORE_SECURE_MODE_TIP
		);
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_secure_mode_error_optionality() {
		let err = SecureModeError::CannotEnableLandlock(String::new());
		assert!(err.is_allowed_in_secure_mode(&SecurityStatus {
			secure_validator_mode: true,
			can_enable_landlock: false,
			can_enable_seccomp: false,
			can_unshare_user_namespace_and_change_root: true
		}));
		assert!(!err.is_allowed_in_secure_mode(&SecurityStatus {
			secure_validator_mode: true,
			can_enable_landlock: false,
			can_enable_seccomp: true,
			can_unshare_user_namespace_and_change_root: false
		}));

		let err = SecureModeError::CannotEnableSeccomp(String::new());
		assert!(!err.is_allowed_in_secure_mode(&SecurityStatus {
			secure_validator_mode: true,
			can_enable_landlock: false,
			can_enable_seccomp: false,
			can_unshare_user_namespace_and_change_root: true
		}));
		assert!(!err.is_allowed_in_secure_mode(&SecurityStatus {
			secure_validator_mode: true,
			can_enable_landlock: false,
			can_enable_seccomp: true,
			can_unshare_user_namespace_and_change_root: false
		}));

		let err = SecureModeError::CannotUnshareUserNamespaceAndChangeRoot(String::new());
		assert!(err.is_allowed_in_secure_mode(&SecurityStatus {
			secure_validator_mode: true,
			can_enable_landlock: true,
			can_enable_seccomp: false,
			can_unshare_user_namespace_and_change_root: false
		}));
		assert!(!err.is_allowed_in_secure_mode(&SecurityStatus {
			secure_validator_mode: true,
			can_enable_landlock: false,
			can_enable_seccomp: true,
			can_unshare_user_namespace_and_change_root: false
		}));
	}
}
