// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The pure genesis computation: the real parachain service, the paras' AURA authorizers, and
//! the cores that carry them, built into the JSON object `gen-spec` reads.

use crate::{
	chain_spec,
	env::{check_polkavm_format, Binaries},
	genesis,
	network::path_str,
	para::{Para, PARACHAIN_SERVICE_ID},
};
use anyhow::{anyhow, Context};
use jam_types::{AuthConfig, Authorizer, ProtocolParameters};
use parachain_chain_spec::{ParachainServiceSpec, ParachainSpec};
use parachain_service_core::{authorizer::AuthorizerHash, types::ParaId};
use serde_json::json;
use std::{
	collections::BTreeMap,
	path::{Path, PathBuf},
	process::Command,
};

/// The balance the service is created with.
pub const PARACHAIN_SERVICE_ENDOWMENT: u64 = 1_000_000_000_000_000;

/// Everything a caller needs to configure zombienet for the paras this genesis serves.
pub struct JamGenesis {
	/// The `services` / `auth_queues` / `assigners` object `gen-spec` reads.
	pub overrides: serde_json::Value,
	/// Each para's patched chain spec, in para order: the file its collators run and the file
	/// its genesis head was exported from.
	pub para_specs: Vec<PathBuf>,
	/// The copy of the authorizer blob whose hash went into genesis. Everything that has to
	/// agree on the authorizer hash must be pointed at this file, never at the build output.
	pub authorizer_blob: PathBuf,
}

/// The full ordered genesis construction for `paras` on a network with `cores` cores.
///
/// The order is load-bearing: each para's genesis head is exported FROM the patched chain spec
/// built just before it, and the service's registration record embeds that very head.
pub fn build_jam_genesis(
	binaries: &Binaries,
	work_dir: &Path,
	paras: &[Para],
	cores: u16,
) -> anyhow::Result<JamGenesis> {
	// Frozen copies: `gen-spec` reads `code` and the preimages off disk, so a PVM rebuild in the
	// source tree mid-run would strand an on-chain hash with no resolvable preimage.
	let service_blob = copy_aside(&binaries.parachain_service_blob, work_dir)?;
	let authorizer_blob = copy_aside(&binaries.authorizer_blob, work_dir)?;
	// Each para's chosen validation code, frozen before anything reads it: the chain spec embeds
	// these very bytes as `:code` and the service registers them as `validation_code`.
	let validation_code_paths = freeze_validation_codes(paras, &binaries.runtime_wasm, work_dir)?;

	// Each para's chain spec is built and patched before its genesis head is exported: the head
	// is the header the patched spec initializes from.
	let para_specs = paras
		.iter()
		.zip(&validation_code_paths)
		.map(|(para, runtime_blob)| {
			let spec = work_dir.join(format!("jam-parachain-{}-spec.json", para.id));
			// Written in authority order, so a runtime that keeps genesis order (no
			// collator-selection) returns the same set `genesis.rs` hashes.
			let collators = chain_spec::in_authority_order(&para.collators)?;
			chain_spec::build(&binaries.omni_node, runtime_blob, &spec, para.id, &collators)?;
			Ok(spec)
		})
		.collect::<anyhow::Result<Vec<_>>>()?;
	let heads = para_specs
		.iter()
		.map(|spec| export_genesis_head(&binaries.omni_node, spec).map(|h| genesis_head_hash(&h)))
		.collect::<anyhow::Result<Vec<_>>>()?;
	let validation_codes = validation_code_paths
		.iter()
		.map(|blob| std::fs::read(blob).with_context(|| format!("reading {}", blob.display())))
		.collect::<anyhow::Result<Vec<_>>>()?;
	let spec = parachain_service_spec(
		paras,
		std::fs::read(&service_blob)
			.with_context(|| format!("reading {}", service_blob.display()))?,
		std::fs::read(&authorizer_blob)
			.with_context(|| format!("reading {}", authorizer_blob.display()))?,
		&validation_codes,
		&heads,
	)?;
	let queues = auth_queues(paras, &spec.authorizer_hashes())?;
	let spare = spare_queues(&queues, cores);
	let genesis = spec.build()?;
	// `genesis.code` is exactly the frozen copy's bytes, so the copy `copy_aside` wrote is
	// the file `gen-spec` reads `code` from — no second sidecar to write or drift.
	let service_blob = path_str(&service_blob)?;
	let preimage_paths = genesis
		.preimages
		.iter()
		.enumerate()
		.map(|(index, blob)| write_sidecar(blob, work_dir, &format!("preimage-{index}.jam")))
		.collect::<anyhow::Result<Vec<_>>>()?;
	let mut overrides = built_genesis_overrides(
		&queues,
		genesis.id,
		&service_blob,
		genesis.balance,
		&preimage_paths
			.iter()
			.map(|path| path_str(path))
			.collect::<anyhow::Result<Vec<_>>>()?,
		&genesis.storage,
	);
	for (core, hash) in spare {
		overrides["auth_queues"][core.to_string()] = json!([hash]);
	}
	if let Some(blob) = &binaries.bootstrap_service_blob {
		overrides["services"][BOOTSTRAP_SERVICE_ID.to_string()] =
			bootstrap_service(blob, work_dir)?;
	}
	overrides["protocol_parameters"] = parameters_for_cores(cores)?;
	overrides["privileges"] = service_privileges(&queues, PARACHAIN_SERVICE_ID);

	Ok(JamGenesis { overrides, para_specs, authorizer_blob })
}

