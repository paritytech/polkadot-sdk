use ethers::prelude::Address;
use snowbridge_smoketest::{
	constants::*,
	contracts::{i_gateway, i_gateway::PricingParametersChangedFilter},
	helper::*,
	parachains::{
		bridgehub,
		bridgehub::api::{
			ethereum_system::events::PricingParametersChanged,
			runtime_types::{
				primitive_types::U256,
				snowbridge_core::pricing::{PricingParameters, Rewards},
				sp_arithmetic::fixed_point::FixedU128,
			},
		},
	},
};
use subxt::tx::TxPayload;

#[tokio::test]
async fn set_pricing_params() {
	let test_clients = initial_clients().await.expect("initialize clients");

	let gateway_addr: Address = GATEWAY_PROXY_CONTRACT.into();
	let ethereum_client = *(test_clients.ethereum_client.clone());
	let gateway = i_gateway::IGateway::new(gateway_addr, ethereum_client.clone());
	let params = gateway.pricing_parameters().await.expect("get fees");
	println!("pricing params {:?}", params);

	let ethereum_system_api = bridgehub::api::ethereum_system::calls::TransactionApi;

	let set_pricing_params_call = ethereum_system_api
		.set_pricing_parameters(PricingParameters {
			exchange_rate: FixedU128(*EXCHANGE_RATE),
			rewards: Rewards { local: *LOCAL_REWARD, remote: U256([*REMOTE_REWARD, 0, 0, 0]) },
			fee_per_gas: U256([*FEE_PER_GAS, 0, 0, 0]),
		})
		.encode_call_data(&test_clients.bridge_hub_client.metadata())
		.expect("encoded call");

	governance_bridgehub_call_from_relay_chain(set_pricing_params_call)
		.await
		.expect("set token fees");

	wait_for_bridgehub_event::<PricingParametersChanged>(&test_clients.bridge_hub_client).await;

	wait_for_ethereum_event::<PricingParametersChangedFilter>(&test_clients.ethereum_client).await;

	let params = gateway.pricing_parameters().await.expect("get fees");
	println!("pricing params {:?}", params);
}
