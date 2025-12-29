use crate::subxt_client::{system::events, Error};
use pallet_revive_eth_rpc::subxt_client::{
	self,
	revive::{calls::types::InstantiateWithCode, events::Instantiated},
	SrcChainConfig,
};
use sp_weights::Weight;
use subxt::OnlineClient;
use subxt_signer::sr25519::dev;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let client = OnlineClient::<SrcChainConfig>::new().await?;

	let args: Vec<String> = std::env::args().collect();
	let contract_path = args.get(1).expect("Usage: move <contract.polkavm> <selector>");
	let bytes = std::fs::read(contract_path)?;
	let selector =
		hex::decode(args.get(2).expect("selector not present")).expect("selector not in hex");

	let tx_payload = subxt_client::tx().revive().instantiate_with_code(
		0u32.into(),
		Weight::from_parts(100_000, 0).into(),
		3_000_000_000_000_000_000,
		bytes,
		vec![0xfa, 0x1e, 0x1f, 0x30],
		None,
	);

	let signed_extrinsic = client
		.tx()
		.create_signed(&tx_payload, &dev::alice(), Default::default())
		.await?;

	let events = signed_extrinsic
		.submit_and_watch()
		.await
		.unwrap()
		.wait_for_finalized_success()
		.await?;

	let instantiated = events
		.find_first::<Instantiated>()?
		.expect("Failed to find a Instantiated event");
	let _extrinsic_success = events
		.find_first::<events::ExtrinsicSuccess>()?
		.expect("Failed to find a ExtrinsicSuccess event");

	let contract_address = instantiated.contract;
	println!("Contract deployed at: {:?}", contract_address);

	let call_payload = subxt_client::tx().revive().call(
		contract_address,
		1234u128,
		Weight::from_parts(100_000_000_000, 500_000).into(),
		3_000_000_000_000_000_000,
		selector,
	);
	let res = client
		.tx()
		.sign_and_submit_then_watch_default(&call_payload, &dev::alice())
		.await?
		.wait_for_finalized()
		.await?;
	println!("result: {:?}", res.extrinsic_hash());
	Ok(())
}
