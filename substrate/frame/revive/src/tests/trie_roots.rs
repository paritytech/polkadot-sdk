// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::evm::block_hash::EthBlockBuilder;
use serde::{Deserialize, Serialize};
use sp_core::H256;

#[derive(Serialize, Deserialize)]
struct TestDataInfo {
	rpc_url: String,
	block_number_requested: String,
}

#[derive(Serialize, Deserialize)]
struct TestData {
	info: TestDataInfo,
	block_number: String,
	block_hash: String,
	transactions: Option<serde_json::Value>,
	transactions_rlp: Vec<String>,
	transactions_root: String,
	receipts: Option<serde_json::Value>,
	receipts_rlp: Vec<String>,
	receipts_root: String,
}

fn load_test_data(filename: &str) -> TestData {
	let test_data_path = format!("test-assets/{}", filename);
	let test_data_json = std::fs::read_to_string(&test_data_path)
		.unwrap_or_else(|_| panic!("Failed to read test data file: {}", test_data_path));
	serde_json::from_str(&test_data_json).unwrap()
}

fn hex_to_h256(hex_str: &str) -> H256 {
	let hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);
	let bytes = alloy_core::hex::decode(hex_str).expect("Invalid hex string");
	H256::from_slice(&bytes)
}

fn hex_to_bytes(hex_str: &str) -> Vec<u8> {
	let hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);
	alloy_core::hex::decode(hex_str).expect("Invalid hex string")
}

#[test]
fn test_transactions_root_verification() {
	for test_file in [
		"test_data_localnet_block_1.json",
		"test_data_localnet_block_6.json",
		"test_data_localnet_block_7.json",
		"test_data_ethereum_block_5094851.json",
		"test_data_ethereum_block_22094877.json",
	] {
		let test_data = load_test_data(test_file);

		// Convert RLP-encoded transactions to bytes
		let transactions_rlp: Vec<Vec<u8>> =
			test_data.transactions_rlp.iter().map(|hex_str| hex_to_bytes(hex_str)).collect();

		// Calculate the receipts root using EthBlockBuilder
		let calculated_root = EthBlockBuilder::compute_trie_root(&transactions_rlp);
		let calculated_root = H256::from_slice(&calculated_root.0);

		// Parse the expected root from the test data
		let expected_root = hex_to_h256(&test_data.transactions_root);

		assert_eq!(
			calculated_root, expected_root,
			"file: {test_file} - Calculated transactions root does not match expected root. \
                Expected: {expected_root:?}, Calculated: {calculated_root:?}"
		);
	}
}

#[test]
fn test_receipts_root_verification() {
	for test_file in [
		"test_data_localnet_block_1.json",
		"test_data_localnet_block_6.json",
		"test_data_localnet_block_7.json",
		"test_data_ethereum_block_5094851.json",
		"test_data_ethereum_block_22094877.json",
		"test_data_ethereum_sepolia_block_8867251.json", // This one includes EIP-4844
	] {
		let test_data = load_test_data(test_file);

		// Convert RLP-encoded receipts to bytes
		let receipts_rlp: Vec<Vec<u8>> =
			test_data.receipts_rlp.iter().map(|hex_str| hex_to_bytes(hex_str)).collect();

		// Calculate the receipts root using EthBlockBuilder
		let calculated_root = EthBlockBuilder::compute_trie_root(&receipts_rlp);
		let calculated_root = H256::from_slice(&calculated_root.0);

		// Parse the expected root from the test data
		let expected_root = hex_to_h256(&test_data.receipts_root);

		assert_eq!(
			calculated_root, expected_root,
			"file: {test_file} - Calculated receipts root does not match expected root. \
                Expected: {expected_root:?}, Calculated: {calculated_root:?}"
		);
	}
}
