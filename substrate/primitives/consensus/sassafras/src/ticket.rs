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

//! Primitives related to tickets.

use crate::vrf::RingVrfSignature;
use scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

pub use sp_core::ed25519::{Public as EphemeralPublic, Signature as EphemeralSignature};

/// Ticket identifier.
///
/// Its value is the output of a VRF whose inputs cannot be controlled by the
/// ticket's creator (refer to [`crate::vrf::ticket_id_input`] parameters).
/// Because of this, it is also used as the ticket score to compare against
/// the epoch ticket's threshold to decide if the ticket is worth being considered
/// for slot assignment (refer to [`ticket_id_threshold`]).
pub type TicketId = u128;

/// Ticket data persisted on-chain.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct TicketBody {
	/// Attempt index.
	pub attempt_idx: u32,
	/// Ephemeral public key which gets erased when the ticket is claimed.
	pub erased_public: EphemeralPublic,
	/// Ephemeral public key which gets exposed when the ticket is claimed.
	pub revealed_public: EphemeralPublic,
}

/// Ticket ring vrf signature.
pub type TicketSignature = RingVrfSignature;

/// Ticket envelope used on during submission.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct TicketEnvelope {
	/// Ticket body.
	pub body: TicketBody,
	/// Ring signature.
	pub signature: TicketSignature,
}

/// Ticket claim information filled by the block author.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct TicketClaim {
	/// Signature verified via `TicketBody::erased_public`.
	pub erased_signature: EphemeralSignature,
}

/// Computes a boundary for [`TicketId`] maximum allowed value for a given epoch.
///
/// Only ticket identifiers below this threshold should be considered as candidates
/// for slot assignment.
///
/// The value is computed as `TicketId::MAX*(redundancy*slots)/(attempts*validators)`
///
/// Where:
/// - `redundancy`: redundancy factor;
/// - `slots`: number of slots in epoch;
/// - `attempts`: max number of tickets attempts per validator;
/// - `validators`: number of validators in epoch.
///
/// If `attempts * validators = 0` then we return 0.
///
/// For details about the formula and implications refer to
/// [*probabilities an parameters*](https://research.web3.foundation/Polkadot/protocols/block-production/SASSAFRAS#probabilities-and-parameters)
/// paragraph of the w3f introduction to the protocol.
// TODO: replace with [RFC-26](https://github.com/polkadot-fellows/RFCs/pull/26)
// "Tickets Threshold" paragraph once is merged
pub fn ticket_id_threshold(
	redundancy: u32,
	slots: u32,
	attempts: u32,
	validators: u32,
) -> TicketId {
	let num = redundancy as u64 * slots as u64;
	let den = attempts as u64 * validators as u64;
	TicketId::max_value()
		.checked_div(den.into())
		.unwrap_or_default()
		.saturating_mul(num.into())
}

#[cfg(test)]
mod tests {
	use super::*;

	// This is a trivial example/check which just better explain explains the rationale
	// behind the threshold.
	//
	// After this reading the formula should become obvious.
	#[test]
	fn ticket_id_threshold_trivial_check() {
		// For an epoch with `s` slots we want to accept a number of tickets equal to ~s·r
		let redundancy = 2;
		let slots = 1000;
		let attempts = 100;
		let validators = 500;

		let threshold = ticket_id_threshold(redundancy, slots, attempts, validators);
		let threshold = threshold as f64 / TicketId::MAX as f64;

		// We expect that the total number of tickets allowed to be submited
		// is slots*redundancy
		let avt = ((attempts * validators) as f64 * threshold) as u32;
		assert_eq!(avt, slots * redundancy);

		println!("threshold: {}", threshold);
		println!("avt = {}", avt);
	}
}
