use pallet_revive_eth_rpc::subxt_client::{self, SrcChainConfig};
use sp_weights::Weight;
use subxt::OnlineClient;
use subxt_signer::sr25519::dev;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let client = OnlineClient::<SrcChainConfig>::new().await?;

	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;

	let tx_payload = subxt_client::tx().revive().instantiate_with_code(
		0u32.into(),
		Weight::from_parts(100_000, 0).into(),
		3_000_000_000_000_000u128.into(),
		bytes,
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
