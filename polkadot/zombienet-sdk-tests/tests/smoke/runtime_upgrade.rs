use anyhow::anyhow;
use std::env;
use zombienet_sdk::NetworkConfigBuilder;

const best_block_metric: &str = "block_height{status=\"best\"}";

#[tokio::test(flavor = "multi_thread")]
async fn runtime_upgrade_test() -> Result<(), anyhow::Error> {
	env_logger::init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_node(|node| {
					node.with_name("latest-release")
						// used for getting the chain-spec
						.with_image("parity/polkadot:latest")
				})
				.with_node(|node| node.with_name("bob"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	// wait 10 blocks
	let latest_release = network.get_node("latest-release")?;
	assert!(latest_release.wait_metric(best_block_metric, |b| b > 10_f64).await.is_ok());

	// get current runtime spec
	let client = network.get_node("latest-release")?.client::<subxt::PolkadotConfig>().await?;
	let current_runtime = client.backend().current_runtime_version().await?;

	// get current best
	node.wait_metric(best_block_metric, |x| x > 10_f64).await?;
	let best_block = node.reports(best_block_metric).await?;

	// upgrade runtime
	let wasm = env::var("ZOMBIE_WASM_INCREMENTED_PATH").unwrap_or(
		"target/testnet/wbuild/rococo-runtime/wasm_binary_spec_version_incremented.rs.compact.wasm",
	);

	network
		.relaychain()
		.runtime_upgrade(RuntimeUpgradeOptions::new(wasm.into()))
		.await?;

	// wait 10 more blocks
	node.wait_metric(best_block_metric, |x| x > best_block + 10_f64);

	let incremented_runtime = client.backend().current_runtime_version().await?;

	assert_eq(incremented_runtime, current_version + 1000, "version should be incremented");
}