/// Freeze each para's chosen validation code into the run's work dir, in para order.
///
/// A para's blob is its own [`Para::runtime`] when it chose one, `default` (the run's
/// `RUNTIME_WASM`) otherwise. The returned path is what its chain spec is built from and what its
/// service `validation_code` is read from, so the two can never disagree. Every blob is checked
/// as a PolkaVM program, because a WASM blob here records a hash the real service refuses.
///
/// PVM builds are not byte-deterministic, so a rebuild in the source tree mid-run would strand an
/// on-chain hash with no resolvable preimage; the frozen copy is that preimage's source.
pub fn freeze_validation_codes(
	paras: &[Para],
	default: &Path,
	work_dir: &Path,
) -> anyhow::Result<Vec<PathBuf>> {
	paras
		.iter()
		.map(|para| {
			let blob = para.runtime_blob(default);
			check_polkavm_format(blob).map_err(|reason| anyhow!("para {}: {reason}", para.id))?;
			if blob == default {
				copy_aside(blob, work_dir)
			} else {
				// A per-para name: two paras may choose different blobs that share a file name.
				let name = blob
					.file_name()
					.with_context(|| format!("{} is not a file", blob.display()))?;
				let copy =
					work_dir.join(format!("runtime-para-{}-{}", para.id, name.to_string_lossy()));
				std::fs::copy(blob, &copy)
					.with_context(|| format!("copying {} to {}", blob.display(), copy.display()))?;
				Ok(copy)
			}
		})
		.collect()
}

/// The protocol parameters for a JAM network with `cores` cores: the tiny preset with the core
/// count widened. The validator count follows from it, because `max_val_count` is derived as
/// `core_count * VALS_PER_CORE`.
pub fn parameters_for_cores(cores: u16) -> anyhow::Result<serde_json::Value> {
	anyhow::ensure!(cores > 0, "a JAM network needs at least one core");
	let mut params = ProtocolParameters::tiny();
	params.core_count = cores;
	serde_json::to_value(&params).context("serializing the JAM protocol parameters")
}

/// PolkaVM cannot use its recompiler's Linux sandbox here (no userfaultfd), and the native
/// provider clears the environment before spawning, so every collator and tool needs these
/// explicitly. `SUBSTRATE_ENABLE_POLKAVM` turns on the PolkaVM executor, which both a chain spec
/// built from a PolkaVM runtime blob and the collators — whose `:code` is that same blob — need
/// to construct the runtime at all. JAM nodes get [`jam_node_env`] instead.
pub fn polkavm_env() -> Vec<(&'static str, &'static str)> {
	vec![
		("POLKAVM_BACKEND", "interpreter"),
		("POLKAVM_ALLOW_INSECURE", "1"),
		("SUBSTRATE_ENABLE_POLKAVM", "1"),
		("RUST_LOG", "debug"),
	]
}

