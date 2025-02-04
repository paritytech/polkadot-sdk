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

// Testsuite of fatp integration tests.

pub mod zombienet;

use std::time::Duration;

use tokio::join;
use zombienet::NetworkSpawner;
use zombienet_sdk::subxt::OnlineClient;

#[tokio::test(flavor = "multi_thread")]
async fn send_future_and_then_ready_from_single_account() {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	let net = NetworkSpawner::init_from_asset_hub_fatp_low_pool_limit_spec().await.unwrap();
	let collator = net.get_node("charlie").unwrap();
	let _client: OnlineClient<zombienet_sdk::subxt::SubstrateConfig> =
		collator.wait_client_with_timeout(120u64).await.unwrap();
	let ws = "ws://127.0.0.1:9933";
	let mut nonce = 0;
	for _ in 0..3 {
		// Spawn future TXs.
		let future_start = nonce + 5;
		let handle1 = tokio::spawn(async move {
			cmd_lib::run_cmd!(RUST_LOG=info ttxt tx --chain=sub --ws=$ws from-single-account --account 0 --count 5 --from $future_start)
		});
		tokio::time::sleep(Duration::from_secs(5)).await;
		let handle2 = tokio::spawn(async move {
			cmd_lib::run_cmd!(RUST_LOG=info ttxt tx --chain=sub --ws=$ws from-single-account --account 0  --count 5 --from $nonce)
		});
		nonce = future_start + 5;
		let (res1, res2) = join!(handle1, handle2);
		assert!(res1.is_ok());
		assert!(res2.is_ok());
	}
}
