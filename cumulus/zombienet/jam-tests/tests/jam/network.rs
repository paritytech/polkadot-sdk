// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The JAM network the collators collate for: spawned by zombienet-sdk from a genesis that
//! already carries the parasim service, the paras' AURA authorizers and their cores.

use super::{chain_spec, collators::Para, env::Binaries, genesis, rpc::JamRpc};
use anyhow::Context;
use codec::DecodeAll;
use jam_cumulus_facade::service_state::{para_info_key, ParaInfo};
use jam_std_common::{ServiceKey, StorageKey};
use serde_json::json;
use sp_runtime::traits::BlakeTwo256;
use std::{
	path::{Path, PathBuf},
	process::Command,
	sync::atomic::{AtomicU16, Ordering},
};
use tokio::time::Instant;
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfigBuilder, NetworkConfigExt};

/// The tiny JAM protocol shape: six validators, and one ordinary node to serve RPC.
const VALIDATORS: usize = 6;
const ORDINARY_NODE: &str = "jam-or";

/// The service id parasim is created under at genesis. The network is freshly spawned and private
/// to one test, so a fixed id is always free — and it is the id the collators are started with
/// (`--jam-service-id`) and the one the authorizer config commits to.
pub const PARASIM_SERVICE_ID: u32 = 5;

/// The balance parasim is created with.
const PARASIM_ENDOWMENT: u64 = 1_000_000_000_000_000;

/// JAM RPC ports sit above the collator range and well away from the 19800 default, so a testnet
/// the user is running themselves is never disturbed. `JAM_TEST_JAM_RPC_PORT` moves the block for
/// one run, for when such a testnet holds the default.
static NEXT_JAM_RPC_PORT: std::sync::LazyLock<AtomicU16> = std::sync::LazyLock::new(|| {
	AtomicU16::new(super::env::port_base("JAM_TEST_JAM_RPC_PORT", 42000))
});

/// PolkaVM cannot use its recompiler in this sandbox (no userfaultfd), and the native provider
/// clears the environment before spawning, so every JAM node needs these explicitly.
///
/// `JAM_NODE_LOG` rides along as the nodes' `RUST_LOG`: a refine failure surfaces nowhere but
/// the guarantors' debug logs (the report says `Panic` and the service can log nothing), so a
/// stuck-head investigation has to be able to turn those on without editing this file.
fn polkavm_env() -> Vec<(&'static str, &'static str)> {
	let mut env = vec![("POLKAVM_BACKEND", "interpreter"), ("POLKAVM_ALLOW_INSECURE", "1")];
	if let Ok(level) = std::env::var("JAM_NODE_LOG") {
		// Leaked because zombienet's `EnvVar` converts from `&str` pairs only; once per process.
		env.push(("RUST_LOG", Box::leak(level.into_boxed_str())));
	}
	env
}

/// A running JAM network whose genesis already holds parasim, the paras' authorizers and the
/// cores that carry them.
pub struct JamNetwork {
	network: Network<LocalFileSystem>,
	/// The connection every state read goes down, kept for the run rather than reopened per read:
	/// the para head is polled every few seconds by every assertion there is.
	rpc: JamRpc,
	pub rpc_url: String,
	pub service_id: u32,
	/// The copy of the authorizer blob whose hash went into genesis. Everything that has to agree
	/// on the authorizer hash — the assigned cores, the collators — is pointed at this file.
	pub authorizer_blob: PathBuf,
	/// Where zombienet wrote the generated chain spec, named by whatever complains that genesis
	/// does not hold what it should.
	spec_path: PathBuf,
	/// The copy of the PVF blob genesis hosts, in a real-service run: the collators must stamp
	/// this very file's hash (`--jam-pvf-blob`). `None` is the parasim arrangement.
	pub pvf_blob: Option<PathBuf>,
}

/// What real-service mode seeds into genesis on top of the parasim-shaped base, so the real
/// parachain service accumulates for the paras without any registration flow: the PVF hosted as
/// a service preimage, and one pre-registered [`ParaInfo`] per para naming its genesis head and
/// that code.
struct ServiceSeed {
	/// The copy of the PVF blob genesis hosts.
	pvf_blob: PathBuf,
	/// Per para: its id, and the SCALE-encoded [`ParaInfo`] seeded under its storage key.
	para_infos: Vec<(u32, Vec<u8>)>,
}

