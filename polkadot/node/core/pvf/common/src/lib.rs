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

//! Contains functionality related to PVFs that is shared by the PVF host and the PVF workers.

pub mod error;
pub mod execute;
pub mod executor_interface;
pub mod prepare;
pub mod pvf;
pub mod worker;
pub mod worker_dir;

pub use cpu_time::ProcessTime;

// Used by `decl_worker_main!`.
pub use sp_tracing;

const LOG_TARGET: &str = "parachain::pvf-common";

use parity_scale_codec::{Decode, Encode};
use std::{
	io::{self, Read, Write},
	mem,
};

#[cfg(feature = "test-utils")]
pub mod tests {
	use std::time::Duration;

	pub const TEST_EXECUTION_TIMEOUT: Duration = Duration::from_secs(3);
	pub const TEST_PREPARATION_TIMEOUT: Duration = Duration::from_secs(30);
}

/// Status of security features on the current system.
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
pub struct SecurityStatus {
	/// Whether Secure Validator Mode is enabled. This mode enforces that all required security
	/// features are present. All features are enabled on a best-effort basis regardless.
	pub secure_validator_mode: bool,
	/// Whether the landlock features we use are fully available on this system.
	pub can_enable_landlock: bool,
	/// Whether the seccomp features we use are fully available on this system.
	pub can_enable_seccomp: bool,
	/// Whether we are able to unshare the user namespace and change the filesystem root.
	pub can_unshare_user_namespace_and_change_root: bool,
	/// Whether we are able to call `clone` with all sandboxing flags.
	pub can_do_secure_clone: bool,
}

/// A handshake with information for the worker.
#[derive(Debug, Encode, Decode)]
pub struct WorkerHandshake {
	pub security_status: SecurityStatus,
}

/// Write some data prefixed by its length into `w`. Sync version of `framed_send` to avoid
/// dependency on tokio.
pub fn framed_send_blocking(w: &mut (impl Write + Unpin), buf: &[u8]) -> io::Result<()> {
	let len_buf = buf.len().to_le_bytes();
	w.write_all(&len_buf)?;
	w.write_all(buf)?;
	Ok(())
}

/// Read some data prefixed by its length from `r`. Sync version of `framed_recv` to avoid
/// dependency on tokio.
pub fn framed_recv_blocking(r: &mut (impl Read + Unpin)) -> io::Result<Vec<u8>> {
	let mut len_buf = [0u8; mem::size_of::<usize>()];
	r.read_exact(&mut len_buf)?;
	let len = usize::from_le_bytes(len_buf);
	let mut buf = vec![0; len];
	r.read_exact(&mut buf)?;
	Ok(buf)
}
