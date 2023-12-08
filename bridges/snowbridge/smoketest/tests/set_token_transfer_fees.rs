use ethers::prelude::Address;
use snowbridge_smoketest::{
	constants::*,
	contracts::{i_gateway, i_gateway::TokenTransferFeesChangedFilter},
	helper::*,
	parachains::bridgehub::api::{
		ethereum_system::events::SetTokenTransferFees,
		runtime_types::{
			self, bridge_hub_rococo_runtime::RuntimeCall as BHRuntimeCall, primitive_types::U256,
		},
	},
};

#[tokio::test]
async fn set_token_transfer_fees() {
	let test_clients = initial_clients().await.expect("initialize clients");

	let gateway_addr: Address = GATEWAY_PROXY_CONTRACT.into();
	let ethereum_client = *(test_clients.ethereum_client.clone());
	let gateway = i_gateway::IGateway::new(gateway_addr, ethereum_client.clone());
	let fees = gateway.quote_register_token_fee().await.expect("get fees");
	println!("register fees {:?}", fees);

	let set_token_fees_call = BHRuntimeCall::EthereumSystem(
		runtime_types::snowbridge_system::pallet::Call::set_token_transfer_fees {
			create_asset_xcm: *CREATE_ASSET_FEE,
			transfer_asset_xcm: *RESERVE_TRANSFER_FEE,
			register_token: U256([*REGISTER_TOKEN_FEE, 0, 0, 0]),
		},
	);

	governance_bridgehub_call_from_relay_chain(vec![set_token_fees_call])
		.await
		.expect("set token fees");

	wait_for_bridgehub_event::<SetTokenTransferFees>(&test_clients.bridge_hub_client).await;

	wait_for_ethereum_event::<TokenTransferFeesChangedFilter>(&test_clients.ethereum_client).await;

	let fees = gateway.quote_register_token_fee().await.expect("get fees");
	println!("asset fees {:?}", fees);
}