/// Assemble the real-service seed: copy the PVF aside, hash the copy, and pre-register every para
/// with its chain spec's genesis head and that code reference.
fn service_seed(
	binaries: &Binaries,
	work_dir: &Path,
	paras: &[Para],
	pvf: &Path,
) -> anyhow::Result<ServiceSeed> {
	let pvf_blob = copy_aside(pvf, work_dir)?;
	let blob = std::fs::read(&pvf_blob)
		.with_context(|| format!("reading the PVF copy at {}", pvf_blob.display()))?;
	let code_hash = jam_std_common::hash_raw(&blob);
	let code_len = u32::try_from(blob.len()).context("a PVF cannot exceed 4 GiB")?;
	log::info!(
		"real-service mode: PVF {} ({code_len} bytes), code hash {}",
		pvf_blob.display(),
		array_bytes::bytes2hex("0x", code_hash),
	);

	let mut para_infos = Vec::with_capacity(paras.len());
	for para in paras {
		let spec = chain_spec::path(work_dir, para.id);
		let head = genesis::export_genesis_head(&binaries.omni_node, &spec)
			.with_context(|| format!("deriving para {}'s genesis head", para.id))?;
		let info = genesis::encode_para_info(&head, code_hash, code_len)
			.with_context(|| format!("encoding para {}'s ParaInfo", para.id))?;
		log::info!(
			"para {} pre-registered: {} bytes of genesis head, ParaInfo {} bytes under key {}",
			para.id,
			head.len(),
			info.len(),
			array_bytes::bytes2hex("0x", para_info_key(para.id.into())),
		);
		para_infos.push((para.id, info));
	}
	Ok(ServiceSeed { pvf_blob, para_infos })
}

impl JamNetwork {
	/// Spawn the network and wait for it to finalize a block.
	///
	/// Everything the collators need is in the chain spec: parasim as service
	/// [`PARASIM_SERVICE_ID`] hosting the AURA authorizer blob, and every para's core queued for
	/// that para's authorizer hash with parasim as its assigner. So there is no bootstrap phase —
	/// once a block is finalized the network is ready to be collated for.
	pub async fn spawn(
		binaries: &Binaries,
		work_dir: &Path,
		deadline: Instant,
		paras: &[Para],
	) -> anyhow::Result<Self> {
		let rpc_port = NEXT_JAM_RPC_PORT.fetch_add(1, Ordering::Relaxed);
		let base_dir = work_dir.join("zombienet");
		std::fs::create_dir_all(&base_dir)?;

		let jam_node = path_str(&binaries.jam_node)?;
		let relay_node = path_str(&binaries.relay_node)?;

		// The blobs go on chain by path, so the copies are what genesis names: a rebuild in the
		// source tree mid-run would otherwise leave the chain holding a hash of bytes that no
		// longer exist anywhere (see `copy_aside`).
		let parasim_blob = path_str(&copy_aside(&binaries.parasim_blob, work_dir)?)?;
		let authorizer_blob = copy_aside(&binaries.authorizer_blob, work_dir)?;
		let queues = auth_queues(paras, &authorizer_blob)?;
		let seed = match &binaries.pvf_blob {
			Some(pvf) => Some(service_seed(binaries, work_dir, paras, pvf)?),
			None => None,
		};
		let overrides =
			genesis_overrides(&queues, &parasim_blob, &path_str(&authorizer_blob)?, seed.as_ref())?;
		let genspec_node = binaries.genspec_node.as_deref().map(path_str).transpose()?;

		let config = NetworkConfigBuilder::new()
			// zombienet-sdk cannot yet express a network without a relay chain: NetworkSpec
			// unconditionally unwraps the relaychain config. One idle validator is the cheapest
			// way to satisfy it; nothing in the test uses it.
			.with_relaychain(|relay| {
				relay
					.with_chain("rococo-local")
					.with_default_command(relay_node.as_str())
					.with_validator(|node| node.with_name("relay-filler"))
			})
			.with_jamchain(|jam| {
				let jam = jam.with_id("dev").with_default_command(jam_node.as_str());
				// The spec is generated by whichever build understands the genesis keys, which
				// is not always the build that runs the nodes — see `Binaries::genspec_node`.
				let jam = match &genspec_node {
					Some(command) => jam.with_chain_spec_command(command.as_str()),
					None => jam,
				};
				let jam = jam.with_genesis_overrides(overrides);
				let jam = jam.with_validator(|node| node.with_name("jam0").with_env(polkavm_env()));
				let jam = (1..VALIDATORS).fold(jam, |jam, index| {
					jam.with_validator(|node| {
						node.with_name(&format!("jam{index}")).with_env(polkavm_env())
					})
				});
				// The RPC port is pinned because jam nodes never enter the `Network` handle, so
				// there is no `get_node("jam-or").ws_uri()` to read it back from.
				jam.with_ordinary(|node| {
					node.with_name(ORDINARY_NODE).with_rpc_port(rpc_port).with_env(polkavm_env())
				})
			})
			.with_global_settings(|settings| settings.with_base_dir(base_dir.clone()))
			.build()
			.map_err(|errors| {
				anyhow::anyhow!(
					"network config: {}",
					errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; ")
				)
			})?;

		let network = config.spawn_native().await.context("spawning the zombienet network")?;
		let rpc_url = format!("ws://127.0.0.1:{rpc_port}");
		log::info!("JAM network up, ordinary node RPC at {rpc_url}");

		let spec_path = base_dir.join("jam_spec.json");
		ensure_spec_holds_parasim(&spec_path, PARASIM_SERVICE_ID)?;
		if let Some(seed) = &seed {
			ensure_spec_holds_para_seed(&spec_path, PARASIM_SERVICE_ID, seed)?;
		}
		let rpc = JamRpc::wait_ready(&rpc_url, deadline)
			.await
			.with_context(|| format!("JAM node log tail:\n{}", log_tail(&network, 60)))?;

		let jam_network = JamNetwork {
			network,
			rpc,
			rpc_url,
			service_id: PARASIM_SERVICE_ID,
			authorizer_blob,
			spec_path,
			pvf_blob: seed.map(|seed| seed.pvf_blob),
		};
		jam_network.ensure_parasim_is_in_genesis().await?;

		Ok(jam_network)
	}

