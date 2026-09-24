// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "jam")]

//! The demo: the same harness the tests use, with no assertion and no end.
//!
//! `cumulus/zombienet/jam-tests/demo.sh` is a thin wrapper around this.

use crate::jam::Para;
use anyhow::anyhow;
use cumulus_jam_zombienet_tests::{
	env::binaries_or_err,
	genesis_build::{build_jam_genesis, polkavm_env},
	network::{
		base_dir, collator_args, copy_para_specs, path_str, work_dir, ORDINARY_NODE,
		VALIDATORS_PER_CORE,
	},
	para::{DEADLINE, TINY_CORES},
	rpc::{CollatorRpc, Height},
};
use std::time::Duration;
use tokio::time::{timeout, Instant};
use zombienet_sdk::{LocalFileSystem, Network};

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

	// This crate has no skip: a missing artifact is a hard error, unlike the old harness.
	binaries_or_err()?;

	let collators = collator_count();
	let para = Para::single(collators);
	let binaries = binaries_or_err()?;
	let (work_dir, _temp) = work_dir("demo")?;
	let paras = std::slice::from_ref(&para);
	let genesis = build_jam_genesis(&binaries, &work_dir, paras, TINY_CORES)?;
	let para_specs = copy_para_specs(&work_dir, &genesis.para_specs, paras)?;
	let base_dir = base_dir(&work_dir)?;
	let jam_node = path_str(&binaries.jam_node)?;
	let genspec_node = binaries.genspec_node.as_deref().map(path_str).transpose()?;
	let omni_node = path_str(&binaries.omni_node)?;
	let authorizer_blob = path_str(&genesis.authorizer_blob)?;
	let overrides = genesis.overrides.clone();
	let no_overrides = std::collections::HashMap::new();
	let validators = TINY_CORES as usize * VALIDATORS_PER_CORE;

	let config =
		zombienet_sdk::NetworkConfigBuilder::new()
			.with_jamchain(|jam| {
				let jam = jam.with_id("jam").with_default_command(jam_node.as_str());
				let jam = match genspec_node.as_deref() {
					Some(command) if command != jam_node.as_str() => {
						jam.with_chain_spec_command(command)
					},
					_ => jam,
				};
				let jam = jam.with_genesis_overrides(overrides);
				let jam = jam.with_validator(|node| node.with_name("jam0").with_env(polkavm_env()));
				let jam = (1..validators).fold(jam, |jam, index| {
					jam.with_validator(|node| {
						node.with_name(&format!("jam{index}")).with_env(polkavm_env())
					})
				});
				jam.with_ordinary(|node| node.with_name(ORDINARY_NODE).with_env(polkavm_env()))
			})
			.with_parachain(|p| {
				let p = p
					.with_id(para.id)
					.with_registration_strategy(zombienet_sdk::RegistrationStrategy::Manual)
					.with_chain_spec_path(para_specs[0].clone())
					.with_default_command(omni_node.as_str());
				let p = p.with_collator(|node| {
					node.with_name(para.collators[0].as_str()).with_env(polkavm_env()).with_args(
						collator_args(&authorizer_blob, &no_overrides, &para.collators[0], true),
					)
				});
				para.collators[1..].iter().fold(p, |p, name| {
					p.with_collator(|node| {
						node.with_name(name.as_str())
							.with_env(polkavm_env())
							.with_args(collator_args(&authorizer_blob, &no_overrides, name, true))
					})
				})
			})
			.with_global_settings(|g| g.with_base_dir(base_dir.clone()))
			.build()
			.map_err(|errors| {
				anyhow!(
					"config errs: {}",
					errors.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ")
				)
			})?;
	let network = zombienet_sdk::environment::get_spawn_fn()(config).await?;

	println!(
		"demo: {collators} collator(s) against JAM at {}\nlogs and chain spec: {}\nCtrl-C to stop",
		crate::jam::jam_rpc_url(&network)?,
		base_dir.display(),
	);

	let deadline = Instant::now() + DEADLINE;
	let mut rpcs = Vec::with_capacity(para.collators.len());
	for name in &para.collators {
		rpcs.push(CollatorRpc::connect(network.get_node(name.as_str())?.ws_uri(), deadline).await?);
	}

	let mut heights = vec![Height::default(); rpcs.len()];
	let result = loop {
		if let Err(error) = check_all_running(&network, &para).await {
			break Err(error);
		}
		for (index, rpc) in rpcs.iter().enumerate() {
			heights[index] = rpc.height().await.unwrap_or(heights[index]);
		}
		println!("{}", describe(&para, &heights));

		// Ctrl-C has to be handled explicitly: the test harness does not unwind on a signal, so
		// without this the collators and the JAM nodes would outlive the demo.
		if timeout(Duration::from_secs(6), tokio::signal::ctrl_c()).await.is_ok() {
			println!("stopping");
			break Ok(());
		}
	};

	if let Err(error) = network.destroy().await {
		log::warn!("tearing down the JAM network failed: {error}");
	}
	result
}

/// Every collator of `para` still has to answer, or the run is over.
async fn check_all_running(network: &Network<LocalFileSystem>, para: &Para) -> anyhow::Result<()> {
	for name in &para.collators {
		if !network.get_node(name.as_str())?.is_responsive().await {
			return Err(anyhow!("collator {name} is not responsive"));
		}
	}
	Ok(())
}

/// One line per collator with its best and finalized height.
fn describe(para: &Para, heights: &[Height]) -> String {
	para.collators
		.iter()
		.zip(heights)
		.map(|(name, height)| {
			format!("{name}: best {} finalized {}", height.best, height.finalized)
		})
		.collect::<Vec<_>>()
		.join("  ")
}
