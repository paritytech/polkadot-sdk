// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

// Integration tests for fork-aware transaction pool.

use anyhow::anyhow;
use which::which;
use zombienet_configuration::shared::types::Arg;
use zombienet_sdk::{
	LocalFileSystem, Network as ZNetwork, NetworkConfig, NetworkConfigBuilder, NetworkConfigExt,
};

const DEFAULT_RC_NODE_RPC_PORT: u16 = 9944;
const DEFAULT_PC_NODE_RPC_PORT: u16 = 8844;

pub trait Network {
	// Ensure the necesary bins are on $PATH.
	fn ensure_bins_on_path(&self) -> bool;
	// Relaychain nodes names.
	fn rc_nodes_names(&self) -> Vec<String>;
	// Relaychain nodes count.
	fn rc_nodes_count(&self) -> usize;
	// Parachain nodes count.
	fn pc_nodes_count(&self) -> usize;
	// Parachain nodes names.
	fn pc_nodes_names(&self) -> Vec<String>;
	// Provide zombienet network config.
	fn config(&self) -> Result<NetworkConfig, anyhow::Error>;
	// Start the network locally.
	fn start(
		&self,
	) -> impl std::future::Future<Output = Result<ZNetwork<LocalFileSystem>, anyhow::Error>> + Send;
}

// A zombienet network with two relaychain 'polkadot' validators and one parachain
// validator based on yap-westend-live-2022 chain spec.
pub struct ParachainNetwork {
	required_bins: Vec<String>,
	pc_nodes_count: usize,
	rc_nodes_count: usize,
}

impl ParachainNetwork {
	pub fn new(pc_nodes_count: usize, rc_nodes_count: usize) -> Self {
		ParachainNetwork {
			required_bins: vec![
				"polkadot".to_owned(),
				"polkadot-parachain".to_owned(),
				"polkadot-prepare-worker".to_owned(),
				"polkadot-execute-worker".to_owned(),
			],
			pc_nodes_count,
			rc_nodes_count,
		}
	}
}

impl Network for ParachainNetwork {
	fn ensure_bins_on_path(&self) -> bool {
		// We need polkadot, polkadot-parachain, polkadot-execute-worker, polkadot-prepare-worker,
		// (and ttxt? - maybe not for the network, but for the tests, definitely)
		self.required_bins
			.iter()
			.fold(true, |acc, bin| acc && which(bin).map(|_| true).unwrap_or(false))
	}

	fn config(&self) -> Result<NetworkConfig, anyhow::Error> {
		let rc_nodes_names = self.rc_nodes_names();
		let config = NetworkConfigBuilder::new()
			.with_relaychain(|r| {
				let rc = r.with_chain("rococo-local").with_default_command("polkadot").with_node(
					|node| {
						node.with_name(
							rc_nodes_names.first().map(|name| name.as_str()).unwrap_or("unamed-0"),
						)
						.with_rpc_port(DEFAULT_RC_NODE_RPC_PORT)
						.validator(true)
					},
				);

				(DEFAULT_RC_NODE_RPC_PORT as usize + 1..
					DEFAULT_RC_NODE_RPC_PORT as usize + rc_nodes_names.len())
					.fold(rc, move |acc, port| {
						acc.with_node(|node| {
							node.with_name(
								rc_nodes_names
									.get(port - DEFAULT_RC_NODE_RPC_PORT as usize)
									.map(|name| name.as_str())
									.unwrap_or(format!("unamed-{}", port).as_str()),
							)
							.with_rpc_port(u16::try_from(port).unwrap_or(0))
							.validator(true)
						})
					})
			})
			.with_parachain(|p| {
				let p = p
					.with_id(2000)
					.cumulus_based(true)
					.with_chain_spec_path("tests/zombienet/chain-specs/yap-westend-live-2022.json");
				let p_args: Vec<Arg> = vec![
					"--force-authoring".into(),
					("--pool-limit", "500000").into(),
					("--pool-kbytes", "2048000").into(),
					("--rpc-max-connections", "15000").into(),
					("--rpc-max-response-size", "150").into(),
					"-lbasic-authorship=info".into(),
					"-ltxpool=info".into(),
					"-lsync=info".into(),
					"-laura::cumulus=info".into(),
					"-lpeerset=info".into(),
					"-lsub-libp2p=info".into(),
					"--state-pruning=1024".into(),
					"--rpc-max-subscriptions-per-connection=128000".into(),
				];

				p.with_collator(|n| {
					n.with_name("charlie")
						.validator(true)
						.with_command("polkadot-parachain")
						.with_rpc_port(9933)
						.with_args(p_args)
				})
			});

		config.build().map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})
	}

	async fn start(&self) -> Result<ZNetwork<LocalFileSystem>, anyhow::Error> {
		let network_config = self.config()?;
		if !self.ensure_bins_on_path() {
			return Err(anyhow!("Error: required bins weren't found on $PATH: polkadot"));
		}
		network_config.spawn_native().await.map_err(|err| anyhow!(format!("{}", err)))
	}

	fn rc_nodes_names(&self) -> Vec<String> {
		vec!["alice".to_owned(), "bob".to_owned()]
			.iter()
			.take(self.rc_nodes_count)
			.collect::<Vec<String>>()
	}

	fn pc_nodes_names(&self) -> Vec<String> {
		vec!["charlie".to_owned(), "dave".to_owned(), "eve".to_owned(), "fredie".to_owned()]
			.iter()
			.take(self.pc_nodes_count)
			.collect::<Vec<String>>()
	}

	fn rc_nodes_count(&self) -> usize {
		self.rc_nodes_count
	}

	fn pc_nodes_count(&self) -> usize {
		self.pc_nodes_count
	}
}
