// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! One run of the whole thing: a JAM network carrying parasim from genesis, and one collator
//! set per para.

use super::{
	collators::{Collators, JamTarget, Para, POLL_INTERVAL},
	env::Binaries,
	genesis,
	network::{JamNetwork, ParaHead},
	rpc::{CollatorRpc, Height},
};
use anyhow::Context;
use std::{
	path::{Path, PathBuf},
	time::Duration,
};
use tokio::time::{sleep, Instant};

/// The whole run — network spin-up and block production — has to fit in this.
///
/// A healthy JAM network produces one parachain block per 6s slot, which would make 30 blocks
/// three minutes. A zombienet-spawned one is slower and lumpier: it records the wrong port for
/// every validator in genesis, so a work package whose guarantor set has just rotated sometimes
/// cannot be reported, and each such failure drops three in-flight blocks that have to be
/// rebuilt. Measured average is ~22s per block rather than 6s. This budget is sized for that,
/// and can come back down to a few minutes once the SDK generates matching addresses.
pub const DEADLINE: Duration = Duration::from_secs(25 * 60);

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
	pub binaries: Binaries,
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

		let target = JamTarget {
			rpc_url: network.rpc_url.clone(),
			service_id: network.service_id,
			authorizer_blob: network.authorizer_blob.clone(),
		};
		let mut started = Vec::with_capacity(paras.len());
		for para in paras {
			let collators = Collators::spawn(binaries, work_dir.path(), &para, &target)
				.with_context(|| format!("starting para {}'s collators", para.id))?;
			started.push(ParaRun { para, collators });
		}

		let mut run =
			Run { network, paras: started, binaries: binaries.clone(), work_dir, deadline };
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
		let jam_head = self.network.para_head(&self.binaries, para.para.id)?;
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
	let heights = run.wait_for_blocks(blocks, finalized).await;
	let result = heights.map(|heights| log::info!("{test}: {}", run.describe(&heights)));
	finish(run, result).await
}
