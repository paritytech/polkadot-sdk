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

use super::{
	chain_spec::{self, DEV_ACCOUNTS},
	collators::Para,
	network::PARASIM_SERVICE_ID,
};
use anyhow::Context;
use codec::{Compact, DecodeAll, Encode};
use jam_cumulus_facade::{
	aura::{build_collator_tree, AuthConfig, CollatorKey},
	authorizer::{AuthConfigBlob, Authorizer, AuthorizerHash, CodeHash},
	service_state::ParaInfo,
};
use sp_core::crypto::ByteArray;
use std::path::Path;

/// One JAM timeslot per parachain slot, which is the collator's `--jam-slot-duration` default and
/// therefore what the harness starts every collator with.
const SLOT_DURATION: u32 = 1;

/// The authorizer hash `para`'s core has to hold for its collators' work packages to run.
pub fn authorizer_hash(para: &Para, authorizer_blob: &Path) -> anyhow::Result<AuthorizerHash> {
	let blob = std::fs::read(authorizer_blob)
		.with_context(|| format!("reading {}", authorizer_blob.display()))?;
	let code_hash = CodeHash::from(jam_std_common::hash_raw(&blob));
	let (collator_set_root, _proofs) = build_collator_tree(&collator_set(para));

	let config = AuthConfig {
		para_ids: vec![para.id.into()],
		parachain_service: PARASIM_SERVICE_ID,
		collator_set_root,
		collator_set_size: para.collators.len() as u32,
		slot_duration: SLOT_DURATION,
	};
	let authorizer = Authorizer { code_hash, config: AuthConfigBlob(config.encode()) };
	Ok(jam_cumulus_facade::authorizer::authorizer_hash(&authorizer))
}

/// The hash as `gen-spec` and a log line spell it: bare lowercase hex, no `0x`.
pub fn hex(hash: &AuthorizerHash) -> String {
	array_bytes::bytes2hex("", hash.0)
}

/// `para`'s collator set as the trie's leaves, which is the runtime's own order.
///
/// [`chain_spec::in_authority_order`] is not a detail that can be skipped: the round-robin index
/// the guest computes is a leaf index, and the runtime hands its authorities back sorted by
/// account id whatever order genesis named them in.
fn collator_set(para: &Para) -> Vec<CollatorKey> {
	chain_spec::in_authority_order(&para.collators)
		.into_iter()
		.map(|index| {
			DEV_ACCOUNTS[index]
				.public()
				.as_slice()
				.try_into()
				.expect("an sr25519 public key is 32 bytes; qed")
		})
		.collect()
}

/// The state balance a pre-registered para is given: unlimited, so no §6.1 headroom check can
/// reject a candidate in a run that is about proving the pipeline, not the accounting.
pub const PARA_STATE_BALANCE: u64 = u64::MAX;

/// The para's genesis head as its collators will derive it: the SCALE-encoded genesis header of
/// the chain spec they are started on, read with the collator binary's own `export-genesis-head`.
///
/// Byte-exactness is the point of shelling out rather than deriving it here: the parachain
/// service accepts a candidate only when its declared parent hash is the blake2b of the stored
/// `head_data`, and the collator's first block declares whatever header its client built from
/// this very spec.
pub fn export_genesis_head(omni_node: &Path, spec: &Path) -> anyhow::Result<Vec<u8>> {
	// Its own base path, because the command builds a client: without one it would litter (or
	// trip over) the machine-wide default.
	let base_path = spec.parent().unwrap_or(Path::new(".")).join("export-genesis-head");
	let output = std::process::Command::new(omni_node)
		.arg("export-genesis-head")
		.arg("--chain")
		.arg(spec)
		.arg("--base-path")
		.arg(&base_path)
		.output()
		.with_context(|| format!("running {} export-genesis-head", omni_node.display()))?;
	anyhow::ensure!(
		output.status.success(),
		"export-genesis-head on {} failed ({}): {}",
		spec.display(),
		output.status,
		String::from_utf8_lossy(&output.stderr),
	);
	let hex = String::from_utf8(output.stdout).context("export-genesis-head wrote non-utf8")?;
	let head = array_bytes::hex2bytes(hex.trim())
		.map_err(|error| anyhow::anyhow!("export-genesis-head wrote no hex: {error:?}"))?;
	log::info!(
		"genesis head of {}: {} bytes, blake2b {} (the parent hash the para's first candidate \
		 must declare)",
		spec.display(),
		head.len(),
		array_bytes::bytes2hex("0x", jam_std_common::hash_raw(&head)),
	);
	Ok(head)
}

