use crate::{
	constants::*,
	contracts::i_gateway,
	parachains::{
		bridgehub::{
			self,
			api::{
				runtime_types::{
					bridge_hub_rococo_runtime::RuntimeCall as BHRuntimeCall,
					snowbridge_core::outbound::v1::OperatingMode,
				},
				utility,
			},
		},
		penpal::{
			api::runtime_types as penpalTypes,
			{self},
		},
		relaychain,
		relaychain::api::runtime_types::{
			pallet_xcm::pallet::Call as RelaychainPalletXcmCall,
			rococo_runtime::RuntimeCall as RelaychainRuntimeCall,
			sp_weights::weight_v2::Weight as RelaychainWeight,
			staging_xcm::v3::multilocation::MultiLocation as RelaychainMultiLocation,
			xcm::{
				double_encoded::DoubleEncoded as RelaychainDoubleEncoded,
				v2::OriginKind as RelaychainOriginKind,
				v3::{
					junction::Junction as RelaychainJunction,
					junctions::Junctions as RelaychainJunctions,
					Instruction as RelaychainInstruction, WeightLimit as RelaychainWeightLimit,
					Xcm as RelaychainXcm,
				},
				VersionedMultiLocation as RelaychainVersionedMultiLocation,
				VersionedXcm as RelaychainVersionedXcm,
			},
		},
	},
};
use ethers::{
	prelude::{
		Address, EthEvent, LocalWallet, Middleware, Provider, Signer, SignerMiddleware,
		TransactionRequest, Ws, U256,
	},
	providers::Http,
	types::Log,
};
use futures::StreamExt;
use penpalTypes::{
	pallet_xcm::pallet::Call,
	penpal_runtime::RuntimeCall,
	staging_xcm::v3::multilocation::MultiLocation,
	xcm::{
		v3::{junction::Junction, junctions::Junctions},
		VersionedMultiLocation, VersionedXcm,
	},
};
use sp_core::{sr25519::Pair, Pair as PairT, H160};
use std::{ops::Deref, sync::Arc, time::Duration};
use subxt::{
	blocks::ExtrinsicEvents,
	events::StaticEvent,
	tx::{PairSigner, TxPayload},
	Config, OnlineClient, PolkadotConfig, SubstrateConfig,
};

/// Custom config that works with Penpal
pub enum PenpalConfig {}

impl Config for PenpalConfig {
	type Index = <PolkadotConfig as Config>::Index;
	type Hash = <PolkadotConfig as Config>::Hash;
	type AccountId = <PolkadotConfig as Config>::AccountId;
	type Address = <PolkadotConfig as Config>::Address;
	type Signature = <PolkadotConfig as Config>::Signature;
	type Hasher = <PolkadotConfig as Config>::Hasher;
	type Header = <PolkadotConfig as Config>::Header;
	type ExtrinsicParams = <SubstrateConfig as Config>::ExtrinsicParams;
}

/// Custom config that works with Statemint
pub enum AssetHubConfig {}

impl Config for AssetHubConfig {
	type Index = <PolkadotConfig as Config>::Index;
	type Hash = <PolkadotConfig as Config>::Hash;
	type AccountId = <PolkadotConfig as Config>::AccountId;
	type Address = <PolkadotConfig as Config>::Address;
	type Signature = <PolkadotConfig as Config>::Signature;
	type Hasher = <PolkadotConfig as Config>::Hasher;
	type Header = <PolkadotConfig as Config>::Header;
	type ExtrinsicParams = <SubstrateConfig as Config>::ExtrinsicParams;
}

pub struct TestClients {
	pub asset_hub_client: Box<OnlineClient<PolkadotConfig>>,
	pub bridge_hub_client: Box<OnlineClient<PolkadotConfig>>,
	pub penpal_client: Box<OnlineClient<PenpalConfig>>,
	pub relaychain_client: Box<OnlineClient<PolkadotConfig>>,
	pub ethereum_client: Box<Arc<Provider<Ws>>>,
	pub ethereum_signed_client: Box<Arc<SignerMiddleware<Provider<Http>, LocalWallet>>>,
}

