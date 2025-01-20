use std::{path::PathBuf, time::SystemTime};

use zombienet_sdk::{
	LocalFileSystem, Network as ZNetwork, NetworkConfig, NetworkConfigBuilder, NetworkConfigExt,
};

use super::{
	Network, Node, ParachainConfig, RelaychainConfig, DEFAULT_BASE_DIR, DEFAULT_PC_NODE_RPC_PORT,
	DEFAULT_RC_NODE_RPC_PORT,
};
use anyhow::anyhow;
use which::which;

// A zombienet network with two relaychain 'polkadot' validators and one parachain
// validator based on yap-westend-live-2022 chain spec.
pub struct Limits30Network {
	rc_config: RelaychainConfig,
	pc_config: ParachainConfig,
	rc_nodes: Vec<Node>,
	pc_nodes: Vec<Node>,
	base_dir: PathBuf,
}

impl Limits30Network {
	/// Creates a new [`SmallNetworkLimits30`].
	pub fn new(
		rc_config: RelaychainConfig,
		pc_config: ParachainConfig,
	) -> Result<Self, anyhow::Error> {
		// Create temporary network base dir.
		let datetime: chrono::DateTime<chrono::Local> = SystemTime::now().into();
		let base_dir =
			PathBuf::from(format!("{}-{}", DEFAULT_BASE_DIR, datetime.format("%Y%m%d_%H%M%S")));
		std::fs::create_dir(base_dir.clone()).map_err(|err| anyhow!(format!("{err}")))?;
		let p_args = vec![
			"--force-authoring".into(),
			("--pool-limit", "300").into(),
			("--pool-kbytes", "2048000").into(),
			("--rpc-max-connections", "15000").into(),
			("--rpc-max-response-size", "150").into(),
			"-lbasic-authorship=info".into(),
			"-ltxpool=trace".into(),
			"-lsync=trace".into(),
			"-laura::cumulus=debug".into(),
			"-lpeerset=trace".into(),
			"-lsub-libp2p=debug".into(),
			"--pool-type=fork-aware".into(),
			"--state-pruning=1024".into(),
			"--rpc-max-subscriptions-per-connection=128000".into(),
		];
		Ok(Limits30Network {
			rc_config,
			pc_config,
			rc_nodes: vec![
				Node::new("alice".to_owned(), vec![], true),
				Node::new("bob".to_owned(), vec![], true),
			],
			pc_nodes: vec![
				Node::new("charlie".to_owned(), p_args.clone(), false),
				Node::new("dave".to_owned(), p_args.clone(), true),
				Node::new("eve".to_owned(), p_args.clone(), true),
				Node::new("ferdie".to_owned(), p_args, true),
			],
			base_dir,
		})
	}
}

#[async_trait::async_trait]
impl Network for Limits30Network {
	fn ensure_bins_on_path(&self) -> bool {
		// We need polkadot, polkadot-parachain, polkadot-execute-worker, polkadot-prepare-worker,
		// (and ttxt? - maybe not for the network, but for the tests, definitely)
		which("polkadot")
			.and_then(|_| {
				which("polkadot-prepare-worker").and_then(|_| {
					which("polkadot-execute-worker").and_then(|_| which("polkadot-parachain"))
				})
			})
			.map(|_| true)
			.unwrap_or(false)
	}

	fn config(&self) -> Result<NetworkConfig, anyhow::Error> {
		let config = NetworkConfigBuilder::new()
			.with_relaychain(|r| {
				let mut rc_nodes_iter = self.rc_nodes.iter();
				let first_node = rc_nodes_iter.next();
				// All nodes should use `polkadot` bin.
				let rc = r
					.with_chain(self.rc_config.chain.as_str())
					.with_default_command(self.rc_config.default_command.as_str())
					.with_node(|node| {
						node.with_name(
							first_node.map(|node| node.name.as_str()).unwrap_or("rc-unamed-0"),
						)
						.with_rpc_port(DEFAULT_RC_NODE_RPC_PORT)
						.validator(first_node.map(|node| node.validator).unwrap_or(false))
					});
				(DEFAULT_RC_NODE_RPC_PORT as usize + 1..
					DEFAULT_RC_NODE_RPC_PORT as usize + self.rc_nodes.len())
					.zip(rc_nodes_iter)
					.fold(rc, move |acc, (port, node)| {
						acc.with_node(|new_node| {
							new_node
								.with_name(&node.name)
								.with_rpc_port(u16::try_from(port).unwrap_or(0))
								.validator(node.validator)
						})
					})
			})
			.with_parachain(|p| {
				let mut pc_nodes_iter = self.pc_nodes.iter();
				let first_node = pc_nodes_iter.next();

				// Set up the parachain and the first collator, obtaining the right type to iterate
				// through the following collators through folding.
				let p = p
					.with_id(self.pc_config.id)
					.cumulus_based(self.pc_config.cumulus_based)
					.with_chain_spec_path(self.pc_config.chain_spec_path.as_str())
					.with_default_command(self.pc_config.default_command.as_str())
					.with_collator(|new_node| {
						new_node
							.with_name(
								first_node.map(|node| node.name.as_str()).unwrap_or("pc-unamed-0"),
							)
							.with_rpc_port(DEFAULT_PC_NODE_RPC_PORT)
							.validator(first_node.map(|node| node.validator).unwrap_or(false))
							.with_args(first_node.map(|node| node.args.clone()).unwrap_or(vec![]))
					});

				(DEFAULT_PC_NODE_RPC_PORT as usize + 1..
					DEFAULT_PC_NODE_RPC_PORT as usize + self.pc_nodes.len())
					.zip(pc_nodes_iter)
					.fold(p, move |acc, (port, pc_node_config)| {
						acc.with_collator(|node| {
							node.with_name(&pc_node_config.name)
								.with_rpc_port(u16::try_from(port).unwrap_or(0))
								.validator(pc_node_config.validator)
								.with_command(self.pc_config.default_command.as_str())
								.with_args(pc_node_config.args.clone())
						})
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

	fn base_dir(&self) -> &PathBuf {
		&self.base_dir
	}
}
