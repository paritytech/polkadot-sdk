// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Utilities for testing runtime code.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use bp_header_chain::justification::{required_justification_precommits, GrandpaJustification};
use bp_parachains::parachain_head_storage_key_at_source;
use bp_polkadot_core::parachains::{ParaHash, ParaHead, ParaHeadsProof, ParaId};
use bp_runtime::record_all_trie_keys;
use codec::Encode;
use sp_consensus_grandpa::{AuthorityId, AuthoritySignature, AuthorityWeight, SetId};
use sp_runtime::traits::{Header as HeaderT, One, Zero};
use sp_std::prelude::*;
use sp_trie::{trie_types::TrieDBMutBuilderV1, LayoutV1, MemoryDB, TrieMut};

// Re-export all our test account utilities
pub use keyring::*;

mod keyring;

/// GRANDPA round number used across tests.
pub const TEST_GRANDPA_ROUND: u64 = 1;
/// GRANDPA validators set id used across tests.
pub const TEST_GRANDPA_SET_ID: SetId = 1;
/// Name of the `Paras` pallet used across tests.
pub const PARAS_PALLET_NAME: &str = "Paras";

/// Configuration parameters when generating test GRANDPA justifications.
#[derive(Clone)]
pub struct JustificationGeneratorParams<H> {
	/// The header which we want to finalize.
	pub header: H,
	/// The GRANDPA round number for the current authority set.
	pub round: u64,
	/// The current authority set ID.
	pub set_id: SetId,
	/// The current GRANDPA authority set.
	///
	/// The size of the set will determine the number of pre-commits in our justification.
	pub authorities: Vec<(Account, AuthorityWeight)>,
	/// The total number of precommit ancestors in the `votes_ancestries` field our justification.
	///
	/// These may be distributed among many forks.
	pub ancestors: u32,
	/// The number of forks.
	///
	/// Useful for creating a "worst-case" scenario in which each authority is on its own fork.
	pub forks: u32,
}

impl<H: HeaderT> Default for JustificationGeneratorParams<H> {
	fn default() -> Self {
		let required_signatures = required_justification_precommits(test_keyring().len() as _);
		Self {
			header: test_header(One::one()),
			round: TEST_GRANDPA_ROUND,
			set_id: TEST_GRANDPA_SET_ID,
			authorities: test_keyring().into_iter().take(required_signatures as _).collect(),
			ancestors: 2,
			forks: 1,
		}
	}
}

/// Make a valid GRANDPA justification with sensible defaults
pub fn make_default_justification<H: HeaderT>(header: &H) -> GrandpaJustification<H> {
	let params = JustificationGeneratorParams::<H> { header: header.clone(), ..Default::default() };

	make_justification_for_header(params)
}

/// Generate justifications in a way where we are able to tune the number of pre-commits
/// and vote ancestries which are included in the justification.
///
/// This is useful for benchmarkings where we want to generate valid justifications with
/// a specific number of pre-commits (tuned with the number of "authorities") and/or a specific
/// number of vote ancestries (tuned with the "votes" parameter).
///
/// Note: This needs at least three authorities or else the verifier will complain about
/// being given an invalid commit.
pub fn make_justification_for_header<H: HeaderT>(
	params: JustificationGeneratorParams<H>,
) -> GrandpaJustification<H> {
	let JustificationGeneratorParams { header, round, set_id, authorities, mut ancestors, forks } =
		params;
	let (target_hash, target_number) = (header.hash(), *header.number());
	let mut votes_ancestries = vec![];
	let mut precommits = vec![];

	assert!(forks != 0, "Need at least one fork to have a chain..");
	assert!(
		forks as usize <= authorities.len(),
		"If we have more forks than authorities we can't create valid pre-commits for all the forks."
	);

	// Roughly, how many vote ancestries do we want per fork
	let target_depth = (ancestors + forks - 1) / forks;

	let mut unsigned_precommits = vec![];
	for i in 0..forks {
		let depth = if ancestors >= target_depth {
			ancestors -= target_depth;
			target_depth
		} else {
			ancestors
		};

		// Note: Adding 1 to account for the target header
		let chain = generate_chain(i, depth + 1, &header);

		// We don't include our finality target header in the vote ancestries
		for child in &chain[1..] {
			votes_ancestries.push(child.clone());
		}

		// The header we need to use when pre-commiting is the one at the highest height
		// on our chain.
		let precommit_candidate = chain.last().map(|h| (h.hash(), *h.number())).unwrap();
		unsigned_precommits.push(precommit_candidate);
	}

	for (i, (id, _weight)) in authorities.iter().enumerate() {
		// Assign authorities to sign pre-commits in a round-robin fashion
		let target = unsigned_precommits[i % forks as usize];
		let precommit = signed_precommit::<H>(id, target, round, set_id);

		precommits.push(precommit);
	}

	GrandpaJustification {
		round,
		commit: finality_grandpa::Commit { target_hash, target_number, precommits },
		votes_ancestries,
	}
}

