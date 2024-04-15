use zombienet_sdk_tests::environment::*;
use zombienet_sdk_tests::paras::{wait_is_registered, wait_para_block_height_from_heads};

use zombienet_sdk::{NetworkConfigBuilder, NetworkConfigExt};
use std::time::Duration;

// [v1 test](../../polkadot/zombienet_test)
#[tokio::test(flavor = "multi_thread")]
async fn parachains_smoke() {
    tracing_subscriber::fmt::init();
    let images: Images = get_images_from_env();

    // build network configuration
	let network_config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
            r
                .with_chain("rococo-local")
                .with_default_image(images.polkadot.as_ref())
				.with_default_command("polkadot")
				.with_default_args(vec![
                    "-lruntime=debug,parachain=trace".into(),
				])
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p
            .with_id(100)
            .cumulus_based(false)
            .with_collator(|n| {
                n
                .with_name("collator")
                .with_image(images.colander.as_ref())
                .with_command("adder-collator")
                // .with_command("polkadot-parachain")
                .with_args(vec!["-lruntime=debug,parachain=trace".into()])
			})
		})
		.build()
		.unwrap();

	let network = match get_provider_from_env() {
		Provider::Native => network_config.spawn_native().await.unwrap(),
		Provider::K8s => network_config.spawn_k8s().await.unwrap(),
	};

    // continue with the test here...
    println!("🚀🚀🚀🚀 network deployed");

    // give some time to node's bootstraping
    tokio::time::sleep(Duration::from_secs(2)).await;

    let alice = network.get_node("alice").unwrap();
    // Assertions
    let alice_client = alice
        .client::<subxt::PolkadotConfig>()
        .await.unwrap();

    // alice: parachain 100 is registered within 225 seconds
    let registered = wait_is_registered(&alice_client, 100, Some(120)).await;
    assert!(
        matches!(registered, Ok(true) if registered.is_ok()),
        "❌ Parachain 100 is not registered in 120 secs"
    );

    // alice: parachain 100 block height is at least 10 within 400 seconds

    let cmp = |n| n >= 10;
    let res = wait_para_block_height_from_heads(&alice_client, 100, cmp, Some(400)).await;
    assert!(
        matches!(res, Ok(())),
        "❌ Parachain 100 block height doesn't reach 10 in 400 secs"
    );
}
