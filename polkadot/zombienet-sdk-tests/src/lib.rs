use serde_json::json;
use subxt::{OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;
use zombienet_sdk::{
	LocalFileSystem, Network, NetworkConfigBuilder, NetworkConfigExt, NetworkNode,
};

#[cfg(test)]
mod tests;

#[subxt::subxt(runtime_metadata_path = "artifacts/polkadot_metadata_small.scale")]
pub mod polkadot {}

pub type Error = Box<dyn std::error::Error>;

#[derive(Debug, Default)]
pub struct Images {
	polkadot: String,
	malus: String,
	cumulus: String,
}

pub enum Provider {
	Native,
	K8s,
}

impl From<String> for Provider {
	fn from(value: String) -> Self {
		if value.to_ascii_lowercase() == "k8s" {
			Provider::K8s
		} else {
			Provider::Native
		}
	}
}

pub fn runtime_config() -> serde_json::Value {
	json!({
		"configuration": {
			"config": {
				"max_validators_per_core": 1,
				"needed_approvals": 1,
				"group_rotation_frequency": 10
			}
		}
	})
}

/// Required bins: polkadot, malus, polkadot-parachain
/// built with `--features fast-runtime`
///
/// This spawns a network with 3 honest validators and 1 malicious backer.
/// One parachain with id 2000 and one cumulus-based collator.
pub async fn spawn_network_malus_backer(
	images: Option<Images>,
	provider: Provider,
) -> Result<Network<LocalFileSystem>, Error> {
	let network_config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let patch = runtime_config();

			let mut builder = r.with_chain("westend-local");
			if let Some(images) = images.as_ref() {
				builder = builder.with_default_image(images.polkadot.as_ref());
			};

			builder
				.with_genesis_overrides(patch)
				.with_default_command("polkadot")
				.with_default_args(vec![
					"--no-hardware-benchmarks".into(),
					"-lparachain=debug".into(),
				])
				.with_node(|node| node.with_name("honest-0"))
				.with_node(|node| node.with_name("honest-1"))
				.with_node(|node| node.with_name("honest-2"))
				.with_node(|node| {
					let mut node_builder = node.with_name("malicious-backer");
					if let Some(images) = images.as_ref() {
						node_builder = node_builder.with_image(images.malus.as_ref());
					};
					node_builder
						.with_command("malus")
						.with_subcommand("suggest-garbage-candidate")
						.with_args(vec!["--no-hardware-benchmarks".into(), "-lMALUS=trace".into()])
				})
		})
		.with_parachain(|p| {
			p.with_id(2000).cumulus_based(true).with_collator(|n| {
				let mut node_builder = n.with_name("collator").with_command("polkadot-parachain");

				if let Some(images) = images {
					node_builder = node_builder.with_image(images.cumulus.as_ref())
				};
				node_builder
			})
		})
		.build()
		.unwrap();

	let network = match provider {
		Provider::Native => network_config.spawn_native().await?,
		Provider::K8s => network_config.spawn_k8s().await?,
	};

	Ok(network)
}

// FIXME: deduplicate this
pub async fn spawn_network_dispute_valid(
	images: Option<Images>,
	provider: Provider,
) -> Result<Network<LocalFileSystem>, Error> {
	let network_config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let mut builder = r.with_chain("westend-local");
			if let Some(images) = images.as_ref() {
				builder = builder.with_default_image(images.polkadot.as_ref());
			};

			let patch = runtime_config();

			builder
				.with_genesis_overrides(patch)
				.with_default_command("polkadot")
				.with_default_args(vec![
					"--no-hardware-benchmarks".into(),
					"-lparachain=debug,parachain::dispute-coordinator=trace".into(),
				])
				.with_node(|node| node.with_name("honest-0"))
				.with_node(|node| node.with_name("honest-1"))
				.with_node(|node| node.with_name("honest-2"))
				.with_node(|node| node.with_name("honest-3"))
				.with_node(|node| node.with_name("honest-4"))
				.with_node(|node| node.with_name("honest-5"))
				.with_node(|node| node.with_name("honest-6"))
				.with_node(|node| node.with_name("honest-7"))
				.with_node(|node| node.with_name("honest-8"))
				.with_node(|node| {
					let mut node_builder = node.with_name("malicious-disputer");
					if let Some(images) = images.as_ref() {
						node_builder = node_builder.with_image(images.malus.as_ref());
					};

					node_builder
						.with_command("malus")
						.with_subcommand("dispute-ancestor")
						.with_args(vec!["--no-hardware-benchmarks".into(), "-lMALUS=trace".into()])
				})
		})
		.with_parachain(|p| {
			p.with_id(2000).cumulus_based(true).with_collator(|n| {
				let mut node_builder = n.with_name("collator").with_command("polkadot-parachain");

				if let Some(images) = images {
					node_builder = node_builder.with_image(images.cumulus.as_ref())
				};
				node_builder
			})
		})
		.build()
		.unwrap();

	let network = match provider {
		Provider::Native => network_config.spawn_native().await?,
		Provider::K8s => network_config.spawn_k8s().await?,
	};

	Ok(network)
}