	/// Fail unless the chain the nodes actually started from is the one that was generated.
	///
	/// One read, not a poll: parasim is genesis state, so it is there in the first block or the
	/// nodes started on some other spec than the one just checked. That leaves every collator
	/// submitting packages nothing will authorize, so this has to be loud and name the files to
	/// look at.
	async fn ensure_parasim_is_in_genesis(&self) -> anyhow::Result<()> {
		let services = self.rpc.services().await.context("listing the chain's services")?;
		anyhow::ensure!(
			services.contains(&(self.service_id as u64)),
			"the chain has services {services:?}, which does not include parasim as service {}; \
			 the generated genesis is not what the nodes are running — see {} and the \
			 jam_config.json beside it",
			self.service_id,
			self.spec_path.display(),
		);
		log::info!("genesis holds parasim as service {}", self.service_id);
		Ok(())
	}

	/// Host the AURA authorizer blob in the bootstrap service too, for `parasim-tool`'s sake.
	///
	/// Only the two dynamic-core tests need this. Their `assign-core` / `free-core` commands are
	/// work packages `parasim-tool` builds with `auth_code_host: 0`, so a guarantor resolves the
	/// authorizer code out of service 0 — which genesis cannot be asked to host a preimage for.
	/// The collators name the parachain service instead and need none of it. Idempotent, and off
	/// everything's critical path: it can go as soon as `parasim-tool` names `--service` there.
	pub fn host_authorizer_for_control_packages(&self, tool: &Path) -> anyhow::Result<()> {
		run_step(
			&format!("deploy-authorizer: {}", self.authorizer_blob.display()),
			self.parasim_tool(tool).arg("deploy-authorizer"),
		)
	}

	/// Point `core`'s authorizer queue at `para`'s AURA authorizer, carried by `via`.
	///
	/// `via` names another para whose core carries the command. It is `None` whenever `core` can
	/// carry the command itself, which covers both of the cases the tests use: a core parked by
	/// [`Self::free_core`], which still runs this para's own authorizer code, and a core that was
	/// never assigned to a para and so still holds the null authorizer genesis left on it.
	///
	/// Which lane the command travels is `parasim-tool`'s business, not this caller's: it reads
	/// who holds the core's assigner privilege, and — for the control lane — whether the carrier
	/// is parked or running the named para, checking either way that what it builds matches the
	/// hash the carrier core actually holds. It returns only once the core's *pool* holds the new
	/// authorizer, so afterwards the core really can carry the para's packages.
	pub fn assign_core(
		&self,
		tool: &Path,
		para: &Para,
		core: u32,
		via: Option<&Para>,
	) -> anyhow::Result<()> {
		let names = para.collator_names();
		// With no carrier, `--via-core` is left off rather than filled in: the tool defaults it to
		// the core being assigned, which is exactly what is wanted, and that is not this para's
		// own core — the reassignment test assigns core 1 to a para sitting on core 0.
		let carrier = via.unwrap_or(para);
		let mut command = self.parasim_tool(tool);
		command
			.args(["--collators", &names])
			.args(["assign-core", &para.id.to_string(), &core.to_string()])
			.args(["--via-para", &carrier.id.to_string()])
			.args(["--via-collators", &carrier.collator_names()]);
		if let Some(via) = via {
			command.args(["--via-core", &via.core.to_string()]);
		}
		run_step(
			&format!(
				"assign-core: para {} onto core {core} for {names}, carried on core {}",
				para.id,
				via.map_or(core, |via| via.core)
			),
			&mut command,
		)
	}

