// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! JAM networks that stand in for a relay test's network config when the test needs more than
//! [`spawn`] with a list of paras: per-node arguments and RPC wiring.

use crate::jam::{
	network::{free_port, VALIDATORS_PER_CORE},
	para::{Para, TINY_CORES},
	spawn::{spawn, CollatorOptions, JamNetwork, SpawnOptions},
};
use std::collections::HashMap;
use zombienet_sdk::Arg;

/// The network of `zombie_ci::pov_recovery` on JAM: the relay test's parachain nodes, in the same
/// roles, with `runtime` as the para's runtime.
///
/// - JAM chain: `TINY_CORES` cores with their validators and the ordinary node `jam-or`
///   - every validator serves RPC: polkajam only serves bundle shards to validators, so a para node
///     can recover work packages only through a validator
/// - parachain nodes (`polkadot-omni-node`, para registered in the JAM genesis), each reading JAM
///   through `jam-or` and recovering work packages through its own validator
///   - bob
///     - collator which produces blocks
///   - alice
///     - collator which produces blocks, but has no para peers
///     - will need to recover the blocks through JAM work-package recovery
///   - charlie
///     - full node without para peers
///     - will need to recover the blocks through JAM work-package recovery
///   - eve
///     - collator which produces blocks, but has no para peers
///     - it fails recovery from time to time to test retries
///   - one
///     - collator which produces blocks, but has no para peers
///   - two
///     - full node without para peers
///
/// No para node gossips blocks to another: `--in-peers=0 --out-peers=0` leaves no sync peers and
/// `--collator-reserved-slots 0` turns off the collator-to-collator connections and package sync.
pub async fn pov_recovery(para_id: u32, runtime: Option<&[u8]>) -> anyhow::Result<JamNetwork> {
	const LOG: &str =
		"-lcumulus-jam-work-package-recovery=debug,cumulus-consensus=debug,sync=debug,info";
	let isolated = || -> Vec<Arg> {
		vec![
			LOG.into(),
			"--in-peers=0".into(),
			"--out-peers=0".into(),
			("--collator-reserved-slots", "0").into(),
		]
	};

	let validator_rpc_ports = (0..TINY_CORES as usize * VALIDATORS_PER_CORE)
		.map(|_| free_port())
		.collect::<Result<Vec<_>, _>>()?;
	let nodes = ["bob", "alice", "charlie", "eve", "one", "two"];
	let collators = nodes
		.iter()
		.zip(validator_rpc_ports.iter().cycle())
		.map(|(name, port)| {
			let mut args = if *name == "bob" {
				vec![LOG.into(), ("--collator-reserved-slots", "0").into()]
			} else {
				isolated()
			};
			if *name == "eve" {
				args.push("--fail-pov-recovery".into());
			}
			args.push(
				("--jam-recovery-rpc-urls", format!("ws://127.0.0.1:{port}").as_str()).into(),
			);
			(name.to_string(), CollatorOptions { args, ..Default::default() })
		})
		.collect::<HashMap<_, _>>();

	let para = Para::new(para_id, 0, &["bob", "alice", "eve", "one"])
		.with_full_nodes(&["charlie", "two"])
		.with_runtime_code("default", runtime)?;
	spawn(
		"pov_recovery",
		&[para],
		SpawnOptions { validator_rpc_ports, collators, ..SpawnOptions::new(TINY_CORES) },
	)
	.await
}
