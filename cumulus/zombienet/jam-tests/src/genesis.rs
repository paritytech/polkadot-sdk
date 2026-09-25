// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The AURA authorizer hash of a para, derived here so it can go straight into the JAM chain's
//! genesis.
//!
//! It is the collator that decides this hash: it reads its set from the runtime, hashes the blob
//! it was handed and builds the config out of both (`AuraAuthorizer::new` in
//! `polkadot-omni-node/lib/src/nodes/jam/authorizer.rs`). Genesis has to arrive at the very same
//! value, because a core's queue holds a hash and nothing else — a hash the collators do not
//! reproduce is a core that authorizes nothing, with a stalled para as the only symptom. So this
//! module walks the same steps through the same crate the collator and the guest use, and the
//! harness checks the result against what the collators log at startup.

use crate::{
	chain_spec,
	para::{Para, PARACHAIN_SERVICE_ID},
};
use anyhow::Context;
use codec::Encode;
use parachain_authorizer::aura::{build_collator_tree, AuthConfig, CollatorKey};
use parachain_service_core::authorizer::{AuthConfigBlob, Authorizer, AuthorizerHash, CodeHash};
use sp_core::crypto::ByteArray;
use std::path::Path;

/// One JAM timeslot per parachain slot, which is the collator's `--jam-slot-duration` default and
/// therefore what the harness starts every collator with.
const SLOT_DURATION: u32 = 1;

/// The authorizer config `para`'s core is queued with, built the way the collator builds it:
/// the para id and host service, the collator-set root in the runtime's own order, the set size
/// and the slot duration — everything `blake2b-256(code_hash ‖ SCALE(config))` commits to.
pub fn aura_config(para: &Para) -> anyhow::Result<AuthConfig> {
	let (collator_set_root, _proofs) = build_collator_tree(&collator_set(para)?);
	Ok(AuthConfig {
		para_ids: vec![para.id.into()],
		parachain_service: PARACHAIN_SERVICE_ID,
		collator_set_root,
		collator_set_size: para.collators.len() as u32,
		slot_duration: SLOT_DURATION,
	})
}

/// The authorizer hash `para`'s core has to hold for its collators' work packages to run.
pub fn authorizer_hash(para: &Para, authorizer_blob: &Path) -> anyhow::Result<AuthorizerHash> {
	let blob = std::fs::read(authorizer_blob)
		.with_context(|| format!("reading {}", authorizer_blob.display()))?;
	let code_hash = CodeHash::from(jam_std_common::hash_raw(&blob));
	let authorizer = Authorizer { code_hash, config: AuthConfigBlob(aura_config(para)?.encode()) };
	Ok(parachain_service_core::authorizer::authorizer_hash(&authorizer))
}

/// The hash as `gen-spec` and a log line spell it: bare lowercase hex, no `0x`.
pub fn hex(hash: &AuthorizerHash) -> String {
	array_bytes::bytes2hex("", hash.0)
}

/// `para`'s collator set as the trie's leaves, in the order the collators are listed.
///
/// The runtime hands its aura authorities back in the order the chain spec's `session.keys`
/// named them (the `with-authority-discovery` test runtime does not re-sort), and the leaf order
/// is what the authorizer hash commits to, so the set is hashed in that same listed order. The
/// parachain template is the exception: its collator-selection re-sorts the invulnerables by
/// account id, so a template run must list its collators in authority order
/// ([`chain_spec::in_authority_order`]).
fn collator_set(para: &Para) -> anyhow::Result<Vec<CollatorKey>> {
	para.collators
		.iter()
		.map(|name| {
			Ok(chain_spec::account_of(name)?
				.as_slice()
				.try_into()
				.expect("an sr25519 public key is 32 bytes; qed"))
		})
		.collect()
}

