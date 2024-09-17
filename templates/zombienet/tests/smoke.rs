//! This test is setup to run with the `native` provider and needs these binaries in your PATH
//! `polkadot`, `polkadot-prepare-worker`, `polkadot-execute-worker`, `parachain-template-node`.
//! You can follow these steps to compile and export the binaries:
//! `cargo build --release -features fast-runtime --bin polkadot --bin polkadot-execute-worker --bin
//! polkadot-prepare-worker`
//! `cargo build --package parachain-template-node --release`
//! `cargo build --package minimal-template-node --release`
//! `export PATH=<path-to-polkadot-sdk-repo>/target/release:$PATH
//!
//! The you can run the test with
//! `cargo test -p template-zombienet-tests`

#[cfg(feature = "zombienet")]
mod smoke {
	use anyhow::anyhow;
	use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder, NetworkConfigExt};

	pub fn get_config(cmd: &str, para_cmd: Option<&str>) -> Result<NetworkConfig, anyhow::Error> {
		let chain = if cmd == "polkadot" { "rococo-local" } else { "dev" };
		let config = NetworkConfigBuilder::new().with_relaychain(|r| {
			r.with_chain(chain)
				.with_default_command(cmd)
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		});

		let config = if let Some(para_cmd) = para_cmd {
			config.with_parachain(|p| {
				p.with_id(1000)
					.with_default_command(para_cmd)
					.with_collator(|n| n.with_name("collator"))
			})
		} else {
			config
		};

		config.build().map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn parachain_template_block_production_test() -> Result<(), anyhow::Error> {
		let _ = env_logger::try_init_from_env(
			env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
		);

		let config = get_config("polkadot", Some("parachain-template-node"))?;

		let network = config.spawn_native().await?;

		// wait 6 blocks of the para
		let collator = network.get_node("collator")?;
		assert!(collator
			.wait_metric("block_height{status=\"best\"}", |b| b > 5_f64)
			.await
			.is_ok());

		Ok(())
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn solochain_template_block_production_test() -> Result<(), anyhow::Error> {
		let _ = env_logger::try_init_from_env(
			env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
		);

		let config = get_config("solochain-template-node", None)?;

		let network = config.spawn_native().await?;

		// wait 6 blocks
		let alice = network.get_node("alice")?;
		assert!(alice.wait_metric("block_height{status=\"best\"}", |b| b > 5_f64).await.is_ok());

		Ok(())
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn minimal_template_block_production_test() -> Result<(), anyhow::Error> {
		let _ = env_logger::try_init_from_env(
			env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
		);

		let config = get_config("minimal-template-node", None)?;

		let network = config.spawn_native().await?;

		// wait 6 blocks
		let alice = network.get_node("alice")?;
		assert!(alice.wait_metric("block_height{status=\"best\"}", |b| b > 5_f64).await.is_ok());

		Ok(())
	}
}