pub async fn initial_clients() -> Result<TestClients, Box<dyn std::error::Error>> {
	let bridge_hub_client: OnlineClient<PolkadotConfig> = OnlineClient::from_url(BRIDGE_HUB_WS_URL)
		.await
		.expect("can not connect to bridgehub");

	let asset_hub_client: OnlineClient<PolkadotConfig> = OnlineClient::from_url(ASSET_HUB_WS_URL)
		.await
		.expect("can not connect to bridgehub");

	let penpal_client: OnlineClient<PenpalConfig> = OnlineClient::from_url(PENPAL_WS_URL)
		.await
		.expect("can not connect to penpal parachain");

	let relaychain_client: OnlineClient<PolkadotConfig> =
		OnlineClient::from_url(RELAY_CHAIN_WS_URL)
			.await
			.expect("can not connect to relaychain");

	let ethereum_provider = Provider::<Ws>::connect(ETHEREUM_API)
		.await
		.unwrap()
		.interval(Duration::from_millis(10u64));

	let ethereum_client = Arc::new(ethereum_provider);

	let ethereum_signed_client = initialize_wallet().await.expect("initialize wallet");

	Ok(TestClients {
		asset_hub_client: Box::new(asset_hub_client),
		bridge_hub_client: Box::new(bridge_hub_client),
		penpal_client: Box::new(penpal_client),
		relaychain_client: Box::new(relaychain_client),
		ethereum_client: Box::new(ethereum_client),
		ethereum_signed_client: Box::new(Arc::new(ethereum_signed_client)),
	})
}

pub async fn wait_for_bridgehub_event<Ev: StaticEvent>(
	bridge_hub_client: &Box<OnlineClient<PolkadotConfig>>,
) {
	let mut blocks = bridge_hub_client
		.blocks()
		.subscribe_finalized()
		.await
		.expect("block subscription")
		.take(5);

	let mut substrate_event_found = false;
	while let Some(Ok(block)) = blocks.next().await {
		println!("Polling bridgehub block {} for expected event.", block.number());
		let events = block.events().await.expect("read block events");
		for event in events.find::<Ev>() {
			let _ = event.expect("expect upgrade");
			println!("Event found at bridgehub block {}.", block.number());
			substrate_event_found = true;
			break
		}
		if substrate_event_found {
			break
		}
	}
	assert!(substrate_event_found);
}

pub async fn wait_for_ethereum_event<Ev: EthEvent>(ethereum_client: &Box<Arc<Provider<Ws>>>) {
	let gateway_addr: Address = GATEWAY_PROXY_CONTRACT.into();
	let gateway = i_gateway::IGateway::new(gateway_addr, (*ethereum_client).deref().clone());

	let wait_for_blocks = 300;
	let mut stream = ethereum_client.subscribe_blocks().await.unwrap().take(wait_for_blocks);

	let mut ethereum_event_found = false;
	while let Some(block) = stream.next().await {
		println!("Polling ethereum block {:?} for expected event", block.number.unwrap());
		if let Ok(events) = gateway.event::<Ev>().at_block_hash(block.hash.unwrap()).query().await {
			for _ in events {
				println!("Event found at ethereum block {:?}", block.number.unwrap());
				ethereum_event_found = true;
				break
			}
		}
		if ethereum_event_found {
			break
		}
	}
	assert!(ethereum_event_found);
}

pub async fn send_sudo_xcm_transact(
	penpal_client: &Box<OnlineClient<PenpalConfig>>,
	message: Box<VersionedXcm>,
) -> Result<ExtrinsicEvents<PenpalConfig>, Box<dyn std::error::Error>> {
	let dest = Box::new(VersionedMultiLocation::V3(MultiLocation {
		parents: 1,
		interior: Junctions::X1(Junction::Parachain(BRIDGE_HUB_PARA_ID)),
	}));

	let sudo_call = penpal::api::sudo::calls::TransactionApi::sudo(
		&penpal::api::sudo::calls::TransactionApi,
		RuntimeCall::PolkadotXcm(Call::send { dest, message }),
	);

	let owner: Pair = Pair::from_string("//Alice", None).expect("cannot create keypair");

	let signer: PairSigner<PenpalConfig, _> = PairSigner::new(owner);

	let result = penpal_client
		.tx()
		.sign_and_submit_then_watch_default(&sudo_call, &signer)
		.await
		.expect("send through xcm call.")
		.wait_for_finalized_success()
		.await
		.expect("xcm call failed");

	Ok(result)
}

pub async fn initialize_wallet(
) -> Result<SignerMiddleware<Provider<Http>, LocalWallet>, Box<dyn std::error::Error>> {
	let provider = Provider::<Http>::try_from(ETHEREUM_HTTP_API)
		.unwrap()
		.interval(Duration::from_millis(10u64));

	let wallet: LocalWallet =
		ETHEREUM_KEY.parse::<LocalWallet>().unwrap().with_chain_id(ETHEREUM_CHAIN_ID);

	Ok(SignerMiddleware::new(provider.clone(), wallet.clone()))
}

pub async fn get_balance(
	client: &Box<Arc<SignerMiddleware<Provider<Http>, LocalWallet>>>,
	who: Address,
) -> Result<U256, Box<dyn std::error::Error>> {
	let balance = client.get_balance(who, None).await?;

	Ok(balance)
}

