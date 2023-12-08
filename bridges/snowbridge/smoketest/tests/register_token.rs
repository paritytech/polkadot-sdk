use codec::Encode;
use ethers::{core::types::Address, utils::parse_units};
use futures::StreamExt;
use snowbridge_smoketest::{
	constants::*,
	contracts::{i_gateway, weth9},
	helper::{initial_clients, print_event_log_for_unit_tests},
	parachains::assethub::api::{
		foreign_assets::events::Created,
		runtime_types::{
			staging_xcm::v3::multilocation::MultiLocation,
			xcm::v3::{
				junction::{
					Junction::{AccountKey20, GlobalConsensus},
					NetworkId,
				},
				junctions::Junctions::X2,
			},
		},
	},
};
use subxt::utils::AccountId32;

#[tokio::test]
async fn register_token() {
	let test_clients = initial_clients().await.expect("initialize clients");
	let ethereum_client = *(test_clients.ethereum_signed_client.clone());
	let assethub = *(test_clients.asset_hub_client.clone());

	let gateway_addr: Address = GATEWAY_PROXY_CONTRACT.into();
	let gateway = i_gateway::IGateway::new(gateway_addr, ethereum_client.clone());

	let weth_addr: Address = WETH_CONTRACT.into();
	let weth = weth9::WETH9::new(weth_addr, ethereum_client.clone());

	let fee = parse_units(2, "ether").unwrap();

	let receipt = gateway
		.register_token(weth.address())
		.value(fee)
		.send()
		.await
		.unwrap()
		.await
		.unwrap()
		.unwrap();

	println!("receipt transaction hash: {:#?}", hex::encode(receipt.transaction_hash));

	// Log for OutboundMessageAccepted
	let outbound_message_accepted_log = receipt.logs.last().unwrap();

	// print log for unit tests
	print_event_log_for_unit_tests(outbound_message_accepted_log);

	assert_eq!(receipt.status.unwrap().as_u64(), 1u64);

	let wait_for_blocks = 50;
	let mut blocks = assethub
		.blocks()
		.subscribe_finalized()
		.await
		.expect("block subscription")
		.take(wait_for_blocks);

	let expected_asset_id: MultiLocation = MultiLocation {
		parents: 2,
		interior: X2(
			GlobalConsensus(NetworkId::Ethereum { chain_id: ETHEREUM_CHAIN_ID }),
			AccountKey20 { network: None, key: WETH_CONTRACT.into() },
		),
	};
	let expected_creator: AccountId32 = SNOWBRIDGE_SOVEREIGN.into();
	let expected_owner: AccountId32 = SNOWBRIDGE_SOVEREIGN.into();

	let mut created_event_found = false;
	while let Some(Ok(block)) = blocks.next().await {
		println!("Polling assethub block {} for created event.", block.number());

		let events = block.events().await.unwrap();
		for created in events.find::<Created>() {
			println!("Created event found in assethub block {}.", block.number());
			let created = created.unwrap();
			assert_eq!(created.asset_id.encode(), expected_asset_id.encode());
			assert_eq!(created.creator, expected_creator);
			assert_eq!(created.owner, expected_owner);
			created_event_found = true;
		}
		if created_event_found {
			break
		}
	}
	assert!(created_event_found)
}