/// The environment of a JAM node. Its refine runs the parachain service, and on the interpreter
/// one package takes about a whole six-second slot, so a para on two cores never gets its
/// packages reported in time. The recompiler is used whenever its Linux sandbox can start, which
/// needs `vm.unprivileged_userfaultfd = 1`.
pub fn jam_node_env() -> Vec<(&'static str, &'static str)> {
	let backend = if userfaultfd_available() {
		"compiler"
	} else {
		log::warn!(
			"vm.unprivileged_userfaultfd is not 1, so the JAM nodes fall back to the slow PVM \
			 interpreter; enable it with `sudo sysctl -w vm.unprivileged_userfaultfd=1`"
		);
		"interpreter"
	};
	vec![("POLKAVM_BACKEND", backend), ("POLKAVM_ALLOW_INSECURE", "1"), ("RUST_LOG", "debug")]
}

fn userfaultfd_available() -> bool {
	std::fs::read_to_string("/proc/sys/vm/unprivileged_userfaultfd")
		.is_ok_and(|value| value.trim() == "1")
}

/// The service this suite bootstraps, as `parachain-chain-spec` describes it: the service code
/// under [`PARACHAIN_SERVICE_ID`] and one AURA authorizer per para, built from the very blob and
/// config the collators derive their hash from. The hash each core's queue must hold comes back
/// from [`ParachainServiceSpec::authorizer_hashes`], so genesis cannot disagree with the service
/// or the collators.
///
/// Every para also registers its own `validation_code` — `validation_codes[i]`, the frozen copy
/// of the blob its chain spec was built from — and the genesis head derived from that chain
/// spec (its genesis header's hash, [`genesis_head_hash`]), so the service's `parent_head_hash`
/// check accepts the collators' first block.
pub fn parachain_service_spec(
	paras: &[Para],
	service_code: Vec<u8>,
	authorizer_code: Vec<u8>,
	validation_codes: &[Vec<u8>],
	heads: &[Vec<u8>],
) -> anyhow::Result<ParachainServiceSpec> {
	anyhow::ensure!(
		paras.len() == validation_codes.len(),
		"{} paras but {} validation codes: every para needs its own",
		paras.len(),
		validation_codes.len(),
	);
	anyhow::ensure!(
		paras.len() == heads.len(),
		"{} paras but {} genesis heads: every para needs the head of its own chain spec",
		paras.len(),
		heads.len(),
	);
	let mut spec = ParachainServiceSpec::new(PARACHAIN_SERVICE_ID, service_code)
		.balance(PARACHAIN_SERVICE_ENDOWMENT);
	for ((para, head), validation_code) in paras.iter().zip(heads).zip(validation_codes) {
		spec = spec.parachain(
			ParachainSpec::new(para.id.into())
				.head_data(head.clone())
				.validation_code(validation_code.clone())
				// The builder's own default is unlimited, stated here so a change to that
				// default cannot silently starve the para (§6.1 headroom check).
				.state_balance(u64::MAX)
				.authorizer(authorizer_code.clone(), &genesis::aura_config(para)?),
		);
	}
	Ok(spec)
}

/// The head JAM holds for a para whose SCALE-encoded genesis header is `header`: its hash.
pub fn genesis_head_hash(header: &[u8]) -> Vec<u8> {
	use sp_runtime::traits::{BlakeTwo256, Hash};
	BlakeTwo256::hash(header).as_ref().to_vec()
}

/// The SCALE-encoded genesis header of the parachain `spec` describes, exported by the very
/// binary the collators run (`export-genesis-head --chain <spec> -r`). JAM holds its hash
/// ([`genesis_head_hash`]), which the para's first block names as its parent.
pub fn export_genesis_head(omni_node: &Path, spec: &Path) -> anyhow::Result<Vec<u8>> {
	let output = Command::new(omni_node)
		.envs(polkavm_env())
		.args(["export-genesis-head", "--chain"])
		.arg(spec)
		.arg("-r")
		.output()
		.with_context(|| {
			format!(
				"running {} export-genesis-head --chain {}",
				omni_node.display(),
				spec.display()
			)
		})?;
	anyhow::ensure!(
		output.status.success(),
		"export-genesis-head failed ({}): {}",
		output.status,
		String::from_utf8_lossy(&output.stderr),
	);
	Ok(output.stdout)
}

