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
use tokio::{
	fs::{File, OpenOptions},
	io::{AsyncReadExt, AsyncSeekExt, SeekFrom},
};

const SECURE_MODE_ANNOUNCEMENT: &'static str =
	"In the next release this will be a hard error by default.
     \nMore information: https://wiki.polkadot.network/docs/maintain-guides-secure-validator#secure-validator-mode";

/// Run checks for supported security features.
pub async fn check_security_status(config: &Config) -> SecurityStatus {
	let Config { prepare_worker_program_path, .. } = config;

	// TODO: add check that syslog is available and that seccomp violations are logged?
	let (landlock, seccomp, change_root) = join!(
		check_landlock(prepare_worker_program_path),
		check_seccomp(prepare_worker_program_path),
		check_can_unshare_user_namespace_and_change_root(prepare_worker_program_path)
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
) -> SecureModeResult {
	cfg_if::cfg_if! {
		if #[cfg(target_os = "linux")] {
			match tokio::process::Command::new(prepare_worker_program_path)
				.arg("--check-can-unshare-user-namespace-and-change-root")
				.output()
				.await
			{
				Ok(output) if output.status.success() => Ok(()),
				Ok(output) => {
					let stderr = std::str::from_utf8(&output.stderr)
						.expect("child process writes a UTF-8 string to stderr; qed")
						.trim();
					Err(SecureModeError::CannotUnshareUserNamespaceAndChangeRoot(
						format!("not available: {}", stderr)
					))
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
				.status()
				.await
			{
				Ok(status) if status.success() => Ok(()),
				Ok(_status) => {
					let abi =
						polkadot_node_core_pvf_common::worker::security::landlock::LANDLOCK_ABI as u8;
					Err(SecureModeError::CannotEnableLandlock(
						format!("landlock ABI {} not available", abi)
					))
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
	#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
	prepare_worker_program_path: &Path,
) -> SecureModeResult {
	cfg_if::cfg_if! {
		if #[cfg(target_os = "linux")] {
			cfg_if::cfg_if! {
				if #[cfg(target_arch = "x86_64")] {
					match tokio::process::Command::new(prepare_worker_program_path)
						.arg("--check-can-enable-seccomp")
						.status()
						.await
					{
						Ok(status) if status.success() => Ok(()),
						Ok(_status) =>
							Err(SecureModeError::CannotEnableSeccomp(
								"not available".into()
							)),
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

const AUDIT_LOG_PATH: &'static str = "/var/log/audit/audit.log";
const SYSLOG_PATH: &'static str = "/var/log/syslog";

/// System audit log.
pub struct AuditLogFile {
	file: File,
	path: &'static str,
}

impl AuditLogFile {
	/// Looks for an audit log file on the system and opens it, seeking to the end to skip any
	/// events from before this was called.
	///
	/// A bit of a verbose name, but it should clue future refactorers not to move calls closer to
	/// where the `AuditLogFile` is used.
	pub async fn try_open_and_seek_to_end() -> Option<Self> {
		let mut path = AUDIT_LOG_PATH;
		let mut file = match OpenOptions::new().read(true).open(AUDIT_LOG_PATH).await {
			Ok(file) => Ok(file),
			Err(_) => {
				path = SYSLOG_PATH;
				OpenOptions::new().read(true).open(SYSLOG_PATH).await
			},
		}
		.ok()?;

		let _pos = file.seek(SeekFrom::End(0)).await;

		Some(Self { file, path })
	}

	async fn read_new_since_open(mut self) -> String {
		let mut buf = String::new();
		let _len = self.file.read_to_string(&mut buf).await;
		buf
	}
}

/// Check if a seccomp violation occurred for the given worker. As the syslog may be in a different
/// location, or seccomp auditing may be disabled, this function provides a best-effort attempt
/// only.
///
/// The `audit_log_file` must have been obtained before the job started. It only allows reading
/// entries that were written since it was obtained, so that we do not consider events from previous
/// processes with the same pid. This can still be racy, but it's unlikely and fine for a
/// best-effort attempt.
pub async fn check_seccomp_violations_for_worker(
	audit_log_file: Option<AuditLogFile>,
	worker_pid: u32,
) -> Vec<u32> {
	let audit_event_pid_field = format!("pid={worker_pid}");

	let audit_log_file = match audit_log_file {
		Some(file) => {
			gum::debug!(
				target: LOG_TARGET,
				%worker_pid,
				audit_log_path = ?file.path,
				"checking audit log for seccomp violations",
			);
			file
		},
		None => {
			gum::warn!(
				target: LOG_TARGET,
				%worker_pid,
				"could not open either {AUDIT_LOG_PATH} or {SYSLOG_PATH} for reading audit logs"
			);
			return vec![]
		},
	};
	let events = audit_log_file.read_new_since_open().await;

	let mut violations = vec![];
	for event in events.lines() {
		if let Some(syscall) = parse_audit_log_for_seccomp_event(event, &audit_event_pid_field) {
			violations.push(syscall);
		}
	}

	violations
}

fn parse_audit_log_for_seccomp_event(event: &str, audit_event_pid_field: &str) -> Option<u32> {
	const SECCOMP_AUDIT_EVENT_TYPE: &'static str = "type=1326";

	// Do a series of simple .contains instead of a regex, because I'm not sure if the fields are
	// guaranteed to always be in the same order.
	if !event.contains(SECCOMP_AUDIT_EVENT_TYPE) || !event.contains(&audit_event_pid_field) {
		return None
	}

	// Get the syscall. Let's avoid a dependency on regex just for this.
	for field in event.split(" ") {
		if let Some(syscall) = field.strip_prefix("syscall=") {
			return syscall.parse::<u32>().ok()
		}
	}

	None
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_audit_log_for_seccomp_event() {
		let audit_event_pid_field = "pid=2559058";

		assert_eq!(
			parse_audit_log_for_seccomp_event(
				r#"Oct 24 13:15:24 build kernel: [5883980.283910] audit: type=1326 audit(1698153324.786:23): auid=0 uid=0 gid=0 ses=2162 subj=unconfined pid=2559058 comm="polkadot-prepar" exe="/root/paritytech/polkadot-sdk-2/target/debug/polkadot-prepare-worker" sig=31 arch=c000003e syscall=53 compat=0 ip=0x7f7542c80d5e code=0x80000000"#,
				audit_event_pid_field
			),
			Some(53)
		);
		// pid is wrong
		assert_eq!(
			parse_audit_log_for_seccomp_event(
				r#"Oct 24 13:15:24 build kernel: [5883980.283910] audit: type=1326 audit(1698153324.786:23): auid=0 uid=0 gid=0 ses=2162 subj=unconfined pid=2559057 comm="polkadot-prepar" exe="/root/paritytech/polkadot-sdk-2/target/debug/polkadot-prepare-worker" sig=31 arch=c000003e syscall=53 compat=0 ip=0x7f7542c80d5e code=0x80000000"#,
				audit_event_pid_field
			),
			None
		);
		// type is wrong
		assert_eq!(
			parse_audit_log_for_seccomp_event(
				r#"Oct 24 13:15:24 build kernel: [5883980.283910] audit: type=1327 audit(1698153324.786:23): auid=0 uid=0 gid=0 ses=2162 subj=unconfined pid=2559057 comm="polkadot-prepar" exe="/root/paritytech/polkadot-sdk-2/target/debug/polkadot-prepare-worker" sig=31 arch=c000003e syscall=53 compat=0 ip=0x7f7542c80d5e code=0x80000000"#,
				audit_event_pid_field
			),
			None
		);
		// no syscall field
		assert_eq!(
			parse_audit_log_for_seccomp_event(
				r#"Oct 24 13:15:24 build kernel: [5883980.283910] audit: type=1327 audit(1698153324.786:23): auid=0 uid=0 gid=0 ses=2162 subj=unconfined pid=2559057 comm="polkadot-prepar" exe="/root/paritytech/polkadot-sdk-2/target/debug/polkadot-prepare-worker" sig=31 arch=c000003e compat=0 ip=0x7f7542c80d5e code=0x80000000"#,
				audit_event_pid_field
			),
			None
		);
	}
}
