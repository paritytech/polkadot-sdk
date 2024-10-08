use hex_literal::hex;
use jsonrpsee::http_client::HttpClientBuilder;
use pallet_revive::evm::{BlockTag, Bytes, ReceiptInfo, H160};
use pallet_revive_eth_rpc::{
	example::{wait_for_receipt, Account},
	EthRpcClient,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let account = Account::default();
	println!("Account address: {:?}", account.address());

	let client = HttpClientBuilder::default().build("http://localhost:9090".to_string())?;

	let balance = client.get_balance(account.address(), BlockTag::Latest.into()).await?;
	println!("Account balance: {:?}", balance);

	let to = Some(H160(hex!("c543bb3eF11d96aCA20b3c906cF2C8Daaff925e4")));
	let value = 10_000_000_000_000_000_000u128.into(); // 10 ETH
	println!("\n\n=== Transferring  ===\n\n");

	let hash = account.send_transaction(&client, value, Bytes::default(), to).await?;
	println!("Transaction hash: {hash:?}");

	let ReceiptInfo { block_number, gas_used, .. } = wait_for_receipt(&client, hash).await?;
	println!("Receipt: ");
	println!("- Block number: {block_number}");
	println!("- Gas used: {gas_used}");

	let balance = client.get_balance(account.address(), BlockTag::Latest.into()).await?;
	println!("Account balance: {:?}", balance);

	Ok(())
}
