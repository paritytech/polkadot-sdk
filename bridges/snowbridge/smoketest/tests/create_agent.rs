use snowbridge_smoketest::{
	contracts::i_gateway::AgentCreatedFilter, helper::*,
	parachains::bridgehub::api::ethereum_system::events::CreateAgent,
	xcm::construct_xcm_message_with_fee,
};

#[tokio::test]
async fn create_agent() {
	let test_clients = initial_clients().await.expect("initialize clients");

	let encoded_call = construct_create_agent_call(&test_clients.bridge_hub_client)
		.await
		.expect("construct inner call.");

	let message = construct_xcm_message_with_fee(encoded_call).await;

	let result = send_sudo_xcm_transact(&test_clients.penpal_client, message)
		.await
		.expect("failed to send xcm transact.");

	println!(
		"xcm call issued at block hash {:?}, transaction hash {:?}",
		result.block_hash(),
		result.extrinsic_hash()
	);

	wait_for_bridgehub_event::<CreateAgent>(&test_clients.bridge_hub_client).await;

	wait_for_ethereum_event::<AgentCreatedFilter>(&test_clients.ethereum_client).await;
}