/// The hash out of a collator's `Derived the para's AURA authorizer` log line, as far as it is
/// spelled there.
///
/// `tracing` prints the hash through its `Debug`, which abbreviates: what comes back is a prefix
/// of the real hex, long enough to tell two authorizers apart and never enough to reconstruct
/// one. Callers must therefore compare it as a prefix. `None` also covers the one hash in
/// 10^13 whose bytes are all printable, which that `Debug` spells as a quoted string instead.
pub fn logged_authorizer_hash(line: &str) -> Option<&str> {
	let (_, rest) = line.split_once("authorizer_hash=0x")?;
	let hex = rest.split_whitespace().next()?.trim_end_matches('.');
	hex.chars().all(|c| c.is_ascii_hexdigit()).then_some(hex)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::chain_spec::DEV_ACCOUNTS;

	/// Any file will do: only its hash reaches the config, and nothing here asserts what that is.
	const SOME_BLOB: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");

	fn para(id: u32, collators: &[&str]) -> Para {
		Para {
			id,
			core: 0,
			also_cores: Vec::new(),
			collators: collators.iter().map(|name| name.to_string()).collect(),
			runtime: None,
			full_nodes: Vec::new(),
		}
	}

	/// The hash, built the long way round: the layout spelled out here rather than delegated to
	/// the same helpers the function under test calls, so that this fails if either the config's
	/// field order or the `code_hash ‖ config` concatenation ever moves. That is the contract
	/// three separate binaries have to agree on — this harness, the collator and the guest.
	fn expected(id: u32, keys: &[[u8; 32]]) -> AuthorizerHash {
		let leaves: Vec<CollatorKey> = keys.to_vec();
		let (root, _) = build_collator_tree(&leaves);
		let mut config = Vec::new();
		// SCALE, field by field: a length-prefixed vec of one para id, the service, the root, the
		// set size, the slot duration.
		config.extend(vec![1u8 << 2]);
		config.extend(id.to_le_bytes());
		config.extend(PARACHAIN_SERVICE_ID.to_le_bytes());
		config.extend(root.as_bytes());
		config.extend((keys.len() as u32).to_le_bytes());
		config.extend(SLOT_DURATION.to_le_bytes());

		let blob = std::fs::read(SOME_BLOB).expect("the manifest is readable");
		let concatenated: Vec<u8> = [&jam_std_common::hash_raw(&blob)[..], &config[..]].concat();
		AuthorizerHash(jam_std_common::hash_raw(&concatenated))
	}

	fn dev_key(index: usize) -> [u8; 32] {
		DEV_ACCOUNTS[index].public().as_slice().try_into().expect("32 bytes")
	}

	#[test]
	fn the_hash_is_the_one_the_collator_and_the_guest_derive() {
		let alice = para(0, &["alice"]);
		assert_eq!(
			authorizer_hash(&alice, Path::new(SOME_BLOB)).unwrap(),
			expected(0, &[dev_key(0)]),
		);
	}

	/// The set is the collators in listed order, which is the order the runtime names its aura
	/// authorities in: the leaf order is what the root — and so the hash — commits to. A harness
	/// that hashed a different order would install a hash no collator ever matches.
	#[test]
	fn the_set_is_hashed_in_listed_order() {
		let two = para(0, &["alice", "bob"]);
		let (bob, alice) = (dev_key(1), dev_key(0));

		let hash = authorizer_hash(&two, Path::new(SOME_BLOB)).unwrap();
		assert_eq!(hash, expected(0, &[alice, bob]));
		assert_ne!(hash, expected(0, &[bob, alice]), "the reversed order is a different core");
		assert_eq!(
			authorizer_hash(&para(0, &["bob", "alice"]), Path::new(SOME_BLOB)).unwrap(),
			expected(0, &[bob, alice]),
			"listing the collators in the other order hashes the set in that order",
		);
	}

	/// Two paras on the same collator set must land on different cores, which is only true
	/// because the para id is inside the config the hash commits to.
	#[test]
	fn each_para_gets_its_own_hash() {
		let blob = Path::new(SOME_BLOB);
		assert_ne!(
			authorizer_hash(&para(0, &["alice"]), blob).unwrap(),
			authorizer_hash(&para(1, &["alice"]), blob).unwrap(),
		);
	}

	#[test]
	fn the_hex_is_bare_lowercase() {
		let hash = AuthorizerHash([0xabu8; 32]);
		assert_eq!(hex(&hash), "ab".repeat(32));
	}

	/// Verbatim from a collator log. The hash is abbreviated there, so the check the harness runs
	/// on it can only ever be a prefix comparison — and it has to survive the surrounding fields.
	#[test]
	fn the_logged_hash_is_read_off_the_startup_line() {
		const LINE: &str = "2026-09-03 12:06:17.822  INFO main jam-collator: Derived the para's \
			AURA authorizer. para_id=0 code_hash=0x7d74cdce72230fde... config_len=49 \
			authorizer_hash=0xb925833c8af4b3f2... own_index=0";

		assert_eq!(logged_authorizer_hash(LINE), Some("b925833c8af4b3f2"));
		// The line the collator prints before it has a hash must not read as one of length zero.
		assert_eq!(logged_authorizer_hash("collator starting"), None);
	}

	/// The abbreviation the log carries has to be a prefix of the hash written into genesis, or
	/// the harness's startup check would pass on any hash at all. Read out of the very `Debug`
	/// the collator prints through, so that a change to it fails here rather than in a
	/// twenty-minute e2e.
	#[test]
	fn the_logged_prefix_belongs_to_the_full_hash() {
		let hash = AuthorizerHash([0xb9u8; 32]);
		let line = format!("authorizer_hash={hash:?} own_index=0");

		let logged = logged_authorizer_hash(&line).expect("the line carries a hash");
		assert!(logged.len() >= 16, "an abbreviation this short would compare nothing: {logged}");
		assert!(hex(&hash).starts_with(logged), "{logged} is not a prefix of {}", hex(&hash));
	}
}
