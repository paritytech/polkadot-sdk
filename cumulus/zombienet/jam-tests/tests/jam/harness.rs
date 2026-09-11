// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! One run of the whole thing: a JAM network carrying parasim from genesis, and one collator
//! set per para.

use super::{
	collators::{Collators, JamTarget, POLL_INTERVAL, Para},
	env::Binaries,
	genesis,
	network::{JamNetwork, ParaHead},
	rpc::{CollatorRpc, Height},
};
use anyhow::Context;
use codec::DecodeAll;
use jam_cumulus_facade::{
	ParaId,
	service_state::{ParaInfo, Tag, para_info_key, storage_key},
};
use std::{
	collections::BTreeMap,
	path::{Path, PathBuf},
	time::Duration,
};
use tokio::time::{Instant, sleep};

/// The whole run — network spin-up and block production — has to fit in this.
///
/// A healthy JAM network produces one parachain block per 6s slot, which would make 30 blocks
/// three minutes. A zombienet-spawned one is slower and lumpier: it records the wrong port for
/// every validator in genesis, so a work package whose guarantor set has just rotated sometimes
/// cannot be reported, and each such failure drops three in-flight blocks that have to be
/// rebuilt. Measured average is ~22s per block rather than 6s. This budget is sized for that,
/// and can come back down to a few minutes once the SDK generates matching addresses.
pub const DEADLINE: Duration = Duration::from_secs(25 * 60);

/// The para head number JAM's own storage must reach before a progress test passes.
///
/// A collator authors blocks regardless of whether JAM accepts them, so `wait_for_blocks` alone
/// is a false positive when the JAM pipeline is completely dead (wrong PVF format, unregistered
/// para, missing code preimage). Reaching this target requires sustained accumulation across most
/// of the run, not a single lucky head. Set below the healthy baseline (~27) to give margin
/// against network jitter without hiding a true stall.
const JAM_HEAD_TARGET: u64 = 20;

/// Budget for the JAM-head assertion. On a healthy network the head already exceeds
/// [`JAM_HEAD_TARGET`] by the time the collator-height check finishes; this budget covers a slow
/// network without masking a real stall. Matched to the core-test warm-up for comparability.
const JAM_HEAD_BUDGET: Duration = Duration::from_secs(8 * 60);

/// Set this to a directory to keep every run's work dir: the run then works in a named
/// subdirectory of it that outlives the run, whether it passed or failed.
const BASE_DIR_VAR: &str = "JAM_TEST_BASE_DIR";

/// Where one run keeps its chain spec, its collator logs, and — under `zombienet/` — the directory
/// of every JAM node zombienet spawns for it. One run is one tree.
enum WorkDir {
	/// The default: deleted when the run ends.
	Temporary(tempfile::TempDir),
	/// Named after the test and kept, because `JAM_TEST_BASE_DIR` is set.
	Kept(PathBuf),
}

impl WorkDir {
	fn create(test: &str) -> anyhow::Result<Self> {
		let Some(base) = std::env::var_os(BASE_DIR_VAR) else {
			return Ok(WorkDir::Temporary(
				tempfile::Builder::new().prefix("jam-collator-test.").tempdir()?,
			));
		};

		let started = chrono::Local::now().format("%Y%m%d-%H%M%S");
		let path = PathBuf::from(base).join(format!("jam-collator-test-{test}-{started}"));
		std::fs::create_dir_all(&path)
			.with_context(|| format!("creating the work dir {}", path.display()))?;
		Ok(WorkDir::Kept(path))
	}

	fn path(&self) -> &Path {
		match self {
			WorkDir::Temporary(dir) => dir.path(),
			WorkDir::Kept(path) => path,
		}
	}
}

/// One reading of a para: where its own chain is, and where JAM thinks it is.
///
/// The two move independently, and every phase-6 assertion is about how: authoring is local and
/// carries on regardless, while the accumulated head only moves when a work package made it all
/// the way through JAM.
#[derive(Clone, Debug)]
pub struct ParaProgress {
	pub height: Height,
	pub jam_head: Option<ParaHead>,
}

