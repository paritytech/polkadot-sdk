// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use jsonrpsee::http_client::HttpClientBuilder;
use pallet_revive::evm::{Account, BlockTag, Bytes, ReceiptInfo};
use pallet_revive_eth_rpc::{
	example::{send_transaction, wait_for_receipt},
	EthRpcClient,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let alith = Account::default();
	let client = HttpClientBuilder::default().build("http://localhost:8545")?;

	let baltathar = Account::from(subxt_signer::eth::dev::baltathar());
	let value = 1_000_000_000_000_000_000u128.into(); // 1 ETH

	let print_balance = || async {
		let balance = client.get_balance(alith.address(), BlockTag::Latest.into()).await?;
		println!("Alith     {:?} balance: {balance:?}", alith.address());
		let balance = client.get_balance(baltathar.address(), BlockTag::Latest.into()).await?;
		println!("Baltathar {:?} balance: {balance:?}", baltathar.address());
		anyhow::Result::<()>::Ok(())
	};

	print_balance().await?;
	println!("\n\n=== Transferring  ===\n\n");

	let hash =
		send_transaction(&alith, &client, value, Bytes::default(), Some(baltathar.address()))
			.await?;
	println!("Transaction hash: {hash:?}");

	let ReceiptInfo { block_number, gas_used, .. } = wait_for_receipt(&client, hash).await?;
	println!("Receipt: ");
	println!("- Block number: {block_number}");
	println!("- Gas used: {gas_used}");

	print_balance().await?;
	Ok(())
}
