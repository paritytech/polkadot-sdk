//! This test is setup to run with the `native` provider and needs these binaries in your PATH
//! `polkadot`, `polkadot-prepare-worker`, `polkadot-execute-worker`, `parachain-template-node`.
//! You can follow these steps to compile and export the binaries:
//! `cargo build --release -features fast-runtime --bin polkadot --bin polkadot-execute-worker --bin
//! polkadot-prepare-worker`
//! `cargo build --package parachain-template-node --release`
//! `export PATH=<path-to-polkadot-sdk-repo>/target/release:$PATH
//!
//! The you can run the test with
//! `cargo test -p parachain-template-zombienet`

use anyhow::anyhow;
use zombienet_sdk::{NetworkConfigBuilder, NetworkConfigExt};

#[tokio::test(flavor = "multi_thread")]
async fn block_production_test() -> Result<(), anyhow::Error> {
	env_logger::init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_node(|node| node.with_name("alice").with_rpc_port(9944))
				.with_node(|node| node.with_name("bob").with_rpc_port(9955))
		})
		.with_parachain(|p| {
			p.with_id(1000)
				.with_default_command("parachain-template-node")
				.with_collator(|n| n.with_name("collator").with_rpc_port(9988))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let network = config.spawn_native().await?;

	// wait 6 blocks of the para
	let collator = network.get_node("collator")?;
	assert!(collator
		.wait_metric("block_height{status=\"best\"}", |b| b > 5_f64)
		.await
		.is_ok());

	Ok(())
}
