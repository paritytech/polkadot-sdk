use ethers::{
	core::types::{Address, U256},
	utils::parse_units,
};
use futures::StreamExt;
use snowbridge_smoketest::{
	constants::*,
	contracts::{i_gateway, weth9},
	helper::{initial_clients, print_event_log_for_unit_tests},
	parachains::assethub::api::{
		foreign_assets::events::Issued,
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
use sp_core::Encode;
use subxt::utils::AccountId32;

#[tokio::test]
async fn send_token() {
	let test_clients = initial_clients().await.expect("initialize clients");
	let ethereum_client = *(test_clients.ethereum_signed_client.clone());
	let assethub = *(test_clients.asset_hub_client.clone());

	let gateway_addr: Address = GATEWAY_PROXY_CONTRACT.into();
	let gateway = i_gateway::IGateway::new(gateway_addr, ethereum_client.clone());

	let weth_addr: Address = WETH_CONTRACT.into();
	let weth = weth9::WETH9::new(weth_addr, ethereum_client.clone());

	// Mint WETH tokens
	let value = parse_units("1", "ether").unwrap();
	let receipt = weth.deposit().value(value).send().await.unwrap().await.unwrap().unwrap();
	assert_eq!(receipt.status.unwrap().as_u64(), 1u64);

	// Approve token spend
	weth.approve(gateway_addr, value.into())
		.send()
		.await
		.unwrap()
		.await
		.unwrap()
		.unwrap();
	assert_eq!(receipt.status.unwrap().as_u64(), 1u64);

	// Lock tokens into vault
	let amount: u128 = U256::from(value).low_u128();
	let fee: u128 = 30_000_000_000_000_000;
	let receipt = gateway
		.send_token(
			weth.address(),
			ASSET_HUB_PARA_ID,
			i_gateway::MultiAddress { kind: 1, data: FERDIE.into() },
			0,
			amount,
		)
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
	let expected_owner: AccountId32 = FERDIE.into();

	let mut issued_event_found = false;
	while let Some(Ok(block)) = blocks.next().await {
		println!("Polling assethub block {} for issued event.", block.number());

		let events = block.events().await.unwrap();
		for issued in events.find::<Issued>() {
			println!("Created event found in assethub block {}.", block.number());
			let issued = issued.unwrap();
			assert_eq!(issued.asset_id.encode(), expected_asset_id.encode());
			assert_eq!(issued.owner, expected_owner);
			assert_eq!(issued.amount, amount);
			issued_event_found = true;
		}
		if issued_event_found {
			break
		}
	}
	assert!(issued_event_found)
}
