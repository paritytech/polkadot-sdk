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

#[cfg(target_os = "linux")]
pub mod change_root;
#[cfg(target_os = "linux")]
pub mod clone;
#[cfg(target_os = "linux")]
pub mod landlock;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub mod seccomp;

use crate::{worker::WorkerInfo, LOG_TARGET};

/// Require env vars to have been removed when spawning the process, to prevent malicious code from
/// accessing them.
pub fn check_env_vars_were_cleared(worker_info: &WorkerInfo) -> bool {
	gum::trace!(
		target: LOG_TARGET,
		?worker_info,
		"clearing env vars in worker",
	);

	let mut ok = true;

	for (key, value) in std::env::vars_os() {
		// TODO: *theoretically* the value (or mere presence) of `RUST_LOG` can be a source of
		// randomness for malicious code. It should be removed in the job process, which does no
		// logging.
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
			?worker_info,
			?key,
			?value,
			"env var was present that should have been removed",
		);

		ok = false;
	}

	ok
}