	/// Park `core`: keep the AURA authorizer on it under a config naming no para, so its pool
	/// drains over the next few blocks and it stops carrying `para`'s work.
	///
	/// Only a core parasim was granted can be parked this way, and the command rides the core
	/// itself: it is a control package under the AURA authorizer that is about to go away, signed
	/// by `para`'s own collator set. Returns once the pool holds the parked authorizer, which is
	/// the moment the drain of the old one starts being visible.
	///
	/// Parked is not unassigned. The core keeps the same authorizer code, so it keeps taking
	/// control packages — which is what leaves [`Self::assign_core`] able to put a para back on it
	/// without a second core to carry the command.
	pub fn free_core(&self, tool: &Path, para: &Para, core: u32) -> anyhow::Result<()> {
		let names = para.collator_names();
		run_step(
			&format!("free-core: core {core}, carried under para {}'s authorizer", para.id),
			self.parasim_tool(tool)
				.args(["--collators", &names])
				.args(["free-core", &core.to_string()])
				.args(["--via-para", &para.id.to_string()]),
		)
	}

	/// The parachain head parasim has accumulated for `para`, or `None` while it has none.
	///
	/// This is the completion signal of the whole pipeline: JAM emits no "accumulated" event, so a
	/// para head that moves is the only proof that a work package was guaranteed, reported and
	/// accumulated. Reading it out of service storage rather than out of a collator's log is what
	/// makes an assertion about it an assertion about the chain.
	///
	/// The read is the collator's own: `serviceValue` at the best block, under the key the
	/// parachain service files a para's [`ParaInfo`] at.
	pub async fn para_head(&self, para: u32) -> anyhow::Result<Option<ParaHead>> {
		let key = para_info_key(para.into());
		let at = self.rpc.best_block_hash().await?;

		let started = std::time::Instant::now();
		let stored = self.rpc.service_value(&at, self.service_id, &key).await?;
		let elapsed = started.elapsed();

		let head = stored
			.as_deref()
			.map(decode_para_head)
			.transpose()
			.with_context(|| format!("para {para}'s entry at block {at}"))?;
		log::info!(
			"serviceValue(service {}, para {para}) at block {at}: {} in {elapsed:?}",
			self.service_id,
			match &head {
				Some(head) => format!("head {head}"),
				None => "no entry".to_string(),
			},
		);
		Ok(head)
	}

	/// A `parasim-tool` invocation carrying the arguments every phase-6 command needs.
	///
	/// `--authorizer-blob`, `--collators` and `--scheme` are what an AURA authorizer hash is built
	/// from, so they have to be exactly what the para's collators are started with. A mismatch
	/// installs a hash nobody will ever satisfy, and the only symptom is a core that authorizes
	/// nothing.
	///
	/// `--scheme` is spelled out even though sr25519 is the tool's default: it is the parachain
	/// template runtime's `AuraId`, and a default that moves in the other repo would silently
	/// point every core here at the wrong verifier blob.
	fn parasim_tool(&self, tool: &Path) -> Command {
		let mut command = Command::new(tool);
		command
			.args(["--rpc", &self.rpc_url])
			.args(["--service", &self.service_id.to_string()])
			.args(["--scheme", "sr25519"])
			.arg("--authorizer-blob")
			.arg(&self.authorizer_blob)
			.envs(polkavm_env());
		command
	}

	pub fn ordinary_node_log_tail(&self, lines: usize) -> String {
		log_tail(&self.network, lines)
	}

	/// Stop every JAM node. Dropping the network does the same via `kill_on_drop`, which is what
	/// covers a panicking test; this is the tidy path.
	pub async fn shutdown(self) {
		if let Err(error) = self.network.destroy().await {
			log::warn!("tearing down the JAM network failed: {error}");
		}
	}
}

/// The tail of the ordinary node's log, for failure diagnostics.
///
/// A free function because the first thing it is needed for is a network that has not finished
/// coming up, and so has no [`JamNetwork`] around it yet.
fn log_tail(network: &Network<LocalFileSystem>, lines: usize) -> String {
	let Some(base) = network.base_dir() else {
		return "(the network has no base dir, so its logs cannot be located)".to_string();
	};
	let path = Path::new(base).join(ORDINARY_NODE).join(format!("{ORDINARY_NODE}.log"));
	format!(
		"----- JAM node {ORDINARY_NODE} ({}) -----\n{}",
		path.display(),
		super::collators::tail(&path, lines)
	)
}

