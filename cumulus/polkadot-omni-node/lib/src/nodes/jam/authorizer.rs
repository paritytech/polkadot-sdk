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
//! The set is the runtime's own — `AuraApi::authorities()`, read once at startup, in list order,
//! which is round-robin order and therefore leaf order in the collator trie. The signing key is
//! this node's aura session key, the very key it claims slots with, and its scheme is whatever
//! the runtime's `AuraId` is: the keystore is reached through the scheme-blind [`Keystore::keys`]
//! / [`Keystore::has_keys`] / [`Keystore::sign_with`] trio, which take the key type and crypto id
//! that `AuraId` already carries. Nothing else here knows a scheme — keys and signatures are raw
//! 32/64-byte arrays.
//!
//! Everything the authorization scheme leaves open — leaf hashing, the round-robin arithmetic,
//! the signing payload — comes out of `parachain-authorizer`, the very crate the guest is built
//! from: an authorizer hash commits to the whole config, so a config this node cannot reproduce
//! byte for byte is a core it cannot use.

use super::LOG_TARGET;
use crate::common::aura::AuraIdT;
use codec::Encode;
use jam_cumulus_facade::{
	H256,
	aura::{AuthConfig, AuthToken, expected_collator_index, signable_work_package_hash},
	authorizer::{AuthConfigBlob, Authorizer, AuthorizerHash, authorizer_hash},
};
use jam_interface::{ServiceId, Slot as JamSlot};
use jam_types::{Authorization, CodeHash, WorkPackage};
use parachain_authorizer::aura::{CollatorKey, CollatorSignature, Command, build_collator_tree};
use sp_core::{
	Pair,
	crypto::{ByteArray, CryptoTypeId, KeyTypeId},
	ed25519, sr25519,
};
use sp_keystore::{Keystore, KeystorePtr};
use sp_runtime::app_crypto::AppCrypto;
use std::{fmt::Debug, path::Path};

/// The public key of one aura authority, as `AuraApi::authorities()` hands it out for a runtime
/// whose authority id is `AuraId`.
pub(crate) type AuraPublic<AuraId> = <<AuraId as AuraIdT>::BoundedPair as Pair>::Public;

/// The AURA authorizer of one para, together with this collator's place in its set.
pub(crate) struct AuraAuthorizer {
	authorizer: Authorizer,
	hash: AuthorizerHash,
	config: AuthConfig,
	/// The set the config's root commits to, kept to catch the runtime's set drifting away from
	/// it (see [`AuraAuthorizer::warn_on_set_drift`]).
	collators: Vec<CollatorKey>,
	/// This collator's leaf index in the set, which is what the round-robin has to name before a
	/// signature of ours is worth anything.
	own_index: u32,
	own_key: CollatorKey,
	/// The collator's own branch of the set trie, rebuilt once at startup because the set the
	/// authorizer hash commits to never changes while the node runs.
	own_proof: Vec<H256>,
	/// Which keystore key signs, and under which scheme: the runtime's `AuraId` in the only two
	/// forms the keystore's scheme-blind calls take.
	key_type: KeyTypeId,
	crypto_id: CryptoTypeId,
	keystore: KeystorePtr,
}