impl std::fmt::Display for ParaProgress {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			formatter,
			"best {} finalized {}, JAM head {}",
			self.height.best,
			self.height.finalized,
			match &self.jam_head {
				Some(head) => head.to_string(),
				None => "none".to_string(),
			}
		)
	}
}

/// One parachain of a run: what it is, and the collators authoring it.
pub struct ParaRun {
	pub para: Para,
	pub collators: Collators,
}

/// A running JAM network plus every para's collators, kept together so they are torn down
/// together.
pub struct Run {
	pub network: JamNetwork,
	/// One entry per para, in the order they were started.
	pub paras: Vec<ParaRun>,
	work_dir: WorkDir,
	pub deadline: Instant,
}

impl Run {
	/// Spin everything up and return once every para's collators are launched and agree with
	/// genesis about their authorizer.
	///
	/// The cores are already pointed at the paras when the network comes up, so a collator has
	/// somewhere to submit to from its first block.
	pub async fn start(test: &str, binaries: &Binaries, paras: Vec<Para>) -> anyhow::Result<Self> {
		let deadline = Instant::now() + DEADLINE;
		let work_dir = WorkDir::create(test)?;
		log::info!("work dir: {}", work_dir.path().display());

		let network = JamNetwork::spawn(binaries, work_dir.path(), deadline, &paras).await?;

		// Verify every para's registration before touching a collator: all three
		// silent-drop paths in `accumulate/package.rs` are detectable from the config
		// file and show up only as a para head that never moves twenty minutes later.
		let zombienet_dir = work_dir.path().join("zombienet");
		check_genesis_registrations(&zombienet_dir, network.service_id, &paras)
			.context("genesis registration check failed — the run would stall silently")?;

		let target = JamTarget {
			rpc_url: network.rpc_url.clone(),
			service_id: network.service_id,
			authorizer_blob: network.authorizer_blob.clone(),
			wasm_overrides_dir: network.wasm_overrides_dir.clone(),
		};
		let mut started = Vec::with_capacity(paras.len());
		for (index, para) in paras.iter().enumerate() {
			// The chain spec `JamNetwork::spawn` built — the file the para's genesis head was
			// derived from, so the collators and the registration cannot disagree.
			let spec = network
				.para_specs
				.get(index)
				.context(format!("para {}'s chain spec", para.id))?;
			let collators = Collators::spawn(binaries, work_dir.path(), para, &target, spec)
				.with_context(|| format!("starting para {}'s collators", para.id))?;
			started.push(ParaRun { para: para.clone(), collators });
		}

		let mut run = Run { network, paras: started, work_dir, deadline };
		run.check_authorizers_agree().await?;
		Ok(run)
	}

	/// Fail unless every collator derived the authorizer hash genesis put in its core's queue.
	///
	/// The two are built from the same inputs by different code in different repos, and if they
	/// disagree the run is pointless: a collator whose hash is in no core's pool authors happily
	/// and is never authorized, so the only symptom, twenty minutes later, is a head that never
	/// moved. `tracing` abbreviates the hash the collator logs, so this is a prefix comparison —
	/// still more than enough to catch a set in the wrong order, a different para id or a stale
	/// blob.
	async fn check_authorizers_agree(&mut self) -> anyhow::Result<()> {
		/// A collator derives its authorizer before it does anything else, so this only has to
		/// cover process start-up.
		const BUDGET: Duration = Duration::from_secs(60);

		let blob = self.network.authorizer_blob.clone();
		for index in 0..self.paras.len() {
			let para = self.paras[index].para.clone();
			let expected = genesis::hex(&genesis::authorizer_hash(&para, &blob)?);
			let until = (Instant::now() + BUDGET).min(self.deadline);

			loop {
				self.check_all_running()?;
				let derived = self.paras[index].collators.derived_authorizer_hashes();
				if derived.len() == self.paras[index].collators.count() {
					for (collator, hash) in &derived {
						anyhow::ensure!(
							expected.starts_with(hash),
							"para {}'s collator {collator} derived authorizer 0x{hash}…, but \
							 genesis queued its core 0x{expected}; nothing it submits will ever \
							 be authorized",
							para.id,
						);
					}
					log::info!(
						"para {}'s {} collator(s) agree with genesis on authorizer 0x{expected}",
						para.id,
						derived.len(),
					);
					break;
				}
				anyhow::ensure!(
					Instant::now() < until,
					"in {BUDGET:?} only {} of para {}'s {} collators said which authorizer they \
					 derived",
					derived.len(),
					para.id,
					self.paras[index].collators.count(),
				);
				sleep(POLL_INTERVAL).await;
			}
		}
		Ok(())
	}