/// Fail unless the chain spec `gen-spec` wrote holds parasim's service record.
///
/// Checked on the file, before a single RPC: a `gen-spec` that does not know the genesis keys
/// drops them without a word, and the spec it writes is the first place that shows. Waiting for
/// the nodes first would report the same thing minutes later.
fn ensure_spec_holds_parasim(spec_path: &Path, service_id: u32) -> anyhow::Result<()> {
	let spec: serde_json::Value = serde_json::from_slice(
		&std::fs::read(spec_path).with_context(|| format!("reading {}", spec_path.display()))?,
	)
	.with_context(|| format!("parsing {}", spec_path.display()))?;
	let key = service_record_key(service_id);
	anyhow::ensure!(
		spec["genesis_state"].get(&key).is_some(),
		"{} holds no record of service {service_id} (genesis_state key {key}): the polkajam that \
		 ran gen-spec ignores the genesis keys; set JAM_GENSPEC_BIN to a build from the \
		 mku-genspec branch",
		spec_path.display(),
	);
	log::info!("{} holds parasim as service {service_id}", spec_path.display());
	Ok(())
}

/// Fail unless the chain spec holds every seeded para registration, byte for byte.
///
/// Same moment and same reason as [`ensure_spec_holds_parasim`]: a `gen-spec` build that predates
/// the `storage` genesis key would drop the registrations without a word, and the only later
/// symptom would be a service silently ignoring every candidate.
fn ensure_spec_holds_para_seed(
	spec_path: &Path,
	service_id: u32,
	seed: &ServiceSeed,
) -> anyhow::Result<()> {
	let spec: serde_json::Value = serde_json::from_slice(
		&std::fs::read(spec_path).with_context(|| format!("reading {}", spec_path.display()))?,
	)
	.with_context(|| format!("parsing {}", spec_path.display()))?;
	for (para, info) in &seed.para_infos {
		let key = para_info_state_key(service_id, *para);
		let expected = serde_json::Value::from(array_bytes::bytes2hex("", info));
		let stored = spec["genesis_state"].get(&key);
		anyhow::ensure!(
			stored == Some(&expected),
			"{} does not hold para {para}'s seeded ParaInfo at genesis_state key {key}: found \
			 {stored:?}; the polkajam that ran gen-spec ignores the 'storage' genesis key — set \
			 JAM_GENSPEC_BIN to a build that reads it",
			spec_path.display(),
		);
		log::info!(
			"{} holds para {para}'s seeded ParaInfo at genesis_state key {key}",
			spec_path.display(),
		);
	}
	Ok(())
}

/// The `genesis_state` key of a para's [`ParaInfo`] entry, as `gen-spec` derives it: JAM's
/// `ServiceKey::Value` over the service's raw storage key, spelled bare-hex.
fn para_info_state_key(service_id: u32, para: u32) -> String {
	let key = para_info_key(para.into());
	let state_key = StorageKey::from(ServiceKey::Value { id: service_id, key: &key });
	array_bytes::bytes2hex("", state_key.0)
}

/// Every para's core, paired with the authorizer hash its queue is filled with at genesis.
///
/// One core each, and a real one: two paras sharing a core would leave the first one's authorizer
/// overwritten, with a para that authors and never accumulates as the only sign of it.
fn auth_queues(paras: &[Para], authorizer_blob: &Path) -> anyhow::Result<Vec<(u16, String)>> {
	let mut queues = Vec::with_capacity(paras.len());
	for para in paras {
		let hash = genesis::authorizer_hash(para, authorizer_blob)
			.with_context(|| format!("deriving para {}'s authorizer hash", para.id))?;
		let core = u16::try_from(para.core)
			.with_context(|| format!("para {} names core {}", para.id, para.core))?;
		anyhow::ensure!(
			queues.iter().all(|(taken, _)| *taken != core),
			"two paras want core {core}: {:?}",
			paras.iter().map(|para| (para.id, para.core)).collect::<Vec<_>>(),
		);
		log::info!("para {} on core {core}, authorizer {}", para.id, genesis::hex(&hash));
		queues.push((core, genesis::hex(&hash)));
	}
	Ok(queues)
}