/// Every para's core, paired with the authorizer hash its queue is filled with at genesis — read
/// out of the built spec's [`ParachainServiceSpec::authorizer_hashes`], so the queues can never
/// disagree with the service records or the collators.
///
/// A para with `also_cores` queues the same hash on each of them: an authorizer hash names a
/// para and its collator set, so one hash in two queues is one para on two cores. Two paras
/// sharing a core would leave the first one's authorizer overwritten, with a para that authors
/// and never accumulates as the only sign of it, so every core here must be distinct.
pub fn auth_queues(
	paras: &[Para],
	hashes: &BTreeMap<ParaId, AuthorizerHash>,
) -> anyhow::Result<Vec<(u16, String)>> {
	let wanted: Vec<(u32, u32)> = paras
		.iter()
		.flat_map(|para| {
			std::iter::once(para.core)
				.chain(para.also_cores.iter().copied())
				.map(|core| (para.id, core))
		})
		.collect();
	let mut queues = Vec::with_capacity(wanted.len());
	for para in paras {
		let hash = hashes
			.get(&para.id.into())
			.with_context(|| format!("the built spec has no authorizer for para {}", para.id))?;
		for core in std::iter::once(para.core).chain(para.also_cores.iter().copied()) {
			let core = u16::try_from(core)
				.with_context(|| format!("para {} names core {core}", para.id))?;
			anyhow::ensure!(
				queues.iter().all(|(taken, _)| *taken != core),
				"two paras want core {core}: {wanted:?}",
			);
			log::info!("para {} on core {core}, authorizer {}", para.id, genesis::hex(hash));
			queues.push((core, genesis::hex(hash)));
		}
	}
	Ok(queues)
}

/// The service id polkajam's bootstrap service runs under.
const BOOTSTRAP_SERVICE_ID: u32 = 0;

/// The bootstrap service as `gen-spec` reads it: its code, with the null authorizer as a
/// preimage so a spare core's packages can be authorized. `gen-spec` only installs the services
/// it is given, so without this there is no service 0 for `parasim-tool` to instruct.
fn bootstrap_service(blob: &Path, work_dir: &Path) -> anyhow::Result<serde_json::Value> {
	let code = copy_aside(blob, work_dir)?;
	let null_authorizer =
		write_sidecar(jam_null_authorizer_bin::BLOB, work_dir, "null-authorizer.jam")?;
	Ok(json!({
		"code": path_str(&code)?,
		"balance": json_balance(PARACHAIN_SERVICE_ENDOWMENT),
		"preimages": [path_str(&null_authorizer)?],
	}))
}

/// Every core of `cores` no para claims in `queues`, paired with the null authorizer.
///
/// `gen-spec` fills an unnamed core's queue with zero hashes, but an unassigned core is one
/// whose queue holds the null authorizer with an empty config: that is what `parasim-tool`'s
/// bootstrap lane looks for. Its assigner stays service 0, the `gen-spec` default.
pub fn spare_queues(queues: &[(u16, String)], cores: u16) -> Vec<(u16, String)> {
	let null = Authorizer {
		code_hash: jam_null_authorizer_bin::HASH.into(),
		config: AuthConfig::default(),
	}
	.hash(jam_std_common::hash_raw);
	(0..cores)
		.filter(|core| queues.iter().all(|(taken, _)| taken != core))
		.map(|core| (core, array_bytes::bytes2hex("", null.0)))
		.collect()
}

