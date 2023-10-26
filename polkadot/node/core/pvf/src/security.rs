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

use crate::LOG_TARGET;
use std::path::Path;

/// Check if we can sandbox the root and emit a warning if not.
///
/// We do this check by spawning a new process and trying to sandbox it. To get as close as possible
/// to running the check in a worker, we try it... in a worker. The expected return status is 0 on
/// success and -1 on failure.
pub async fn check_can_unshare_user_namespace_and_change_root(
	#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
	prepare_worker_program_path: &Path,
) -> bool {
	cfg_if::cfg_if! {
		if #[cfg(target_os = "linux")] {
			match tokio::process::Command::new(prepare_worker_program_path)
				.arg("--check-can-unshare-user-namespace-and-change-root")
				.output()
				.await
			{
				Ok(output) if output.status.success() => true,
				Ok(output) => {
					let stderr = std::str::from_utf8(&output.stderr)
						.expect("child process writes a UTF-8 string to stderr; qed")
						.trim();
					gum::warn!(
						target: LOG_TARGET,
						?prepare_worker_program_path,
						// Docs say to always print status using `Display` implementation.
						status = %output.status,
						%stderr,
						"Cannot unshare user namespace and change root, which are Linux-specific kernel security features. Running validation of malicious PVF code has a higher risk of compromising this machine. Consider running with support for unsharing user namespaces for maximum security."
					);
					false
				},
				Err(err) => {
					gum::warn!(
						target: LOG_TARGET,
						?prepare_worker_program_path,
						"Could not start child process: {}",
						err
					);
					false
				},
			}
		} else {
			gum::warn!(
				target: LOG_TARGET,
				"Cannot unshare user namespace and change root, which are Linux-specific kernel security features. Running validation of malicious PVF code has a higher risk of compromising this machine. Consider running on Linux with support for unsharing user namespaces for maximum security."
			);
			false
		}
	}
}

/// Check if landlock is supported and emit a warning if not.
///
/// We do this check by spawning a new process and trying to sandbox it. To get as close as possible
/// to running the check in a worker, we try it... in a worker. The expected return status is 0 on
/// success and -1 on failure.
pub async fn check_landlock(
	#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
	prepare_worker_program_path: &Path,
) -> bool {
	cfg_if::cfg_if! {
		if #[cfg(target_os = "linux")] {
			match tokio::process::Command::new(prepare_worker_program_path)
				.arg("--check-can-enable-landlock")
				.status()
				.await
			{
				Ok(status) if status.success() => true,
				Ok(status) => {
					let abi =
						polkadot_node_core_pvf_common::worker::security::landlock::LANDLOCK_ABI as u8;
					gum::warn!(
						target: LOG_TARGET,
						?prepare_worker_program_path,
						?status,
						%abi,
						"Cannot fully enable landlock, a Linux-specific kernel security feature. Running validation of malicious PVF code has a higher risk of compromising this machine. Consider upgrading the kernel version for maximum security."
					);
					false
				},
				Err(err) => {
					gum::warn!(
						target: LOG_TARGET,
						?prepare_worker_program_path,
						"Could not start child process: {}",
						err
					);
					false
				},
			}
		} else {
			gum::warn!(
				target: LOG_TARGET,
				"Cannot enable landlock, a Linux-specific kernel security feature. Running validation of malicious PVF code has a higher risk of compromising this machine. Consider running on Linux with landlock support for maximum security."
			);
			false
		}
	}
}

/// Check if seccomp is supported and emit a warning if not.
///
/// We do this check by spawning a new process and trying to sandbox it. To get as close as possible
/// to running the check in a worker, we try it... in a worker. The expected return status is 0 on
/// success and -1 on failure.
pub async fn check_seccomp(
	#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
	prepare_worker_program_path: &Path,
) -> bool {
	cfg_if::cfg_if! {
		if #[cfg(target_os = "linux")] {
			match tokio::process::Command::new(prepare_worker_program_path)
				.arg("--check-can-enable-seccomp")
				.status()
				.await
			{
				Ok(status) if status.success() => true,
				Ok(status) => {
					gum::warn!(
						target: LOG_TARGET,
						?prepare_worker_program_path,
						?status,
						"Cannot fully enable seccomp, a Linux-specific kernel security feature. Running validation of malicious PVF code has a higher risk of compromising this machine. Consider upgrading the kernel version for maximum security."
					);
					false
				},
				Err(err) => {
					gum::warn!(
						target: LOG_TARGET,
						?prepare_worker_program_path,
						"Could not start child process: {}",
						err
					);
					false
				},
			}
		} else {
			gum::warn!(
				target: LOG_TARGET,
				"Cannot enable seccomp, a Linux-specific kernel security feature. Running validation of malicious PVF code has a higher risk of compromising this machine. Consider running on Linux with seccomp support for maximum security."
			);
			false
		}
	}
}
