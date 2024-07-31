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
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{bounded::BoundedVec, ConstU32};

pub use sp_core::ed25519::{Public as EphemeralPublic, Signature as EphemeralSignature};
use sp_core::U256;

const TICKET_EXTRA_MAX_LEN: u32 = 128;

/// Ticket identifier.
///
/// Its value is the output of a VRF whose inputs cannot be controlled by the
/// ticket's creator (refer to [`crate::vrf::ticket_id_input`] parameters).
/// Because of this, it is also used as the ticket score to compare against
/// the epoch ticket's threshold to decide if the ticket is worth being considered
/// for slot assignment (refer to [`ticket_id_threshold`]).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct TicketId(pub [u8; 32]);

impl core::fmt::Debug for TicketId {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "{}", sp_core::hexdisplay::HexDisplay::from(&self.0))
	}
}

impl From<U256> for TicketId {
	fn from(value: U256) -> Self {
		let mut inner = [0; 32];
		value.to_big_endian(&mut inner);
		Self(inner)
	}
}

impl From<TicketId> for U256 {
	fn from(ticket: TicketId) -> U256 {
		U256::from_big_endian(&ticket.0[..])
	}
}

/// Ticket data persisted on-chain.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct TicketBody {
	/// Ticket identifier.
	pub id: TicketId,
	/// Attempt index.
	pub attempt: u8,
	/// User opaque extra data.
	pub extra: BoundedVec<u8, ConstU32<TICKET_EXTRA_MAX_LEN>>,
}

impl Ord for TicketBody {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.id.cmp(&other.id)
	}
}

impl PartialOrd for TicketBody {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

/// Ticket ring vrf signature.
pub type TicketSignature = RingVrfSignature;

/// Ticket envelope used during submission.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct TicketEnvelope {
	/// Attempt index.
	pub attempt: u8,
	/// User opaque extra data.
	pub extra: BoundedVec<u8, ConstU32<TICKET_EXTRA_MAX_LEN>>,
	/// Ring signature.
	pub signature: TicketSignature,
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
pub fn ticket_id_threshold(slots: u32, validators: u32, attempts: u8, redundancy: u8) -> TicketId {
	let den = attempts as u64 * validators as u64;
	let num = redundancy as u64 * slots as u64;
	U256::MAX
		.checked_div(den.into())
		.unwrap_or_default()
		.saturating_mul(num.into())
		.into()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn normalize_u256(bytes: [u8; 32]) -> f64 {
		let max_u128 = u128::MAX as f64;
		let base = max_u128 + 1.0;
		let max = max_u128 * (base + 1.0);

		// Extract two u128 segments from the byte array
		let h = u128::from_be_bytes(bytes[..16].try_into().unwrap()) as f64;
		let l = u128::from_be_bytes(bytes[16..].try_into().unwrap()) as f64;
		(h * base + l) / max
	}

	// This is a trivial example/check which just better explain explains the rationale
	// behind the threshold.
	//
	// After this reading the formula should become obvious.
	#[test]
	fn ticket_id_threshold_trivial_check() {
		// For an epoch with `s` slots, with a redundancy factor `r`, we want to accept
		// a number of tickets equal to ~s·r.
		let redundancy = 2;
		let slots = 1000;
		let attempts = 100;
		let validators = 500;

		let threshold = ticket_id_threshold(slots, validators, attempts, redundancy);
		println!("{:?}", threshold);
		let threshold = normalize_u256(threshold.0);
		println!("{}", threshold);

		// We expect that the total number of tickets allowed to be submitted is slots*redundancy
		let avt = ((attempts as u32 * validators) as f64 * threshold) as u32;
		assert_eq!(avt, slots * redundancy as u32);

		println!("threshold: {}", threshold);
		println!("avt = {}", avt);
	}
}
