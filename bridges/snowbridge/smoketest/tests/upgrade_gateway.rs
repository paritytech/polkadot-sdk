use std::{sync::Arc, time::Duration};

use ethers::{
	abi::{Address, Token},
	prelude::*,
	providers::{Provider, Ws},
	utils::keccak256,
};
use futures::StreamExt;
use hex_literal::hex;
use snowbridge_smoketest::{
	constants::*,
	contracts::{
		gateway_upgrade_mock::{self, InitializedFilter},
		i_gateway::{self, UpgradedFilter},
	},
	parachains::{
		bridgehub::{
			self,
			api::{ethereum_system, runtime_types::snowbridge_core::outbound::v1::Initializer},
		},
		relaychain,
		relaychain::api::runtime_types::{
			pallet_xcm::pallet::Call,
			rococo_runtime::RuntimeCall,
			sp_weights::weight_v2::Weight,
			staging_xcm::v3::multilocation::MultiLocation,
			xcm::{
				double_encoded::DoubleEncoded,
				v2::OriginKind,
				v3::{junction::Junction, junctions::Junctions, Instruction, WeightLimit, Xcm},
				VersionedMultiLocation, VersionedXcm,
			},
		},
	},
};
use sp_core::{sr25519::Pair, Pair as PairT};
use subxt::{
	tx::{PairSigner, TxPayload},
	OnlineClient, PolkadotConfig,
};

const GATETWAY_UPGRADE_MOCK_CONTRACT: [u8; 20] = hex!("f8f7758fbcefd546eaeff7de24aff666b6228e73");

#[tokio::test]
async fn upgrade_gateway() {
	let ethereum_provider = Provider::<Ws>::connect(ETHEREUM_API)
		.await
		.unwrap()
		.interval(Duration::from_millis(10u64));

	let ethereum_client = Arc::new(ethereum_provider);

	let gateway_addr: Address = GATEWAY_PROXY_CONTRACT.into();
	let gateway = i_gateway::IGateway::new(gateway_addr, ethereum_client.clone());

	let mock_gateway_addr: Address = GATETWAY_UPGRADE_MOCK_CONTRACT.into();
	let mock_gateway =
		gateway_upgrade_mock::GatewayUpgradeMock::new(mock_gateway_addr, ethereum_client.clone());

	let relaychain: OnlineClient<PolkadotConfig> =
		OnlineClient::from_url(RELAY_CHAIN_WS_URL).await.unwrap();
	let bridgehub: OnlineClient<PolkadotConfig> =
		OnlineClient::from_url(BRIDGE_HUB_WS_URL).await.unwrap();

	let sudo: Pair = Pair::from_string("//Alice", None).expect("cannot create sudo keypair");

	let signer: PairSigner<PolkadotConfig, _> = PairSigner::new(sudo);

	let ethereum_system_api = bridgehub::api::ethereum_system::calls::TransactionApi;

	let d_0 = 99;
	let d_1 = 66;
	let params = ethers::abi::encode(&[Token::Uint(d_0.into()), Token::Uint(d_1.into())]);

	let code = ethereum_client
		.get_code(NameOrAddress::Address(GATETWAY_UPGRADE_MOCK_CONTRACT.into()), None)
		.await
		.unwrap();

	let gateway_upgrade_mock_code_hash = keccak256(code);

	// The upgrade call
	let upgrade_call = ethereum_system_api
		.upgrade(
			GATETWAY_UPGRADE_MOCK_CONTRACT.into(),
			gateway_upgrade_mock_code_hash.into(),
			Some(Initializer { params, maximum_required_gas: 100_000 }),
		)
		.encode_call_data(&bridgehub.metadata())
		.expect("encoded call");

	let weight = 3000000000;
	let proof_size = 18000;

	let dest = Box::new(VersionedMultiLocation::V3(MultiLocation {
		parents: 0,
		interior: Junctions::X1(Junction::Parachain(BRIDGE_HUB_PARA_ID)),
	}));
	let message = Box::new(VersionedXcm::V3(Xcm(vec![
		Instruction::UnpaidExecution { weight_limit: WeightLimit::Unlimited, check_origin: None },
		Instruction::Transact {
			origin_kind: OriginKind::Superuser,
			require_weight_at_most: Weight { ref_time: weight, proof_size },
			call: DoubleEncoded { encoded: upgrade_call },
		},
	])));

	let sudo_api = relaychain::api::sudo::calls::TransactionApi;
	let sudo_call = sudo_api.sudo(RuntimeCall::XcmPallet(Call::send { dest, message }));

	let result = relaychain
		.tx()
		.sign_and_submit_then_watch_default(&sudo_call, &signer)
		.await
		.expect("send through sudo call.")
		.wait_for_finalized_success()
		.await
		.expect("sudo call success");

	println!("Sudo call issued at relaychain block hash {:?}", result.block_hash());

	let wait_for_blocks = 5;
	let mut blocks = bridgehub
		.blocks()
		.subscribe_finalized()
		.await
		.expect("block subscription")
		.take(wait_for_blocks);

	let mut upgrade_event_found = false;
	while let Some(Ok(block)) = blocks.next().await {
		println!("Polling bridgehub block {} for upgrade event.", block.number());
		let upgrades = block.events().await.expect("read block events");
		for upgrade in upgrades.find::<ethereum_system::events::Upgrade>() {
			let _upgrade = upgrade.expect("expect upgrade");
			println!("Event found at bridgehub block {}.", block.number());
			upgrade_event_found = true;
		}
		if upgrade_event_found {
			break
		}
	}
	assert!(upgrade_event_found);

	let wait_for_blocks = 500;
	let mut stream = ethereum_client.subscribe_blocks().await.unwrap().take(wait_for_blocks);

	let mut upgrade_event_found = false;
	let mut initialize_event_found = false;
	while let Some(block) = stream.next().await {
		println!("Polling ethereum block {:?} for upgraded event", block.number.unwrap());
		if let Ok(upgrades) = gateway
			.event::<UpgradedFilter>()
			.at_block_hash(block.hash.unwrap())
			.query()
			.await
		{
			for _upgrade in upgrades {
				println!("Upgrade event found at ethereum block {:?}", block.number.unwrap());
				upgrade_event_found = true;
			}
			if upgrade_event_found {
				if let Ok(initializes) = mock_gateway
					.event::<InitializedFilter>()
					.at_block_hash(block.hash.unwrap())
					.query()
					.await
				{
					for initialize in initializes {
						println!(
							"Initialize event found at ethereum block {:?}",
							block.number.unwrap()
						);
						assert_eq!(initialize.d_0, d_0.into());
						assert_eq!(initialize.d_1, d_1.into());
						initialize_event_found = true;
					}
				}
			}
		}
		if upgrade_event_found {
			break
		}
	}
	assert!(upgrade_event_found);
	assert!(initialize_event_found);
}
