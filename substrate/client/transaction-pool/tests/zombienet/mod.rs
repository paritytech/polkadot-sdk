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

/// Type of the substrate node.

#[derive(EnumString)]
pub enum NodeType {
	#[strum(serialize = "polkadot")]
	Polkadot,
	#[strum(serialize = "polkadot-parachain")]
	PolkadotParachain,
}

/// Wrapper over a substrate node managed by zombienet..
pub struct Node {
	r#type: NodeType,
	name: String,
	args: Vec<Arg>,
}

impl Node {
	pub fn new(r#type: NodeType, name: String, args: Vec<Arg>) -> Self {
		Node { r#type, name, args }
	}
}

pub trait Network {
	// Ensure the necesary bins are on $PATH.
	fn ensure_bins_on_path(&self) -> bool;
	// Relaychain nodes.
	fn rc_nodes(&self) -> Vec<Node>;
	// Parachain nodes.
	fn pc_nodes(&self) -> Vec<Node>;

	// Provide zombienet network config.
	fn config(&self) -> Result<NetworkConfig, anyhow::Error>;
	// Start the network locally.
	fn start(
		&self,
	) -> impl std::future::Future<Output = Result<ZNetwork<LocalFileSystem>, anyhow::Error>> + Send;
}

// A zombienet network with two relaychain 'polkadot' validators and one parachain
// validator based on yap-westend-live-2022 chain spec.
pub struct SmallNetworkYap {
	rc_nodes: Vec<Node>,
	pc_nodes: Vec<Node>,
}

impl SmallNetworkYap {
	pub fn new() -> Self {
		SmallNetworkYap {
			rc_nodes: vec![Node::new(NodeType::Polkadot, "alice".to_owned(), vec![]), Node::new(NodeType::Polkadot, "bob", vec![])],
			pc_nodes: vec![Node::new(NodeType::PolkadotParachain, "charlie".to_owned(), vec![
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
				])],
		}
	}
}

impl Network for SmallNetworkYap {
	fn ensure_bins_on_path(&self) -> bool {
		// We need polkadot, polkadot-parachain, polkadot-execute-worker, polkadot-prepare-worker,
		// (and ttxt? - maybe not for the network, but for the tests, definitely)
		self.

			.iter()
			.fold(true, |acc, bin| {
				if
			}

				acc && which(bin).map(|_| true).unwrap_or(false))
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

	fn rc_nodes(&self) -> &Vec<Node> {
		&self.rc_nodes
	}

	fn pc_nodes(&self) -> &Vec<Node> {
		&self.pc_nodes
	}
}
