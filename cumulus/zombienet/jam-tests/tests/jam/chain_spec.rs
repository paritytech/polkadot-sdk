// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Building the collators' chain spec.
//!
//! The `development` preset of the parachain template pins para id 1000 and endows exactly two
//! collators. The JAM demo needs para id 0 — under the dev-genesis null authorizer parasim falls
//! back to `ParaId(0)` — and one authority per running collator, so the generated spec is patched
//! before any collator sees it.

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

/// The `--alice` .. `--ferdie` flag that makes a collator author as [`DEV_ACCOUNTS`]`[index]`.
pub fn dev_account_flag(index: usize) -> String {
	format!("--{}", DEV_ACCOUNTS[index].to_string().to_lowercase())
}

fn ss58(keyring: Sr25519Keyring) -> String {
	keyring.to_account_id().to_ss58check()
}

/// Generate the collators' chain spec at `path`, with `collators` authorities.
pub fn build(omni_node: &Path, runtime_wasm: &Path, path: &Path, collators: usize) -> anyhow::Result<()> {
	anyhow::ensure!(
		(1..=DEV_ACCOUNTS.len()).contains(&collators),
		"between 1 and {} collators are supported, got {collators}",
		DEV_ACCOUNTS.len()
	);

	let status = Command::new(omni_node)
		.args(["chain-spec-builder", "--chain-spec-path"])
		.arg(path)
		.args(["create", "--relay-chain", "jam", "--para-id", "0", "-r"])
		.arg(runtime_wasm)
		.args(["named-preset", "development"])
		.status()
		.with_context(|| format!("running {} chain-spec-builder", omni_node.display()))?;
	anyhow::ensure!(status.success(), "chain-spec-builder failed: {status}");

	patch(path, collators)
}

/// Point the spec at para 0 and give every running collator an aura slot.
///
/// The authority set comes from `session.keys` (plus `collatorSelection.invulnerables`), NOT from
/// `aura.authorities`: the preset never sets the latter, and pallet-session would overwrite it at
/// the genesis session anyway. The authority count must equal the number of running collators or
/// the unfilled slots stall block production for a full slot each.
fn patch(path: &Path, collators: usize) -> anyhow::Result<()> {
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
		"the development preset changed shape: expected session.keys for {expected:?}, found {found:?}"
	);

	let accounts: Vec<String> = DEV_ACCOUNTS[..collators].iter().map(|k| ss58(*k)).collect();
	patch["session"]["keys"] = accounts
		.iter()
		.map(|account| json!([account, account, { "aura": account }]))
		.collect();
	patch["collatorSelection"]["invulnerables"] = accounts.into();
	patch["parachainInfo"]["parachainId"] = 0.into();

	// `--para-id 0` / `--relay-chain jam` already set these; assert rather than re-set them, so a
	// chain-spec-builder change cannot silently leave the collators on the wrong chain.
	anyhow::ensure!(spec["para_id"] == json!(0), "chain spec para_id is {}", spec["para_id"]);
	anyhow::ensure!(spec["relay_chain"] == json!("jam"), "chain spec relay_chain is {}", spec["relay_chain"]);

	std::fs::write(path, serde_json::to_vec_pretty(&spec)?)?;
	Ok(())
}