pub async fn fund_account(
	client: &Box<Arc<SignerMiddleware<Provider<Http>, LocalWallet>>>,
	address_to: Address,
) -> Result<(), Box<dyn std::error::Error>> {
	let tx = TransactionRequest::new()
		.to(address_to)
		.from(client.address())
		.value(U256::from(ethers::utils::parse_ether(1)?));
	let tx = client.send_transaction(tx, None).await?.await?;
	assert_eq!(tx.clone().unwrap().status.unwrap().as_u64(), 1u64);
	println!("receipt: {:#?}", hex::encode(tx.unwrap().transaction_hash));
	Ok(())
}

pub async fn construct_create_agent_call(
	bridge_hub_client: &Box<OnlineClient<PolkadotConfig>>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
	let call = bridgehub::api::ethereum_system::calls::TransactionApi
		.create_agent()
		.encode_call_data(&bridge_hub_client.metadata())?;

	Ok(call)
}

pub async fn construct_create_channel_call(
	bridge_hub_client: &Box<OnlineClient<PolkadotConfig>>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
	let call = bridgehub::api::ethereum_system::calls::TransactionApi
		.create_channel(OperatingMode::Normal)
		.encode_call_data(&bridge_hub_client.metadata())?;

	Ok(call)
}

pub async fn construct_transfer_native_from_agent_call(
	bridge_hub_client: &Box<OnlineClient<PolkadotConfig>>,
	recipient: H160,
	amount: u128,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
	let call = bridgehub::api::ethereum_system::calls::TransactionApi
		.transfer_native_from_agent(recipient, amount)
		.encode_call_data(&bridge_hub_client.metadata())?;

	Ok(call)
}

pub async fn governance_bridgehub_call_from_relay_chain(
	calls: Vec<BHRuntimeCall>,
) -> Result<(), Box<dyn std::error::Error>> {
	let test_clients = initial_clients().await.expect("initialize clients");

	let sudo: Pair = Pair::from_string("//Alice", None).expect("cannot create sudo keypair");

	let signer: PairSigner<PolkadotConfig, _> = PairSigner::new(sudo);

	let utility_api = utility::calls::TransactionApi;
	let batch_call = utility_api
		.batch_all(calls)
		.encode_call_data(&test_clients.bridge_hub_client.metadata())
		.expect("encoded call");

	let weight = 180000000000;
	let proof_size = 900000;

	let dest = Box::new(RelaychainVersionedMultiLocation::V3(RelaychainMultiLocation {
		parents: 0,
		interior: RelaychainJunctions::X1(RelaychainJunction::Parachain(BRIDGE_HUB_PARA_ID)),
	}));
	let message = Box::new(RelaychainVersionedXcm::V3(RelaychainXcm(vec![
		RelaychainInstruction::UnpaidExecution {
			weight_limit: RelaychainWeightLimit::Unlimited,
			check_origin: None,
		},
		RelaychainInstruction::Transact {
			origin_kind: RelaychainOriginKind::Superuser,
			require_weight_at_most: RelaychainWeight { ref_time: weight, proof_size },
			call: RelaychainDoubleEncoded { encoded: batch_call },
		},
	])));

	let sudo_api = relaychain::api::sudo::calls::TransactionApi;
	let sudo_call = sudo_api
		.sudo(RelaychainRuntimeCall::XcmPallet(RelaychainPalletXcmCall::send { dest, message }));

	let result = test_clients
		.relaychain_client
		.tx()
		.sign_and_submit_then_watch_default(&sudo_call, &signer)
		.await
		.expect("send through sudo call.")
		.wait_for_finalized_success()
		.await
		.expect("sudo call success");

	println!("Sudo call issued at relaychain block hash {:?}", result.block_hash());

	Ok(())
}

pub async fn fund_agent(agent_id: [u8; 32]) -> Result<(), Box<dyn std::error::Error>> {
	let test_clients = initial_clients().await.expect("initialize clients");
	let gateway_addr: Address = GATEWAY_PROXY_CONTRACT.into();
	let ethereum_client = *(test_clients.ethereum_client.clone());
	let gateway = i_gateway::IGateway::new(gateway_addr, ethereum_client.clone());
	let agent_address = gateway.agent_of(agent_id).await.expect("find agent");

	println!("agent address {}", hex::encode(agent_address));

	fund_account(&test_clients.ethereum_signed_client, agent_address)
		.await
		.expect("fund account");
	Ok(())
}

pub fn print_event_log_for_unit_tests(log: &Log) {
	let topics: Vec<String> = log.topics.iter().map(|t| hex::encode(t.as_ref())).collect();
	println!("Log {{");
	println!("	address: hex!(\"{}\").into(),", hex::encode(log.address.as_ref()));
	println!("	topics: vec![");
	for topic in topics.iter() {
		println!("		hex!(\"{}\").into(),", topic);
	}
	println!("	],");
	println!("	data: hex!(\"{}\").into(),", hex::encode(&log.data));

	println!("}}")
}
