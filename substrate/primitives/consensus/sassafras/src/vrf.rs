// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Utilities related to VRF input, pre-output and signatures.

use crate::{Randomness, TicketBody, TicketId};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use codec::Encode;
use sp_consensus_slots::Slot;

pub use sp_core::bandersnatch::{
	ring_vrf::{RingProver, RingVerifier, RingVerifierKey, RingVrfSignature},
	vrf::{VrfInput, VrfPreOutput, VrfSignData, VrfSignature},
};

/// Ring size (aka authorities count) for Sassafras consensus.
pub const RING_SIZE: usize = 1024;

/// Bandersnatch VRF [`RingContext`] specialization for Sassafras using [`RING_SIZE`].
pub type RingContext = sp_core::bandersnatch::ring_vrf::RingContext<RING_SIZE>;

/// Input for slot claim
pub fn slot_claim_input(randomness: &Randomness, slot: Slot, epoch: u64) -> VrfInput {
	let v = [b"sassafras-ticket", randomness.as_slice(), &slot.to_le_bytes(), &epoch.to_le_bytes()]
		.concat();
	VrfInput::new(&v[..])
}

/// Signing-data to claim slot ownership during block production.
pub fn slot_claim_sign_data(randomness: &Randomness, slot: Slot, epoch: u64) -> VrfSignData {
	let v = [b"sassafras-ticket", randomness.as_slice(), &slot.to_le_bytes(), &epoch.to_le_bytes()]
		.concat();
	VrfSignData::new(&v[..], &[])
}

/// VRF input to generate the ticket id.
pub fn ticket_id_input(randomness: &Randomness, attempt: u32, epoch: u64) -> VrfInput {
	let v =
		[b"sassafras-ticket", randomness.as_slice(), &attempt.to_le_bytes(), &epoch.to_le_bytes()]
			.concat();
	VrfInput::new(&v[..])
}

/// Data to be signed via ring-vrf.
pub fn ticket_body_sign_data(ticket_body: &TicketBody, ticket_id_input: VrfInput) -> VrfSignData {
	VrfSignData { vrf_input: ticket_id_input, aux_data: ticket_body.encode() }
}

/// Make ticket-id from the given VRF pre-output.
///
/// Pre-output should have been obtained from the input directly using the vrf
/// secret key or from the vrf signature pre-output.
pub fn make_ticket_id(preout: &VrfPreOutput) -> TicketId {
	let bytes: [u8; 16] = preout.make_bytes()[..16].try_into().unwrap();
	u128::from_le_bytes(bytes)
}
