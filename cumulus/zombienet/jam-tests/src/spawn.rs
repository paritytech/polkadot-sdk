// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The single place a JAM test builds a zombienet network: the JAM chain, one parachain per
//! [`Para`] with its collators, and the handle a test drives the run through.

use crate::{
	env::{binaries_or_err, init_logger},
	genesis_build::{build_jam_genesis, polkavm_env, JamGenesis},
	network::{
		base_dir, collator_args, copy_para_specs, path_str, work_dir, ORDINARY_NODE,
		VALIDATORS_PER_CORE,
	},
	para::{Para, DEADLINE},
	rpc::{CollatorRpc, JamRpc},
};
use anyhow::{anyhow, Context};
use cumulus_zombienet_sdk_helpers::ParaConfig;
use std::{collections::HashMap, path::PathBuf, time::Duration};
use tokio::time::Instant;
use zombienet_sdk::{subxt::OnlineClient, Arg, LocalFileSystem, Network, NetworkNode};

/// How the network is built: the core count and the few per-node choices a test needs to pin.
pub struct SpawnOptions {
	/// How many JAM cores the network runs; the validators follow as
	/// `cores * VALIDATORS_PER_CORE`.
	pub cores: u16,
	/// The ordinary node's RPC port, `None` lets zombienet pick one.
	pub ordinary_rpc_port: Option<u16>,
	/// Per-collator choices, keyed by collator name. A collator absent from the map takes the
	/// defaults: `DEFAULT_JAM_RPC_URL` and a random p2p port.
	pub collators: HashMap<String, CollatorOptions>,
	/// How long [`JamRpc::wait_ready`] may take before the spawn fails.
	pub ready_timeout: Duration,
}

impl SpawnOptions {
	/// The defaults for a network with `cores` cores: no pinned ports and no collator options.
	pub fn new(cores: u16) -> Self {
		Self { cores, ordinary_rpc_port: None, collators: HashMap::new(), ready_timeout: DEADLINE }
	}
}

/// The per-collator choices a test pins.
#[derive(Clone, Debug, Default)]
pub struct CollatorOptions {
	/// The collator's `--jam-rpc-urls`, instead of `DEFAULT_JAM_RPC_URL`. The
	/// resubmission test points a collator at a proxy this way.
	pub jam_rpc_url: Option<String>,
	/// A fixed p2p port. Pinning it also emits `--public-addr` for that port, so authority
	/// discovery hands the collator's own loopback address back to its peers.
	pub p2p_port: Option<u16>,
}

