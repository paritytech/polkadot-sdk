// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Building the collators' chain spec.
//!
//! A `development` preset names the collators and the para id its own runtime was written for —
//! the parachain template pins para id 1000 and two collators, Asset Hub Rococo pins 1000 and
//! one. A JAM run needs the para id its core was assigned to, and one authority per running
//! collator, so the generated spec is patched before any collator sees it.
//!
//! The para id is the caller's: it is what `parasim-tool assign-core <para> <core>` writes into
//! the authorizer config the core commits to, and the collator reads its own id straight out of
//! this spec. The two must agree or the collator computes an authorizer hash no core holds.

use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use sp_core::crypto::Ss58Codec;
use sp_keyring::Sr25519Keyring;
use std::{path::Path, process::Command};

/// The dev accounts the collators run as, in the order the harness hands them out. Not every
/// preset endows all of them — Asset Hub Rococo's funds only Alice and Bob — so [`endow`] tops up
/// whoever is missing.
pub const DEV_ACCOUNTS: [Sr25519Keyring; 6] = [
	Sr25519Keyring::Alice,
	Sr25519Keyring::Bob,
	Sr25519Keyring::Charlie,
	Sr25519Keyring::Dave,
	Sr25519Keyring::Eve,
	Sr25519Keyring::Ferdie,
];

/// The lowercase dev name of [`DEV_ACCOUNTS`]`[index]`, which is what `parasim-tool
/// --collators` names a collator by.
pub fn dev_name(index: usize) -> String {
	DEV_ACCOUNTS[index].to_string().to_lowercase()
}

/// The `--alice` .. `--ferdie` flag that makes a collator author as [`DEV_ACCOUNTS`]`[index]`.
pub fn dev_account_flag(index: usize) -> String {
	format!("--{}", dev_name(index))
}

