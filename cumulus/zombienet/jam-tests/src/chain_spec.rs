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
use sp_core::{
	crypto::{AccountId32, Ss58Codec},
	sr25519, Pair,
};
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

/// The sr25519 key zombienet gives a node called `name`, derived from the node name exactly
/// as zombienet does. This is the only way to know a collator's key before it is spawned.
pub fn account_of(name: &str) -> anyhow::Result<sr25519::Public> {
	anyhow::ensure!(!name.is_empty(), "cannot derive an account from an empty node name");
	// zombienet's rule, at `orchestrator/src/network_spec/node.rs`: `format!("//{}{name}",
	// name.remove(0).to_uppercase())` — a `//` hard-derivation of the name with its first
	// character uppercased.
	let mut rest = name.to_string();
	let first = rest.remove(0).to_uppercase();
	let seed = format!("//{first}{rest}");
	Ok(sr25519::Pair::from_string(&seed, None)?.public())
}

/// The `--<name>` flag that makes a collator author as the key zombienet derives from its name.
pub fn dev_account_flag(name: &str) -> String {
	format!("--{name}")
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
pub fn in_authority_order(collators: &[String]) -> anyhow::Result<Vec<String>> {
	let mut ordered: Vec<(sr25519::Public, String)> = collators
		.iter()
		.map(|name| {
			let public = account_of(name)
				.with_context(|| format!("deriving the key for collator {name}"))?;
			Ok((public, name.clone()))
		})
		.collect::<anyhow::Result<Vec<_>>>()?;
	ordered.sort_by_key(|(public, _)| *public);
	Ok(ordered.into_iter().map(|(_, name)| name).collect())
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
		let names = |collators: &[&str]| -> Vec<String> {
			let collators: Vec<String> = collators.iter().map(|name| name.to_string()).collect();
			in_authority_order(&collators).expect("the dev names derive")
		};
		assert_eq!(names(&["alice", "bob"]), ["bob", "alice"]);
		assert_eq!(names(&["charlie", "dave"]), ["dave", "charlie"]);
		assert_eq!(names(&["alice", "bob", "charlie"]), ["bob", "charlie", "alice"]);
		assert_eq!(
			names(&["alice", "bob", "charlie", "dave", "eve", "ferdie"]),
			["ferdie", "dave", "bob", "charlie", "alice", "eve"]
		);
		// A single collator is the case that hides the bug: any order is the right one.
		assert_eq!(names(&["alice"]), ["alice"]);
	}

	/// The derivation the whole approach rests on: zombienet derives a node's key from its name,
	/// so a collator's aura key is knowable before it is spawned only if this matches exactly.
	/// Pinned against the keyring's own keys, not against a second call to `account_of`, so a
	/// drift in the derivation rule fails here instead of as a core no collator ever matches.
	#[test]
	fn account_of_is_the_key_zombienet_gives_the_name() {
		assert_eq!(
			account_of("alice").expect("//Alice is a valid hard derivation"),
			Sr25519Keyring::Alice.public()
		);
		assert_eq!(
			account_of("bob").expect("//Bob is a valid hard derivation"),
			Sr25519Keyring::Bob.public()
		);
	}

	/// A preset with `collators` invulnerables, shaped the way both runtimes' `development`
	/// presets come out of `chain-spec-builder`.
	fn preset(collators: usize) -> Value {
		let accounts: Vec<String> =
			DEV_ACCOUNTS[..collators].iter().map(|k| ss58(k.public())).collect();
		json!({
			"balances": { "balances": accounts.iter().map(|a| json!([a, 1_000u64])).collect::<Vec<_>>() },
			"collatorSelection": { "invulnerables": accounts },
			"parachainInfo": { "parachainId": 1000 },
			"session": { "keys": (0..collators).map(|i| {
				let account = ss58(DEV_ACCOUNTS[i].public());
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

	/// A preset with no `pallet_collator_selection` — both of `cumulus-test-runtime`'s flavors
	/// come out that way — has nothing to patch there, and the absence must neither be an error
	/// nor lead the rewrite to invent the key.
	#[test]
	fn a_preset_without_collator_selection_is_patchable() {
		let mut spec = preset(1);
		spec.as_object_mut().unwrap().remove("collatorSelection");
		assert!(ensure_patchable(&spec).is_ok(), "no collatorSelection is not an error");

		rewrite_preset(&mut spec, 0, &["alice".to_string()]).unwrap();
		assert!(spec.get("collatorSelection").is_none(), "the rewrite must not add one");
		assert_eq!(spec["parachainInfo"]["parachainId"], json!(0));
	}

	/// `cumulus-test-runtime`'s default flavor has no `pallet_session`: it seeds `pallet_aura`
	/// directly, so its preset names the set under `aura.authorities` — which is what the
	/// rewrite has to replace, with no `session.keys` to write.
	#[test]
	fn a_preset_with_aura_authorities_instead_of_session_keys_is_patchable() {
		let mut spec = preset(2);
		spec.as_object_mut().unwrap().remove("session");
		spec["aura"] = json!({
			"authorities": [ss58(DEV_ACCOUNTS[0].public()), ss58(DEV_ACCOUNTS[1].public())],
		});
		assert!(ensure_patchable(&spec).is_ok());

		rewrite_preset(&mut spec, 0, &["alice".to_string()]).unwrap();
		assert_eq!(
			spec["aura"]["authorities"],
			json!([ss58(Sr25519Keyring::Alice.public())]),
			"the set is exactly the running collators",
		);
		assert!(spec.get("session").is_none(), "there is no session palette to write");
	}

	/// The `with-authority-discovery` flavor's session key map carries
	/// `{aura, authority_discovery}` over the same raw sr25519 key. The rewrite sets both to the
	/// running collator's account — the aura set and the AD set have to stay index-aligned — and
	/// keeps the entry shape, rather than dropping the second field and producing an undecodable
	/// genesis.
	#[test]
	fn authority_discovery_beside_aura_is_set_to_the_same_collator() {
		let mut spec = preset(2);
		spec["session"]["keys"][0][2]["authority_discovery"] =
			json!(ss58(Sr25519Keyring::Charlie.public()));
		assert!(ensure_patchable(&spec).is_ok(), "a second session key is not a reason to refuse");

		rewrite_preset(&mut spec, 0, &["alice".to_string()]).unwrap();
		let entry = &spec["session"]["keys"][0];
		let alice = json!(ss58(Sr25519Keyring::Alice.public()));
		assert_eq!(entry[0], alice);
		assert_eq!(entry[1], alice);
		assert_eq!(entry[2]["aura"], alice);
		assert_eq!(
			entry[2]["authority_discovery"], alice,
			"authority discovery is the same collator account as aura",
		);
		assert_eq!(spec["session"]["keys"].as_array().unwrap().len(), 1, "sized to the collators");
	}

	/// A collator the preset does not name is appended with the preset's own entry shape:
	/// `authority_discovery` too, when the runtime's `SessionKeys` names it, so an AD runtime's
	/// appended collator is not left without an AD key.
	#[test]
	fn an_appended_collator_gets_the_presets_session_key_shape() {
		let mut spec = preset(1);
		spec["session"]["keys"][0][2]["authority_discovery"] =
			json!(ss58(Sr25519Keyring::Alice.public()));
		assert!(ensure_patchable(&spec).is_ok());

		rewrite_preset(&mut spec, 0, &["alice".to_string(), "bob".to_string()]).unwrap();
		let bob = json!(ss58(Sr25519Keyring::Bob.public()));
		let appended = &spec["session"]["keys"][1];
		assert_eq!(appended[0], bob);
		assert_eq!(appended[1], bob);
		assert_eq!(appended[2]["aura"], bob);
		assert_eq!(appended[2]["authority_discovery"], bob, "the appended entry keeps the shape");
	}

	/// The other half of the assumption: the rewrite writes the account into both the account and
	/// the validator-id slot, so a preset that separates them means something different by them.
	#[test]
	fn a_validator_id_that_is_not_the_account_is_refused() {
		let mut spec = preset(2);
		spec["session"]["keys"][0][1] = json!(ss58(Sr25519Keyring::Charlie.public()));
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
		endow(
			&mut spec,
			&[ss58(Sr25519Keyring::Bob.public()), ss58(Sr25519Keyring::Charlie.public())],
		)
		.unwrap();

		let funded = spec["balances"]["balances"].as_array().unwrap();
		let accounts: Vec<&Value> = funded.iter().map(|entry| &entry[0]).collect();
		let expected = [Sr25519Keyring::Alice, Sr25519Keyring::Bob, Sr25519Keyring::Charlie]
			.map(|keyring| json!(ss58(keyring.public())));
		assert_eq!(accounts, expected.iter().collect::<Vec<_>>());
		// Charlie is endowed the same amount the preset chose for its own accounts.
		assert_eq!(funded[2][1], funded[0][1]);
	}
}

fn ss58(public: sr25519::Public) -> String {
	AccountId32::from(public.0).to_ss58check()
}

/// Generate the chain spec of para `para_id` at `path`, with one authority per entry of
/// `collators` — node names, in the order the AURA round-robin walks them.
pub fn build(
	omni_node: &Path,
	runtime_wasm: &Path,
	path: &Path,
	para_id: u32,
	collators: &[String],
) -> anyhow::Result<()> {
	anyhow::ensure!(
		!collators.is_empty(),
		"a para's collators must be a non-empty list of node names, got {collators:?}"
	);

	let status = Command::new(omni_node)
		// The runtime the chain spec is built from is the code JAM validates with: when it is a
		// PolkaVM blob (validation code), constructing it requires the experimental PolkaVM
		// executor, which is off unless this flag is set. Inert for a WASM runtime.
		.env("SUBSTRATE_ENABLE_POLKAVM", "1")
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
/// The authority count must equal the number of running collators or the unfilled slots stall
/// block production for a full slot each. Where the set lives is the preset's shape — see
/// [`rewrite_preset`] — and `collators` must be in the order the AURA round-robin walks them,
/// because the authorizer hash commits to that order ([`in_authority_order`]).
fn patch(path: &Path, para_id: u32, collators: &[String]) -> anyhow::Result<()> {
	let mut spec: Value = serde_json::from_slice(&std::fs::read(path)?)
		.with_context(|| format!("parsing {}", path.display()))?;

	let patch = spec
		.pointer_mut("/genesis/runtimeGenesis/patch")
		.ok_or_else(|| anyhow!("chain spec has no genesis.runtimeGenesis.patch"))?;

	rewrite_preset(patch, para_id, collators)?;

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

/// The rewrite [`patch`] applies to a parsed preset: one authority per running collator, and
/// `para_id` in the genesis config.
///
/// Which storage holds the authority set is the runtime's choice and both shapes occur:
/// `session.keys` when the preset has `pallet_session` (the parachain template, Asset Hub
/// Rococo, `cumulus-test-runtime`'s `with-authority-discovery` flavor), and `aura.authorities`
/// when it seeds `pallet_aura` directly (`cumulus-test-runtime`'s default flavor, which has no
/// `pallet_session`). `collatorSelection.invulnerables` is written when that pallet is there and
/// skipped when it is not.
fn rewrite_preset(patch: &mut Value, para_id: u32, collators: &[String]) -> anyhow::Result<()> {
	ensure_patchable(patch)?;

	let accounts: Vec<String> = collators
		.iter()
		.map(|name| {
			let public = account_of(name)
				.with_context(|| format!("deriving the key for collator {name}"))?;
			Ok(ss58(public))
		})
		.collect::<anyhow::Result<Vec<_>>>()?;

	set_authorities(patch, &accounts)?;
	if let Some(invulnerables) = patch
		.get_mut("collatorSelection")
		.and_then(|selection| selection.get_mut("invulnerables"))
	{
		*invulnerables = accounts.clone().into();
	}
	endow(patch, &accounts)?;
	patch["parachainInfo"]["parachainId"] = para_id.into();
	Ok(())
}

/// One authority per running collator, in whichever storage the preset seeds.
///
/// With `session.keys` the list is rewritten in place: the account, the validator id and the
/// `aura` key become the running collator's. When the runtime's `SessionKeys` also carries
/// `authority_discovery` — `cumulus-test-runtime --features with-authority-discovery` seeds it,
/// deriving both keys from the same raw sr25519 public — that key is set to the same collator
/// account, because the same account string is a valid `AuraId` and `AuthorityDiscoveryId` and
/// the aura and AD sets have to stay index-aligned. A runtime whose `SessionKeys` has no
/// `authority_discovery` (the parachain template) keeps its entry shape, so no key the runtime
/// cannot decode is invented. The list is then sized to the running collators, dropping trailing
/// preset entries and appending entries shaped like the preset's for collators it does not name.
///
/// A preset with no `session.keys` seeds `pallet_aura` directly, and there the authorities are
/// simply the running collators.
fn set_authorities(patch: &mut Value, accounts: &[String]) -> anyhow::Result<()> {
	if let Some(entries) = patch
		.get_mut("session")
		.and_then(|session| session.get_mut("keys"))
		.and_then(Value::as_array_mut)
	{
		// Which keys a session entry carries is the runtime's `SessionKeys`: the AD flavor names
		// `authority_discovery` as well as `aura`, the template names `aura` alone. Read it off
		// the preset's own entries so an appended entry has the same shape and the rewrite never
		// invents a key the runtime cannot decode.
		let with_authority_discovery = entries
			.iter()
			.filter_map(Value::as_array)
			.filter_map(|triple| triple.get(2))
			.filter_map(Value::as_object)
			.any(|keys| keys.contains_key("authority_discovery"));
		for (index, account) in accounts.iter().enumerate() {
			if let Some(entry) = entries.get_mut(index) {
				if let Some(triple) = entry.as_array_mut() {
					triple[0] = json!(account);
					triple[1] = json!(account);
					if let Some(keys) = triple.get_mut(2).and_then(Value::as_object_mut) {
						keys.insert("aura".to_string(), json!(account));
						if with_authority_discovery {
							keys.insert("authority_discovery".to_string(), json!(account));
						}
					}
				}
			} else {
				let mut keys = serde_json::Map::new();
				keys.insert("aura".to_string(), json!(account));
				if with_authority_discovery {
					keys.insert("authority_discovery".to_string(), json!(account));
				}
				entries.push(json!([account, account, keys]));
			}
		}
		entries.truncate(accounts.len());
		return Ok(());
	}

	let authorities = patch
		.get_mut("aura")
		.and_then(|aura| aura.get_mut("authorities"))
		.ok_or_else(|| anyhow!("preset has neither session.keys nor aura.authorities"))?;
	*authorities = json!(accounts);
	Ok(())
}

/// A self-check on the preset before the rewrite above replaces parts of it wholesale.
///
/// It is structural rather than a comparison against one runtime's accounts, because the runtime
/// is the caller's (`RUNTIME_WASM`) and every preset names its own collators. What it insists on
/// is what the rewrite assumes: an authority set — [`account, account, keys`] triples with an
/// `aura` field, or `aura.authorities` when there is no `pallet_session` — somewhere to read an
/// endowment from, and `collatorSelection.invulnerables` only if that pallet is present. A
/// second session key is fine, because the rewrite keeps it; the account and validator id still
/// have to agree, because the rewrite writes both.
fn ensure_patchable(patch: &Value) -> anyhow::Result<()> {
	if let Some(entries) = patch.get("session").and_then(|session| session.get("keys")) {
		let entries =
			entries.as_array().ok_or_else(|| anyhow!("preset has no session.keys array"))?;
		anyhow::ensure!(!entries.is_empty(), "preset's session.keys is empty");
		for entry in entries {
			let triple = entry.as_array().filter(|triple| triple.len() == 3);
			let session_keys = triple.and_then(|triple| triple[2].as_object());
			anyhow::ensure!(
				triple.is_some_and(|triple| triple[0].is_string() && triple[0] == triple[1]) &&
					session_keys.is_some_and(|keys| keys.contains_key("aura")),
				"preset's session.keys is not [account, account, {{aura, ..}}] triples: {entry}"
			);
		}
	} else {
		let authorities = patch["aura"]["authorities"]
			.as_array()
			.ok_or_else(|| anyhow!("preset has neither session.keys nor aura.authorities"))?;
		anyhow::ensure!(!authorities.is_empty(), "preset's aura.authorities is empty");
		anyhow::ensure!(
			authorities.iter().all(Value::is_string),
			"preset's aura.authorities is not a list of keys: {authorities:?}"
		);
	}
	if let Some(invulnerables) = patch
		.get("collatorSelection")
		.and_then(|selection| selection.get("invulnerables"))
	{
		anyhow::ensure!(
			invulnerables.is_array(),
			"preset's collatorSelection.invulnerables is not an array"
		);
	}
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