	/// Give a run more wall clock than [`DEADLINE`], for a test that waits out several phases.
	pub fn extend_deadline(&mut self, extra: Duration) {
		self.deadline += extra;
	}

	/// Where the chain specs, the collator base paths and every log file live.
	pub fn work_dir(&self) -> &Path {
		self.work_dir.path()
	}

	/// Fail if any collator of any para has exited.
	pub fn check_all_running(&mut self) -> anyhow::Result<()> {
		for para in &mut self.paras {
			para.collators.check_all_running()?;
		}
		Ok(())
	}

	/// An RPC client for the first collator of every para, in para order — the ones the assertions
	/// and the demo read.
	pub async fn rpcs(&self) -> anyhow::Result<Vec<CollatorRpc>> {
		let mut rpcs = Vec::with_capacity(self.paras.len());
		for para in &self.paras {
			rpcs.push(para.collators.rpc(self.deadline).await?);
		}
		Ok(rpcs)
	}

	/// Wait until every para's best block reaches `blocks` and its finalized block `finalized`.
	///
	/// Finality is part of the same wait, rather than checked once best hits the target, because
	/// it trails the best block by about four.
	pub async fn wait_for_blocks(
		&mut self,
		blocks: u64,
		finalized: u64,
	) -> anyhow::Result<Vec<Height>> {
		let rpcs = self.rpcs().await?;
		let mut heights = vec![Height::default(); self.paras.len()];

		while Instant::now() < self.deadline {
			self.check_all_running()?;
			for (index, rpc) in rpcs.iter().enumerate() {
				heights[index] = rpc.height().await.unwrap_or(heights[index]);
			}
			log::info!("{}", self.describe(&heights));
			if heights.iter().all(|h| h.best >= blocks && h.finalized >= finalized) {
				return Ok(heights);
			}
			sleep(POLL_INTERVAL).await;
		}

		Err(anyhow::anyhow!(
			"in {DEADLINE:?} the run reached {}, wanted best {blocks} / finalized {finalized} on \
			 every para",
			self.describe(&heights),
		))
	}

	/// One reading of para `index`: how far its own chain has got, and the head JAM holds for it.
	pub async fn sample(&self, index: usize, rpc: &CollatorRpc) -> anyhow::Result<ParaProgress> {
		let para = self.paras.get(index).context("no para with that index")?;
		let height = rpc.height().await?;
		let jam_head = self.network.para_head(para.para.id).await?;
		Ok(ParaProgress { height, jam_head })
	}

