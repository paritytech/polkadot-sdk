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

//! The para's AURA authorizer: the collator set a core is dedicated to, the config that core's
//! authorizer queue commits to, and the token this collator signs every work package with.
//!
//! The set is *named*, never supplied: `--jam-collators alice,bob` derives the same `//Name`
//! ed25519 keys `parasim-tool` derives when it installs the queue, so nothing here needs key
//! material beyond this node's own signing key in the keystore. Everything the scheme leaves
//! open — leaf hashing, the round-robin arithmetic, the signing payload — comes out of
//! `parachain-authorizer`, the very crate the guest is built from: an authorizer hash commits to
//! the whole config, so a config this node cannot reproduce byte for byte is a core it cannot
//! use.

use super::LOG_TARGET;
use codec::Encode;
use jam_cumulus_facade::{
	H256,
	aura::{AuthConfig, AuthToken, expected_collator_index, signable_work_package_hash},
	authorizer::{AuthConfigBlob, Authorizer, AuthorizerHash, authorizer_hash},
};
use jam_interface::{ServiceId, Slot as JamSlot};
use jam_types::{Authorization, CodeHash, WorkPackage};
use parachain_authorizer::aura::{Command, build_collator_tree};
use sp_core::{Pair as _, crypto::KeyTypeId, ed25519};
use sp_keystore::{Keystore, KeystorePtr};
use std::path::Path;

/// Key type of the collator's work-package signing key.
///
/// Deliberately not `aura`: the Aura key is sr25519 and, under `--alice`, lives only in memory,
/// while this one is ed25519 (what the guest's `verify_strict` takes) and has to survive on disk.
/// The harness provisions it with `key insert --scheme ed25519 --key-type coll --suri //<Name>`.
pub(crate) const COLLATOR_KEY_TYPE: KeyTypeId = KeyTypeId(*b"coll");

/// The AURA authorizer of one para, together with this collator's place in its set.
pub(crate) struct AuraAuthorizer {
	authorizer: Authorizer,
	hash: AuthorizerHash,
	config: AuthConfig,
	/// This collator's leaf index in the set, which is what the round-robin has to name before a
	/// signature of ours is worth anything.
	own_index: u32,
	own_key: ed25519::Public,
	/// The collator's own branch of the set trie, rebuilt once at startup because the set never
	/// changes while the node runs.
	own_proof: Vec<H256>,
	keystore: KeystorePtr,
}

impl AuraAuthorizer {
	/// Derive the para's authorizer from the dev names of its collator set, and find this node in
	/// it.
	///
	/// Fails loudly rather than starting a collator that could never author: a set this node is
	/// not in, an unreadable blob, or a degenerate config the guest itself rejects.
	pub(crate) fn new(
		names: &str,
		blob_path: &Path,
		para_id: u32,
		parachain_service: ServiceId,
		slot_duration: u32,
		keystore: KeystorePtr,
	) -> Result<Self, String> {
		if slot_duration == 0 {
			return Err("`--jam-slot-duration` must be at least one JAM timeslot".into());
		}

		let blob = std::fs::read(blob_path).map_err(|error| {
			format!("cannot read `--jam-authorizer-blob` {blob_path:?}: {error}")
		})?;
		let code_hash = CodeHash::from(jam_std_common::hash_raw(&blob));

		let collators = dev_keys(names)?;
		let (collator_set_root, proofs) =
			build_collator_tree(&collators.iter().map(|key| key.0).collect::<Vec<_>>());
		let config = AuthConfig {
			para_ids: vec![para_id.into()],
			parachain_service,
			collator_set_root,
			collator_set_size: collators.len() as u32,
			slot_duration,
		};
		let authorizer = Authorizer { code_hash, config: AuthConfigBlob(config.encode()) };
		let hash = authorizer_hash(&authorizer);

		let ours = keystore.ed25519_public_keys(COLLATOR_KEY_TYPE);
		let own_index = collators
			.iter()
			.position(|collator| ours.contains(collator))
			.ok_or_else(|| own_key_missing(names, &collators, &ours))?;

		tracing::info!(
			target: LOG_TARGET,
			para_id,
			parachain_service,
			slot_duration,
			blob = ?blob_path,
			blob_len = blob.len(),
			?code_hash,
			collators = names,
			collator_set_size = collators.len(),
			?collator_set_root,
			config_len = authorizer.config.len(),
			authorizer_hash = ?hash,
			own_index,
			own_key = ?collators[own_index],
			proof_len = proofs[own_index].len(),
			"Derived the para's AURA authorizer; this is the hash a core's pool must hold for \
			 our packages to be authorized.",
		);

		Ok(Self {
			authorizer,
			hash,
			config,
			own_index: own_index as u32,
			own_key: collators[own_index],
			own_proof: proofs[own_index].clone(),
			keystore,
		})
	}

