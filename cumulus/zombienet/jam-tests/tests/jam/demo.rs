// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The demo: the same harness the tests use, with no assertion and no end.
//!
//! `cumulus/scripts/jam-collator-demo.sh` is a thin wrapper around this.

use super::{env, harness::Run, rpc::Height};
use std::time::Duration;
use tokio::time::{sleep, Instant};

/// How many collators the demo runs. One is enough to watch a block get built, submitted as a work
/// package and reported.
fn collator_count() -> usize {
	std::env::var("NUM_COLLATORS").ok().and_then(|n| n.parse().ok()).unwrap_or(1)
}

/// Runs until killed, so it is `#[ignore]`d and a plain `cargo test` never picks it up.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the demo runs until it is killed"]
async fn demo() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let Some(binaries) = env::binaries_or_skip("demo") else { return Ok(()) };

	let collators = collator_count();
	let mut run = Run::start(&binaries, collators).await?;
	// The demo has no deadline; the one the harness set is only for the start-up it just did.
	run.deadline = Instant::now() + Duration::from_secs(u32::MAX as u64);

	println!(
		"demo: {collators} collator(s) against JAM at {}\nlogs and chain spec: {}\nCtrl-C to stop",
		run.network.rpc_url,
		run.work_dir().display(),
	);

	let rpc = run.collators.rpc(run.deadline).await?;
	let mut height = Height::default();
	loop {
		run.collators.check_all_running()?;
		height = rpc.height().await.unwrap_or(height);
		println!("parachain best {} finalized {}", height.best, height.finalized);
		sleep(Duration::from_secs(6)).await;
	}
}
