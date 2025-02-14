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
use pallet_revive_eth_rpc::subxt_client::{self, system::calls::types::Remark, SrcChainConfig};
use subxt::OnlineClient;
use subxt_signer::sr25519::dev;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let client = OnlineClient::<SrcChainConfig>::new().await?;
	let tx_payload = subxt_client::tx().system().remark(b"bonjour".to_vec());
	let res = client
		.tx()
		.sign_and_submit_then_watch_default(&tx_payload, &dev::alice())
		.await?
		.wait_for_finalized()
		.await?;

	println!("Transaction finalized: {:?}", res.extrinsic_hash());
	let block_hash = res.block_hash();
	let block = client.blocks().at(block_hash).await.unwrap();
	let extrinsics = block.extrinsics().await.unwrap();
	let remarks = extrinsics
		.find::<Remark>()
		.map(|remark| remark.unwrap().value)
		.collect::<Vec<_>>();

	dbg!(remarks);
	Ok(())
}