	/// The hash a core's authorizer pool has to hold for this para's packages to run on it.
	pub(crate) fn hash(&self) -> AuthorizerHash {
		self.hash
	}

	/// The authorizer a work package of this para carries.
	pub(crate) fn authorizer(&self) -> Authorizer {
		self.authorizer.clone()
	}

	pub(crate) fn own_index(&self) -> u32 {
		self.own_index
	}

	pub(crate) fn collator_set_size(&self) -> u32 {
		self.config.collator_set_size
	}

	/// The collator the round-robin names for `slot` — what the guest computes from the package's
	/// *lookup* anchor slot, and therefore what the lookup-anchor policy has to satisfy.
	pub(crate) fn collator_for(&self, slot: JamSlot) -> u32 {
		expected_collator_index(slot, &self.config)
	}

	/// Whether `slot` names this collator.
	pub(crate) fn names_us(&self, slot: JamSlot) -> bool {
		self.collator_for(slot) == self.own_index
	}

	/// How far back a search for a slot naming this collator has to look: one full turn of the
	/// round-robin, past which the arithmetic only repeats itself.
	pub(crate) fn round_robin_window(&self) -> u32 {
		self.config.collator_set_size.saturating_mul(self.config.slot_duration)
	}

	/// Sign `package` as this collator and put the token in its authorization.
	///
	/// The signed hash excludes the authorization by construction, which is exactly what lets the
	/// signature live inside the package it signs — so this runs *after* the package is otherwise
	/// complete, and re-anchoring re-signs. The command slot is always `None` here (only
	/// `parasim-tool` ever sends one) but is still bound into the payload, because the guest binds
	/// it too.
	pub(crate) fn authorize(&self, package: &mut WorkPackage) -> Result<(), String> {
		let wp_hash = signable_work_package_hash(package);
		let command: Option<Command> = None;
		let payload = AuthToken::signing_payload(wp_hash, &command);
		let signature = self
			.keystore
			.ed25519_sign(COLLATOR_KEY_TYPE, &self.own_key, payload.as_bytes())
			.map_err(|error| format!("the `coll` keystore refused to sign: {error}"))?
			.ok_or_else(|| {
				format!(
					"the keystore no longer holds the `coll` key {:?} it was started with",
					self.own_key
				)
			})?;
		let token = AuthToken {
			proof: self.own_proof.clone(),
			key: self.own_key.0,
			signature: signature.0,
			control_command: command,
		};
		let authorization = Authorization(token.encode());

		tracing::debug!(
			target: LOG_TARGET,
			?wp_hash,
			signing_payload = ?payload,
			lookup_anchor_slot = package.context.lookup_anchor_slot,
			expected_collator = self.collator_for(package.context.lookup_anchor_slot),
			own_index = self.own_index,
			own_key = ?self.own_key,
			proof_len = self.own_proof.len(),
			token_len = authorization.len(),
			"Signed the work package as the collator its lookup anchor names.",
		);
		package.authorization = authorization;
		Ok(())
	}
}

/// What to tell an operator whose node holds no key of the set it was pointed at.
fn own_key_missing(
	names: &str,
	collators: &[ed25519::Public],
	ours: &[ed25519::Public],
) -> String {
	format!(
		"none of this node's `coll` keys is in the collator set `{names}`. The set is {collators:?} \
		 and the keystore holds {ours:?}; provision the signing key with `key insert --scheme \
		 ed25519 --key-type coll --suri //<Name>`",
	)
}

/// The collator set behind a comma-separated list of dev names, in round-robin order.
fn dev_keys(names: &str) -> Result<Vec<ed25519::Public>, String> {
	let keys = names
		.split(',')
		.map(|name| dev_key(name.trim()))
		.collect::<Result<Vec<_>, _>>()?;
	if keys.is_empty() {
		return Err("`--jam-collators` must name at least one collator".into());
	}
	Ok(keys)
}

