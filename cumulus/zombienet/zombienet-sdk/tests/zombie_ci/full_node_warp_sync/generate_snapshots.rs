// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Test for warp syncing nodes.
//!
//! This test starts a relaychain + parachain network where some nodes run from db snapshots
//! while others warp sync to the latest state.
//!
//! ## Updating Test Artifacts
//!
//! To update chain specs and snapshots after changes to `cumulus-test-runtime` or `rococo-local`,
//! see `README.md` and use the `generate-snapshots.sh` automation script.
use crate::{
	utils::{initialize_network, BEST_BLOCK_METRIC},
	zombie_ci::full_node_warp_sync::common::{build_network_config, PARA_BEST_BLOCK_TO_WAIT_FOR},
};

#[tokio::test(flavor = "multi_thread")]
async fn generate_snapshots() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config(false).await?;
	let network = initialize_network(config).await?;

	log::info!("Checking progress");
	for name in ["one", "two"] {
		log::info!("Checking full node {name} is syncing");
		network
			.get_node(name)?
			.wait_metric_with_timeout(
				BEST_BLOCK_METRIC,
				|b| b >= PARA_BEST_BLOCK_TO_WAIT_FOR,
				86000u64, // Wait up to 24h
			)
			.await?;
	}

	Ok(())
}
