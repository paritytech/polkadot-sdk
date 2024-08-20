use anyhow::anyhow;
use zombienet_sdk::NetworkConfigBuilder;

// Chain generator command template
const CMD_TPL: &str = "chain-spec-generator {{chainName}}";
const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

#[tokio::test(flavor = "multi_thread")]
async fn polkadot_coretime_test() -> Result<(), anyhow::Error> {
	env_logger::init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("polkadot-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_chain_spec_command(CMD_TPL)
				.chain_spec_command_is_local(true)
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(1005)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain_spec_command(CMD_TPL)
				.chain_spec_command_is_local(true)
				.with_chain("coretime-polkadot-local")
				.with_collator(|n| n.with_name("coretime-collator"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	// wait 6 blocks of relay/para
	let at_lest_6 = |b| b > 5_f64;

	let alice = network.get_node("alice")?;
	assert!(alice.wait_metric(BEST_BLOCK_METRIC, at_lest_6).await.is_ok());

	let collator = network.get_node("coretime-collator")?;
	assert!(collator.wait_metric(BEST_BLOCK_METRIC, at_lest_6).await.is_ok());

	Ok(())
}
