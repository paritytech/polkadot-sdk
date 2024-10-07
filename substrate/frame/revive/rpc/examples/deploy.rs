use codec::Encode;
use eth_rpc::{example::Account, EthRpcClient, ReceiptInfo};
use jsonrpsee::http_client::HttpClientBuilder;
use polkadot_sdk::pallet_revive::{
	create1,
	evm::{BlockTag, Bytes, U256},
	EthInstantiateInput,
};

static DUMMY_BYTES: &[u8] = include_bytes!("./dummy.polkavm");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	env_logger::init();
	let account = Account::default();
	let data = vec![];
	let input = EthInstantiateInput { code: DUMMY_BYTES.to_vec(), data: data.clone() };

	println!("Account address: {:?}", account.address());
	let client = HttpClientBuilder::default().build("http://localhost:9090".to_string())?;

	println!("\n\n=== Deploying contract ===\n\n");

	let input = input.encode();
	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let hash = account.send_transaction(&client, U256::zero(), input.into(), None).await?;
	println!("Deploy Tx hash: {hash:?}");

	tokio::time::sleep(std::time::Duration::from_secs(2)).await;
	let ReceiptInfo { block_number, gas_used, contract_address, .. } =
		client.get_transaction_receipt(hash).await?.unwrap();
	println!("Receipt received: ");
	println!("Block number: {block_number}");
	println!("Gas used: {gas_used}");
	println!("Contract address: {contract_address:?}");

	if std::env::var("SKIP_CALL").is_ok() {
		return Ok(())
	}

	let contract_address = create1(&account.address(), nonce.try_into().unwrap());
	println!("\n\n=== Calling contract ===\n\n");

	let hash = account
		.send_transaction(&client, U256::zero(), Bytes::default(), Some(contract_address))
		.await?;

	println!("Contract call tx hash: {hash:?}");
	tokio::time::sleep(std::time::Duration::from_secs(2)).await;

	let ReceiptInfo { block_number, gas_used, to, .. } =
		client.get_transaction_receipt(hash).await?.unwrap();
	println!("Receipt received: ");
	println!("Block number: {block_number}");
	println!("Gas used: {gas_used}");
	println!("To: {to:?}");
	Ok(())
}