fn generate_chain<H: HeaderT>(fork_id: u32, depth: u32, ancestor: &H) -> Vec<H> {
	let mut headers = vec![ancestor.clone()];

	for i in 1..depth {
		let parent = &headers[(i - 1) as usize];
		let (hash, num) = (parent.hash(), *parent.number());

		let mut header = test_header::<H>(num + One::one());
		header.set_parent_hash(hash);

		// Modifying the digest so headers at the same height but in different forks have different
		// hashes
		header.digest_mut().logs.push(sp_runtime::DigestItem::Other(fork_id.encode()));

		headers.push(header);
	}

	headers
}

/// Make valid proof for parachain `heads`
pub fn prepare_parachain_heads_proof<H: HeaderT>(
	heads: Vec<(u32, ParaHead)>,
) -> (H::Hash, ParaHeadsProof, Vec<(ParaId, ParaHash)>) {
	let mut parachains = Vec::with_capacity(heads.len());
	let mut root = Default::default();
	let mut mdb = MemoryDB::default();
	{
		let mut trie = TrieDBMutBuilderV1::<H::Hashing>::new(&mut mdb, &mut root).build();
		for (parachain, head) in heads {
			let storage_key =
				parachain_head_storage_key_at_source(PARAS_PALLET_NAME, ParaId(parachain));
			trie.insert(&storage_key.0, &head.encode())
				.map_err(|_| "TrieMut::insert has failed")
				.expect("TrieMut::insert should not fail in tests");
			parachains.push((ParaId(parachain), head.hash()));
		}
	}

	// generate storage proof to be delivered to This chain
	let storage_proof = record_all_trie_keys::<LayoutV1<H::Hashing>, _>(&mdb, &root)
		.map_err(|_| "record_all_trie_keys has failed")
		.expect("record_all_trie_keys should not fail in benchmarks");

	(root, ParaHeadsProof { storage_proof }, parachains)
}

/// Create signed precommit with given target.
pub fn signed_precommit<H: HeaderT>(
	signer: &Account,
	target: (H::Hash, H::Number),
	round: u64,
	set_id: SetId,
) -> finality_grandpa::SignedPrecommit<H::Hash, H::Number, AuthoritySignature, AuthorityId> {
	let precommit = finality_grandpa::Precommit { target_hash: target.0, target_number: target.1 };

	let encoded = sp_consensus_grandpa::localized_payload(
		round,
		set_id,
		&finality_grandpa::Message::Precommit(precommit.clone()),
	);

	let signature = signer.sign(&encoded);
	let raw_signature: Vec<u8> = signature.to_bytes().into();

	// Need to wrap our signature and id types that they match what our `SignedPrecommit` is
	// expecting
	let signature = AuthoritySignature::try_from(raw_signature).expect(
		"We know our Keypair is good,
		so our signature must also be good.",
	);
	let id = (*signer).into();

	finality_grandpa::SignedPrecommit { precommit, signature, id }
}

/// Get a header for testing.
///
/// The correct parent hash will be used if given a non-zero header.
pub fn test_header<H: HeaderT>(number: H::Number) -> H {
	let default = |num| {
		H::new(num, Default::default(), Default::default(), Default::default(), Default::default())
	};

	let mut header = default(number);
	if number != Zero::zero() {
		let parent_hash = default(number - One::one()).hash();
		header.set_parent_hash(parent_hash);
	}

	header
}

