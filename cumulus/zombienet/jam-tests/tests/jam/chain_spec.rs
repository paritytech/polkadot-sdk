// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Building the collators' chain spec.
//!
//! The `development` preset of the parachain template pins para id 1000 and endows exactly two
//! collators. A JAM run needs the para id its core was assigned to, and one authority per running
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

/// The dev accounts the collators run as, in the order the harness hands them out. The template's
/// `development` preset endows all of them, so no balances patch is needed.
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
}

fn ss58(keyring: Sr25519Keyring) -> String {
	keyring.to_account_id().to_ss58check()
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

	// A self-check on the preset we are patching: it must still be the two-collator sr25519
	// Alice/Bob shape this code assumes.
	let expected: Vec<String> = DEV_ACCOUNTS[..2].iter().map(|k| ss58(*k)).collect();
	let found: Vec<String> = patch["session"]["keys"]
		.as_array()
		.ok_or_else(|| anyhow!("preset has no session.keys array"))?
		.iter()
		.map(|entry| entry[0].as_str().unwrap_or_default().to_string())
		.collect();
	anyhow::ensure!(
		found == expected,
		"the development preset changed shape: session.keys is {found:?}, expected {expected:?}"
	);

	let accounts: Vec<String> = collators.iter().map(|index| ss58(DEV_ACCOUNTS[*index])).collect();
	patch["session"]["keys"] = accounts
		.iter()
		.map(|account| json!([account, account, { "aura": account }]))
		.collect();
	patch["collatorSelection"]["invulnerables"] = accounts.into();
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
