// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! One run of the whole thing: a JAM network, parasim on it, N collators against it.

use super::{
	collators::{Collators, JamTarget, POLL_INTERVAL},
	env::Binaries,
	network::JamNetwork,
	rpc::Height,
};
use anyhow::Context;
use std::time::Duration;
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

/// The core parasim guarantees the parachain's work packages on.
const JAM_CORE: u32 = 0;

/// A running JAM network plus its collators, kept together so they are torn down together.
pub struct Run {
	pub network: JamNetwork,
	pub collators: Collators,
	work_dir: tempfile::TempDir,
	pub deadline: Instant,
}

impl Run {
	/// Spin everything up and return once all collators are launched.
	pub async fn start(binaries: &Binaries, collators: usize) -> anyhow::Result<Self> {
		let deadline = Instant::now() + DEADLINE;
		let work_dir = tempfile::Builder::new().prefix("jam-collator-test.").tempdir()?;
		log::info!("work dir: {}", work_dir.path().display());

		let network = JamNetwork::spawn(binaries, work_dir.path(), deadline).await?;
		let target = JamTarget {
			rpc_url: network.rpc_url.clone(),
			service_id: network.service_id,
			core: JAM_CORE,
		};
		let collators = Collators::spawn(binaries, work_dir.path(), collators, &target)
			.context("starting the collators")?;

		Ok(Run { network, collators, work_dir, deadline })
	}

	/// Where the chain spec, the collator base paths and every log file live.
	pub fn work_dir(&self) -> &std::path::Path {
		self.work_dir.path()
	}

	/// Wait until the parachain's best block reaches `blocks`, then until finality catches up to
	/// `finalized`.
	///
	/// Finality is checked second, and with its own budget, because it trails the best block by
	/// about four: reading it once at the moment best hits the target would be a race.
	pub async fn wait_for_blocks(&mut self, blocks: u64, finalized: u64) -> anyhow::Result<Height> {
		let rpc = self.collators.rpc(self.deadline).await?;
		let mut height = Height::default();

		while Instant::now() < self.deadline {
			self.collators.check_all_running()?;
			height = rpc.height().await.unwrap_or(height);
			log::info!("parachain best {} finalized {}", height.best, height.finalized);
			if height.best >= blocks && height.finalized >= finalized {
				return Ok(height);
			}
			sleep(POLL_INTERVAL).await;
		}

		Err(anyhow::anyhow!(
			"the parachain reached best {} / finalized {} in {:?}, wanted {blocks} / {finalized}",
			height.best,
			height.finalized,
			DEADLINE,
		))
	}

	/// The log tails a failure should show: every collator, plus the JAM node they talk to.
	pub fn diagnostics(&self) -> String {
		format!("{}\n{}", self.collators.log_tails(40), self.network.ordinary_node_log_tail(40))
	}

	/// Tear the network down tidily. Dropping `Run` also works, and is what covers a panic.
	pub async fn shutdown(self) {
		let Run { network, collators, work_dir, .. } = self;
		drop(collators);
		network.shutdown().await;
		drop(work_dir);
	}
}

/// Run `collators` collators and assert the parachain keeps moving.
pub async fn assert_collators_build_blocks(
	test: &str,
	collators: usize,
	blocks: u64,
	finalized: u64,
) -> anyhow::Result<()> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let Some(binaries) = super::env::binaries_or_skip(test) else { return Ok(()) };

	let mut run = Run::start(&binaries, collators).await?;
	let result = run.wait_for_blocks(blocks, finalized).await;
	match result {
		Ok(height) => {
			log::info!(
				"{test}: {collators} collators reached best {} / finalized {}",
				height.best,
				height.finalized
			);
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
