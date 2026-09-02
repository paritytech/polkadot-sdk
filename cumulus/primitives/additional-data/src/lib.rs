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

//! Relay/JAM chain-state read side of the additional-data channel.
//!
//! The generic additional-data machinery (the [`AdditionalData`] map, the finalizer registry and
//! the `finalize` host function) lives in `sp-additional-data`. This crate holds the parts specific
//! to *reading relay/JAM chain state* into that channel:
//!
//! - [`RELAY_PROOF_KEY`] — the map key under which the relay read-proof is carried,
//! - [`RelayStateReader`] + [`RelayStateExt`] — the externalities extension the read host function
//!   dispatches through,
//! - [`relay_chain_state::read_relay_chain_state`] — the host function a parachain runtime calls to
//!   read relay/JAM storage dynamically during block execution.
//!
//! A read [`RELAY_PROOF_KEY`] entry pairs with an `sp-additional-data` finalizer registered under
//! the same key, so the relay read-proof is both served (here) and committed to (in the generic
//! digest).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use sp_runtime_interface::{
	pass_by::{PassFatPointerAndRead, PassFatPointerAndWrite},
	runtime_interface,
};

#[cfg(feature = "std")]
use sp_externalities::ExternalitiesExt;

/// Key under which the relay/JAM state read-proof lives in the additional-data map.
///
/// The value is the SCALE-encoding of `(root, sp_trie::StorageProof)`.
pub const RELAY_PROOF_KEY: &str = "polkadot/relay_proof";

/// Serves relay/JAM chain-state reads for [`read_relay_chain_state`], recording the proof it
/// touches.
///
/// On build it reads the value live and collects the touched proof nodes; on validation/import it
/// reads the value back from — and authenticates it against — the collected proof and the trusted
/// root. Registered as a [`RelayStateExt`] before executing a block that reads relay state.
///
/// [`read_relay_chain_state`]: relay_chain_state::read_relay_chain_state
pub trait RelayStateReader: Send {
	/// Read a relay/JAM storage `key`, returning its value or `None` when (provably) absent.
	fn read(&self, key: &[u8]) -> Option<Vec<u8>>;

	/// Estimated encoded size of the proof recorded so far — the additional-data contribution to
	/// the PoV, so the runtime's proof-size accounting budgets for it. `0` when nothing was
	/// recorded.
	fn proof_size(&self) -> usize;
}

/// Lets a shared provider register as the reader: the build/import side wraps its (only-`Send`)
/// provider in an `Arc` (of a `Sync` cell) and registers a clone under [`RelayStateExt`] while the
/// same object serves the additional-data digest under `AdditionalDataExt`.
impl<T: RelayStateReader + Sync + ?Sized> RelayStateReader for alloc::sync::Arc<T> {
	fn read(&self, key: &[u8]) -> Option<Vec<u8>> {
		(**self).read(key)
	}

	fn proof_size(&self) -> usize {
		(**self).proof_size()
	}
}

#[cfg(feature = "std")]
sp_externalities::decl_extension! {
	/// Externalities extension backing [`read_relay_chain_state`].
	///
	/// Register this before executing a block that calls
	/// [`read_relay_chain_state`](relay_chain_state::read_relay_chain_state) — on build, on
	/// `validate_block`, and on the generic block-import path.
	pub struct RelayStateExt(alloc::boxed::Box<dyn RelayStateReader>);
}

/// Runtime interface for reading relay/JAM chain state into a block's additional data.
///
/// `read_relay_chain_state` **panics** when [`RelayStateExt`] is not registered — the read is
/// consensus-critical (its proof feeds the additional-data digest), so a missing extension must
/// fail loudly rather than silently diverge.
#[runtime_interface]
pub trait RelayChainState {
	/// Read `key` from the relay/JAM chain state, writing the value into `value_out` and returning
	/// its full length, or `-1` when the key is (provably) absent.
	///
	/// Runtime-side-allocation compatible: the runtime owns `value_out`; this host function never
	/// allocates guest memory. Prefer the [`read_relay_chain_state`] wrapper, which reconstructs an
	/// `Option<Vec<u8>>` (resizing its buffer if the value is larger than `value_out`). On build
	/// the value is read live and its proof collected; on validation/import it is read back from —
	/// and verified against — the carried proof and the trusted root.
	///
	/// # Panics
	///
	/// If [`RelayStateExt`] is not registered in the externalities.
	#[polkavm_index(242)]
	#[raw_api]
	fn read_relay_chain_state_into(
		&mut self,
		key: PassFatPointerAndRead<&[u8]>,
		value_out: PassFatPointerAndWrite<&mut [u8]>,
	) -> i64 {
		let value = self
			.extension::<RelayStateExt>()
			.expect(
				"RelayStateExt extension not registered; \
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

	/// Read `key` from the relay/JAM chain state, returning its value or `None` when (provably)
	/// absent.
	///
	/// Ergonomic wrapper over [`read_relay_chain_state_into`] that owns the destination buffer
	/// runtime-side, resizing once if the value is larger than the initial guess.
	#[wrapper]
	fn read_relay_chain_state(key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
		let mut buf = Vec::new();
		buf.resize(256, 0u8);
		let len = read_relay_chain_state_into__raw(key.as_ref(), &mut buf[..]);
		if len < 0 {
			return None;
		}
		let len = len as usize;
		if len > buf.len() {
			buf.resize(len, 0u8);
			read_relay_chain_state_into__raw(key.as_ref(), &mut buf[..]);
		}
		buf.truncate(len);
		Some(buf)
	}
}