// FIXME: deduplicate this too
pub async fn spawn_honest_network(
	images: Option<Images>,
	provider: Provider,
) -> Result<Network<LocalFileSystem>, Error> {
	let network_config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let mut builder = r.with_chain("westend-local");
			if let Some(images) = images.as_ref() {
				builder = builder.with_default_image(images.polkadot.as_ref());
			};
			let patch = runtime_config();

			builder
				.with_genesis_overrides(patch)
				.with_default_command("polkadot")
				.with_default_args(vec![
					"--no-hardware-benchmarks".into(),
					"-lparachain=debug,parachain::dispute-coordinator=trace".into(),
				])
				.with_node(|node| node.with_name("honest-0"))
				.with_node(|node| node.with_name("honest-1"))
				.with_node(|node| node.with_name("honest-2"))
				.with_node(|node| node.with_name("honest-3"))
				.with_node(|node| node.with_name("honest-4"))
				.with_node(|node| node.with_name("honest-5"))
				.with_node(|node| node.with_name("honest-6"))
				.with_node(|node| node.with_name("honest-7"))
				.with_node(|node| node.with_name("honest-8"))
				.with_node(|node| node.with_name("honest-9"))
		})
		.with_parachain(|p| {
			p.with_id(2000).cumulus_based(true).with_collator(|n| {
				let mut node_builder = n.with_name("collator").with_command("polkadot-parachain");

				if let Some(images) = images {
					node_builder = node_builder.with_image(images.cumulus.as_ref())
				};
				node_builder
			})
		})
		.build()
		.unwrap();

	let network = match provider {
		Provider::Native => network_config.spawn_native().await?,
		Provider::K8s => network_config.spawn_k8s().await?,
	};

	Ok(network)
}

pub async fn get_client(
	network: &Network<LocalFileSystem>,
	name: &str,
) -> Result<OnlineClient<PolkadotConfig>, Error> {
	let client = network
		.get_node(name)?
		.client::<subxt::config::polkadot::PolkadotConfig>()
		.await?;
	Ok(client)
}

pub async fn wait_for_block(
	number: u32,
	client: OnlineClient<PolkadotConfig>,
) -> Result<(), Error> {
	println!("Waiting for block #{number}:");
	let mut best = client.blocks().subscribe_best().await?;

	while let Some(block) = best.next().await {
		let n = block?.header().number;
		println!("Current best block: #{n}");
		if n >= number {
			break;
		}
	}

	Ok(())
}

pub async fn wait_for_metric(node: &NetworkNode, metric: &str, value: u64) -> Result<(), Error> {
	println!("Waiting for {metric} to reach {value}:");
	loop {
		let current = node.reports(metric).await.unwrap_or(0.0) as u64;
		println!("{metric} = {current}");
		if current >= value {
			return Ok(());
		}
	}
}

pub async fn get_runtime_version(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<polkadot::runtime_types::sp_version::RuntimeVersion, Error> {
	let call = polkadot::apis().core().version();
	client.runtime_api().at_latest().await?.call(call).await.map_err(|e| e.into())
}

pub async fn perform_runtime_upgrade(
	client: &OnlineClient<PolkadotConfig>,
	code: Vec<u8>,
) -> Result<(), Error> {
	let set_code =
		subxt::dynamic::tx("System", "set_code", vec![subxt::dynamic::Value::from_bytes(code)]);
	let tx = subxt::dynamic::tx("Sudo", "sudo", vec![set_code.into_value()]);
	let signer = dev::alice(); // Alice has got sudo access

	client
		.tx()
		.sign_and_submit_then_watch_default(&tx, &signer)
		.await?
		.wait_for_finalized_success()
		.await
		.map(|_| ())
		.map_err(|e| e.into())
}
