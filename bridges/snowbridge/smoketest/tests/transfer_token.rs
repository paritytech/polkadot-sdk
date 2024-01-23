use assethub::api::polkadot_xcm::calls::TransactionApi;
use ethers::{
	prelude::Middleware,
	providers::{Provider, Ws},
	types::Address,
};
use futures::StreamExt;
use hex_literal::hex;
use snowbridge_smoketest::{
	constants::*,
	contracts::{
		i_gateway::IGateway,
		weth9::{TransferFilter, WETH9},
	},
	helper::AssetHubConfig,
	parachains::assethub::{
		api::runtime_types::{
			staging_xcm::v3::multilocation::MultiLocation,
			xcm::{
				v3::{
					junction::{Junction, NetworkId},
					junctions::Junctions,
					multiasset::{AssetId, Fungibility, MultiAsset, MultiAssets},
				},
				VersionedMultiAssets, VersionedMultiLocation,
			},
		},
		{self},
	},
};
use sp_core::{sr25519::Pair, Pair as PairT};
use std::{sync::Arc, time::Duration};
use subxt::{tx::PairSigner, OnlineClient};

const DESTINATION_ADDRESS: [u8; 20] = hex!("44a57ee2f2FCcb85FDa2B0B18EBD0D8D2333700e");

#[tokio::test]
async fn transfer_token() {
	let ethereum_provider = Provider::<Ws>::connect(ETHEREUM_API)
		.await
		.unwrap()
		.interval(Duration::from_millis(10u64));

	let ethereum_client = Arc::new(ethereum_provider);

	let weth_addr: Address = WETH_CONTRACT.into();
	let weth = WETH9::new(weth_addr, ethereum_client.clone());

	let gateway = IGateway::new(GATEWAY_PROXY_CONTRACT, ethereum_client.clone());
	let agent_src =
		gateway.agent_of(ASSET_HUB_AGENT_ID).await.expect("could not get agent address");

	let assethub: OnlineClient<AssetHubConfig> =
		OnlineClient::from_url(ASSET_HUB_WS_URL).await.unwrap();

	let keypair: Pair = Pair::from_string("//Ferdie", None).expect("cannot create keypair");

	let signer: PairSigner<AssetHubConfig, _> = PairSigner::new(keypair);

	let amount: u128 = 1_000_000_000;
	let assets = VersionedMultiAssets::V3(MultiAssets(vec![MultiAsset {
		id: AssetId::Concrete(MultiLocation {
			parents: 2,
			interior: Junctions::X2(
				Junction::GlobalConsensus(NetworkId::Ethereum { chain_id: ETHEREUM_CHAIN_ID }),
				Junction::AccountKey20 { network: None, key: WETH_CONTRACT.into() },
			),
		}),
		fun: Fungibility::Fungible(amount),
	}]));

	let destination = VersionedMultiLocation::V3(MultiLocation {
		parents: 2,
		interior: Junctions::X1(Junction::GlobalConsensus(NetworkId::Ethereum {
			chain_id: ETHEREUM_CHAIN_ID,
		})),
	});

	let beneficiary = VersionedMultiLocation::V3(MultiLocation {
		parents: 0,
		interior: Junctions::X1(Junction::AccountKey20 {
			network: None,
			key: DESTINATION_ADDRESS.into(),
		}),
	});

	let token_transfer_call =
		TransactionApi.reserve_transfer_assets(destination, beneficiary, assets, 0);

	let result = assethub
		.tx()
		.sign_and_submit_then_watch_default(&token_transfer_call, &signer)
		.await
		.expect("send through call.")
		.wait_for_finalized_success()
		.await
		.expect("call success");

	println!(
		"reserve_transfer_assets call issued at assethub block hash {:?}",
		result.block_hash()
	);

	let wait_for_blocks = 50;
	let mut stream = ethereum_client.subscribe_blocks().await.unwrap().take(wait_for_blocks);

	let mut transfer_event_found = false;
	while let Some(block) = stream.next().await {
		println!("Polling ethereum block {:?} for transfer event", block.number.unwrap());
		if let Ok(transfers) =
			weth.event::<TransferFilter>().at_block_hash(block.hash.unwrap()).query().await
		{
			for transfer in transfers {
				println!("Transfer event found at ethereum block {:?}", block.number.unwrap());
				assert_eq!(transfer.src, agent_src.into());
				assert_eq!(transfer.dst, DESTINATION_ADDRESS.into());
				assert_eq!(transfer.wad, amount.into());
				transfer_event_found = true;
			}
		}
		if transfer_event_found {
			break
		}
	}
	assert!(transfer_event_found);
}
