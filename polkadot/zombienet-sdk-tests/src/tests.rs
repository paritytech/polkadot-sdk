use crate::*;
use std::{env, path::Path, time::Duration};
use tokio::time::sleep;

const DISPUTES_TOTAL_METRIC: &str = "polkadot_parachain_candidate_disputes_total";
const DISPUTES_CONCLUDED_VALID: &str =
	"polkadot_parachain_candidate_dispute_concluded{validity=\"valid\"}";

#[tokio::test]
async fn test_backing_disabling() -> Result<(), Error> {
	tracing_subscriber::fmt::init();

	let network = spawn_network_malus_backer().await?;

	println!("🚀🚀🚀 network deployed");

	let honest = network.get_node("honest-0")?;
	let role = honest.reports("node_roles").await?;
	assert_eq!(role as u64, 4);

	let collator_client = get_client(&network, "collator").await?;

	wait_for_block(1, collator_client).await?;

	wait_for_metric(honest, DISPUTES_TOTAL_METRIC, 1).await?;

	let honest_client = honest.client::<subxt::PolkadotConfig>().await?;

	// wait until we have the malicious validator disabled
	loop {
		let call = polkadot::apis().parachain_host().disabled_validators();
		let disabled = honest_client.runtime_api().at_latest().await?.call(call).await?;
		if disabled.len() == 1 {
			break;
		}
		sleep(Duration::from_secs(5)).await;
	}

	// NOTE: there's a race condition possible
	// after the validator got disabled, but disputes are still ongoing
	// wait for a couple of blocks to avoid it
	sleep(Duration::from_secs(12)).await;

	// get the current disputes metric
	let total_disputes = honest.reports(DISPUTES_TOTAL_METRIC).await? as u64;

	// wait a bit
	sleep(Duration::from_secs(120)).await;

	let new_total_disputes = honest.reports(DISPUTES_TOTAL_METRIC).await? as u64;

	// ensure that no new disputes were created after validator got disabled
	assert_eq!(total_disputes, new_total_disputes);

	Ok(())
}

#[tokio::test]
async fn test_disputes_offchain_disabling() -> Result<(), Error> {
	tracing_subscriber::fmt::init();

	let network = spawn_network_dispute_valid().await?;

	println!("🚀🚀🚀 network deployed");

	let honest = network.get_node("honest-0")?;
	let role = honest.reports("node_roles").await?;
	assert_eq!(role as u64, 4);

	let collator_client = get_client(&network, "collator").await?;

	wait_for_block(1, collator_client).await?;

	wait_for_metric(honest, DISPUTES_CONCLUDED_VALID, 1).await?;

	// NOTE: there's a race condition possible
	// after the dispute concluded and before the validator got disabled
	// wait for a block to avoid it
	sleep(Duration::from_secs(6)).await;

	// get the current disputes metric
	let total_disputes = honest.reports(DISPUTES_CONCLUDED_VALID).await? as u64;

	// wait a bit
	sleep(Duration::from_secs(120)).await;

	let new_total_disputes = honest.reports(DISPUTES_CONCLUDED_VALID).await? as u64;

	// ensure that no new disputes were created after validator got disabled offchain
	assert_eq!(total_disputes, new_total_disputes);

	Ok(())
}

// The test is intend to work with pre-disabling binaries (e.g. polkadot 1.6) and to test a runtime
// upgrade with a runtime from https://github.com/paritytech/polkadot-sdk/pull/2226.
// The test expects pre-disabling binaries to be in system PATH before running it.
// The test mimics what is likely to happen in the real deployment - old nodes get updated first and
// then a runtime upgrade is performed.
#[tokio::test]
async fn test_runtime_upgrade() -> Result<(), Error> {
	tracing_subscriber::fmt::init();

	let network = spawn_honest_network().await?;

	println!("🚀🚀🚀 network deployed");

	let honest = network.get_node("honest-0")?;
	let role = honest.reports("node_roles").await?;
	assert_eq!(role as u64, 4);

	// No disputes in honest network
	wait_for_metric(honest, DISPUTES_CONCLUDED_VALID, 0).await?;

	// ensure old binary is running
	// Right now zombienet doesn't parse metric labels so querying exact node version is a bit
	// problematic. The workaround is to get the binary version manually and hardcode it here. I'm
	// leaving the line below for reference but disabled because it is frustrating to keep it up to
	// date. assert_eq!(honest.reports("substrate_build_info{name=\"honest-0\",version=\"1.6.
	// 0-481165d9229\",chain=\"westend_local_testnet\"}").await?, 1_f64);

	perform_nodes_upgrade(&network).await?;

	let client = get_client(&network, "honest-0").await?;
	// ensure new binary is running - see the comment for 'ensure old binary is running'
	// assert_eq!(honest.reports("substrate_build_info{name=\"honest-0\",version=\"1.6.
	// 0-5c3e98e9d3e\",chain=\"westend_local_testnet\"}").await?, 1_f64);

	// Netowrk is still healthy
	assert_blocks_are_being_finalized(&client).await?;

	// get runtime version before the upgrade
	let version_before = get_runtime_version(&client).await?;

	// runtime upgrade
	// Put a runtime from the disabling branch compiled with 'fast runtime'
	// as artifacts/westend_runtime.compact.compressed.wasm
	// compile command: `cargo build --release --features=fast-runtime -p westend-runtime`
	let code = std::fs::read(
		Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("artifacts/westend_runtime.compact.compressed.wasm"),
	)?;

	println!("Starting runtime upgrade");
	perform_runtime_upgrade(&client, code).await?;
	println!("🚀🚀🚀 runtime upgrade complete");

	assert_blocks_are_being_finalized(&client).await?;

	// get runtime version after the upgrade - is it newer?
	let version_after = get_runtime_version(&client).await?;
	assert!(version_after.spec_version > version_before.spec_version);

	// still no disputes
	wait_for_metric(honest, DISPUTES_CONCLUDED_VALID, 0).await?;

	Ok(())
}