/// Get a header for testing with given `state_root`.
///
/// The correct parent hash will be used if given a non-zero header.
pub fn test_header_with_root<H: HeaderT>(number: H::Number, state_root: H::Hash) -> H {
	let mut header: H = test_header(number);
	header.set_state_root(state_root);
	header
}

/// Convenience function for generating a Header ID at a given block number.
pub fn header_id<H: HeaderT>(index: u8) -> (H::Hash, H::Number) {
	(test_header::<H>(index.into()).hash(), index.into())
}

#[macro_export]
/// Adds methods for testing the `set_owner()` and `set_operating_mode()` for a pallet.
/// Some values are hardcoded like:
/// - `run_test()`
/// - `Pallet::<TestRuntime>`
/// - `PalletOwner::<TestRuntime>`
/// - `PalletOperatingMode::<TestRuntime>`
/// While this is not ideal, all the pallets use the same names, so it works for the moment.
/// We can revisit this in the future if anything changes.
macro_rules! generate_owned_bridge_module_tests {
	($normal_operating_mode: expr, $halted_operating_mode: expr) => {
		#[test]
		fn test_set_owner() {
			run_test(|| {
				PalletOwner::<TestRuntime>::put(1);

				// The root should be able to change the owner.
				assert_ok!(Pallet::<TestRuntime>::set_owner(RuntimeOrigin::root(), Some(2)));
				assert_eq!(PalletOwner::<TestRuntime>::get(), Some(2));

				// The owner should be able to change the owner.
				assert_ok!(Pallet::<TestRuntime>::set_owner(RuntimeOrigin::signed(2), Some(3)));
				assert_eq!(PalletOwner::<TestRuntime>::get(), Some(3));

				// Other users shouldn't be able to change the owner.
				assert_noop!(
					Pallet::<TestRuntime>::set_owner(RuntimeOrigin::signed(1), Some(4)),
					DispatchError::BadOrigin
				);
				assert_eq!(PalletOwner::<TestRuntime>::get(), Some(3));
			});
		}

		#[test]
		fn test_set_operating_mode() {
			run_test(|| {
				PalletOwner::<TestRuntime>::put(1);
				PalletOperatingMode::<TestRuntime>::put($normal_operating_mode);

				// The root should be able to halt the pallet.
				assert_ok!(Pallet::<TestRuntime>::set_operating_mode(
					RuntimeOrigin::root(),
					$halted_operating_mode
				));
				assert_eq!(PalletOperatingMode::<TestRuntime>::get(), $halted_operating_mode);
				// The root should be able to resume the pallet.
				assert_ok!(Pallet::<TestRuntime>::set_operating_mode(
					RuntimeOrigin::root(),
					$normal_operating_mode
				));
				assert_eq!(PalletOperatingMode::<TestRuntime>::get(), $normal_operating_mode);

				// The owner should be able to halt the pallet.
				assert_ok!(Pallet::<TestRuntime>::set_operating_mode(
					RuntimeOrigin::signed(1),
					$halted_operating_mode
				));
				assert_eq!(PalletOperatingMode::<TestRuntime>::get(), $halted_operating_mode);
				// The owner should be able to resume the pallet.
				assert_ok!(Pallet::<TestRuntime>::set_operating_mode(
					RuntimeOrigin::signed(1),
					$normal_operating_mode
				));
				assert_eq!(PalletOperatingMode::<TestRuntime>::get(), $normal_operating_mode);

				// Other users shouldn't be able to halt the pallet.
				assert_noop!(
					Pallet::<TestRuntime>::set_operating_mode(
						RuntimeOrigin::signed(2),
						$halted_operating_mode
					),
					DispatchError::BadOrigin
				);
				assert_eq!(PalletOperatingMode::<TestRuntime>::get(), $normal_operating_mode);
				// Other users shouldn't be able to resume the pallet.
				PalletOperatingMode::<TestRuntime>::put($halted_operating_mode);
				assert_noop!(
					Pallet::<TestRuntime>::set_operating_mode(
						RuntimeOrigin::signed(2),
						$normal_operating_mode
					),
					DispatchError::BadOrigin
				);
				assert_eq!(PalletOperatingMode::<TestRuntime>::get(), $halted_operating_mode);
			});
		}
	};
}
