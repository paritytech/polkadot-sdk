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

use clap::Parser;
use jsonrpsee::http_client::HttpClientBuilder;
use pallet_revive::evm::{BlockNumberOrTag, ReceiptInfo, TransactionInfo, H256, U256};
use pallet_revive_eth_rpc::EthRpcClient;
use serde::{de::Error, Deserialize, Deserializer, Serialize};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "collect-test-data")]
#[command(about = "Collect block data, transactions and receipts for test data generation")]
struct Args {
	/// RPC URL (e.g., http://localhost:8545)
	#[arg(short, long, default_value = "http://localhost:8545")]
	rpc_url: String,

	/// Block number (if not provided, uses latest block)
	#[arg(short, long)]
	block_number: Option<u64>,
}

#[derive(Serialize)]
struct TestData {
	block_number: U256,
	block_hash: H256,
	transactions: Option<Vec<TransactionInfo>>,
	transactions_rlp: Vec<String>,
	transactions_root: H256,
	receipts: Option<Vec<ReceiptInfo>>,
	receipts_rlp: Vec<String>,
	receipts_root: H256,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let args = Args::parse();

	let client = Arc::new(HttpClientBuilder::default().build(&args.rpc_url)?);

	// Determine block number to use
	let block_number = match args.block_number {
		Some(num) => BlockNumberOrTag::U256(U256::from(num)),
		None => BlockNumberOrTag::BlockTag(pallet_revive::evm::BlockTag::Latest),
	};

	println!("Fetching block data for: {:?}", block_number);

	// Get block information
	let block = client
		.get_block_by_number(block_number.clone(), true)
		.await?
		.ok_or_else(|| anyhow::anyhow!("Block not found"))?;

	println!("Block hash: {:?}", block.hash);
	println!("Block number: {:?}", block.number);
	println!(
		"Transactions count: {}",
		match &block.transactions {
			pallet_revive::evm::HashesOrTransactionInfos::TransactionInfos(txs) => txs.len(),
			pallet_revive::evm::HashesOrTransactionInfos::Hashes(hashes) => hashes.len(),
		}
	);

	// Extract transaction infos
	let transaction_infos = match block.transactions {
		pallet_revive::evm::HashesOrTransactionInfos::TransactionInfos(txs) => txs,
		pallet_revive::evm::HashesOrTransactionInfos::Hashes(hashes) => {
			// If we only have hashes, fetch full transaction info
			let mut txs = Vec::new();
			for hash in hashes {
				if let Some(tx) = client.get_transaction_by_hash(hash).await? {
					txs.push(tx);
				}
			}
			txs
		},
	};

	// Collect receipts
	let mut receipts = Vec::new();
	for tx in &transaction_infos {
		if let Some(receipt) = client.get_transaction_receipt(tx.hash).await? {
			receipts.push(receipt);
		}
	}

	println!("Collected {} receipts", receipts.len());

	// Generate RLP encoded transactions
	let mut transactions_rlp = Vec::new();
	for tx in &transaction_infos {
		let rlp_encoded = tx.transaction_signed.encode_2718();
		transactions_rlp.push(format!("0x{}", hex::encode(rlp_encoded)));
	}

	// Generate RLP encoded receipts
	let mut receipts_rlp = Vec::new();
	for receipt in &receipts {
		let rlp_encoded = receipt.encode_2718();
		receipts_rlp.push(format!("0x{}", hex::encode(rlp_encoded)));
	}

	// Create test data structure
	let test_data = TestData {
		block_number: block.number,
		block_hash: block.hash,
		transactions: transaction_infos,
		transactions_rlp,
		transactions_root: block.transactions_root,
		receipts,
		receipts_rlp,
		receipts_root: block.receipts_root,
	};

	// Output as JSON
	let json_output = serde_json::to_string_pretty(&test_data)?;
	println!("\n=== TEST DATA ===\n");
	println!("{}", json_output);

	// Also save to file
	let filename = format!("test_data_block_{}.json", block.number);
	std::fs::write(&filename, &json_output)?;
	println!("\n=== SAVED TO FILE ===");
	println!("Test data saved to: {}", filename);

	Ok(())
}
