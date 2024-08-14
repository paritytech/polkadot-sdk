//! This test is setup to run with the `native` provider and needs these binaries in your PATH
//! `polkadot`, `polkadot-prepare-worker`, `polkadot-execute-worker`, `parachain-template-node`.
//! You can follow these steps to compile and export the binaries:
//! `cargo build --release -features fast-runtime --bin polkadot --bin polkadot-execute-worker --bin
//! polkadot-prepare-worker`
//! `cargo build --package parachain-template-node --release`
//! `export PATH=<path-to-polkadot-sdk-repo>/target/release:$PATH
//!
//! The you can run the test with
//! `cargo test -p template-zombienet-tests`

#[cfg(feature = "zombienet")]
mod smoke {
	use zombienet_sdk::NetworkConfigExt;
	use template_zombienet_tests::{get_config, requirements_are_meet};

	#[tokio::test(flavor = "multi_thread")]
	async fn parachain_template_block_production_test() -> Result<(), anyhow::Error> {
		let _ = env_logger::try_init_from_env(
			env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
		);

		requirements_are_meet(&vec!["polkadot", "parachain-template-node"])?;

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

		requirements_are_meet(&vec!["solochain-template-node"])?;

		let config = get_config("solochain-template-node", None)?;

		let network = config.spawn_native().await?;

		// wait 6 blocks
		let alice = network.get_node("alice")?;
		assert!(alice
			.wait_metric("block_height{status=\"best\"}", |b| b > 5_f64)
			.await
			.is_ok());

		Ok(())
	}


	#[tokio::test(flavor = "multi_thread")]
	async fn minimal_template_block_production_test() -> Result<(), anyhow::Error> {
		let _ = env_logger::try_init_from_env(
			env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
		);

		requirements_are_meet(&vec!["minimal-template-node"])?;

		let config = get_config("minimal-template-node", None)?;

		let network = config.spawn_native().await?;

		// wait 6 blocks
		let alice = network.get_node("alice")?;
		assert!(alice
			.wait_metric("block_height{status=\"best\"}", |b| b > 5_f64)
			.await
			.is_ok());

		Ok(())
	}
}