/// The genesis beyond the validator set, spelled as `gen-spec` reads it. zombienet merges it into
/// the `jam_config.json` it generates, knowing nothing about these keys.
///
/// Parasim is created as service [`PARASIM_SERVICE_ID`] from `parasim_blob` and hosts
/// `authorizer_blob`, which is where a guarantor resolves the authorizer code a collator's package
/// names. Every core in `queues` is filled with its para's authorizer hash, so nothing has to be
/// assigned once the network is up, and parasim is made those cores' assigner because the
/// dynamic-core tests move them mid-run.
///
/// A `seed` — real-service mode — additionally hosts the PVF beside the authorizer and writes
/// every para's [`ParaInfo`] into the service's storage, both spelled as bare hex the way
/// `gen-spec`'s `storage` object takes them.
fn genesis_overrides(
	queues: &[(u16, String)],
	parasim_blob: &str,
	authorizer_blob: &str,
	seed: Option<&ServiceSeed>,
) -> anyhow::Result<serde_json::Value> {
	let auth_queues: serde_json::Map<String, serde_json::Value> =
		queues.iter().map(|(core, hash)| (core.to_string(), json!(hash))).collect();
	let assigners: serde_json::Map<String, serde_json::Value> = queues
		.iter()
		.map(|(core, _)| (core.to_string(), json!(PARASIM_SERVICE_ID)))
		.collect();
	let mut overrides = json!({
		"services": [{
			"id": PARASIM_SERVICE_ID,
			"code": parasim_blob,
			"balance": json_balance(PARASIM_ENDOWMENT),
			"preimages": [authorizer_blob],
		}],
		"auth_queues": auth_queues,
		"assigners": assigners,
	});

	if let Some(seed) = seed {
		let service = &mut overrides["services"][0];
		service["preimages"]
			.as_array_mut()
			.expect("spelled as an array just above; qed")
			.push(json!(path_str(&seed.pvf_blob)?));
		service["storage"] = seed
			.para_infos
			.iter()
			.map(|(para, info)| {
				(
					array_bytes::bytes2hex("", para_info_key((*para).into())),
					json!(array_bytes::bytes2hex("", info)),
				)
			})
			.collect::<serde_json::Map<_, _>>()
			.into();
	}
	Ok(overrides)
}

/// A balance as the config takes it: a bare number while JSON carries it exactly, a decimal
/// string above 2^53 — `gen-spec` refuses a lossy number rather than rounding it.
fn json_balance(balance: u64) -> serde_json::Value {
	if balance <= 1 << 53 {
		json!(balance)
	} else {
		json!(balance.to_string())
	}
}

/// The `genesis_state` key of service `id`'s record, as `gen-spec` spells it: JAM's
/// `ServiceKey::Info` — `ff`, then the id's four little-endian bytes each followed by a zero —
/// padded to the 31-byte state key.
fn service_record_key(id: u32) -> String {
	let mut key = [0u8; 31];
	key[0] = 0xff;
	for (index, byte) in id.to_le_bytes().into_iter().enumerate() {
		key[1 + 2 * index] = byte;
	}
	array_bytes::bytes2hex("", key)
}

fn path_str(path: &Path) -> anyhow::Result<String> {
	path.to_str().map(str::to_string).with_context(|| format!("{} is not utf-8", path.display()))
}

/// Copy a blob into the run's work dir and return the copy, which is what everything else names.
///
/// PVM builds are not byte-deterministic, so a rebuild in the source tree while a run is going
/// would leave an on-chain hash — a service's code, an authorizer's code — without a resolvable
/// preimage. The copy is also the run's record of what was actually put on the chain.
fn copy_aside(blob: &Path, work_dir: &Path) -> anyhow::Result<PathBuf> {
	let name = blob.file_name().with_context(|| format!("{} is not a file", blob.display()))?;
	let copy = work_dir.join(name);
	std::fs::copy(blob, &copy)
		.with_context(|| format!("copying {} to {}", blob.display(), copy.display()))?;
	Ok(copy)
}

/// Run one setup step, saying what it is about to submit and what came back.
///
/// Every step here changes JAM state through a work package, and a package JAM refuses leaves no
/// trace on the chain — so the command line, the exit status, the output and the wall clock are
/// all part of the record. Both tools read the state back themselves and exit non-zero when the
/// change did not land, which is why a bad status is fatal rather than a warning.
fn run_step(what: &str, command: &mut Command) -> anyhow::Result<()> {
	log::info!("{what}: running {command:?}");
	let started = std::time::Instant::now();
	let stdout = capture_step(what, command)?;
	log::info!("{what}: ok in {:?}\n{stdout}", started.elapsed());
	Ok(())
}