/// `collators` reordered the way the runtime hands the set back, which is *not* the order genesis
/// names them in.
///
/// `AuraApi::authorities()` is the collator set, and its order is the round-robin order that the
/// authorizer hash commits to. But pallet-collator-selection keeps its invulnerables sorted by
/// account id and pallet-session builds the aura authorities from that, so the runtime returns
/// the set ascending by account id however genesis wrote it — `alice,bob` comes back as
/// `bob,alice`, because Bob's key sorts below Alice's.
///
/// Everything that has to reproduce the set byte for byte therefore has to use this order:
/// `parasim-tool --collators`, which builds the collator trie the authorizer hash commits to. Get
/// it wrong and the hash is one no collator will ever match, with a core that authorizes nothing
/// as the only symptom.
pub fn in_authority_order(collators: &[usize]) -> Vec<usize> {
	let mut ordered = collators.to_vec();
	ordered.sort_by_key(|index| DEV_ACCOUNTS[*index].to_account_id());
	ordered
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Pinned against the dev keys themselves rather than against a second call to the same sort,
	/// so this fails if the ordering rule ever stops matching what the runtime does. Alice is
	/// named first everywhere in this harness and comes back second, which is exactly the trap
	/// this function exists to avoid — and it is invisible to a single-collator run.
	#[test]
	fn the_authority_order_is_by_account_id_not_by_name() {
		let names = |collators: &[usize]| -> Vec<String> {
			in_authority_order(collators).into_iter().map(dev_name).collect()
		};
		assert_eq!(names(&[0, 1]), ["bob", "alice"]);
		assert_eq!(names(&[2, 3]), ["dave", "charlie"]);
		assert_eq!(names(&[0, 1, 2]), ["bob", "charlie", "alice"]);
		assert_eq!(names(&[0, 1, 2, 3, 4, 5]), ["ferdie", "dave", "bob", "charlie", "alice", "eve"]);
		// A single collator is the case that hides the bug: any order is the right one.
		assert_eq!(names(&[0]), ["alice"]);
	}

	/// A preset with `collators` invulnerables, shaped the way both runtimes' `development`
	/// presets come out of `chain-spec-builder`.
	fn preset(collators: usize) -> Value {
		let accounts: Vec<String> = DEV_ACCOUNTS[..collators].iter().map(|k| ss58(*k)).collect();
		json!({
			"balances": { "balances": accounts.iter().map(|a| json!([a, 1_000u64])).collect::<Vec<_>>() },
			"collatorSelection": { "invulnerables": accounts },
			"parachainInfo": { "parachainId": 1000 },
			"session": { "keys": (0..collators).map(|i| {
				let account = ss58(DEV_ACCOUNTS[i]);
				json!([account, account, { "aura": account }])
			}).collect::<Vec<_>>() },
		})
	}

	/// The self-check has to pass whatever runtime `RUNTIME_WASM` names, so it must not care how
	/// many collators the preset happens to pin: the template's two and Asset Hub Rococo's one
	/// are both fine.
	#[test]
	fn any_number_of_aura_only_collators_is_patchable() {
		assert!(ensure_patchable(&preset(1)).is_ok());
		assert!(ensure_patchable(&preset(2)).is_ok());
	}

	/// The reason the check exists at all: `patch` replaces `session.keys` wholesale with
	/// aura-only triples, so a runtime whose `SessionKeys` has a second field would have it
	/// dropped and produce a genesis the runtime cannot decode. That has to fail here, loudly.
	#[test]
	fn a_session_key_beside_aura_is_refused() {
		let mut spec = preset(1);
		spec["session"]["keys"][0][2]["beefy"] = json!("0x00");
		assert!(ensure_patchable(&spec).is_err());
	}

	/// The other half of the assumption: the rewrite writes the account into both the account and
	/// the validator-id slot, so a preset that separates them means something different by them.
	#[test]
	fn a_validator_id_that_is_not_the_account_is_refused() {
		let mut spec = preset(2);
		spec["session"]["keys"][0][1] = json!(ss58(Sr25519Keyring::Charlie));
		assert!(ensure_patchable(&spec).is_err());

		let mut missing = preset(1);
		missing["session"]["keys"] = json!([]);
		assert!(ensure_patchable(&missing).is_err());
	}

	/// Asset Hub Rococo's preset funds Alice and Bob only, so a six-collator run needs the rest
	/// topped up — and an already-funded collator must not be pushed a second time, because
	/// pallet-balances rejects a duplicate account at genesis.
	#[test]
	fn only_the_unfunded_collators_are_endowed() {
		let mut spec = preset(2);
		endow(&mut spec, &[ss58(Sr25519Keyring::Bob), ss58(Sr25519Keyring::Charlie)]).unwrap();

		let funded = spec["balances"]["balances"].as_array().unwrap();
		let accounts: Vec<&Value> = funded.iter().map(|entry| &entry[0]).collect();
		let expected = [Sr25519Keyring::Alice, Sr25519Keyring::Bob, Sr25519Keyring::Charlie]
			.map(|keyring| json!(ss58(keyring)));
		assert_eq!(accounts, expected.iter().collect::<Vec<_>>());
		// Charlie is endowed the same amount the preset chose for its own accounts.
		assert_eq!(funded[2][1], funded[0][1]);
	}
}

fn ss58(keyring: Sr25519Keyring) -> String {
	keyring.to_account_id().to_ss58check()
}

/// Where a run keeps para `para_id`'s chain spec. One place, because two moments need it: the
/// genesis assembly (which derives the para's genesis head from it) runs before the collators
/// (which are started on it).
pub fn path(work_dir: &Path, para_id: u32) -> std::path::PathBuf {
	work_dir.join(format!("jam-parachain-{para_id}-spec.json"))
}

/// Generate the chain spec of para `para_id` at `path`, with one authority per entry of
/// `collators` — indices into [`DEV_ACCOUNTS`], in the order the AURA round-robin walks them.
pub fn build(
	omni_node: &Path,
	runtime_wasm: &Path,
	path: &Path,
	para_id: u32,
	collators: &[usize],
) -> anyhow::Result<()> {
	anyhow::ensure!(
		!collators.is_empty() && collators.iter().all(|index| *index < DEV_ACCOUNTS.len()),
		"a para's collators must be a non-empty pick of the {} dev accounts, got {collators:?}",
		DEV_ACCOUNTS.len()
	);

	let status = Command::new(omni_node)
		.args(["chain-spec-builder", "--chain-spec-path"])
		.arg(path)
		.args(["create", "--relay-chain", "jam", "--para-id", &para_id.to_string(), "-r"])
		.arg(runtime_wasm)
		.args(["named-preset", "development"])
		.status()
		.with_context(|| format!("running {} chain-spec-builder", omni_node.display()))?;
	anyhow::ensure!(status.success(), "chain-spec-builder failed: {status}");

	patch(path, para_id, collators)
}

