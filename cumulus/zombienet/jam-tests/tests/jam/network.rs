// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The JAM network the collators collate for: spawned by zombienet-sdk from a genesis that
//! already carries the real parachain service, the paras' AURA authorizers and their cores.

use super::{chain_spec, collators::Para, env::Binaries, genesis, rpc::JamRpc};
use anyhow::Context;
use codec::DecodeAll;
use jam_cumulus_facade::{
	authorizer::AuthorizerHash,
	service_state::{para_info_key, storage_key, ParaInfo, Tag},
	ParaId,
};
use parachain_chain_spec::{ParachainServiceSpec, ParachainSpec};
use serde_json::json;
use sp_runtime::traits::BlakeTwo256;
use std::{
	collections::BTreeMap,
	path::{Path, PathBuf},
	process::Command,
	sync::atomic::{AtomicU16, Ordering},
};
use tokio::time::Instant;
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfigBuilder, NetworkConfigExt};

/// The tiny JAM protocol shape: six validators, and one ordinary node to serve RPC.
const VALIDATORS: usize = 6;
const ORDINARY_NODE: &str = "jam-or";

/// The service id the genesis creates the parachain service under. The network is freshly spawned
/// and private to one test, so a fixed id is always free — and it is the id the collators are
/// started with (`--jam-service-id`) and the one the authorizer config commits to.
pub const PARACHAIN_SERVICE_ID: u32 = 5;

/// The balance the service is created with.
const PARACHAIN_SERVICE_ENDOWMENT: u64 = 1_000_000_000_000_000;

/// JAM RPC ports sit above the collator range and well away from the 19800 default, so a testnet
/// the user is running themselves is never disturbed.
static NEXT_JAM_RPC_PORT: AtomicU16 = AtomicU16::new(42000);

/// PolkaVM cannot use its recompiler in this sandbox (no userfaultfd), and the native provider
/// clears the environment before spawning, so every JAM node needs these explicitly. The last
/// one turns on the PolkaVM executor everywhere, which both a JAM chain spec built from a
/// PolkaVM runtime blob and the collators — whose `:code` is that same blob — need to
/// construct the runtime at all. The collators get the same environment here, from `spawn` in
/// `collators.rs`.
pub fn polkavm_env() -> Vec<(&'static str, &'static str)> {
	vec![
		("POLKAVM_BACKEND", "interpreter"),
		("POLKAVM_ALLOW_INSECURE", "1"),
		("SUBSTRATE_ENABLE_POLKAVM", "1"),
		("RUST_LOG", "debug"),
	]
}

/// `--telemetry <HOST:PORT>` for every JAM node when `JAM_TELEMETRY` names an endpoint, so a run
/// can be watched in TART (`https://github.com/paritytech/jamtart`, JIP-3 over TCP, default port
/// 9000). Empty otherwise: polkajam dials the endpoint at start-up and logs a reconnect error
/// every few seconds when nothing is listening, so this stays opt-in.
pub fn telemetry_args() -> Vec<zombienet_sdk::Arg> {
	match std::env::var("JAM_TELEMETRY") {
		Ok(endpoint) if !endpoint.trim().is_empty() => {
			vec![zombienet_sdk::Arg::Option("--telemetry".into(), endpoint)]
		},
		_ => Vec::new(),
	}
}

/// A running JAM network whose genesis already holds the real parachain service, the paras'
/// authorizers and the cores that carry them.
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
	/// Each para's chain spec, in para order: built once here so each para's genesis head can be
	/// derived from the very file its collators run.
	pub para_specs: Vec<PathBuf>,
	/// Where zombienet wrote the generated chain spec, named by whatever complains that genesis
	/// does not hold what it should.
	spec_path: PathBuf,
}

impl JamNetwork {
	/// Spawn the network and wait for it to finalize a block.
	///
	/// Everything the collators need is in the chain spec: the real parachain service at
	/// [`PARACHAIN_SERVICE_ID`] hosting the AURA authorizer blob, and every para's core queued for
	/// that para's authorizer hash with the service as its assigner. So there is no bootstrap
	/// phase — once a block is finalized the network is ready to be collated for.
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

		// The genesis is built by `parachain-chain-spec`, not spelled by hand: one AURA
		// authorizer per para (its hash filling the core's queue and its blob hosted as a
		// preimage) and the paras' records as storage. `gen-spec` reads `code` and `preimages`
		// off disk, so the built bytes are written into the work dir — the same freeze that kept
		// the source blobs from being rebuilt mid-run (see `copy_aside`).
		let service_blob = copy_aside(&binaries.parachain_service_blob, work_dir)?;
		let authorizer_blob = copy_aside(&binaries.authorizer_blob, work_dir)?;
		let runtime_blob = copy_aside(&binaries.runtime_wasm, work_dir)?;