// All prereqs for `test_runtime_upgrade` are valid here too.
// The test performs runtime upgrade with old client nodes. They should work fine with the new
// runtime.
#[tokio::test]
async fn test_runtime_upgrade_with_old_client() -> Result<(), Error> {
	tracing_subscriber::fmt::init();

	let network = spawn_honest_network().await?;

	println!("🚀🚀🚀 network deployed");

	let honest = network.get_node("honest-0")?;
	let role = honest.reports("node_roles").await?;
	assert_eq!(role as u64, 4);

	// No disputes in honest network
	wait_for_metric(honest, DISPUTES_CONCLUDED_VALID, 0).await?;

	let client = get_client(&network, "honest-0").await?;

	// Netowrk is still healthy
	assert_blocks_are_being_finalized(&client).await?;

	// get runtime version before the upgrade
	let version_before = get_runtime_version(&client).await?;

	// runtime upgrade
	let code = std::fs::read(
		Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("artifacts/westend_runtime.compact.compressed.wasm"),
	)?;

	println!("Starting runtime upgrade");
	perform_runtime_upgrade(&client, code).await?;
	println!("🚀🚀🚀 runtime upgrade complete");

	assert_blocks_are_being_finalized(&client).await?;

	// get runtime version after the upgrade - is it newer?
	let version_after = get_runtime_version(&client).await?;
	assert!(version_after.spec_version > version_before.spec_version);

	// still no disputes
	wait_for_metric(honest, DISPUTES_CONCLUDED_VALID, 0).await?;

	Ok(())
}

async fn assert_blocks_are_being_finalized(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<(), Error> {
	let mut finalized_blocks = client.blocks().subscribe_finalized().await?;
	let first_measurement = finalized_blocks
		.next()
		.await
		.ok_or(Error::from("Can't get finalized block from stream"))??
		.number();
	let second_measurement = finalized_blocks
		.next()
		.await
		.ok_or(Error::from("Can't get finalized block from stream"))??
		.number();

	assert!(second_measurement > first_measurement);

	Ok(())
}

// This function contains hacks.
// At the moment zombienet doesn't support restarting a process with a new binary. As a work around
// system PATH is modified (by prepending a new location) before restarting the processes. This way
// zombienet will start the new binary and perform a 'client update'.
// The function also expects 'the new binaries' to be located in `artifacts/polkadot-disabling`.
// Building a set of binaries can be done with:
// `cargo build --profile testnet --features pyroscope,fast-runtime \
// -p test-parachain-adder-collator -p test-parachain-undying-collator -p polkadot-test-malus -p
// polkadot -p polkadot-parachain-bin`. List of the required binaries:
//  * target/testnet/adder-collator
//  * target/testnet/malus
//  * target/testnet/polkadot
//  * target/testnet/polkadot-execute-worker
//  * target/testnet/polkadot-prepare-worker
//  * target/testnet/undying-collator
//  * target/testnet/polkadot-parachain
pub async fn perform_nodes_upgrade(network: &Network<LocalFileSystem>) -> Result<(), Error> {
	prepend_to_system_path(
		Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("artifacts/polkadot-disabling")
			.as_path(),
	)
	.await?;

	println!("Starting nodes update");

	for node in network.nodes() {
		node.restart(Some(Duration::from_secs(5))).await?;
	}

	println!("🚀🚀🚀 nodes update complete");

	Ok(())
}

// Prepends new location to the PATH variable
async fn prepend_to_system_path(new_path: &Path) -> Result<(), Error> {
	let current_path = env::var("PATH")?;
	let mut updated_path = new_path.to_str().unwrap().to_string();
	updated_path.push_str(":");
	updated_path.push_str(&current_path);
	env::set_var("PATH", updated_path.clone());
	assert_eq!(env::var("PATH"), Ok(updated_path));

	Ok(())
}