	/// Poll para `index` until `reached` accepts a reading, or the budget runs out.
	///
	/// A failed read is retried rather than fatal: both RPCs are remote, and one dropped call says
	/// nothing about the para. A collator that has *exited* is fatal, and is checked every round.
	async fn wait_until(
		&mut self,
		index: usize,
		rpc: &CollatorRpc,
		what: &str,
		budget: Duration,
		mut reached: impl FnMut(&ParaProgress) -> bool,
	) -> anyhow::Result<ParaProgress> {
		let id = self.paras[index].para.id;
		let started = Instant::now();
		let until = (started + budget).min(self.deadline);
		let mut last = None;
		let mut failure = None;

		log::info!("{what}: watching para {id}, budget {budget:?}");
		while Instant::now() < until {
			self.check_all_running()?;
			match self.sample(index, rpc).await {
				Ok(progress) => {
					log::info!("{what}: para {id} at {progress} after {:?}", started.elapsed());
					if reached(&progress) {
						log::info!("{what}: reached after {:?}", started.elapsed());
						return Ok(progress);
					}
					last = Some(progress);
				},
				Err(problem) => {
					log::warn!("{what}: reading para {id} failed: {problem:#}");
					failure = Some(problem);
				},
			}
			sleep(POLL_INTERVAL).await;
		}

		Err(anyhow::anyhow!(
			"{what}: nothing in {:?}; para {id} was last at {}{}",
			started.elapsed(),
			last.map_or("no reading at all".to_string(), |progress| progress.to_string()),
			failure.map_or(String::new(), |problem| format!(" (last read failed: {problem:#})")),
		))
	}

	/// Wait until JAM has accumulated a head of at least `number` for para `index`.
	pub async fn wait_for_jam_head(
		&mut self,
		index: usize,
		rpc: &CollatorRpc,
		number: u64,
		budget: Duration,
	) -> anyhow::Result<ParaProgress> {
		let what = format!("JAM should accumulate head #{number}");
		self.wait_until(index, rpc, &what, budget, |progress| {
			progress.jam_head.as_ref().is_some_and(|head| head.number >= number)
		})
		.await
	}

	/// Wait until JAM's head for para `index` has stood still for `still_for`.
	///
	/// Standing still is what a stall looks like from the chain: nothing announces that packages
	/// stopped being reported, the head simply stops moving. The reading returned is the frozen
	/// head, and the collator's own heights in it are what say whether authoring carried on.
	pub async fn wait_for_frozen_jam_head(
		&mut self,
		index: usize,
		rpc: &CollatorRpc,
		still_for: Duration,
		budget: Duration,
	) -> anyhow::Result<ParaProgress> {
		let what = format!("the JAM head should stand still for {still_for:?}");
		let mut standing: Option<(Option<ParaHead>, Instant)> = None;
		self.wait_until(index, rpc, &what, budget, |progress| match &standing {
			Some((head, since)) if *head == progress.jam_head => since.elapsed() >= still_for,
			_ => {
				standing = Some((progress.jam_head.clone(), Instant::now()));
				false
			},
		})
		.await
	}

	/// Where every para has got to, as one line.
	pub fn describe(&self, heights: &[Height]) -> String {
		self.paras
			.iter()
			.zip(heights)
			.map(|(run, height)| {
				format!("para {} best {} finalized {}", run.para.id, height.best, height.finalized)
			})
			.collect::<Vec<_>>()
			.join(", ")
	}

	/// The log tails a failure should show: every collator, plus the JAM node they talk to.
	pub fn diagnostics(&self) -> String {
		let collators = self
			.paras
			.iter()
			.map(|run| run.collators.log_tails(40))
			.collect::<Vec<_>>()
			.join("\n");
		format!("{collators}\n{}", self.network.ordinary_node_log_tail(40))
	}

	/// Tear the network down tidily. Dropping `Run` also works, and is what covers a panic.
	pub async fn shutdown(self) {
		let Run { network, paras, work_dir, .. } = self;
		drop(paras);
		network.shutdown().await;
		drop(work_dir);
	}
}