		// Each para's chain spec is built here, before the genesis: the para's genesis head is
		// derived from it, and the collators run the very same file (`para_specs`).
		let para_specs = paras
			.iter()
			.map(|para| {
				let spec = work_dir.join(format!("jam-parachain-{}-spec.json", para.id));
				chain_spec::build(
					&binaries.omni_node,
					&runtime_blob,
					&spec,
					para.id,
					&para.collators,
				)?;
				Ok(spec)
			})
			.collect::<anyhow::Result<Vec<_>>>()?;
		let heads = para_specs
			.iter()
			.map(|spec| export_genesis_head(&binaries.omni_node, spec))
			.collect::<anyhow::Result<Vec<_>>>()?;
		let spec = parachain_service_spec(
			paras,
			std::fs::read(&service_blob)
				.with_context(|| format!("reading {}", service_blob.display()))?,
			std::fs::read(&authorizer_blob)
				.with_context(|| format!("reading {}", authorizer_blob.display()))?,
			std::fs::read(&runtime_blob)
				.with_context(|| format!("reading {}", runtime_blob.display()))?,
			&heads,
		)?;
		let queues = auth_queues(paras, &spec.authorizer_hashes())?;
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
		overrides["privileges"] = service_privileges(&queues, PARACHAIN_SERVICE_ID);
		let genspec_node = binaries.genspec_node.as_deref().map(path_str).transpose()?;

