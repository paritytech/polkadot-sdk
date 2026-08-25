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

//! The JAM (riscv/PolkaVM) implementation of validate block.
//!
//! On JAM the PVF is invoked with *no arguments* and reads its inputs / writes its outputs
//! through the child-PVM host functions of the Parachain Service (spec §4.2). The validation
//! core is shared with the polkadot implementation via
//! [`super::polkadot_implementation::validate_block`]; this module is responsible for the
//! host-call surface that feeds it and the host-call side effects that sink its outputs.
//!
//! Per spec §4.2: "The PVF reads its inputs (PoV, context, downward transfers) through host
//! functions and writes its outputs (head data, code upgrades, transfers) through host
//! functions. It does not return a value directly — the `ParachainWorkDigest` is assembled by
//! the Parachain Service's Refine wrapper from the accumulated host-function side effects."
//!
//! Host call indices below mirror the Parachain Service's `HostCall` enum (indices 0-28 in
//! `parachain-service/service-interface/src/host_call.rs`) and the frameless runtime's
//! verified `#[polkavm_import(index = N)]` block (spec §4.3 table).

use super::MemoryOptimizedValidationParams;
use codec::Decode;
use frame_support::traits::{ExecuteBlock, IsSubType};
use sp_crypto_hashing::blake2_256;
use sp_runtime::traits::{Block as BlockT, ExtrinsicCall};

/// Bounded, opaque error payload the caller leaves on the failure report path when the PVF
/// aborts abnormally (spec §4.2 / `report_error`, index 24). Kept a static slice so no
/// unbounded allocation happens on the abort path.
const ERR_PAYLOAD_NO_WORK_ITEM: &[u8] = b"jam_validate_block:no-work-item-payload@0";
const ERR_PAYLOAD_DECODE_FAILED: &[u8] = b"jam_validate_block:params-decode-failed";

/// Leave a bounded error payload on the failure report and stop the PVF.
fn report_error(data: &[u8]) {
	// `report_error` is a terminal side effect; the service unwinds the PVM.
	host::report_error(data);
}

/// The single entry point the Parachain Service's Refine (spawned child PVM) calls (spec §4.2).
///
/// SAME validation as the polkadot path — [`super::polkadot_implementation::validate_block`] —
/// instantiated with the same concrete `B`/`E`/`PSC` by the runtime layer, but with the JAM
/// setup: the candidate (PoV/params) is read from the child-PVM `work_item_payload` host
/// function and the `ValidationResult` outputs are written via host side effects instead of
/// returned.
#[allow(clippy::unused_unit)]
pub fn jam_validate_block<B: BlockT, E: ExecuteBlock<B>, PSC: crate::Config>() where
	B::Extrinsic: ExtrinsicCall,
	<B::Extrinsic as ExtrinsicCall>::Call: IsSubType<crate::Call<PSC>>,
{
	// 1. Read the work-item payload. The Refine invokes the child PVM with a single work item
	// (index 0); its payload *is* the SCALE-encoded `MemoryOptimizedValidationParams`.
	let payload = match host::work_item_payload(0) {
		Some(payload) => payload,
		None => {
			report_error(ERR_PAYLOAD_NO_WORK_ITEM);
			panic!("missing work-item payload for the child PVM; qed");
		},
	};

	// 2. Decode the same params `validate_block` consumes.
	let params = match MemoryOptimizedValidationParams::decode(&mut &payload[..]) {
		Ok(params) => params,
		Err(_) => {
			report_error(ERR_PAYLOAD_DECODE_FAILED);
			panic!("invalid validation params supplied by the work-item payload; qed");
		},
	};

	// 3. Declare the parent head hash this candidate is built on, exactly once (mandatory).
	host::set_parent_head_hash(&blake2_256(&params.parent_head));

	// 4. Run the SAME validation core as the polkadot path, returning the same
	// `ValidationResult`.
	let result = super::polkadot_implementation::validate_block::<B, E, PSC>(params);

	// 5. Sink the result through host side effects (spec §4.2).
	host::set_head(&result.head_data.0);
	if let Some(code) = &result.new_validation_code {
		let hash = blake2_256(&code.0);
		host::request_code_upgrade(&hash, code.0.len() as u32);
	}
}

/// Child host calls of the Parachain Service's Refine, per the ABI in `parachain-service`'s
/// PVF executor (indices from `parachain-service-interface`'s `HostCall`, copied verbatim from
/// the frameless runtime's verified import block — spec §4.3 table).
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod host {
	use alloc::{vec, vec::Vec};

	#[polkavm_derive::polkavm_import]
	extern "C" {
		// --- Data access (spec §4.3) ---
		#[polkavm_import(index = 2)]
		fn gas_raw() -> u64;
		#[polkavm_import(index = 9)]
		fn work_item_payload_raw(index: u32, out_ptr: u32, out_cap: u32) -> u64;

		// --- Side effects (spec §4.3) ---
		#[polkavm_import(index = 12)]
		fn set_parent_head_hash_raw(hash_ptr: u32);
		#[polkavm_import(index = 13)]
		fn set_head_raw(ptr: u32, len: u32);
		#[polkavm_import(index = 14)]
		fn request_code_upgrade_raw(hash_ptr: u32, len: u32);
		#[polkavm_import(index = 24)]
		fn report_error_raw(ptr: u32, len: u32);
	}

	/// Declare the parent head hash this candidate was built on (called once).
	pub fn set_parent_head_hash(hash: &[u8; 32]) {
		unsafe { set_parent_head_hash_raw(hash.as_ptr() as u32) }
	}

	/// Declare the new head data this parachain block produced.
	pub fn set_head(head: &[u8]) {
		unsafe { set_head_raw(head.as_ptr() as u32, head.len() as u32) }
	}

	/// Signal a PVF code upgrade request (`hash` + encoded-code length).
	pub fn request_code_upgrade(hash: &[u8; 32], len: u32) {
		unsafe { request_code_upgrade_raw(hash.as_ptr() as u32, len) }
	}

	/// Abort the PVF with an opaque error payload (host-side; execution never resumes).
	pub fn report_error(data: &[u8]) {
		unsafe { report_error_raw(data.as_ptr() as u32, data.len() as u32) }
	}

	/// Fetch work-item payload at `index`; `None` if absent.
	///
	/// Buffer protocol per the executor: probe with zero capacity to get the byte length, then
	/// retry with a large enough buffer.
	pub fn work_item_payload(index: u32) -> Option<Vec<u8>> {
		let len = unsafe { work_item_payload_raw(index, 0, 0) };
		if len == u64::MAX {
			return None;
		}
		let mut buf = vec![0u8; len as usize];
		loop {
			let actual =
				unsafe { work_item_payload_raw(index, buf.as_ptr() as u32, buf.len() as u32) };
			if actual == u64::MAX {
				return None;
			}
			let actual = actual as usize;
			if actual <= buf.len() {
				buf.truncate(actual);
				return Some(buf);
			}
			buf.resize(actual, 0);
		}
	}
}
