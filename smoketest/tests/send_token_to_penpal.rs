use ethers::{
	core::types::{Address, U256},
	utils::parse_units,
};
use futures::StreamExt;
use snowbridge_smoketest::{
	constants::*,
	contracts::{i_gateway, weth9},
	helper::{initial_clients, PenpalConfig},
	parachains::{
		assethub::api::{
			foreign_assets::events::Issued as AssetHubIssued,
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
		penpal::{self, api::foreign_assets::events::Issued as PenpalIssued},
	},
};
use sp_core::{sr25519::Pair, Encode, Pair as PairT};
use subxt::{
	tx::PairSigner,
	utils::{AccountId32, MultiAddress},
	OnlineClient,
};

#[tokio::test]
async fn send_token_to_penpal() {
	let test_clients = initial_clients().await.expect("initialize clients");
	let ethereum_client = *(test_clients.ethereum_signed_client.clone());
	let assethub_client = *(test_clients.asset_hub_client.clone());
	let penpal_client = *(test_clients.penpal_client.clone());

	let gateway_addr: Address = GATEWAY_PROXY_CONTRACT.into();
	let gateway = i_gateway::IGateway::new(gateway_addr, ethereum_client.clone());

	let weth_addr: Address = WETH_CONTRACT.into();
	let weth = weth9::WETH9::new(weth_addr, ethereum_client.clone());

	// Mint WETH tokens
	let value = parse_units("1", "ether").unwrap();
	let receipt = weth.deposit().value(value).send().await.unwrap().await.unwrap().unwrap();
	assert_eq!(receipt.status.unwrap().as_u64(), 1u64);

	ensure_penpal_asset_exists(&mut test_clients.penpal_client.clone()).await;

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
	let receipt = gateway
		.send_token(
			weth.address(),
			PENPAL_PARA_ID,
			i_gateway::MultiAddress { kind: 1, data: FERDIE.into() },
			amount,
		)
		.value(1000)
		.send()
		.await
		.unwrap()
		.await
		.unwrap()
		.unwrap();

	println!("receipt: {:#?}", receipt);

	assert_eq!(receipt.status.unwrap().as_u64(), 1u64);

	let wait_for_blocks = 50;
	let mut assethub_blocks = assethub_client
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
	let assethub_expected_owner: AccountId32 = PENPAL_SOVEREIGN.into();

	let mut issued_event_found = false;
	while let Some(Ok(block)) = assethub_blocks.next().await {
		println!("Polling assethub block {} for issued event.", block.number());

		let events = block.events().await.unwrap();
		for issued in events.find::<AssetHubIssued>() {
			println!("Created event found in assethub block {}.", block.number());
			let issued = issued.unwrap();
			assert_eq!(issued.asset_id.encode(), expected_asset_id.encode());
			assert_eq!(issued.owner, assethub_expected_owner);
			assert_eq!(issued.amount, amount);
			issued_event_found = true;
		}
		if issued_event_found {
			break
		}
	}
	assert!(issued_event_found);

	let mut penpal_blocks = penpal_client
		.blocks()
		.subscribe_finalized()
		.await
		.expect("block subscription")
		.take(wait_for_blocks);

	let penpal_expected_owner: AccountId32 = FERDIE.into();

	issued_event_found = false;
	while let Some(Ok(block)) = penpal_blocks.next().await {
		println!("Polling penpal block {} for issued event.", block.number());

		let events = block.events().await.unwrap();
		for issued in events.find::<PenpalIssued>() {
			println!("Created event found in penpal block {}.", block.number());
			let issued = issued.unwrap();
			assert_eq!(issued.asset_id.encode(), expected_asset_id.encode());
			assert_eq!(issued.owner, penpal_expected_owner);
			assert_eq!(issued.amount, amount);
			issued_event_found = true;
		}
		if issued_event_found {
			break
		}
	}
	assert!(issued_event_found);
}

async fn ensure_penpal_asset_exists(penpal_client: &mut OnlineClient<PenpalConfig>) {
	use penpal::api::runtime_types::{
		staging_xcm::v3::multilocation::MultiLocation,
		xcm::v3::{
			junction::{
				Junction::{AccountKey20, GlobalConsensus},
				NetworkId,
			},
			junctions::Junctions::X2,
		},
	};
	let penpal_asset_id = MultiLocation {
		parents: 2,
		interior: X2(
			GlobalConsensus(NetworkId::Ethereum { chain_id: ETHEREUM_CHAIN_ID }),
			AccountKey20 { network: None, key: WETH_CONTRACT.into() },
		),
	};

	let penpal_asset_address = penpal::api::storage().foreign_assets().asset(&penpal_asset_id);
	let result = penpal_client
		.storage()
		.at(None)
		.await
		.unwrap()
		.fetch(&penpal_asset_address)
		.await
		.unwrap();

	if result.is_some() {
		println!("WETH asset exists on penpal.");
		return
	}

	println!("creating WETH on penpal.");
	let admin = MultiAddress::Id(ASSET_HUB_SOVEREIGN.into());
	let keypair: Pair = Pair::from_string("//Ferdie", None).expect("cannot create keypair");
	let signer: PairSigner<PenpalConfig, _> = PairSigner::new(keypair);

	let create_asset_call = penpal::api::tx().foreign_assets().create(penpal_asset_id, admin, 1);
	penpal_client
		.tx()
		.sign_and_submit_then_watch_default(&create_asset_call, &signer)
		.await
		.unwrap()
		.wait_for_finalized_success()
		.await
		.expect("asset created");
}