/// Verify every para's code is registered and resolvable before starting collators.
///
/// Checks the three silent-drop paths in `service/src/accumulate/package.rs` — each a bare
/// `return` with no log. Without this check they show up only as a para head that never moves:
/// 1. No `ParaInfo` at `para_info_key` → unregistered para silently skipped.
/// 2. No `PreimageRegistry` entry for `(hash, len)` → `historical_lookup` returns `None`.
/// 3. Preimage bytes hash to wrong value → refine rejects with `InvalidCodeHash`.
fn check_para_code_resolvable(
	storage: &BTreeMap<Vec<u8>, Vec<u8>>,
	preimages: &[Vec<u8>],
	paras: &[Para],
) -> anyhow::Result<()> {
	for para in paras {
		let key = para_info_key(ParaId(para.id));
		let stored = storage.get(&key).with_context(|| {
			format!(
				"para {id}'s registration is absent from genesis (no entry at {k}): \
				 accumulate will silently drop every candidate — check the genesis builder",
				id = para.id,
				k = array_bytes::bytes2hex("", &key),
			)
		})?;

		let info = ParaInfo::decode_all(&mut &stored[..]).with_context(|| {
			format!(
				"para {id}'s genesis entry at {k} does not decode as ParaInfo ({n} bytes)",
				id = para.id,
				k = array_bytes::bytes2hex("", &key),
				n = stored.len(),
			)
		})?;

		let Some(code) = info.validation_code.as_ref() else {
			anyhow::bail!(
				"para {id} is registered in genesis without validation code: accumulate \
				 will silently drop every candidate — check the genesis registration path",
				id = para.id,
			);
		};

		let hash = code.code_ref.hash.0;
		let len = code.code_ref.len;
		let hash_hex = array_bytes::bytes2hex("", hash);

		// The registry entry is what `historical_lookup` uses to resolve the code: absent →
		// `None` → candidate silently dropped. This is the highest-value pre-spawn check because
		// it is fully detectable from the config and is precisely the silent failure mode a
		// misbuilt genesis triggers — a hash on chain with no resolvable preimage.
		let registry_key = storage_key(Tag::PreimageRegistry, &(hash, len));
		anyhow::ensure!(
			storage.contains_key(&registry_key),
			"para {id}'s validation code ({h}, {len} bytes) has no PreimageRegistry entry \
			 at {rk}: historical_lookup will return None and every candidate will be silently \
			 dropped — code is registered but not resolvable at refine time",
			id = para.id,
			h = hash_hex,
			rk = array_bytes::bytes2hex("", &registry_key),
		);

		// The preimage bytes must hash to the registered hash at the registered length;
		// a mismatch means refine resolves a different hash and rejects with InvalidCodeHash.
		let hosted = preimages
			.iter()
			.any(|blob| blob.len() == len as usize && jam_std_common::hash_raw(blob) == hash);
		anyhow::ensure!(
			hosted,
			"para {id}'s validation code ({h}, {len} bytes) is not hosted as a matching \
			 preimage: no hosted preimage has length {len} and blake2b-256 hash {h}; \
			 refine will reject every candidate with InvalidCodeHash",
			id = para.id,
			h = hash_hex,
		);

		log::info!(
			"para {id}: genesis registration complete — code ({h}, {len} bytes), \
			 PreimageRegistry entry present, preimage bytes verified",
			id = para.id,
			h = hash_hex,
		);
	}
	Ok(())
}

/// Read the service's genesis storage and hosted preimage bytes from `jam_config.json`, then
/// run [`check_para_code_resolvable`] on every para.
///
/// `jam_config.json` is written by zombienet before it calls `gen-spec`, so it captures exactly
/// what genesis will carry. Calling this after [`JamNetwork::spawn`] returns but before any
/// collator starts converts a silent 20-minute stall into a named failure that identifies which
/// para, which key, and which invariant broke.
fn check_genesis_registrations(
	zombienet_dir: &Path,
	service_id: u32,
	paras: &[Para],
) -> anyhow::Result<()> {
	let config_path = zombienet_dir.join("jam_config.json");
	let config: serde_json::Value = serde_json::from_slice(
		&std::fs::read(&config_path)
			.with_context(|| format!("reading {}", config_path.display()))?,
	)
	.with_context(|| format!("parsing {}", config_path.display()))?;

	let services = config["services"]
		.as_object()
		.with_context(|| format!("{}: `services` is not a map", config_path.display()))?;
	let service = services
		.get(&service_id.to_string())
		.with_context(|| {
			format!(
				"{}: no service with id {service_id} — the genesis config is missing \
				 this service's record",
				config_path.display(),
			)
		})?;

	let mut storage: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
	if let Some(obj) = service["storage"].as_object() {
		for (hex_key, hex_val) in obj {
			let key = array_bytes::hex2bytes(hex_key)
				.map_err(|e| anyhow::anyhow!("storage key {hex_key:?} is not valid hex: {e:?}"))?;
			let val_str = hex_val.as_str().with_context(|| {
				format!(
					"{}: storage value for key {hex_key} is not a string",
					config_path.display()
				)
			})?;
			let val = array_bytes::hex2bytes(val_str).map_err(|e| {
				anyhow::anyhow!("storage value for {hex_key} is not valid hex: {e:?}")
			})?;
			storage.insert(key, val);
		}
	}

	let preimage_bytes = service["preimages"]
		.as_array()
		.map(Vec::as_slice)
		.unwrap_or_default()
		.iter()
		.map(|v| {
			let path_str = v.as_str().with_context(|| {
				format!("{}: preimage entry is not a string", config_path.display())
			})?;
			std::fs::read(path_str).with_context(|| format!("reading preimage {path_str}"))
		})
		.collect::<anyhow::Result<Vec<Vec<u8>>>>()?;

	check_para_code_resolvable(&storage, &preimage_bytes, paras)?;
	log::info!("genesis registration check passed for {} para(s)", paras.len());
	Ok(())
}

