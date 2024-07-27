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

use crate::{Randomness, TicketId};
use sp_consensus_slots::Slot;

pub use sp_core::bandersnatch::{
	ring_vrf::{RingProver, RingVerifier, RingVerifierKey, RingVrfSignature},
	vrf::{VrfInput, VrfPreOutput, VrfSignData, VrfSignature},
};

/// Ring VRF domain size for Sassafras consensus.
pub const RING_VRF_DOMAIN_SIZE: u32 = 2048;

const TICKET_SEAL_CONTEXT: &[u8] = b"sassafras_ticket_seal";
// const FALLBACK_SEAL_CONTEXT: &[u8] = b"sassafras_fallback_seal";
const BLOCK_ENTROPY_CONTEXT: &[u8] = b"sassafras_entropy";

/// Bandersnatch VRF [`RingContext`] specialization for Sassafras using [`RING_VRF_DOMAIN_SIZE`].
pub type RingContext = sp_core::bandersnatch::ring_vrf::RingContext<RING_VRF_DOMAIN_SIZE>;

/// VRF input to generate the ticket id.
pub fn ticket_id_input(randomness: &Randomness, attempt: u8) -> VrfInput {
	VrfInput::new(b"sassafras", [TICKET_SEAL_CONTEXT, randomness.as_slice(), &[attempt]].concat())
}

/// Data to be signed via ring-vrf.
pub fn ticket_id_sign_data(ticket_id_input: VrfInput, extra_data: &[u8]) -> VrfSignData {
	VrfSignData::new_unchecked(
		b"sassafras-ticket-body-transcript",
		Some(extra_data),
		Some(ticket_id_input),
	)
}

/// VRF input to produce randomness.
pub fn block_randomness_input(randomness: &Randomness, slot: Slot) -> VrfInput {
	// TODO: @davxy: implement as JAM
	VrfInput::new(
		b"sassafras",
		[BLOCK_ENTROPY_CONTEXT, randomness.as_slice(), &slot.to_le_bytes()].concat(),
	)
}

/// Signing-data to claim slot ownership during block production.
pub fn block_randomness_sign_data(randomness: &Randomness, slot: Slot) -> VrfSignData {
	let input = block_randomness_input(randomness, slot);
	VrfSignData::new_unchecked(
		b"sassafras-randomness-transcript",
		Option::<&[u8]>::None,
		Some(input),
	)
}

/// Make ticket-id from the given VRF input and pre-output.
///
/// Input should have been obtained via [`ticket_id_input`].
/// Pre-output should have been obtained from the input directly using the vrf
/// secret key or from the vrf signature pre-outputs.
pub fn make_ticket_id(input: &VrfInput, pre_output: &VrfPreOutput) -> TicketId {
	TicketId(pre_output.make_bytes::<32>(b"ticket-id", input))
}
