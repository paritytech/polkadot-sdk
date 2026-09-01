// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! One run of the whole thing: a JAM network, parasim on it, and one collator set per para.

use super::{
	collators::{Collators, JamTarget, Para, POLL_INTERVAL},
	env::Binaries,
	network::JamNetwork,
	rpc::{CollatorRpc, Height},
};
use anyhow::Context;
use std::{
	path::{Path, PathBuf},
	time::Duration,
};
use tokio::time::{sleep, Instant};

/// The whole run — network spin-up, parasim registration and block production — has to fit in
/// this.
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
	/// Spin everything up and return once every para's collators are launched.
	///
	/// Cores are assigned before any collator starts. A collator whose authorizer is in no core's
	/// pool keeps authoring locally and submits nothing, so starting first would only mean a run
	/// that begins with a stretch of packages nobody can guarantee.
	pub async fn start(test: &str, binaries: &Binaries, paras: Vec<Para>) -> anyhow::Result<Self> {
		let deadline = Instant::now() + DEADLINE;
		let work_dir = WorkDir::create(test)?;
		log::info!("work dir: {}", work_dir.path().display());

		let network = JamNetwork::spawn(binaries, work_dir.path(), deadline).await?;
		network.assign_cores(binaries, &paras)?;

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

		Ok(Run { network, paras: started, work_dir, deadline })
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

/// Run one collator set per para and assert every parachain keeps moving.
pub async fn assert_paras_build_blocks(
	test: &str,
	paras: Vec<Para>,
	blocks: u64,
	finalized: u64,
) -> anyhow::Result<()> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let Some(binaries) = super::env::binaries_or_skip(test) else { return Ok(()) };

	let mut run = Run::start(test, &binaries, paras).await?;
	let result = run.wait_for_blocks(blocks, finalized).await;
	match result {
		Ok(heights) => {
			log::info!("{test}: {}", run.describe(&heights));
			run.shutdown().await;
			Ok(())
		},
		Err(error) => {
			// One message, reason first: anyhow prints the outermost context before its cause, so
			// wrapping would bury the reason under forty lines of log.
			let report = format!("{error}\n\n{}", run.diagnostics());
			run.shutdown().await;
			Err(anyhow::anyhow!(report))
		},
	}
}