/// Run one step and hand back its stdout, for the reads whose value the caller parses.
///
/// Quieter than [`run_step`] because a state read happens on a poll loop: the caller logs the
/// value it extracted instead, and the full transcript stays at `debug`. A non-zero exit is still
/// fatal, and still carries the whole transcript.
fn capture_step(what: &str, command: &mut Command) -> anyhow::Result<String> {
	let started = std::time::Instant::now();
	let output = command.output().with_context(|| format!("{what}: running {command:?}"))?;
	let elapsed = started.elapsed();
	let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
	let stderr = String::from_utf8_lossy(&output.stderr);

	anyhow::ensure!(
		output.status.success(),
		"{what} failed ({}) after {elapsed:?}:\n{stdout}{stderr}",
		output.status,
	);
	if !stderr.trim().is_empty() {
		log::info!("{what}: stderr\n{stderr}");
	}
	log::debug!("{what}: ok in {elapsed:?}\n{stdout}");
	Ok(stdout)
}

/// A parachain head as JAM has accumulated it: the tip parasim believes the chain has reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParaHead {
	pub number: u64,
	/// The block hash, `0x`-prefixed, in the same spelling a collator's RPC uses.
	pub hash: String,
}

impl std::fmt::Display for ParaHead {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(formatter, "#{} {}", self.number, self.hash)
	}
}

/// The parachain header type, which is the parachain template runtime's `Header`.
type ParaHeader = sp_runtime::generic::Header<u32, BlakeTwo256>;