impl AuraAuthorizer {
	/// Derive the para's authorizer from the runtime's aura authorities, and find this node in
	/// them.
	///
	/// Fails loudly rather than starting a collator that could never author: a set this node
	/// holds no key of, an unreadable blob, or a degenerate config the guest itself rejects.
	pub(crate) fn new<AuraId: AuraIdT>(
		authorities: &[AuraPublic<AuraId>],
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

		let collators = collator_keys(authorities)?;
		if collators.is_empty() {
			return Err("the runtime's aura authority set is empty, so there is no collator set \
			            to authorize against"
				.into());
		}
		let (collator_set_root, proofs) = build_collator_tree(&collators);
		let config = AuthConfig {
			para_ids: vec![para_id.into()],
			parachain_service,
			collator_set_root,
			collator_set_size: collators.len() as u32,
			slot_duration,
		};
		let authorizer = Authorizer { code_hash, config: AuthConfigBlob(config.encode()) };
		let hash = authorizer_hash(&authorizer);

		let key_type = <AuraId as AppCrypto>::ID;
		let crypto_id = <AuraId as AppCrypto>::CRYPTO_ID;
		let scheme = scheme_name(crypto_id);
		let own_index = own_index(&*keystore, key_type, &collators)
			.ok_or_else(|| own_key_missing(&*keystore, key_type, &scheme, &collators))?;

		tracing::info!(
			target: LOG_TARGET,
			para_id,
			parachain_service,
			slot_duration,
			%scheme,
			blob = ?blob_path,
			blob_len = blob.len(),
			?code_hash,
			collators = ?collators.iter().map(hex).collect::<Vec<_>>(),
			collator_set_size = collators.len(),
			?collator_set_root,
			config_len = authorizer.config.len(),
			authorizer_hash = ?hash,
			own_index,
			own_key = %hex(&collators[own_index]),
			proof_len = proofs[own_index].len(),
			"Derived the para's AURA authorizer from the runtime's aura authorities; this is the \
			 hash a core's pool must hold for our packages to be authorized.",
		);

		Ok(Self {
			authorizer,
			hash,
			config,
			own_index: own_index as u32,
			own_key: collators[own_index],
			own_proof: proofs[own_index].clone(),
			collators,
			key_type,
			crypto_id,
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

	/// Whether the runtime's authorities have moved away from the set this authorizer commits to.
	pub(crate) fn set_drifted<Public: ByteArray>(&self, authorities: &[Public]) -> bool {
		authorities.len() != self.collators.len() ||
			authorities
				.iter()
				.zip(&self.collators)
				.any(|(now, committed)| now.as_slice() != &committed[..])
	}

	/// Say so, loudly, when it has.
	///
	/// An authorizer hash commits to a *snapshot* of the set, so a session rotation leaves the
	/// core's queue pointing at a root the new collators cannot prove against: their packages die
	/// in-core with nothing to show for it until somebody re-assigns the core with the new root.
	pub(crate) fn warn_on_set_drift<Public: ByteArray>(
		&self,
		at: impl Debug,
		authorities: &[Public],
	) {
		if !self.set_drifted(authorities) {
			return;
		}
		let current: Vec<_> =
			authorities.iter().map(|key| array_bytes::bytes2hex("0x", key.as_slice())).collect();
		tracing::warn!(
			target: LOG_TARGET,
			at = ?at,
			authorities = ?current,
			committed = ?self.collators.iter().map(hex).collect::<Vec<_>>(),
			authorizer_hash = ?self.hash,
			"The runtime's aura authorities are no longer the set this para's authorizer hash was \
			 built from. Our packages stop being authorized until the core is re-assigned with the \
			 new set's root.",
		);
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
			.sign_with(self.key_type, self.crypto_id, &self.own_key, payload.as_bytes())
			.map_err(|error| format!("the aura keystore refused to sign: {error}"))?
			.ok_or_else(|| {
				format!(
					"the keystore no longer holds the aura key {} it was started with",
					hex(&self.own_key),
				)
			})?;
		let signature: CollatorSignature = signature.as_slice().try_into().map_err(|_| {
			format!(
				"the keystore signed with a {}-byte signature; a collator token takes 64",
				signature.len(),
			)
		})?;
		let token = AuthToken {
			proof: self.own_proof.clone(),
			key: self.own_key,
			signature,
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
			own_key = %hex(&self.own_key),
			proof_len = self.own_proof.len(),
			token_len = authorization.len(),
			"Signed the work package as the collator its lookup anchor names.",
		);
		package.authorization = authorization;
		Ok(())
	}
}

/// The set's raw public keys, in the order `authorities()` returned them, which is the order the
/// round-robin walks and the order the trie's leaves are in.
fn collator_keys<Public: ByteArray>(authorities: &[Public]) -> Result<Vec<CollatorKey>, String> {
	authorities
		.iter()
		.enumerate()
		.map(|(index, key)| {
			key.as_slice().try_into().map_err(|_| {
				format!(
					"the runtime's aura authority {index} is {} bytes long; a collator set takes \
					 32-byte keys",
					key.as_slice().len(),
				)
			})
		})
		.collect()
}

/// This node's leaf index: the first authority the keystore can sign for.
fn own_index(
	keystore: &dyn Keystore,
	key_type: KeyTypeId,
	collators: &[CollatorKey],
) -> Option<usize> {
	collators.iter().position(|key| keystore.has_keys(&[(key.to_vec(), key_type)]))
}

/// What to tell an operator whose node holds no aura key of the set the runtime named.
fn own_key_missing(
	keystore: &dyn Keystore,
	key_type: KeyTypeId,
	scheme: &str,
	collators: &[CollatorKey],
) -> String {
	let ours = keystore
		.keys(key_type)
		.unwrap_or_default()
		.iter()
		.map(|key| array_bytes::bytes2hex("0x", key))
		.collect::<Vec<_>>();
	format!(
		"none of this node's `aura` keys is in the runtime's authority set. The set is {:?} and \
		 the keystore holds {ours:?}; a collator signs its work packages with the aura key it \
		 authors blocks with, so that key has to be in the keystore — `--alice` and friends put it \
		 there in memory, otherwise `key insert --scheme {scheme} --key-type aura --suri //<Name>`",
		collators.iter().map(hex).collect::<Vec<_>>(),
	)
}

/// A raw collator key as an operator reads it, and as `key insert` spells it.
fn hex(key: &CollatorKey) -> String {
	array_bytes::bytes2hex("0x", key)
}

/// The runtime's signature scheme, spelled the way an operator would; the raw crypto code for
/// anything the two authorizer blobs do not cover.
fn scheme_name(crypto_id: CryptoTypeId) -> String {
	match crypto_id {
		sr25519::CRYPTO_ID => "sr25519".into(),
		ed25519::CRYPTO_ID => "ed25519".into(),
		other => String::from_utf8_lossy(&other.0).into_owned(),
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;
	use codec::DecodeAll;
	use jam_types::{RefineContext, WorkItem, WorkPayload};
	use parachain_authorizer::aura::AuthToken as GuestToken;
	use parachain_authorizer_ed25519::Ed25519;
	use sp_consensus_aura::{
		ed25519::AuthorityId as Ed25519AuraId, sr25519::AuthorityId as Sr25519AuraId,
	};
	use sp_keystore::testing::MemoryKeystore;
	use std::sync::Arc;

	pub(crate) const SERVICE_ID: ServiceId = 5;
	pub(crate) const PARA_ID: u32 = 1000;

	/// Any file will do: the blob reaches the config only through its hash, and nothing here
	/// asserts what that hash is.
	const SOME_BLOB: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");

	/// The authorities a dev runtime returns for `names`: the `//Name` keys on the runtime's own
	/// curve, which is what its genesis session keys are made of.
	fn authorities<AuraId: AuraIdT>(names: &str) -> Vec<AuraPublic<AuraId>> {
		names.split(',').map(|name| dev_key::<AuraId>(name.trim())).collect()
	}

	fn dev_key<AuraId: AuraIdT>(name: &str) -> AuraPublic<AuraId> {
		<AuraId::BoundedPair as Pair>::from_string(&dev_suri(name), None)
			.expect("a dev suri derives; qed")
			.public()
	}

	/// Capitalisation is the operator's convenience, not a different key.
	fn dev_suri(name: &str) -> String {
		let mut capitalized = name.trim().to_string();
		capitalized[..1].make_ascii_uppercase();
		format!("//{capitalized}")
	}

	/// A keystore provisioned as a dev collator's is: `aura` keys of the runtime's scheme,
	/// exactly what `--alice` puts there in memory.
	fn keystore_of<AuraId: AuraIdT>(names: &str) -> KeystorePtr {
		let keystore = MemoryKeystore::new();
		for name in names.split(',') {
			let public = dev_key::<AuraId>(name);
			keystore
				.insert(<AuraId as AppCrypto>::ID, &dev_suri(name), public.as_slice())
				.expect("the memory keystore takes any key; qed");
		}
		Arc::new(keystore)
	}

	fn authorizer_for<AuraId: AuraIdT>(
		names: &str,
		own: &str,
		slot_duration: u32,
	) -> AuraAuthorizer {
		AuraAuthorizer::new::<AuraId>(
			&authorities::<AuraId>(names),
			Path::new(SOME_BLOB),
			PARA_ID,
			SERVICE_ID,
			slot_duration,
			keystore_of::<AuraId>(own),
		)
		.expect("the authority set names the node; qed")
	}

	/// sr25519 is the parachain template's scheme, so it is what the tests that are not about
	/// schemes run on.
	pub(crate) fn authorizer_of(names: &str, own: &str, slot_duration: u32) -> AuraAuthorizer {
		authorizer_for::<Sr25519AuraId>(names, own, slot_duration)
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

	/// The trie's leaves are the authorities' raw bytes, so what `authorities()` hands out has to
	/// reach the set unchanged — on either curve. Pinned against substrate's published dev keys
	/// rather than against ourselves, which is what makes this a contract and not a tautology.
	#[test]
	fn the_set_is_the_authorities_raw_keys_works() {
		let sr25519 = collator_keys(&authorities::<Sr25519AuraId>("alice,bob")).expect("32 bytes");
		assert_eq!(
			hex(&sr25519[0]),
			"0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d",
		);
		let ed25519 = collator_keys(&authorities::<Ed25519AuraId>("alice,bob")).expect("32 bytes");
		assert_eq!(
			hex(&ed25519[0]),
			"0x88dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0ee",
		);
		assert_ne!(sr25519, ed25519, "the same names on two curves are two different sets");
	}

	/// The order `authorities()` returns *is* the round-robin, so a node's own index is its
	/// position in that list and nothing else — found by asking the keystore which of those keys
	/// it can sign for.
	#[test]
	fn own_index_is_the_position_in_the_authorities_works() {
		assert_eq!(authorizer_of("alice,bob,charlie", "Bob", 1).own_index(), 1);
		assert_eq!(authorizer_of("charlie,bob,alice", "Bob", 1).own_index(), 1);
		assert_eq!(authorizer_of("charlie,alice,bob", "Bob", 1).own_index(), 2);
		// A keystore holding more than one of the set's keys still authors as one collator: the
		// first it can sign for, so that the index it proves against is stable across restarts.
		let keystore = keystore_of::<Sr25519AuraId>("Bob,Alice");
		let set = collator_keys(&authorities::<Sr25519AuraId>("alice,bob,charlie")).expect("keys");
		assert_eq!(own_index(&*keystore, <Sr25519AuraId as AppCrypto>::ID, &set), Some(0));
	}

	/// A node whose keystore holds no key of the set could sign nothing a guarantor would accept,
	/// so it must refuse to start rather than author blocks that can never be authorized — and the
	/// error has to say how to fix it, because the fix is one key away.
	#[test]
	fn a_node_outside_the_authorities_errors() {
		let error = AuraAuthorizer::new::<Sr25519AuraId>(
			&authorities::<Sr25519AuraId>("alice,bob"),
			Path::new(SOME_BLOB),
			PARA_ID,
			SERVICE_ID,
			1,
			keystore_of::<Sr25519AuraId>("Dave"),
		)
		.map(drop)
		.expect_err("Dave is not an authority");
		assert!(error.contains("key insert --scheme sr25519"), "the error names the fix: {error}");
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
		let other_curve = authorizer_for::<Ed25519AuraId>("alice,bob", "Alice", 1);
		assert_eq!(one.hash(), same.hash(), "the hash is the para's, not the signer's");
		assert_ne!(one.hash(), smaller.hash(), "a different set is a different core");
		assert_ne!(
			one.hash(),
			other_curve.hash(),
			"the same names on another curve are another set"
		);
	}

	/// The authorizer hash commits to the set as it was at startup, so a runtime that rotates its
	/// authorities silently stops authorizing this collator. Drift is what the tick has to spot.
	#[test]
	fn set_drift_is_any_change_to_the_authorities_works() {
		let alice = authorizer_of("alice,bob", "Alice", 1);
		assert!(!alice.set_drifted(&authorities::<Sr25519AuraId>("alice,bob")));
		assert!(alice.set_drifted(&authorities::<Sr25519AuraId>("alice,bob,charlie")), "grown");
		assert!(alice.set_drifted(&authorities::<Sr25519AuraId>("alice")), "shrunk");
		assert!(alice.set_drifted(&authorities::<Sr25519AuraId>("alice,charlie")), "replaced");
		assert!(alice.set_drifted(&authorities::<Sr25519AuraId>("bob,alice")), "reordered");
		assert!(
			alice.set_drifted(&authorities::<Ed25519AuraId>("alice,bob")),
			"the same names on another curve are other keys",
		);
	}

	/// The drift alarm between this node and the guest: a token this node assembles must satisfy
	/// the very checks `is_authorized` runs on it — the proof against the set root, and the
	/// signature over the token-free package hash bound to the (absent) command, verified by the
	/// ed25519 blob's own verifier. If either side's hashing, bit order or payload ever moves,
	/// every package this collator sends starts failing in-core with nothing but a guarantor's
	/// silence to show for it, so it has to fail here.
	#[test]
	fn an_ed25519_token_satisfies_the_guest_verifier_works() {
		let bob = authorizer_for::<Ed25519AuraId>("alice,bob,charlie", "Bob", 1);
		// Slot 4 names collator 4 % 3 == 1, which is Bob.
		let mut package = package(4, &bob);
		bob.authorize(&mut package).expect("the keystore holds Bob's aura key; qed");

		let token = GuestToken::decode_all(&mut &package.authorization[..])
			.expect("the guest decodes the token this node encoded");
		let index = expected_collator_index(package.context.lookup_anchor_slot, &bob.config);
		assert_eq!(index, bob.own_index(), "the lookup anchor names the signer");
		token
			.check_proof(&bob.config, index)
			.expect("the proof puts Bob at his own leaf");
		token
			.check_signature::<Ed25519>(signable_work_package_hash(&package))
			.expect("the signature is over the payload the guest recomputes");
		assert!(token.control_command.is_none(), "a collator never sends a command");
		// ...and the same token must fail once it is read as a *different* collator's, which is
		// the only thing standing between the round-robin and any collator authoring any slot.
		assert!(token.check_proof(&bob.config, 0).is_err(), "Bob's proof is not Alice's");
	}

	/// The same contract on the other curve, verified against schnorrkel itself rather than
	/// against a keystore call: what this pins is that `sign_with` signs under the `b"substrate"`
	/// transcript context, which is the one thing an sr25519 guest verifier has to agree with and
	/// the one thing `sp_core`'s API gives a signer no say over.
	#[test]
	fn an_sr25519_token_verifies_under_the_substrate_context_works() {
		let bob = authorizer_of("alice,bob,charlie", "Bob", 1);
		let mut package = package(4, &bob);
		bob.authorize(&mut package).expect("the keystore holds Bob's aura key; qed");

		let token = GuestToken::decode_all(&mut &package.authorization[..])
			.expect("the guest decodes the token this node encoded");
		token
			.check_proof(&bob.config, bob.own_index())
			.expect("the proof puts Bob at his leaf");
		let payload = GuestToken::signing_payload(
			signable_work_package_hash(&package),
			&token.control_command,
		);
		let key = schnorrkel::PublicKey::from_bytes(&token.key).expect("a valid ristretto key");
		let signature =
			schnorrkel::Signature::from_bytes(&token.signature).expect("a valid signature");
		key.verify_simple(b"substrate", payload.as_bytes(), &signature)
			.expect("the keystore signs sr25519 under the `substrate` context");
		assert!(
			key.verify_simple(b"jam:parachain-service:aura", payload.as_bytes(), &signature)
				.is_err(),
			"the context is signed too, so a verifier that picks its own rejects every token",
		);
		assert!(token.check_proof(&bob.config, 0).is_err(), "Bob's proof is not Alice's");
	}

	/// Signing is the last step for a reason: it commits to the package's context and items, so a
	/// re-anchored package needs a new token, not a copied one.
	#[test]
	fn re_anchoring_needs_a_new_signature_works() {
		let alice = authorizer_for::<Ed25519AuraId>("alice", "Alice", 1);
		let mut first = package(4, &alice);
		let mut second = package(7, &alice);
		alice.authorize(&mut first).expect("signs");
		alice.authorize(&mut second).expect("signs");

		assert_ne!(first.authorization, second.authorization);
		let stale = GuestToken::decode_all(&mut &first.authorization[..]).expect("decodes");
		assert!(
			stale.check_signature::<Ed25519>(signable_work_package_hash(&second)).is_err(),
			"the first token does not authorize the re-anchored package",
		);
	}
}
