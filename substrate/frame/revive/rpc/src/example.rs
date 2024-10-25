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
//! Example utilities
#![cfg(any(feature = "example", test))]

use crate::{EthRpcClient, ReceiptInfo};
use anyhow::Context;
use pallet_revive::evm::{
	rlp::*, Account, BlockTag, Bytes, GenericTransaction, TransactionLegacyUnsigned, H160, H256,
	U256,
};

/// Wait for a transaction receipt.
pub async fn wait_for_receipt(
	client: &(impl EthRpcClient + Send + Sync),
	hash: H256,
) -> anyhow::Result<ReceiptInfo> {
	for _ in 0..30 {
		tokio::time::sleep(std::time::Duration::from_secs(2)).await;
		let receipt = client.get_transaction_receipt(hash).await?;
		if let Some(receipt) = receipt {
			return Ok(receipt)
		}
	}

	anyhow::bail!("Failed to get receipt")
}

/// Send a transaction.
pub async fn send_transaction(
	signer: &Account,
	client: &(impl EthRpcClient + Send + Sync),
	value: U256,
	input: Bytes,
	to: Option<H160>,
) -> anyhow::Result<H256> {
	let from = signer.address();

	let chain_id = Some(client.chain_id().await?);

	let gas_price = client.gas_price().await?;
	let nonce = client
		.get_transaction_count(from, BlockTag::Latest.into())
		.await
		.with_context(|| "Failed to fetch account nonce")?;

	let gas = client
		.estimate_gas(
			GenericTransaction {
				from: Some(from),
				input: Some(input.clone()),
				value: Some(value),
				gas_price: Some(gas_price),
				to,
				..Default::default()
			},
			None,
		)
		.await
		.with_context(|| "Failed to fetch gas estimate")?;

	let unsigned_tx = TransactionLegacyUnsigned {
		gas,
		nonce,
		to,
		value,
		input,
		gas_price,
		chain_id,
		..Default::default()
	};

	let tx = signer.sign_transaction(unsigned_tx.clone());
	let bytes = tx.rlp_bytes().to_vec();

	let hash = client
		.send_raw_transaction(bytes.clone().into())
		.await
		.with_context(|| "transaction failed")?;

	Ok(hash)
}