/// Read the accumulated head out of a para's stored [`ParaInfo`].
///
/// Both decodes go through the real types — the service's own `ParaInfo`, and the header type the
/// runtime the collators run defines — so no layout is spelled out here to drift out of step with
/// either. A value that does not decode is an error rather than "no head": the two mean opposite
/// things to a stall assertion, so a layout that has moved on has to say so loudly.
fn decode_para_head(stored: &[u8]) -> anyhow::Result<ParaHead> {
	let info = ParaInfo::decode_all(&mut &stored[..])
		.with_context(|| format!("decoding {} bytes as the service's ParaInfo", stored.len()))?;
	let head = info.head_data.into_inner();
	let header = ParaHeader::decode_all(&mut &head[..]).with_context(|| {
		format!("decoding ParaInfo's {} bytes of head_data as a substrate header", head.len())
	})?;

	Ok(ParaHead {
		number: header.number.into(),
		hash: array_bytes::bytes2hex("0x", header.hash()),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;

	/// The whole of what this harness knows about `gen-spec`'s config is these keys and shapes.
	/// Anything more belongs to polkajam's `jam-chainspec`, which owns the schema.
	#[test]
	fn the_override_spells_exactly_the_keys_gen_spec_reads() {
		let queues = vec![(0u16, "aa".repeat(32)), (1, "bb".repeat(32))];

		let overrides = genesis_overrides(
			&queues,
			"/run/parasim-service.jam",
			"/run/parachain-authorizer-sr25519.jam",
			None,
		)
		.expect("nothing to fail without a seed");

		assert_eq!(
			overrides,
			json!({
				"services": [{
					"id": 5,
					"code": "/run/parasim-service.jam",
					"balance": 1_000_000_000_000_000u64,
					"preimages": ["/run/parachain-authorizer-sr25519.jam"],
				}],
				"auth_queues": { "0": "aa".repeat(32), "1": "bb".repeat(32) },
				"assigners": { "0": 5, "1": 5 },
			})
		);
	}

	/// Real-service mode adds exactly two things, both under the service: the PVF as a second
	/// hosted preimage, and one `storage` entry per para — the raw service key and the raw
	/// `ParaInfo`, spelled bare-hex the way `gen-spec` reads them. Everything else must stay
	/// byte-identical to the parasim shape, because it is the same service record.
	#[test]
	fn a_seed_adds_the_pvf_preimage_and_the_para_registrations() {
		let queues = vec![(0u16, "aa".repeat(32))];
		let seed = ServiceSeed {
			pvf_blob: "/run/frameless.polkavm".into(),
			para_infos: vec![(0, vec![0xde, 0xad]), (7, vec![0xbe, 0xef])],
		};

		let overrides = genesis_overrides(
			&queues,
			"/run/parachain-service.jam",
			"/run/parachain-authorizer-sr25519.jam",
			Some(&seed),
		)
		.expect("the paths are utf-8");

		assert_eq!(
			overrides,
			json!({
				"services": [{
					"id": 5,
					"code": "/run/parachain-service.jam",
					"balance": 1_000_000_000_000_000u64,
					"preimages": [
						"/run/parachain-authorizer-sr25519.jam",
						"/run/frameless.polkavm",
					],
					"storage": {
						"0000000000": "dead",
						"0007000000": "beef",
					},
				}],
				"auth_queues": { "0": "aa".repeat(32) },
				"assigners": { "0": 5 },
			})
		);
	}

	/// Hand-woven from JAM's `ServiceKey::Value` layout — blake2b over `ffffffff ‖ raw key`, the
	/// id's and the hash's bytes interleaved — so the seed check looks for the key `gen-spec`
	/// really writes and not merely for one this code computes.
	#[test]
	fn the_para_info_state_key_is_the_interleaved_service_value_key() {
		let raw_key = para_info_key(0.into());
		assert_eq!(raw_key, vec![0u8; 5]);
		let hash = jam_std_common::hash_raw(&[&[0xff, 0xff, 0xff, 0xff], &raw_key[..]].concat());

		let id = 5u32.to_le_bytes();
		let mut expected = [0u8; 31];
		expected[..8]
			.copy_from_slice(&[id[0], hash[0], id[1], hash[1], id[2], hash[2], id[3], hash[3]]);
		expected[8..].copy_from_slice(&hash[4..27]);

		assert_eq!(para_info_state_key(5, 0), array_bytes::bytes2hex("", expected));
	}

	/// JSON numbers are exact only up to 2^53, and `gen-spec` refuses a lossy one rather than
	/// rounding it, so anything bigger has to travel as a decimal string.
	#[test]
	fn a_balance_beyond_2_to_the_53_is_written_as_a_decimal_string() {
		assert_eq!(json_balance(PARASIM_ENDOWMENT), json!(1_000_000_000_000_000u64));
		assert_eq!(json_balance(1 << 53), json!(9_007_199_254_740_992u64));
		assert_eq!(json_balance((1 << 53) + 1), json!("9007199254740993"));
		assert_eq!(json_balance(u64::MAX), json!("18446744073709551615"));
	}

	/// Hand-written from JAM's `ServiceKey::Info` layout, so the early spec check looks for the
	/// key `gen-spec` really writes and not merely for one this code computes.
	#[test]
	fn the_service_record_key_is_ff_and_the_id_interleaved_with_zeros() {
		assert_eq!(service_record_key(5), format!("ff05000000000000{}", "00".repeat(23)));
		assert_eq!(service_record_key(0x0403_0201), format!("ff01000200030004{}", "00".repeat(23)));
	}

	/// A header of the kind a collator files as its para head.
	fn header(number: u32) -> ParaHeader {
		ParaHeader {
			parent_hash: sp_core::H256::repeat_byte(0xaa),
			number,
			state_root: sp_core::H256::repeat_byte(0xbb),
			extrinsics_root: sp_core::H256::repeat_byte(0xcc),
			digest: Default::default(),
		}
	}

	/// The bytes the parachain service files under a para's key, given its `head_data`.
	fn stored_entry(head_data: Vec<u8>) -> Vec<u8> {
		ParaInfo {
			head_data: head_data.try_into().expect("the head fits in HeadData; qed"),
			validation_code: None,
			pending_upgrade: None,
			total_state_balance: 0,
			used_state_balance: 0,
			is_deregistering: false,
		}
		.encode()
	}

	/// The head arrives wrapped in `ParaInfo`, so the two decodes have to compose. Both fields are
	/// asserted because a header read at the wrong offset would still yield *some* number and
	/// *some* hash, and every phase assertion in this suite is a comparison of those.
	#[test]
	fn the_accumulated_head_is_the_header_in_para_infos_head_data() {
		let header = header(17);

		let head = decode_para_head(&stored_entry(header.encode())).expect("the entry decodes");

		assert_eq!(head.number, 17);
		// The collator's RPC is handed this string verbatim, and substrate reads a block hash as
		// `0x` and 32 bytes of hex.
		assert_eq!(head.hash, array_bytes::bytes2hex("0x", header.hash()));
		assert_eq!(head.hash.len(), 2 + 64);
	}

	#[test]
	fn a_head_of_zero_is_a_real_head() {
		// Height zero is a real head — the genesis one — so "nothing accumulated yet" has to come
		// from the para having no entry at all, never from its number, or a stall would read as
		// progress.
		let stored = stored_entry(header(0).encode());
		assert_eq!(decode_para_head(&stored).expect("the entry decodes").number, 0);
	}

	#[test]
	fn an_entry_that_is_not_a_para_info_is_an_error() {
		assert!(decode_para_head(&[0xff; 8]).is_err());
	}

	#[test]
	fn a_head_that_is_not_a_substrate_header_is_an_error() {
		assert!(decode_para_head(&stored_entry(vec![0xff; 8])).is_err());
	}
}
