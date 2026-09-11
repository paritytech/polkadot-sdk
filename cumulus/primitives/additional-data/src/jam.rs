// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! JAM chain-state read side of the additional-data channel.
//!
//! The generic additional-data machinery (the [`AdditionalData`] map, the finalizer registry and
//! the `finalize` host function) lives in `sp-additional-data`. This crate holds the parts specific
//! to *reading JAM chain state* into that channel, mirroring the relay side in `lib.rs`:
//!
//! - [`JAM_PROOF_KEY`] — the map key under which the JAM read-proof is carried,
//! - [`JamStateReader`] + [`JamStateExt`] — the externalities extension the read host function
//!   dispatches through,
//! - [`jam_state::jam_state_read`] — the host function a parachain runtime calls to read JAM
//!   storage dynamically during block execution.
//!
//! The `key` argument is the service-local key (e.g. `para_info_key(id)`); the reader derives the
//! 31-byte state key via `service_value_state_key`, so the service id never enters the runtime.
//!
//! A read [`JAM_PROOF_KEY`] entry pairs with an `sp-additional-data` finalizer registered under
//! the same key, so the JAM read-proof is both served (here) and committed to (in the generic
//! digest).

extern crate alloc;

use alloc::vec::Vec;
use sp_runtime_interface::{
	pass_by::{PassFatPointerAndRead, PassFatPointerAndWrite},
	runtime_interface,
};

#[cfg(feature = "std")]
use sp_externalities::ExternalitiesExt;

/// Key under which the JAM state read-proof lives in the additional-data map.
///
/// The value is the SCALE-encoding of `(state_root, jam_state_helpers::StateProof)`.
pub const JAM_PROOF_KEY: &str = "jam/state_proof";

/// Serves JAM chain-state reads for [`jam_state::jam_state_read`], recording the proof it
/// touches.
///
/// On build it reads the value live and collects the touched proof nodes; on validation/import it
/// reads the value back from — and authenticates it against — the collected proof and the trusted
/// state root. Registered as a [`JamStateExt`] before executing a block that reads JAM state.
///
/// [`jam_state::jam_state_read`]: jam_state::jam_state_read
pub trait JamStateReader: Send {
	/// Read a JAM storage `key` (service-local), returning its value or `None` when (provably)
	/// absent. The reader derives the 31-byte service state key from `key` via
	/// `service_value_state_key`.
	fn read(&self, key: &[u8]) -> Option<Vec<u8>>;

	/// Estimated encoded size of the proof recorded so far — the additional-data contribution to
	/// the PoV, so the runtime's proof-size accounting budgets for it. `0` when nothing was
	/// recorded.
	fn proof_size(&self) -> usize;
}

/// Lets a shared provider register as the reader: the build/import side wraps its (only-`Send`)
/// provider in an `Arc` (of a `Sync` cell) and registers a clone under [`JamStateExt`] while the
/// same object serves the additional-data digest under `AdditionalDataExt`.
impl<T: JamStateReader + Sync + ?Sized> JamStateReader for alloc::sync::Arc<T> {
	fn read(&self, key: &[u8]) -> Option<Vec<u8>> {
		(**self).read(key)
	}

	fn proof_size(&self) -> usize {
		(**self).proof_size()
	}
}

#[cfg(feature = "std")]
sp_externalities::decl_extension! {
	/// Externalities extension backing [`jam_state::jam_state_read`].
	///
	/// Register this before executing a block that calls [`jam_state::jam_state_read`]
	/// (jam_state::jam_state_read) — on build, on `validate_block`, and on the generic block-import
	/// path.
	pub struct JamStateExt(alloc::boxed::Box<dyn JamStateReader>);
}

/// Runtime interface for reading JAM chain state into a block's additional data.
///
/// `jam_state_read` **panics** when [`JamStateExt`] is not registered — the read is
/// consensus-critical (its proof feeds the additional-data digest), so a missing extension must
/// fail loudly rather than silently diverge.
#[runtime_interface]
pub trait JamState {
	/// Read `key` from the JAM chain state, writing the value into `value_out` and returning
	/// its full length, or `-1` when the key is (provably) absent.
	///
	/// Runtime-side-allocation compatible: the runtime owns `value_out`; this host function never
	/// allocates guest memory. Prefer the [`jam_state::jam_state_read`] wrapper, which
	/// reconstructs an `Option<Vec<u8>>` (resizing its buffer if the value is larger than
	/// `value_out`). On build the value is read live and its proof collected; on validation/import
	/// it is read back from — and verified against — the carried proof and the trusted state root.
	///
	/// # Panics
	///
	/// If [`JamStateExt`] is not registered in the externalities.
	#[polkavm_index(344)]
	#[raw_api]
	fn jam_state_read_into(
		&mut self,
		key: PassFatPointerAndRead<&[u8]>,
		value_out: PassFatPointerAndWrite<&mut [u8]>,
	) -> i64 {
		let value = self
			.extension::<JamStateExt>()
			.expect(
				"JamStateExt extension not registered; \
				 this host function is consensus-critical and cannot silently diverge",
			)
			.0
			.read(key);
		match value {
			Some(v) => {
				let n = core::cmp::min(v.len(), value_out.len());
				value_out[..n].copy_from_slice(&v[..n]);
				v.len() as i64
			},
			None => -1,
		}
	}

	/// Read `key` from the JAM chain state, returning its value or `None` when (provably) absent.
	///
	/// Ergonomic wrapper over [`jam_state::jam_state_read_into`] that owns the destination buffer
	/// runtime-side, resizing once if the value is larger than the initial guess.
	#[wrapper]
	fn jam_state_read(key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
		let mut buf = Vec::new();
		buf.resize(256, 0u8);
		let len = jam_state_read_into__raw(key.as_ref(), &mut buf[..]);
		if len < 0 {
			return None;
		}
		let len = len as usize;
		if len > buf.len() {
			buf.resize(len, 0u8);
			jam_state_read_into__raw(key.as_ref(), &mut buf[..]);
		}
		buf.truncate(len);
		Some(buf)
	}
}

#[cfg(test)]
mod tests {
	use sp_state_machine::BasicExternalities;

	use super::{jam_state, JamStateExt, JamStateReader};

	struct StubReader;

	impl JamStateReader for StubReader {
		fn read(&self, key: &[u8]) -> Option<alloc::vec::Vec<u8>> {
			if key == b"present" {
				Some(vec![1u8, 2, 3, 4])
			} else if key == b"large" {
				Some(vec![0xABu8; 300])
			} else {
				None
			}
		}

		fn proof_size(&self) -> usize {
			0
		}
	}

	#[test]
	fn reads_value_through_stub_reader() {
		let mut ext = BasicExternalities::default();
		ext.register_extension(JamStateExt(Box::new(StubReader)));

		ext.execute_with(|| {
			let value = jam_state::jam_state_read(b"present");
			assert_eq!(value, Some(vec![1u8, 2, 3, 4]));

			let absent = jam_state::jam_state_read(b"missing");
			assert_eq!(absent, None);
		});
	}

	#[test]
	fn resize_path_returns_full_value_over_256_bytes() {
		let mut ext = BasicExternalities::default();
		ext.register_extension(JamStateExt(Box::new(StubReader)));

		ext.execute_with(|| {
			let value = jam_state::jam_state_read(b"large");
			assert_eq!(value, Some(vec![0xABu8; 300]));
		});
	}

	#[test]
	#[should_panic(expected = "JamStateExt extension not registered")]
	fn missing_extension_panics() {
		BasicExternalities::default().execute_with(|| {
			let _ = jam_state::jam_state_read(b"present");
		});
	}
}