/// Run `collators` collators on the single para of [`Para::single`] and assert it keeps moving.
pub async fn assert_collators_build_blocks(
	test: &str,
	collators: usize,
	blocks: u64,
	finalized: u64,
) -> anyhow::Result<()> {
	assert_paras_build_blocks(test, vec![Para::single(collators)], blocks, finalized).await
}

/// Start logging and resolve the artifacts, or explain what is missing and skip the test.
pub fn setup(test: &str) -> Option<Binaries> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	super::env::binaries_or_skip(test)
}

/// Tear a run down, attaching the logs to whatever it failed on.
///
/// One message, reason first: anyhow prints the outermost context before its cause, so wrapping
/// would bury the reason under forty lines of log.
pub async fn finish(run: Run, result: anyhow::Result<()>) -> anyhow::Result<()> {
	match result {
		Ok(()) => {
			run.shutdown().await;
			Ok(())
		},
		Err(error) => {
			let report = format!("{error}\n\n{}", run.diagnostics());
			run.shutdown().await;
			Err(anyhow::anyhow!(report))
		},
	}
}

/// Run one collator set per para and assert every parachain keeps moving.
pub async fn assert_paras_build_blocks(
	test: &str,
	paras: Vec<Para>,
	blocks: u64,
	finalized: u64,
) -> anyhow::Result<()> {
	let Some(binaries) = setup(test) else { return Ok(()) };

	let mut run = Run::start(test, &binaries, paras).await?;
	let result = async {
		let heights = run.wait_for_blocks(blocks, finalized).await?;
		log::info!("{test}: {}", run.describe(&heights));
		assert_jam_heads_advance(&mut run).await
	}
	.await;
	finish(run, result).await
}