/// Spawn the JAM network `paras` describes, with `options` choosing the topology and the
/// per-collator overrides.
///
/// The returned [`JamNetwork`] owns the run's work dir and the spawned nodes:
/// [`JamNetwork::finish`] tears the network down and passes a test's result through,
/// [`JamNetwork::destroy`] only tears it down.
pub async fn spawn(
	test: &str,
	paras: &[Para],
	options: SpawnOptions,
) -> anyhow::Result<JamNetwork> {
	init_logger();

	let binaries = binaries_or_err()?;
	let (work_dir, _temp) = work_dir(test)?;
	let genesis = build_jam_genesis(&binaries, &work_dir, paras, options.cores)?;
	let para_specs = copy_para_specs(&work_dir, &genesis.para_specs, paras)?;
	let base_dir = base_dir(&work_dir)?;
	let jam_node = path_str(&binaries.jam_node)?;
	let genspec_node = binaries.genspec_node.as_deref().map(path_str).transpose()?;
	let omni_node = path_str(&binaries.omni_node)?;
	// The collators hash the same authorizer blob genesis queued, so they are pointed at the
	// frozen copy, never at the build output.
	let authorizer_blob = path_str(&genesis.authorizer_blob)?;
	let genesis_overrides = genesis.overrides.clone();
	let jam_rpc_url_overrides: HashMap<String, String> = options
		.collators
		.iter()
		.filter_map(|(name, options)| {
			options.jam_rpc_url.as_ref().map(|url| (name.clone(), url.clone()))
		})
		.collect();
	let validators = options.cores as usize * VALIDATORS_PER_CORE;
	let public_addr =
		|port: u16| Arg::Option("--public-addr".into(), format!("/ip4/127.0.0.1/tcp/{port}/ws"));

	// A para without a collator would make the collator folds below index an empty list.
	for para in paras {
		anyhow::ensure!(!para.collators.is_empty(), "para {} names no collator", para.id);
	}

	let builder = zombienet_sdk::NetworkConfigBuilder::new().with_jamchain(|jam| {
		let jam = jam.with_id("jam").with_default_command(jam_node.as_str());
		let jam = match genspec_node.as_deref() {
			Some(command) if command != jam_node.as_str() => jam.with_chain_spec_command(command),
			_ => jam,
		};
		let jam = jam.with_genesis_overrides(genesis_overrides);
		let jam = jam.with_validator(|node| node.with_name("jam0").with_env(polkavm_env()));
		let jam = (1..validators).fold(jam, |jam, index| {
			jam.with_validator(|node| {
				node.with_name(&format!("jam{index}")).with_env(polkavm_env())
			})
		});
		match options.ordinary_rpc_port {
			Some(port) => jam.with_ordinary(|node| {
				node.with_name(ORDINARY_NODE).with_env(polkavm_env()).with_rpc_port(port)
			}),
			None => jam.with_ordinary(|node| node.with_name(ORDINARY_NODE).with_env(polkavm_env())),
		}
	});
	let builder = paras.iter().zip(para_specs.iter()).fold(builder, |config, (para, spec)| {
		config.with_parachain(|p| {
			let p = p
				.with_id(para.id)
				.with_registration_strategy(zombienet_sdk::RegistrationStrategy::Manual)
				.with_chain_spec_path(spec.clone())
				.with_default_command(omni_node.as_str());
			let first = para.collators.first().expect("a para names at least one collator; qed");
			let p = p.with_collator(|node| {
				let mut args = collator_args(&authorizer_blob, &jam_rpc_url_overrides, first);
				let node = node.with_name(first.as_str()).with_env(polkavm_env());
				let node = match options.collators.get(first).and_then(|options| options.p2p_port) {
					Some(port) => {
						args.push(public_addr(port));
						node.with_p2p_port(port)
					},
					None => node,
				};
				node.with_args(args)
			});
			para.collators[1..].iter().fold(p, |p, name| {
				p.with_collator(|node| {
					let mut args = collator_args(&authorizer_blob, &jam_rpc_url_overrides, name);
					let node = node.with_name(name.as_str()).with_env(polkavm_env());
					let node =
						match options.collators.get(name).and_then(|options| options.p2p_port) {
							Some(port) => {
								args.push(public_addr(port));
								node.with_p2p_port(port)
							},
							None => node,
						};
					node.with_args(args)
				})
			})
		})
	});
	let config =
		builder
			.with_global_settings(|g| g.with_base_dir(base_dir))
			.build()
			.map_err(|errors| {
				anyhow!(
					"config errs: {}",
					errors.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ")
				)
			})?;

	let network = zombienet_sdk::environment::get_spawn_fn()(config).await?;
	let jam_url = network.get_jam_node(ORDINARY_NODE)?.ws_uri();
	let jam_rpc = JamRpc::wait_ready(&jam_url, Instant::now() + options.ready_timeout).await?;

	Ok(JamNetwork { network, jam_rpc, jam_url, genesis, work_dir, _temp })
}

/// A spawned JAM network plus the handles a test needs to drive it.
pub struct JamNetwork {
	/// The running network.
	pub network: Network<LocalFileSystem>,
	/// The ordinary node's RPC, past genesis.
	pub jam_rpc: JamRpc,
	/// The ordinary node's RPC URL, the one JAM reads and the control lane go through.
	pub jam_url: String,
	/// The genesis the network was built from: its overrides and the paras' specs.
	pub genesis: JamGenesis,
	/// The run's work dir. Kept alive by `_temp` unless `JAM_TEST_BASE_DIR` names it.
	pub work_dir: PathBuf,
	/// Keeps the work dir alive for the network's lifetime: dropping it deletes the directory
	/// the network is rooted in. `None` when `JAM_TEST_BASE_DIR` keeps the run's dir.
	_temp: Option<tempfile::TempDir>,
}

impl JamNetwork {
	/// The collator node named `name`.
	pub fn collator(&self, name: &str) -> anyhow::Result<&NetworkNode> {
		self.network
			.get_node(name)
			.with_context(|| format!("the JAM network has no collator {name}"))
	}

	/// Connect to collator `name`'s parachain RPC.
	pub async fn collator_rpc(&self, name: &str) -> anyhow::Result<CollatorRpc> {
		let node = self.collator(name)?;
		CollatorRpc::connect(node.ws_uri(), Instant::now() + DEADLINE).await
	}

	/// The subxt client for collator `name`'s parachain RPC.
	pub async fn collator_client(&self, name: &str) -> anyhow::Result<OnlineClient<ParaConfig>> {
		self.collator(name)?.wait_client::<ParaConfig>().await
	}

	/// Tear the network down, logging rather than propagating a teardown failure: the assertions
	/// have already run, and a dropped network cleans up after a panic anyway.
	pub async fn destroy(self) {
		if let Err(error) = self.network.destroy().await {
			log::warn!("tearing down the JAM network failed: {error}");
		}
	}

	/// Destroy the network and return `result`, so a test hands its outcome straight through.
	pub async fn finish(self, result: anyhow::Result<()>) -> anyhow::Result<()> {
		self.destroy().await;
		result
	}
}