/// Point the spec at `para_id` and give every running collator an aura slot.
///
/// The authority set comes from `session.keys` (plus `collatorSelection.invulnerables`), NOT from
/// `aura.authorities`: the preset never sets the latter, and pallet-session would overwrite it at
/// the genesis session anyway. The authority count must equal the number of running collators or
/// the unfilled slots stall block production for a full slot each.
fn patch(path: &Path, para_id: u32, collators: &[usize]) -> anyhow::Result<()> {
	let mut spec: Value = serde_json::from_slice(&std::fs::read(path)?)
		.with_context(|| format!("parsing {}", path.display()))?;

	let patch = spec
		.pointer_mut("/genesis/runtimeGenesis/patch")
		.ok_or_else(|| anyhow!("chain spec has no genesis.runtimeGenesis.patch"))?;

	ensure_patchable(patch)?;

	let accounts: Vec<String> = collators.iter().map(|index| ss58(DEV_ACCOUNTS[*index])).collect();
	patch["session"]["keys"] = accounts
		.iter()
		.map(|account| json!([account, account, { "aura": account }]))
		.collect();
	patch["collatorSelection"]["invulnerables"] = accounts.clone().into();
	endow(patch, &accounts)?;
	patch["parachainInfo"]["parachainId"] = para_id.into();

	// `--para-id` / `--relay-chain jam` already set these; assert rather than re-set them, so a
	// chain-spec-builder change cannot silently leave the collators on the wrong chain.
	anyhow::ensure!(
		spec["para_id"] == json!(para_id),
		"chain spec para_id is {}, expected {para_id}",
		spec["para_id"]
	);
	anyhow::ensure!(
		spec["relay_chain"] == json!("jam"),
		"chain spec relay_chain is {}",
		spec["relay_chain"]
	);

	std::fs::write(path, serde_json::to_vec_pretty(&spec)?)?;
	Ok(())
}

/// A self-check on the preset before the rewrite above replaces parts of it wholesale.
///
/// It is structural rather than a comparison against one runtime's accounts, because the runtime
/// is the caller's (`RUNTIME_WASM`) and every preset names its own collators. What it insists on
/// is what the rewrite assumes: an aura-only [`account, account, keys`] triple per collator, and
/// somewhere to read an endowment from. A runtime with a second session key would have it dropped
/// silently, so that is a loud failure here instead of an undecodable genesis later.
fn ensure_patchable(patch: &Value) -> anyhow::Result<()> {
	let entries = patch["session"]["keys"]
		.as_array()
		.ok_or_else(|| anyhow!("preset has no session.keys array"))?;
	anyhow::ensure!(!entries.is_empty(), "preset's session.keys is empty");
	for entry in entries {
		let triple = entry.as_array().filter(|triple| triple.len() == 3);
		let session_keys = triple.and_then(|triple| triple[2].as_object());
		anyhow::ensure!(
			triple.is_some_and(|triple| triple[0].is_string() && triple[0] == triple[1]) &&
				session_keys.is_some_and(|keys| keys.len() == 1 && keys.contains_key("aura")),
			"preset's session.keys is not [account, account, {{aura}}] triples: {entry}"
		);
	}
	anyhow::ensure!(
		patch["collatorSelection"]["invulnerables"].is_array(),
		"preset has no collatorSelection.invulnerables array"
	);
	anyhow::ensure!(
		patch["parachainInfo"]["parachainId"].is_u64(),
		"preset has no parachainInfo.parachainId"
	);
	Ok(())
}

/// Endow every collator the preset does not, as generously as it endows its own accounts.
///
/// The template funds all six dev accounts, Asset Hub Rococo funds only Alice and Bob. Copying
/// the preset's own endowment rather than naming an amount keeps this free of any per-runtime
/// unit, and leaves a preset that already funds a collator untouched.
fn endow(patch: &mut Value, accounts: &[String]) -> anyhow::Result<()> {
	let balances = patch["balances"]["balances"]
		.as_array_mut()
		.ok_or_else(|| anyhow!("preset has no balances.balances array"))?;
	let endowment = balances
		.first()
		.and_then(|entry| entry.get(1))
		.cloned()
		.ok_or_else(|| anyhow!("preset endows nobody, so there is no endowment to copy"))?;

	for account in accounts {
		if !balances.iter().any(|entry| entry[0] == *account) {
			balances.push(json!([account, endowment.clone()]));
		}
	}
	Ok(())
}