/// Assert that JAM has accumulated the para heads to [`JAM_HEAD_TARGET`], not just that the
/// collator authored blocks.
///
/// `wait_for_blocks` checks the collator's own chain height, which advances whether or not JAM
/// accepts any work. Without this assertion, a completely dead JAM pipeline lets every progress
/// test pass on collator height alone — the false positive this function was added to kill. Remove
/// it and a wrong PVF format, unregistered para, or missing code preimage silently pass as green.
async fn assert_jam_heads_advance(run: &mut Run) -> anyhow::Result<()> {
	let rpcs = run.rpcs().await?;
	for (index, rpc) in rpcs.iter().enumerate() {
		run.wait_for_jam_head(index, rpc, JAM_HEAD_TARGET, JAM_HEAD_BUDGET).await?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use jam_cumulus_facade::{
		ParaId,
		service_state::{Tag, para_info_key, storage_key},
	};
	use parachain_chain_spec::{ParachainServiceSpec, ParachainSpec};

	const TEST_SERVICE_ID: u32 = 5;
	/// Distinct enough from the authorizer blob that no hash collision is possible.
	const TEST_BLOB: &[u8] = b"test-validation-code-for-t8-harness-unit-tests";

	fn test_para() -> Para {
		Para { id: 0, core: 0, collators: vec![0] }
	}

	/// Build a minimal genesis with one para registered under `blob` as its validation code.
	/// Returns the service storage (binary key → value) and hosted preimage bytes — exactly
	/// what [`check_para_code_resolvable`] takes, so each failure test modifies one side only.
	fn test_genesis(blob: &[u8]) -> (BTreeMap<Vec<u8>, Vec<u8>>, Vec<Vec<u8>>) {
		let para = test_para();
		let genesis = ParachainServiceSpec::new(TEST_SERVICE_ID, b"service-code")
			.balance(1_000_000_000_000_000u64)
			.parachain(
				ParachainSpec::new(ParaId(0))
					.head_data(b"genesis-head".to_vec())
					.validation_code(blob)
					.state_balance(u64::MAX)
					.authorizer(b"auth-blob", &genesis::aura_config(&para)),
			)
			.build()
			.expect("minimal genesis must build; qed");
		(genesis.storage, genesis.preimages)
	}

	/// Happy path: a complete registration passes every check.
	#[test]
	fn check_passes_on_a_complete_registration() {
		let (storage, preimages) = test_genesis(TEST_BLOB);
		check_para_code_resolvable(&storage, &preimages, &[test_para()])
			.expect("a complete registration must pass all three checks");
	}

	/// Silent-drop case 1: no `ParaInfo` at the para key → accumulate drops silently.
	/// Only the storage side is perturbed; preimages are untouched.
	#[test]
	fn check_fails_when_para_record_is_absent() {
		let (mut storage, preimages) = test_genesis(TEST_BLOB);
		storage.remove(&para_info_key(ParaId(0)));

		let err = check_para_code_resolvable(&storage, &preimages, &[test_para()])
			.expect_err("an absent para record must be flagged");
		assert!(
			err.to_string().contains("accumulate will silently drop"),
			"the error must name the consequence: {err}",
		);
	}

	/// Silent-drop case 2: `ParaInfo` present but `PreimageRegistry` absent →
	/// `historical_lookup` returns `None` and the candidate is silently dropped.
	/// Only the registry key is removed; preimages and para record are untouched.
	#[test]
	fn check_fails_when_preimage_registry_entry_is_absent() {
		let (mut storage, preimages) = test_genesis(TEST_BLOB);
		let code_hash = jam_std_common::hash_raw(TEST_BLOB);
		let registry_key = storage_key(Tag::PreimageRegistry, &(code_hash, TEST_BLOB.len() as u32));
		storage.remove(&registry_key);

		let err = check_para_code_resolvable(&storage, &preimages, &[test_para()])
			.expect_err("an absent registry entry must be flagged");
		assert!(
			err.to_string().contains("historical_lookup"),
			"the error must name historical_lookup as the failure point: {err}",
		);
	}

	/// Silent-drop case 3: registry entry present but preimage bytes hash differently →
	/// refine rejects with `InvalidCodeHash`.
	/// Only the preimage side is perturbed; storage (including the registered hash) is untouched.
	#[test]
	fn check_fails_when_preimage_bytes_do_not_hash_to_code_ref() {
		let (storage, _preimages) = test_genesis(TEST_BLOB);
		let wrong = b"completely-different-preimage-bytes-for-t8-mismatch".to_vec();
		assert_ne!(
			jam_std_common::hash_raw(&wrong),
			jam_std_common::hash_raw(TEST_BLOB),
			"the wrong blob must hash differently from TEST_BLOB",
		);

		let err = check_para_code_resolvable(&storage, &[wrong], &[test_para()])
			.expect_err("a preimage that does not match the registered hash must be flagged");
		assert!(
			err.to_string().contains("InvalidCodeHash"),
			"the error must name the refine rejection path: {err}",
		);
	}
}