/// The ed25519 key behind a dev name, derived exactly as `key insert --suri //Name` would.
///
/// The derivation has to match `parasim-tool`'s to the byte: it is the tool that puts the set root
/// on chain, and a root this node cannot reproduce is a proof no guarantor will accept.
fn dev_key(name: &str) -> Result<ed25519::Public, String> {
	if name.is_empty() {
		return Err("`--jam-collators` contains an empty collator name".into());
	}
	let mut capitalized = name.to_string();
	capitalized[..1].make_ascii_uppercase();
	ed25519::Pair::from_string(&format!("//{capitalized}"), None)
		.map(|pair| pair.public())
		.map_err(|error| format!("no dev key for collator {name:?}: {error:?}"))
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;
	use codec::DecodeAll;
	use jam_types::{RefineContext, WorkItem, WorkPayload};
	use parachain_authorizer::aura::AuthToken as GuestToken;
	use sp_keystore::testing::MemoryKeystore;
	use std::sync::Arc;

	pub(crate) const SERVICE_ID: ServiceId = 5;
	pub(crate) const PARA_ID: u32 = 1000;

	/// Any file will do: the blob reaches the config only through its hash, and nothing here
	/// asserts what that hash is.
	const SOME_BLOB: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");

	/// A keystore provisioned exactly as the harness provisions a collator's: one ed25519 `coll`
	/// key derived from the node's own dev name.
	pub(crate) fn keystore_of(name: &str) -> KeystorePtr {
		let keystore = MemoryKeystore::new();
		keystore
			.ed25519_generate_new(COLLATOR_KEY_TYPE, Some(&format!("//{name}")))
			.expect("the dev suri derives; qed");
		Arc::new(keystore)
	}

	pub(crate) fn authorizer_of(names: &str, own: &str, slot_duration: u32) -> AuraAuthorizer {
		AuraAuthorizer::new(
			names,
			Path::new(SOME_BLOB),
			PARA_ID,
			SERVICE_ID,
			slot_duration,
			keystore_of(own),
		)
		.expect("the collator set names the node; qed")
	}

	fn package(lookup_anchor_slot: JamSlot, authorizer: &AuraAuthorizer) -> WorkPackage {
		let item = WorkItem {
			service: SERVICE_ID,
			code_hash: CodeHash::from([9u8; 32]),
			payload: WorkPayload(vec![1, 2, 3]),
			refine_gas_limit: 1_000,
			accumulate_gas_limit: 1_000,
			import_segments: Default::default(),
			extrinsics: Default::default(),
			export_count: 0,
		};
		WorkPackage {
			authorization: Authorization::default(),
			auth_code_host: 0,
			authorizer: authorizer.authorizer(),
			context: RefineContext {
				anchor: [1u8; 32].into(),
				state_root: [2u8; 32].into(),
				beefy_root: [3u8; 32].into(),
				lookup_anchor: [4u8; 32].into(),
				lookup_anchor_slot,
				prerequisites: Default::default(),
			},
			items: vec![item].try_into().expect("a single work item always fits; qed"),
		}
	}

	/// The set root commits every collator's key and the node signs with a key it reads from a
	/// keystore `parasim-tool` never sees, so the two derivations have to agree. Pinned against
	/// substrate's published dev key rather than against ourselves, which is what makes this a
	/// contract and not a tautology.
	#[test]
	fn dev_names_are_substrates_dev_keys_works() {
		let alice = dev_key("alice").expect("//Alice derives; qed");
		assert_eq!(
			array_bytes::bytes2hex("", alice.0),
			"88dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0ee",
		);
		// Capitalisation is the operator's convenience, not a different key.
		assert_eq!(dev_key("Alice").expect("//Alice derives; qed"), alice);
		assert_ne!(dev_key("bob").expect("//Bob derives; qed"), alice);
	}

	/// The order of `--jam-collators` *is* the round-robin, so a node's own index is its position
	/// in that list and nothing else.
	#[test]
	fn own_index_is_the_position_in_the_named_set_works() {
		assert_eq!(authorizer_of("alice,bob,charlie", "Bob", 1).own_index(), 1);
		assert_eq!(authorizer_of("charlie,bob,alice", "Bob", 1).own_index(), 1);
		assert_eq!(authorizer_of("charlie,alice,bob", "Bob", 1).own_index(), 2);
	}

	/// A node whose keystore holds no key of the set could sign nothing a guarantor would accept,
	/// so it must refuse to start rather than author blocks that can never be authorized — and the
	/// error has to say how to fix it, because the fix is one `key insert` away.
	#[test]
	fn a_node_outside_the_collator_set_errors() {
		let error = AuraAuthorizer::new(
			"alice,bob",
			Path::new(SOME_BLOB),
			PARA_ID,
			SERVICE_ID,
			1,
			keystore_of("Dave"),
		)
		.map(drop)
		.expect_err("Dave is not in the set");
		assert!(error.contains("key insert"), "the error names the fix: {error}");
	}

	/// The round-robin is the guest's, read off the config the package itself carries: one
	/// collator per para slot, and a para slot is `slot_duration` JAM timeslots long.
	#[test]
	fn the_round_robin_walks_the_set_once_per_para_slot_works() {
		let alice = authorizer_of("alice,bob,charlie", "Alice", 2);
		assert_eq!(
			(0..8).map(|slot| alice.collator_for(slot)).collect::<Vec<_>>(),
			vec![0, 0, 1, 1, 2, 2, 0, 0],
		);
		assert!(alice.names_us(0) && alice.names_us(1) && alice.names_us(6));
		assert!(!alice.names_us(2));
		// One full turn is how far back a search for a naming slot ever has to look.
		assert_eq!(alice.round_robin_window(), 6);
	}

	/// A queue hash commits to the whole config, so two paras on the same collator set must land
	/// on different cores, and the same para must hash the same way on every node.
	#[test]
	fn each_para_gets_its_own_authorizer_works() {
		let one = authorizer_of("alice,bob", "Alice", 1);
		let same = authorizer_of("alice,bob", "Bob", 1);
		let smaller = authorizer_of("alice", "Alice", 1);
		assert_eq!(one.hash(), same.hash(), "the hash is the para's, not the signer's");
		assert_ne!(one.hash(), smaller.hash(), "a different set is a different core");
	}

	/// The drift alarm between this node and the guest: a token this node assembles must satisfy
	/// the very checks `is_authorized` runs on it — the proof against the set root, and the
	/// signature over the token-free package hash bound to the (absent) command. If either side's
	/// hashing, bit order or payload ever moves, every package this collator sends starts failing
	/// in-core with nothing but a guarantor's silence to show for it, so it has to fail here.
	#[test]
	fn a_signed_token_satisfies_the_guest_verifier_works() {
		let bob = authorizer_of("alice,bob,charlie", "Bob", 1);
		// Slot 4 names collator 4 % 3 == 1, which is Bob.
		let mut package = package(4, &bob);
		bob.authorize(&mut package).expect("the keystore holds Bob's `coll` key; qed");

		let token = GuestToken::decode_all(&mut &package.authorization[..])
			.expect("the guest decodes the token this node encoded");
		let index = expected_collator_index(package.context.lookup_anchor_slot, &bob.config);
		assert_eq!(index, bob.own_index(), "the lookup anchor names the signer");
		token.check_proof(&bob.config, index).expect("the proof puts Bob at his own leaf");
		token
			.check_signature(signable_work_package_hash(&package))
			.expect("the signature is over the payload the guest recomputes");
		assert!(token.control_command.is_none(), "a collator never sends a command");
	}

	/// ...and the same token must fail once it is read as a *different* collator's, which is the
	/// only thing standing between the round-robin and any collator authoring any slot.
	#[test]
	fn a_token_read_at_another_collators_index_errors() {
		let bob = authorizer_of("alice,bob,charlie", "Bob", 1);
		let mut package = package(4, &bob);
		bob.authorize(&mut package).expect("the keystore holds Bob's `coll` key; qed");

		let token = GuestToken::decode_all(&mut &package.authorization[..]).expect("decodes");
		assert!(token.check_proof(&bob.config, 0).is_err(), "Bob's proof is not Alice's");
	}

	/// Signing is the last step for a reason: it commits to the package's context and items, so a
	/// re-anchored package needs a new token, not a copied one.
	#[test]
	fn re_anchoring_needs_a_new_signature_works() {
		let alice = authorizer_of("alice", "Alice", 1);
		let mut first = package(4, &alice);
		let mut second = package(7, &alice);
		alice.authorize(&mut first).expect("signs");
		alice.authorize(&mut second).expect("signs");

		assert_ne!(first.authorization, second.authorization);
		let stale = GuestToken::decode_all(&mut &first.authorization[..]).expect("decodes");
		assert!(
			stale.check_signature(signable_work_package_hash(&second)).is_err(),
			"the first token does not authorize the re-anchored package",
		);
	}
}
