use pallet_revive_eth_rpc::subxt_client::{self, SrcChainConfig};
use sp_weights::Weight;
use subxt::OnlineClient;
use subxt_signer::sr25519::dev;

static DUMMY_BYTES: &[u8] = include_bytes!("./dummy.polkavm");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let client = OnlineClient::<SrcChainConfig>::new().await?;

	println!("\n\n=== Deploying contract ===\n\n");

	let tx_payload = subxt_client::tx().revive().instantiate_with_code(
		0u32.into(),
		Weight::from_parts(2_000, 2_000).into(),
		10000000u32.into(),
		DUMMY_BYTES.to_vec(),
		vec![],
		None,
	);

	let res = client
		.tx()
		.sign_and_submit_then_watch_default(&tx_payload, &dev::alice())
		.await?
		.wait_for_finalized_success()
		.await?;
	println!("Transaction finalized: {:?}", res.extrinsic_hash());

	Ok(())
}
