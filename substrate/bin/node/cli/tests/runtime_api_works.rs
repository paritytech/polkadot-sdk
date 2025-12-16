// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![cfg(unix)]
use codec::Decode;
use polkadot_sdk::sp_runtime::traits::NumberFor;
use std::time::Duration;
use substrate_cli_test_utils as common;
use substrate_rpc_client::{ws_client, StateApi};

#[tokio::test]
async fn transaction_storage_runtime_api_call_works() {
	common::run_with_timeout(Duration::from_secs(60 * 10), async move {
		// Run the node.
		let mut node = common::KillChildOnDrop(common::start_node());
		let stderr = node.stderr.take().unwrap();
		let ws_url = common::extract_info_from_output(stderr).0.ws_url;
		common::wait_n_finalized_blocks(1, &ws_url).await;
		let block_hash = common::block_hash(1, &ws_url).await.unwrap();
		node.assert_still_running();

		// Call the runtime API.
		let rpc = ws_client(ws_url).await.unwrap();
		let result = rpc
			.call(
				String::from("TransactionStorageApi_storage_period"),
				vec![].into(),
				Some(block_hash),
			)
			.await
			.unwrap();

		// Decode and assert the received value.
		let storage_period: NumberFor<kitchensink_runtime::Block> =
			Decode::decode(&mut &result.0[..]).unwrap();
		assert_eq!(storage_period, 100800);

		node.stop();
	})
	.await;
}