/// A para's `ParaInfo` as the parachain service stores it, for seeding into genesis: `head_data`,
/// an active validation code of `(code_hash, code_len)`, no pending upgrade, [`PARA_STATE_BALANCE`]
/// and not deregistering.
///
/// The bytes are laid out here because the facade does not re-export the service's
/// `ValidationCode` constructor; the decode through the service's real `ParaInfo` below is what
/// keeps this spelling from drifting away from the service.
pub fn encode_para_info(
	head_data: &[u8],
	code_hash: [u8; 32],
	code_len: u32,
) -> anyhow::Result<Vec<u8>> {
	let mut bytes = Vec::new();
	head_data.to_vec().encode_to(&mut bytes); // head_data
	bytes.push(1); // validation_code: Some(..)
	bytes.extend_from_slice(&code_hash); // ..code_ref.hash
	code_len.encode_to(&mut bytes); // ..code_ref.len
	bytes.push(0); // ..pinned: false
	bytes.push(0); // pending_upgrade: None
	Compact(PARA_STATE_BALANCE).encode_to(&mut bytes); // total_state_balance
	Compact(0u64).encode_to(&mut bytes); // used_state_balance
	bytes.push(0); // is_deregistering: false

	let decoded = ParaInfo::decode_all(&mut &bytes[..])
		.context("the hand-laid bytes no longer decode as the service's ParaInfo")?;
	anyhow::ensure!(
		decoded.head_data.as_slice() == head_data &&
			decoded.validation_code.is_some() &&
			decoded.total_state_balance == PARA_STATE_BALANCE &&
			decoded.encode() == bytes,
		"the hand-laid ParaInfo layout has drifted from the service's",
	);
	Ok(bytes)
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

	/// Any file will do: only its hash reaches the config, and nothing here asserts what that is.
	const SOME_BLOB: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");

	fn para(id: u32, collators: Vec<usize>) -> Para {
		Para { id, core: 0, collators }
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
		config.extend(PARASIM_SERVICE_ID.to_le_bytes());
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
		let alice = para(0, vec![0]);
		assert_eq!(
			authorizer_hash(&alice, Path::new(SOME_BLOB)).unwrap(),
			expected(0, &[dev_key(0)]),
		);
	}

	/// The set is the runtime's, not the harness's: `alice,bob` comes back from the runtime as
	/// `bob,alice`, and the leaf order is what the root — and so the hash — commits to. A
	/// harness that hashed its own order would install a hash no collator ever matches.
	#[test]
	fn the_set_is_hashed_in_authority_order() {
		let two = para(0, vec![0, 1]);
		let (bob, alice) = (dev_key(1), dev_key(0));

		let hash = authorizer_hash(&two, Path::new(SOME_BLOB)).unwrap();
		assert_eq!(hash, expected(0, &[bob, alice]));
		assert_ne!(hash, expected(0, &[alice, bob]), "the naive order is a different core");
		// ...and how the caller happened to list them makes no difference, because the runtime
		// sorts either way.
		assert_eq!(hash, authorizer_hash(&para(0, vec![1, 0]), Path::new(SOME_BLOB)).unwrap());
	}

	/// Two paras on the same collator set must land on different cores, which is only true
	/// because the para id is inside the config the hash commits to.
	#[test]
	fn each_para_gets_its_own_hash() {
		let blob = Path::new(SOME_BLOB);
		assert_ne!(
			authorizer_hash(&para(0, vec![0]), blob).unwrap(),
			authorizer_hash(&para(1, vec![0]), blob).unwrap(),
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

	/// The seeded entry read back through the service's own type: what the run's assertions (and
	/// the collator) decode is `ParaInfo`, so this is the round trip the seed lives or dies by.
	#[test]
	fn the_seeded_para_info_decodes_as_the_services() {
		use codec::DecodeAll;

		let head = vec![1u8, 2, 3];
		let bytes = encode_para_info(&head, [7u8; 32], 42).expect("the layout self-check passes");

		let info = ParaInfo::decode_all(&mut &bytes[..]).expect("the service's type decodes it");
		assert_eq!(info.head_data.as_slice(), &head[..]);
		assert!(info.validation_code.is_some());
		assert!(info.pending_upgrade.is_none());
		assert_eq!(info.total_state_balance, PARA_STATE_BALANCE);
		assert_eq!(info.used_state_balance, 0);
		assert!(!info.is_deregistering);
	}

	/// The `(hash, len)` pair, byte for byte where accumulate's step-5 comparison reads it: right
	/// after the head data, hash first, length as four little-endian bytes, unpinned. A pair at
	/// the wrong offset would still decode as *a* code reference — and silently reject every
	/// candidate, because the service compares the whole pair.
	#[test]
	fn the_code_reference_is_hash_then_le_len_unpinned() {
		let bytes = encode_para_info(&[], [7u8; 32], 42).expect("the layout self-check passes");

		let mut expected = vec![0u8, 1]; // empty head_data, then Some
		expected.extend([7u8; 32]);
		expected.extend(42u32.to_le_bytes());
		expected.push(0); // pinned: false
		assert_eq!(&bytes[..expected.len()], &expected[..]);
	}

	/// A head over the service's 4 KiB `HeadData` bound must be refused here, at seed time — the
	/// service could never have stored it, so seeding it would fake a state the pipeline cannot
	/// reach.
	#[test]
	fn an_oversized_head_is_refused() {
		assert!(encode_para_info(&vec![0u8; 4097], [7u8; 32], 42).is_err());
		assert!(encode_para_info(&vec![0u8; 4096], [7u8; 32], 42).is_ok());
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