/// The genesis beyond the validator set, spelled as `gen-spec` reads it, from the built service:
/// its record counts `code` and `preimages` by file path — written from the built bytes by
/// [`write_sidecar`], since `gen-spec` reads them off disk — and `storage` as hex entries, which
/// have no file-path analogue. zombienet merges the object into the `jam_config.json` it
/// generates, knowing nothing about these keys.
pub fn built_genesis_overrides(
	queues: &[(u16, String)],
	service_id: u32,
	code_path: &str,
	balance: u64,
	preimage_paths: &[String],
	storage: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> serde_json::Value {
	let mut service = json!({
		"code": code_path,
		"balance": json_balance(balance),
		"preimages": preimage_paths,
	});
	if !storage.is_empty() {
		service["storage"] = json_storage(storage);
	}
	// Each core's queue is a list; this harness puts exactly one authorizer on a core.
	let auth_queues: serde_json::Map<String, serde_json::Value> =
		queues.iter().map(|(core, hash)| (core.to_string(), json!([hash]))).collect();
	let assigners: serde_json::Map<String, serde_json::Value> =
		queues.iter().map(|(core, _)| (core.to_string(), json!(service_id))).collect();
	let services: serde_json::Map<String, serde_json::Value> =
		[service_id.to_string()].into_iter().map(|id| (id, service.clone())).collect();
	json!({ "services": services, "auth_queues": auth_queues, "assigners": assigners })
}

/// Grants the service its five JAM privileges and the always-accumulate gas the housekeeping
/// phase needs. These are `ChainSpecConfig`-level, not `GenesisService`-level, so they go into
/// the overrides directly, not through the `parachain-chain-spec` builder.
///
/// `assign` covers every core in `queues`; `always_acc` is set to 10M to handle the worst-case
/// due-assign flush (341 cores × 80-hash queues, measured at ~9.94M gas — see PS `GENESIS.md`).
pub fn service_privileges(queues: &[(u16, String)], service_id: u32) -> serde_json::Value {
	let assign: serde_json::Map<String, serde_json::Value> =
		queues.iter().map(|(core, _)| (core.to_string(), json!(service_id))).collect();
	let mut always_acc = serde_json::Map::new();
	always_acc.insert(service_id.to_string(), json!(10_000_000u64));
	json!({
		"bless": service_id,
		"designate": service_id,
		"register": service_id,
		"assign": assign,
		"always_acc": always_acc,
	})
}

/// The built service's storage, spelled as `gen-spec` reads it: hex key, hex value.
pub fn json_storage(storage: &BTreeMap<Vec<u8>, Vec<u8>>) -> serde_json::Value {
	serde_json::Value::Object(
		storage
			.iter()
			.map(|(key, value)| {
				(
					array_bytes::bytes2hex("", &key[..]),
					json!(array_bytes::bytes2hex("", &value[..])),
				)
			})
			.collect(),
	)
}

/// A balance as the config takes it: a bare number while JSON carries it exactly, a decimal
/// string above 2^53 — `gen-spec` refuses a lossy number rather than rounding it.
pub fn json_balance(balance: u64) -> serde_json::Value {
	if balance <= 1 << 53 {
		json!(balance)
	} else {
		json!(balance.to_string())
	}
}

/// Copy a blob into the run's work dir and return the copy, which is what everything else names.
///
/// PVM builds are not byte-deterministic, so a rebuild in the source tree while a run is going
/// would leave an on-chain hash — a service's code, an authorizer's code — without a resolvable
/// preimage. The copy is also the run's record of what was actually put on the chain.
pub(crate) fn copy_aside(blob: &Path, work_dir: &Path) -> anyhow::Result<PathBuf> {
	let name = blob.file_name().with_context(|| format!("{} is not a file", blob.display()))?;
	let copy = work_dir.join(name);
	std::fs::copy(blob, &copy)
		.with_context(|| format!("copying {} to {}", blob.display(), copy.display()))?;
	Ok(copy)
}

/// Write bytes a service's genesis record names — `gen-spec` reads `code` and `preimages` off
/// disk — and return the file, which is the copy everything else refers to, like [`copy_aside`].
pub(crate) fn write_sidecar(bytes: &[u8], work_dir: &Path, name: &str) -> anyhow::Result<PathBuf> {
	let path = work_dir.join(name);
	std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
	Ok(path)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_genesis_head_hash_is_the_genesis_header_hash() {
		use codec::Encode;
		use sp_runtime::traits::{BlakeTwo256, Header as _};
		let header = sp_runtime::generic::Header::<u32, BlakeTwo256>::new(
			0,
			Default::default(),
			sp_core::H256::repeat_byte(0xbb),
			Default::default(),
			Default::default(),
		);
		assert_eq!(genesis_head_hash(&header.encode()), header.hash().as_ref());
	}
	use codec::DecodeAll;
	use parachain_service_core::{para_info_key, types::ParaId, ParaInfo};
	use serde_json::json;

	/// A para's chosen runtime is the blob its chain spec is built from *and* the bytes the
	/// service registers as its `validation_code`; a para that chose none keeps the run's default.
	/// Both seams are exercised together so the override cannot reach one and not the other.
	#[test]
	fn a_per_para_runtime_reaches_the_chain_spec_and_the_service_validation_code() {
		let root = tempfile::tempdir().expect("a temp dir; qed");
		let source = root.path().join("source");
		let work = root.path().join("work");
		std::fs::create_dir_all(&source).expect("create source; qed");
		std::fs::create_dir_all(&work).expect("create work; qed");

		let default_blob = source.join("default.polkavm");
		let override_blob = source.join("override.polkavm");
		std::fs::write(&default_blob, b"PVM\0default-runtime").expect("write; qed");
		std::fs::write(&override_blob, b"PVM\0override-runtime").expect("write; qed");

		let paras =
			vec![Para::single(1), Para::new(1, 1, &["bob"]).with_runtime(override_blob.clone())];

		let frozen = freeze_validation_codes(&paras, &default_blob, &work)
			.expect("both blobs are PolkaVM programs; qed");
		assert_eq!(frozen[0], work.join("default.polkavm"));
		assert_eq!(
			std::fs::read(&frozen[1]).expect("read; qed"),
			b"PVM\0override-runtime",
			"para 1's chain spec must be built from its own blob, not the run's default",
		);

		let codes = frozen
			.iter()
			.map(|blob| std::fs::read(blob).expect("read; qed"))
			.collect::<Vec<_>>();
		let spec = parachain_service_spec(
			&paras,
			b"service".to_vec(),
			b"authorizer".to_vec(),
			&codes,
			&[b"head 0".to_vec(), b"head 1".to_vec()],
		)
		.expect("a two-para spec builds; qed");
		let built = spec.build().expect("a two-para spec builds; qed");

		for (para, expected) in
			[(0u32, &b"PVM\0default-runtime"[..]), (1, b"PVM\0override-runtime")]
		{
			let stored = built
				.storage
				.get(&para_info_key(ParaId(para)))
				.expect("every para is registered; qed");
			let info = ParaInfo::decode_all(&mut &stored[..]).expect("decode ParaInfo; qed");
			let code = info.validation_code.expect("the para has a validation code; qed");
			assert_eq!(code.len, expected.len() as u32, "para {para} code length");
			assert_eq!(
				code.hash.0,
				jam_std_common::hash_raw(expected),
				"para {para} validation code",
			);
		}
	}

	/// The whole of what this harness knows about `gen-spec`'s config is these keys and shapes,
	/// asserted without any real blob. Anything more belongs to polkajam's `jam-chainspec`,
	/// which owns the schema.
	#[test]
	fn the_genesis_overrides_shape_is_what_build_jam_genesis_emits() {
		let queues = vec![(0u16, "aa".repeat(32)), (7, "bb".repeat(32))];
		let overrides = built_genesis_overrides(
			&queues,
			PARACHAIN_SERVICE_ID,
			"/run/parachain-service.jam",
			(1 << 53) + 1,
			&["/run/preimage-0.jam".to_string()],
			&BTreeMap::new(),
		);

		assert!(overrides["services"].is_object(), "services must be an object keyed by id");
		assert!(overrides["auth_queues"].is_object());
		assert!(overrides["assigners"].is_object());
		assert_eq!(
			overrides["services"][PARACHAIN_SERVICE_ID.to_string()]["balance"],
			json!("9007199254740993"),
		);
		assert_eq!(overrides["auth_queues"]["0"], json!(["aa".repeat(32)]));
		assert_eq!(overrides["auth_queues"]["7"], json!(["bb".repeat(32)]));
		assert_eq!(overrides["assigners"]["7"], json!(PARACHAIN_SERVICE_ID));
	}

	/// A core no para names gets the null authorizer, which is what `parasim-tool` recognises as
	/// unassigned; a zero hash would leave the bootstrap lane with no core to run on.
	#[test]
	fn spare_cores_hold_the_null_authorizer() {
		let queues = vec![(0u16, "aa".repeat(32))];
		let null = jam_std_common::hash_raw(&jam_null_authorizer_bin::HASH);
		assert_eq!(spare_queues(&queues, 3), [1u16, 2].map(|core| (core, hex(null))));
		assert!(spare_queues(&queues, 1).is_empty());

		fn hex(hash: [u8; 32]) -> String {
			array_bytes::bytes2hex("", hash)
		}
	}

	/// A custom core count has no named set upstream, so the override has to be the object form,
	/// and the validator count has to follow from the core count: `max_val_count()` keeps deriving
	/// `core_count * VALS_PER_CORE`.
	#[test]
	fn parameters_for_cores_widen_tiny_and_keep_the_validator_count_in_step() {
		let params = parameters_for_cores(3).expect("three cores must serialize; qed");
		assert!(params.is_object(), "a custom core count cannot be a named set");
		assert_eq!(params["core_count"], json!(3));
		let parsed: ProtocolParameters =
			serde_json::from_value(params).expect("the override round-trips; qed");
		assert_eq!(parsed.max_val_count(), 9);
		assert!(parameters_for_cores(0).is_err(), "a network with no cores is an error");
	}
}