		// No relaychain: the orchestrator takes its JAM spawn path only for a network without
		// one, so a filler validator here would shadow the jamchain entirely.
		let config = NetworkConfigBuilder::new()
			.with_jamchain(|jam| {
				let jam = jam.with_id("dev").with_default_command(jam_node.as_str());
				// The spec is generated by whichever build understands the genesis keys, which
				// is not always the build that runs the nodes — see `Binaries::genspec_node`.
				let jam = match &genspec_node {
					Some(command) => jam.with_chain_spec_command(command.as_str()),
					None => jam,
				};
				let jam = jam.with_genesis_overrides(overrides);
				let jam = jam.with_validator(|node| {
					node.with_name("jam0").with_env(polkavm_env()).with_args(telemetry_args())
				});
				let jam = (1..VALIDATORS).fold(jam, |jam, index| {
					jam.with_validator(|node| {
						node.with_name(&format!("jam{index}"))
							.with_env(polkavm_env())
							.with_args(telemetry_args())
					})
				});
				// The RPC port is pinned because jam nodes never enter the `Network` handle, so
				// there is no `get_node("jam-or").ws_uri()` to read it back from.
				jam.with_ordinary(|node| {
					node.with_name(ORDINARY_NODE)
						.with_rpc_port(rpc_port)
						.with_env(polkavm_env())
						.with_args(telemetry_args())
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
		ensure_spec_holds_service(&spec_path, PARACHAIN_SERVICE_ID)?;
		let rpc = JamRpc::wait_ready(&rpc_url, deadline)
			.await
			.with_context(|| format!("JAM node log tail:\n{}", log_tail(&network, 60)))?;

		let jam_network = JamNetwork {
			network,
			rpc,
			rpc_url,
			service_id: PARACHAIN_SERVICE_ID,
			authorizer_blob,
			para_specs,
			spec_path,
		};
		jam_network.ensure_service_is_in_genesis().await?;

		Ok(jam_network)
	}

	/// Fail unless the chain the nodes actually started from is the one that was generated.
	///
	/// One read, not a poll: the parachain service is genesis state, so it is there in the first
	/// block or the nodes started on some other spec than the one just checked. That leaves every
	/// collator submitting packages nothing will authorize, so this has to be loud and name the
	/// files to look at.
	async fn ensure_service_is_in_genesis(&self) -> anyhow::Result<()> {
		let services = self.rpc.services().await.context("listing the chain's services")?;
		anyhow::ensure!(
			services.contains(&(self.service_id as u64)),
			"the chain has services {services:?}, which does not include the parachain service as service {}; \
			 the generated genesis is not what the nodes are running — see {} and the \
			 jam_config.json beside it",
			self.service_id,
			self.spec_path.display(),
		);
		log::info!("genesis holds the parachain service as service {}", self.service_id);
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
	/// Only a core the parachain service was granted can be parked this way, and the command rides
	/// the core itself: it is a control package under the AURA authorizer that is about to go
	/// away, signed by `para`'s own collator set. Returns once the pool holds the parked
	/// authorizer, which is the moment the drain of the old one starts being visible.
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

	/// The parachain head the service has accumulated for `para`, or `None` while it has none.
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

/// Fail unless the chain spec `gen-spec` wrote holds the parachain service's record.
///
/// Checked on the file, before a single RPC: a `gen-spec` that does not know the genesis keys
/// drops them without a word, and the spec it writes is the first place that shows. Waiting for
/// the nodes first would report the same thing minutes later.
fn ensure_spec_holds_service(spec_path: &Path, service_id: u32) -> anyhow::Result<()> {
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
	log::info!("{} holds the parachain service as service {service_id}", spec_path.display());
	Ok(())
}

/// The service this suite bootstraps, as `parachain-chain-spec` describes it: the service code
/// under [`PARACHAIN_SERVICE_ID`] and one AURA authorizer per para, built from the very blob and
/// config the collators derive their hash from. The hash each core's queue must hold comes back
/// from [`ParachainServiceSpec::authorizer_hashes`], so genesis cannot disagree with the service
/// or the collators.
///
/// Every para also registers `validation_code` — the copied runtime blob, i.e. the very bytes
/// its chain spec was built from — and the genesis head derived from that chain spec, so the
/// service's `parent_head_hash` check accepts the collators' first block.
fn parachain_service_spec(
	paras: &[Para],
	service_code: Vec<u8>,
	authorizer_code: Vec<u8>,
	validation_code: Vec<u8>,
	heads: &[Vec<u8>],
) -> anyhow::Result<ParachainServiceSpec> {
	anyhow::ensure!(
		paras.len() == heads.len(),
		"{} paras but {} genesis heads: every para needs the head of its own chain spec",
		paras.len(),
		heads.len(),
	);
	let mut spec = ParachainServiceSpec::new(PARACHAIN_SERVICE_ID, service_code)
		.balance(PARACHAIN_SERVICE_ENDOWMENT);
	for (para, head) in paras.iter().zip(heads) {
		spec = spec.parachain(
			ParachainSpec::new(para.id.into())
				.head_data(head.clone())
				.validation_code(validation_code.clone())
				// The builder's own default is unlimited, stated here so a change to that
				// default cannot silently starve the para (§6.1 headroom check).
				.state_balance(u64::MAX)
				.authorizer(authorizer_code.clone(), &genesis::aura_config(para)),
		);
	}
	Ok(spec)
}

/// The SCALE-encoded genesis header of the parachain `spec` describes, exported by the very
/// binary the collators run (`export-genesis-head --chain <spec> -r`): the header the chain
/// initializes from is the one its first block is built on, so this is the `head_data` JAM must
/// hold for the service's `parent_head_hash == blake2_256(head_data)` check to pass.
fn export_genesis_head(omni_node: &Path, spec: &Path) -> anyhow::Result<Vec<u8>> {
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
/// One core each, and a real one: two paras sharing a core would leave the first one's authorizer
/// overwritten, with a para that authors and never accumulates as the only sign of it.
fn auth_queues(
	paras: &[Para],
	hashes: &BTreeMap<ParaId, AuthorizerHash>,
) -> anyhow::Result<Vec<(u16, String)>> {
	let mut queues = Vec::with_capacity(paras.len());
	for para in paras {
		let hash = hashes
			.get(&para.id.into())
			.with_context(|| format!("the built spec has no authorizer for para {}", para.id))?;
		let core = u16::try_from(para.core)
			.with_context(|| format!("para {} names core {}", para.id, para.core))?;
		anyhow::ensure!(
			queues.iter().all(|(taken, _)| *taken != core),
			"two paras want core {core}: {:?}",
			paras.iter().map(|para| (para.id, para.core)).collect::<Vec<_>>(),
		);
		log::info!("para {} on core {core}, authorizer {}", para.id, genesis::hex(hash));
		queues.push((core, genesis::hex(hash)));
	}
	Ok(queues)
}

/// The genesis beyond the validator set, spelled as `gen-spec` reads it, from the built service:
/// its record counts `code` and `preimages` by file path — written from the built bytes by
/// [`write_sidecar`], since `gen-spec` reads them off disk — and `storage` as hex entries, which
/// have no file-path analogue. zombienet merges the object into the `jam_config.json` it
/// generates, knowing nothing about these keys.
fn built_genesis_overrides(
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
	let auth_queues: serde_json::Map<String, serde_json::Value> =
		queues.iter().map(|(core, hash)| (core.to_string(), json!(hash))).collect();
	let assigners: serde_json::Map<String, serde_json::Value> =
		queues.iter().map(|(core, _)| (core.to_string(), json!(service_id))).collect();
	let services: serde_json::Map<String, serde_json::Value> =
		[service_id.to_string()].into_iter().map(|id| (id, service.clone())).collect();
	json!({ "services": services, "auth_queues": auth_queues, "assigners": assigners })
}

/// The one-service, one-preimage shape the unit test pins as the schema `gen-spec` reads; the
/// run's genesis goes through [`built_genesis_overrides`] instead.
#[cfg(test)]
fn genesis_overrides(
	queues: &[(u16, String)],
	parasim_blob: &str,
	authorizer_blob: &str,
) -> serde_json::Value {
	built_genesis_overrides(
		queues,
		PARACHAIN_SERVICE_ID,
		parasim_blob,
		PARACHAIN_SERVICE_ENDOWMENT,
		&[authorizer_blob.to_string()],
		&BTreeMap::new(),
	)
}

/// Grants the service its five JAM privileges and the always-accumulate gas the housekeeping
/// phase needs. These are `ChainSpecConfig`-level, not `GenesisService`-level, so they go into
/// the overrides directly, not through the `parachain-chain-spec` builder.
///
/// `assign` covers every core in `queues`; `always_acc` is set to 10M to handle the worst-case
/// due-assign flush (341 cores × 80-hash queues, measured at ~9.94M gas — see PS `GENESIS.md`).
fn service_privileges(queues: &[(u16, String)], service_id: u32) -> serde_json::Value {
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
fn json_storage(storage: &BTreeMap<Vec<u8>, Vec<u8>>) -> serde_json::Value {
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
	path.to_str()
		.map(str::to_string)
		.with_context(|| format!("{} is not utf-8", path.display()))
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

/// Write bytes a service's genesis record names — `gen-spec` reads `code` and `preimages` off
/// disk — and return the file, which is the copy everything else refers to, like [`copy_aside`].
fn write_sidecar(bytes: &[u8], work_dir: &Path, name: &str) -> anyhow::Result<PathBuf> {
	let path = work_dir.join(name);
	std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
	Ok(path)
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

/// A parachain head as JAM has accumulated it: the tip the service believes the chain has reached.
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

	Ok(ParaHead { number: header.number.into(), hash: array_bytes::bytes2hex("0x", header.hash()) })
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
			"/run/parachain-service.jam",
			"/run/parachain-authorizer-sr25519.jam",
		);

		assert_eq!(
			overrides,
			json!({
				"services": { "5": {
					"code": "/run/parachain-service.jam",
					"balance": 1_000_000_000_000_000u64,
					"preimages": ["/run/parachain-authorizer-sr25519.jam"],
				}},
				"auth_queues": { "0": "aa".repeat(32), "1": "bb".repeat(32) },
				"assigners": { "0": 5, "1": 5 },
			})
		);
	}

	/// JSON numbers are exact only up to 2^53, and `gen-spec` refuses a lossy one rather than
	/// rounding it, so anything bigger has to travel as a decimal string.
	#[test]
	fn a_balance_beyond_2_to_the_53_is_written_as_a_decimal_string() {
		assert_eq!(json_balance(PARACHAIN_SERVICE_ENDOWMENT), json!(1_000_000_000_000_000u64));
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

	/// The storage the built service writes is what reaches genesis: every para's record sits
	/// under `[0x00] ‖ SCALE(ParaId)` — `Tag::Parachains` — spelled as the hex keys `gen-spec`
	/// reads.
	#[test]
	fn the_built_genesis_storage_carries_the_para_record() {
		let paras = vec![Para { id: 0, core: 0, collators: vec![0] }];
		let spec = parachain_service_spec(
			&paras,
			b"service code".to_vec(),
			b"authorizer".to_vec(),
			b"validation code".to_vec(),
			&[b"the head".to_vec()],
		)
		.expect("a tiny spec builds; qed");
		let hashes = spec.authorizer_hashes();
		let queues = auth_queues(&paras, &hashes).expect("one para, one core; qed");
		let genesis = spec.build().expect("a tiny spec builds; qed");

		let overrides = built_genesis_overrides(
			&queues,
			genesis.id,
			"/run/parachain-service.jam",
			genesis.balance,
			&["/run/preimage-0.jam".to_string()],
			&genesis.storage,
		);

		let storage = overrides["services"][genesis.id.to_string()]["storage"]
			.as_object()
			.expect("a registered para makes the storage non-empty; qed");
		let key = array_bytes::bytes2hex("", &para_info_key(ParaId(0)));
		assert!(key.starts_with("00"), "the record key carries Tag::Parachains as its first byte");
		assert!(
			storage.contains_key(&key),
			"genesis storage holds para 0's record at {key}, got: {}",
			storage.keys().cloned().collect::<Vec<_>>().join(", "),
		);
	}

	/// T7's canonical build of the real parachain service, at the evidence path T7 installed it.
	/// sha256 verified externally on 2026-09-09:
	/// `aba975fc9aa57a2b471c23d8be68f29b98bb77ef62e4f1a3a65c3bc3a34681c6`.
	///
	/// Repinned 2026-09-09 after the first end-to-end run froze the head: the previous canonical
	/// blob (`fc1cb0e2…`) predated the child-PVF host-call ABI consolidation (per-message calls →
	/// one SCALE `SendUpwardMessage` at index 102) and mis-dispatched every host call the current
	/// `jam_validate_block` guest makes, so refine failed for every candidate. The current-source
	/// build below matches that ABI; the pin moves to it.
	///
	/// Repinned again on 2026-09-09 for the child-PVF heap-growth path: the previous pin
	/// (`d82ade5b…`) panicked on the guest's `grow_heap` host call (executor FIXME), which made
	/// every work item's refine produce a gray-paper `WorkExecResult::Error` that accumulate
	/// skips, freezing the head at genesis with an empty para log. The executor now tracks the
	/// child's heap break and maps its pages on demand, mirroring gp-v0.8.0's host `grow_heap`.
	///
	/// Repinned again on 2026-09-09 for the polkavm version bump: the previous pin (`ec5b4f92…`)
	/// linked polkavm 0.30 into the service guest, which could not parse the 0.35-linked child PVF
	/// blob the SDK's wasm-builder produces. The service's root `Cargo.toml` now pins polkavm
	/// 0.36.0 (matching polkajam post-gp-v0.8.0 and the SDK's polkavm-linker), and the rebuilt
	/// blob below links it.
	///
	/// Repinned again on 2026-09-09 for the vendored-polkajam move: the previous pin
	/// (`f773711a…`) linked polkavm 0.30 into the service guest via PS's vendored polkajam
	/// submodule (`cargo-jam-build` at `3ecd9ba0`), and the 0.36 JAM host's `Module::from_blob`
	/// rejected it ("validation failed at offset 143933"). PS's `vendor/polkajam` now sits at the
	/// merged gp-v0.8.0 HEAD (`2c34621b`, polkavm-linker 0.36), the same lineage as the host and
	/// the SDK's wasm-builder, and the blob below is relinked with it.
	const SERVICE_BLOB: &str = concat!(
		env!("CARGO_MANIFEST_DIR"),
		"/../../../.omo/evidence/jam-zombienet-real-service/parachain-service.jam",
	);
	const SERVICE_BLOB_LEN: usize = 198_558;
	/// `blake2b-256` of the T7 blob, which is what
	/// `parachain_service::work_digest::validation_code_hash` computes for a code preimage: the
	/// hash JAM records for the service's code.
	const SERVICE_CODE_HASH: [u8; 32] = [
		0xcb, 0x9c, 0x2f, 0x50, 0x9e, 0x76, 0x99, 0x42, 0x92, 0x8c, 0xe5, 0xac, 0x66, 0x27, 0xa3,
		0x89, 0x18, 0xd7, 0x81, 0x3b, 0x0c, 0xa0, 0xe2, 0xdb, 0x50, 0xf1, 0x1e, 0x58, 0x66, 0x97,
		0x9c, 0x96,
	];

	/// T7's acceptance: the genesis holds the real service, not parasim, and the two blobs are
	/// `.jam` files whose names differ by one word — so the check is on the *content* of the file
	/// `services[0].code` names (length and code hash), never on the path.
	#[test]
	fn the_genesis_names_the_real_service_blob_bytes() {
		let blob = std::fs::read(Path::new(SERVICE_BLOB))
			.expect("T7's canonical service blob must exist at the evidence path; qed");
		assert_eq!(
			blob.len(),
			SERVICE_BLOB_LEN,
			"T7 recorded byte length — a rebuilt blob changes the code hash every pin commits to"
		);
		assert_eq!(jam_std_common::hash_raw(&blob), SERVICE_CODE_HASH, "the installed code hash");

		// Mirror `spawn`: freeze the blob into the work dir, build the spec from the frozen copy,
		// and spell the overrides from the built service.
		let work = tempfile::Builder::new()
			.prefix("t7-service.")
			.tempdir()
			.expect("a temp dir; qed");
		let frozen = copy_aside(Path::new(SERVICE_BLOB), work.path())
			.expect("freezing the blob, as spawn does; qed");
		let paras = vec![Para { id: 0, core: 0, collators: vec![0] }];
		let spec = parachain_service_spec(
			&paras,
			std::fs::read(&frozen).expect("reading the frozen copy; qed"),
			b"authorizer".to_vec(),
			b"validation code".to_vec(),
			&[b"the head".to_vec()],
		)
		.expect("a tiny spec builds; qed");
		let hashes = spec.authorizer_hashes();
		let genesis = spec.build().expect("a tiny spec builds; qed");
		let queues = auth_queues(&paras, &hashes).expect("one para, one core; qed");
		let overrides = built_genesis_overrides(
			&queues,
			genesis.id,
			&path_str(&frozen).expect("a utf-8 tempdir path; qed"),
			genesis.balance,
			&[],
			&genesis.storage,
		);

		let service = overrides["services"][PARACHAIN_SERVICE_ID.to_string()]
			.as_object()
			.expect("the built overrides must carry the service under its id");
		let Some(code_path) = service["code"].as_str() else {
			panic!("the built overrides must carry the service code as a path string");
		};
		let named =
			std::fs::read(Path::new(code_path)).expect("the file genesis names must exist; qed");
		assert_eq!(
			named.len(),
			SERVICE_BLOB_LEN,
			"the file genesis names is not the real service ({} bytes)",
			SERVICE_BLOB_LEN
		);
		assert_eq!(
			jam_std_common::hash_raw(&named),
			SERVICE_CODE_HASH,
			"the bytes genesis names are the real blob's, not a filename lookalike"
		);
	}

	/// T2's canonical PolkaVM build of the parachain template runtime, at the path T3's test
	/// pins it at. sha256 verified externally on 2026-09-11:
	/// `ac1816f3461d84956b977f8a9f24fbff25222078b6dc3b62130760ac2e721791`.
	const POLKAVM_BLOB: &str = concat!(
		env!("CARGO_MANIFEST_DIR"),
		"/../../../.omo/evidence/jam-zombienet-real-service/parachain-template-runtime.polkavm",
	);
	const POLKAVM_BLOB_LEN: usize = 7_014_288;
	/// `blake2b-256` of the T2 blob, which is what
	/// `parachain_service::work_digest::validation_code_hash` computes: the `code_ref.hash` the
	/// registration must land on.
	const T2_CODE_HASH: [u8; 32] = [
		0xf0, 0x44, 0xfb, 0xb9, 0xde, 0x5b, 0x4c, 0xed, 0x38, 0x44, 0x3f, 0x53, 0x74, 0xfa, 0x5d, 0x0d,
		0x6a, 0x92, 0x31, 0x5c, 0x10, 0x3b, 0xb5, 0x3c, 0x65, 0xf7, 0x73, 0x61, 0x57, 0xdd, 0xdc, 0xf1,
	];
	/// A para's registration baseline plus the preimage footprint of the T2 blob, from PS
	/// `service/src/state_balance.rs`: `PARA_INFO_FOOTPRINT` 4_246 + `PARA_LOG_FOOTPRINT` 65_585
	/// for any non-AssetHub para, and `187 + len` for a solicited preimage of `len` bytes.
	const REGISTRATION_MIN_TOTAL: u64 = 4_246 + 65_585 + 187 + POLKAVM_BLOB_LEN as u64;

	/// The chain spec the suite builds, patched, shells the binary the collators run; this is
	/// the harness's own default.
	fn omni_node() -> PathBuf {
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/release/polkadot-omni-node")
	}

	use sp_runtime::traits::Hash;

	/// Read T2's blob, refusing a fixture that is no longer the recorded one (length and magic:
	/// the same guards T3's hash test runs).
	fn read_pvf_blob() -> Vec<u8> {
		let blob = std::fs::read(Path::new(POLKAVM_BLOB))
			.expect("T2's canonical PolkaVM blob must exist at the evidence path; qed");
		assert_eq!(
			blob.len(),
			POLKAVM_BLOB_LEN,
			"T2 recorded byte length — a rebuilt blob changes the hash every registration pins"
		);
		assert_eq!(&blob[..4], b"PVM\0", "the blob must be a PolkaVM program, not WASM");
		blob
	}

	/// Build one para's registration exactly the way `spawn` does: the chain spec from the
	/// runtime blob, its genesis head exported from that very spec, and the blob registered as
	/// the para's validation code under `total`.
	///
	/// Returns what [`check_para_registration`] asserts on: the built genesis storage, the
	/// service's hosted preimages, and the derived head.
	fn registered_storage(
		blob: &[u8],
		total: u64,
		work: &Path,
	) -> anyhow::Result<(BTreeMap<Vec<u8>, Vec<u8>>, Vec<Vec<u8>>, Vec<u8>)> {
		let blob_path = work.join("runtime.polkavm");
		let spec_path = work.join("jam-parachain-0-spec.json");
		std::fs::write(&blob_path, blob)?;
		chain_spec::build(&omni_node(), &blob_path, &spec_path, 0, &[0])?;
		let head = export_genesis_head(&omni_node(), &spec_path)?;

		let para = Para { id: 0, core: 0, collators: vec![0] };
		let genesis = ParachainServiceSpec::new(PARACHAIN_SERVICE_ID, b"service code")
			.balance(PARACHAIN_SERVICE_ENDOWMENT)
			.parachain(
				ParachainSpec::new(ParaId(0))
					.head_data(head.clone())
					.validation_code(blob)
					.state_balance(total)
					.authorizer(b"authorizer", &genesis::aura_config(&para)),
			)
			.build()?;
		Ok((genesis.storage, genesis.preimages, head))
	}

	/// The four acceptance properties of the para's genesis registration, as one check: the
	/// registered code is the blob at its full length under the given hash, the total balance
	/// covers baseline plus the preimage footprint, the head is the chain spec's own genesis
	/// header (byte for byte, so the service's `parent_head_hash` check passes for the
	/// collators' first block), and the head really decodes as a substrate header.
	fn check_para_registration(
		storage: &BTreeMap<Vec<u8>, Vec<u8>>,
		head: &[u8],
		code_hash: &[u8; 32],
		code_len: u32,
		min_total: u64,
	) -> anyhow::Result<()> {
		let key = para_info_key(ParaId(0));
		let stored = storage
			.get(&key)
			.with_context(|| format!("no para record at {}", array_bytes::bytes2hex("", &key)))?;
		let info = ParaInfo::decode_all(&mut &stored[..]).with_context(|| {
			format!(
				"decoding {} bytes at {} as the service's ParaInfo",
				stored.len(),
				array_bytes::bytes2hex("", &key)
			)
		})?;

		let Some(code) = info.validation_code.as_ref() else {
			return Err(anyhow::anyhow!("para 0 is registered without validation code"));
		};
		anyhow::ensure!(
			code.code_ref.hash.0 == *code_hash,
			"registered code hash {} != the blob's {}",
			array_bytes::bytes2hex("", code.code_ref.hash.0),
			array_bytes::bytes2hex("", code_hash),
		);
		anyhow::ensure!(
			code.code_ref.len == code_len,
			"registered code length {} != the blob's {code_len}",
			code.code_ref.len,
		);

		anyhow::ensure!(
			info.total_state_balance >= min_total,
			"para 0 total_state_balance {} is under the registration's {min_total}",
			info.total_state_balance,
		);

		let head_data = info.head_data.into_inner();
		anyhow::ensure!(
			head_data == *head,
			"the registered head_data ({} bytes) is not the chain spec's genesis header ({} bytes)",
			head_data.len(),
			head.len(),
		);
		let header = ParaHeader::decode_all(&mut &head_data[..])
			.with_context(|| "the registered head_data does not decode as a substrate header")?;
		// The service computes `blake2_256(head_data)` and compares it to `parent_head_hash`;
		// `header.hash()` is what the chain spec's genesis header is hashed to, so the two
		// agreeing is the registration lining up with the collators' first block.
		anyhow::ensure!(
			header.hash() == BlakeTwo256::hash(&head_data),
			"blake2_256 of the registered head is not the header's own hash",
		);
		Ok(())
	}

	/// The acceptance test of T5: the built genesis storage decodes to a `ParaInfo` whose code
	/// is the T2 blob at its full length, whose balance covers the registration, and whose head
	/// is the chain spec's genesis header — the header the collators' first block builds on.
	#[test]
	fn para_is_registered_with_its_real_genesis_head_pvf_and_balance() {
		let blob = read_pvf_blob();
		let work = tempfile::Builder::new()
			.prefix("t5-registration.")
			.tempdir()
			.expect("a temp dir; qed");
		let (storage, preimages, head) = registered_storage(&blob, u64::MAX, work.path())
			.expect("building the registration; qed");

		check_para_registration(
			&storage,
			&head,
			&T2_CODE_HASH,
			POLKAVM_BLOB_LEN as u32,
			REGISTRATION_MIN_TOTAL,
		)
		.expect("the para's registration must satisfy all four properties");

		// The preimage is resolvable on chain: the blob is hosted as a service preimage under
		// its `(hash, len)`, and the registry entry at that key names the para as its
		// referencer — the state a completed §6.2 registration (`seed_para_inner`) leaves.
		let blob_hash = jam_std_common::hash_raw(&blob);
		assert_eq!(blob_hash, T2_CODE_HASH, "the registered hash is T2's, not a rebuild's");
		assert!(preimages.contains(&blob), "the blob must be hosted as a preimage of the service",);
		let registry_key =
			storage_key(Tag::PreimageRegistry, &(blob_hash, POLKAVM_BLOB_LEN as u32));
		assert!(
			storage.contains_key(&registry_key),
			"the (hash, len) registry must hold para 0's record at {}",
			array_bytes::bytes2hex("", &registry_key),
		);
	}

	/// The failure half of the QA pair: a registration one balance unit under what it needs is
	/// caught by [`check_para_registration`] — the §6.1 headroom check would starve the para
	/// later, so a registration that cannot pay for its own preimage is the silent-drop bug.
	#[test]
	fn an_underfunded_registration_is_flagged() {
		let blob = read_pvf_blob();
		let work = tempfile::Builder::new()
			.prefix("t5-underfunded.")
			.tempdir()
			.expect("a temp dir; qed");
		let (storage, _, head) = registered_storage(&blob, REGISTRATION_MIN_TOTAL - 1, work.path())
			.expect("building the registration; qed");

		assert!(
			check_para_registration(
				&storage,
				&head,
				&T2_CODE_HASH,
				POLKAVM_BLOB_LEN as u32,
				REGISTRATION_MIN_TOTAL
			)
			.is_err(),
			"a balance one under the registration's needs must be flagged",
		);
	}

	/// One byte of the expected head flipped must fail the check: the head assertion is
	/// load-bearing, and only one side is perturbed so a false pass has nowhere to hide.
	#[test]
	fn a_wrong_genesis_head_is_flagged_not_matched() {
		let blob = read_pvf_blob();
		let work = tempfile::Builder::new().prefix("t5-head.").tempdir().expect("a temp dir; qed");
		let (storage, _, head) = registered_storage(&blob, u64::MAX, work.path())
			.expect("building the registration; qed");

		let mut wrong = head.clone();
		wrong[0] ^= 0x01;
		assert!(
			check_para_registration(
				&storage,
				&wrong,
				&T2_CODE_HASH,
				POLKAVM_BLOB_LEN as u32,
				REGISTRATION_MIN_TOTAL
			)
			.is_err(),
			"a head one byte off must fail the registration check",
		);
	}

	/// A para whose head exceeds the 4 KiB bound is rejected by `build` with a typed error, not
	/// a panic mid-assembly.
	#[test]
	fn an_oversized_head_is_a_typed_error_not_a_panic() {
		let big = vec![0u8; 4 * 1024 + 1];
		let err = ParachainServiceSpec::new(PARACHAIN_SERVICE_ID, b"service code")
			.parachain(ParachainSpec::new(ParaId(1)).head_data(big.clone()))
			.build()
			.expect_err("an oversized head must be rejected, not panicked");
		assert!(matches!(err, parachain_chain_spec::Error::HeadDataTooLarge { para: 1, len }
				if len == big.len()),);
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

	/// The privileges JSON the real service needs to operate: all five roles name the service,
	/// and the always-accumulate allotment covers the worst-case due-assign flush.
	#[test]
	fn genesis_overrides_grant_all_five_privileges_and_always_acc_gas() {
		let queues = vec![(0u16, "aa".repeat(32)), (1u16, "bb".repeat(32))];
		let p = service_privileges(&queues, PARACHAIN_SERVICE_ID);

		assert_eq!(p["bless"], json!(PARACHAIN_SERVICE_ID), "bless must name the service");
		assert_eq!(p["designate"], json!(PARACHAIN_SERVICE_ID), "designate must name the service");
		assert_eq!(p["register"], json!(PARACHAIN_SERVICE_ID), "register must name the service");
		assert_eq!(
			p["assign"]["0"],
			json!(PARACHAIN_SERVICE_ID),
			"assign[0] must name the service"
		);
		assert_eq!(
			p["assign"]["1"],
			json!(PARACHAIN_SERVICE_ID),
			"assign[1] must name the service"
		);

		let id_key = PARACHAIN_SERVICE_ID.to_string();
		let gas = p["always_acc"][id_key.as_str()]
			.as_u64()
			.expect("always_acc must contain the service's gas allotment");
		assert!(gas >= 10_000_000, "always_acc gas {gas} is below the 10M minimum");
	}
}
