// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Kademlia provider-key derivation for parachain bootnode discovery (RFC-0008), with an optional
//! capability tag.

use sc_network::KademliaKey;
use sp_consensus_babe::Randomness;

/// The Kademlia provider key a parachain advertises its bootnodes under:
/// `para_id ++ capability ++ randomness`, where `para_id` is the SCALE-compact-encoded parachain
/// id and `randomness` is the relay epoch randomness.
///
/// An **empty** `capability` reproduces the plain RFC-0008 key (`para_id ++ randomness`)
/// byte-for-byte, so the default discovery/advertisement path is unchanged. A **non-empty**
/// capability (e.g. `b"spec-msg/v1"`) yields a distinct, capability-scoped key: only nodes
/// advertising that capability publish there, so a capability-aware discoverer resolves *only*
/// them. This sidesteps the closest-K dilution where a serving subset is lost among all of a
/// parachain's collators under the single plain key.
pub(crate) fn provider_key(
	para_id_scale_compact: &[u8],
	capability: &[u8],
	randomness: &Randomness,
) -> KademliaKey {
	para_id_scale_compact
		.iter()
		.copied()
		.chain(capability.iter().copied())
		.chain(randomness.iter().copied())
		.collect::<Vec<_>>()
		.into()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn bytes(key: KademliaKey) -> Vec<u8> {
		key.as_ref().to_vec()
	}

	// SCALE-compact-encoded `ParaId(1)` stand-in; only the bytes matter here.
	const PARA_ID: &[u8] = &[1, 0, 0, 0];
	const RANDOMNESS: Randomness = [7u8; 32];

	#[test]
	fn empty_capability_is_the_plain_rfc0008_key() {
		let plain: Vec<u8> = PARA_ID.iter().copied().chain(RANDOMNESS.iter().copied()).collect();
		assert_eq!(bytes(provider_key(PARA_ID, &[], &RANDOMNESS)), plain);
	}

	#[test]
	fn capability_scopes_the_key_and_differs_from_plain() {
		let cap = b"spec-msg/v1";
		let expected: Vec<u8> = PARA_ID
			.iter()
			.copied()
			.chain(cap.iter().copied())
			.chain(RANDOMNESS.iter().copied())
			.collect();
		let scoped = provider_key(PARA_ID, cap, &RANDOMNESS);
		assert_eq!(bytes(scoped.clone()), expected);
		assert_ne!(bytes(scoped), bytes(provider_key(PARA_ID, &[], &RANDOMNESS)));
	}

	#[test]
	fn different_capabilities_yield_different_keys() {
		assert_ne!(
			bytes(provider_key(PARA_ID, b"spec-msg/v1", &RANDOMNESS)),
			bytes(provider_key(PARA_ID, b"other/v1", &RANDOMNESS)),
		);
	}

	#[test]
	fn different_para_ids_yield_different_keys() {
		let cap = b"spec-msg/v1";
		assert_ne!(
			bytes(provider_key(&[1, 0, 0, 0], cap, &RANDOMNESS)),
			bytes(provider_key(&[2, 0, 0, 0], cap, &RANDOMNESS)),
		);
	}
}
