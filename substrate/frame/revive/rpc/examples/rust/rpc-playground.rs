use eth_rpc::{example::Account, EthRpcClient};
use jsonrpsee::http_client::HttpClientBuilder;
use polkadot_sdk::pallet::revive_evm::BlockTag;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let data = hex_literal::hex!("0xf86b800184254ac125947bf369283338e12c90514468aa3868a551ab29298898a7d9b8314c0000808204cba0d82f7414276d8c4925f69c1b1c2507be08973e03ea2cdf5d2cae86610929cbe2a0033f3f7e1dad86b3e2cb466abe41a1658c7247f5ac4cbe237d47564bf990139e");
	let tx = rlp::decode::<TransactionLegacySigned>(&data).unwrap();
	dbg!(tx);

	let account = Account::default();
	println!("Account address: {:?}", account.address());

	let client = HttpClientBuilder::default().build("http://localhost:9090".to_string())?;

	let block = client.get_block_by_number(BlockTag::Latest.into(), false).await?;
	println!("Latest block: {block:#?}");

	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	println!("Account nonce: {nonce:?}");

	let balance = client.get_balance(account.address(), BlockTag::Latest.into()).await?;
	println!("Account balance: {balance:?}");

	Ok(())
}
//method: eth_sendRawTransaction params:
// ["0xf86b800184254ac125947bf369283338e12c90514468aa3868a551ab29298898a7d9b8314c0000808204cba0d82f7414276d8c4925f69c1b1c2507be08973e03ea2cdf5d2cae86610929cbe2a0033f3f7e1dad86b3e2cb466abe41a1658c7247f5ac4cbe237d47564bf990139e"
// ],
