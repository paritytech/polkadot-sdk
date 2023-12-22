use ethers::prelude::Address;
use snowbridge_smoketest::{
	constants::*,
	contracts::{i_gateway, i_gateway::InboundMessageDispatchedFilter},
	helper::*,
	parachains::bridgehub::api::ethereum_system::events::TransferNativeFromAgent,
	xcm::construct_xcm_message_with_fee,
};

#[tokio::test]
async fn transfer_native_from_agent() {
	let test_clients = initial_clients().await.expect("initialize clients");

	let gateway_addr: Address = GATEWAY_PROXY_CONTRACT.into();
	let ethereum_client = *(test_clients.ethereum_client.clone());
	let gateway = i_gateway::IGateway::new(gateway_addr, ethereum_client.clone());
	let agent_address = gateway.agent_of(SIBLING_AGENT_ID).await.expect("find agent");

	println!("agent address {}", hex::encode(agent_address));

	fund_account(&test_clients.ethereum_signed_client, agent_address)
		.await
		.expect("fund account");

	let before = get_balance(&test_clients.ethereum_signed_client, ETHEREUM_ADDRESS.into())
		.await
		.expect("get balance");

	println!("balance before: {}", before);

	const TRANSFER_AMOUNT: u128 = 1000000000;

	let message = construct_xcm_message_with_fee(
		construct_transfer_native_from_agent_call(
			&test_clients.bridge_hub_client,
			ETHEREUM_ADDRESS.into(),
			TRANSFER_AMOUNT,
		)
		.await
		.expect("construct inner call."),
	)
	.await;

	let result = send_sudo_xcm_transact(&test_clients.penpal_client, message)
		.await
		.expect("failed to send xcm transact.");

	println!(
		"xcm call issued at block hash {:?}, transaction hash {:?}",
		result.block_hash(),
		result.extrinsic_hash()
	);

	wait_for_bridgehub_event::<TransferNativeFromAgent>(&test_clients.bridge_hub_client).await;

	wait_for_ethereum_event::<InboundMessageDispatchedFilter>(&test_clients.ethereum_client).await;

	let after = get_balance(&test_clients.ethereum_signed_client, ETHEREUM_ADDRESS.into())
		.await
		.expect("get balance");

	println!("balance after: {}", after);
	assert!(before + TRANSFER_AMOUNT >= after);
}
