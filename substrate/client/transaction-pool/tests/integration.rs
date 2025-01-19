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

use zombienet::Network;

#[tokio::test(flavor = "multi_thread")]
async fn test_tryout() {
	let small_net = zombienet::small_network_yap::SmallNetworkYap::new();
	let _network = small_net.start().await.unwrap();

	// Show basedir.
	//println!("network_base_dir: {}", network.base_dir().unwrap());

	//let ws = "--ws=ws://127.0.0.1:9944";
	tokio::time::sleep(Duration::from_secs(350)).await;
	//let mut result = cmd_lib::spawn_with_output!(sleep 350).unwrap();
	//println!("{}", result.wait_with_output().unwrap());
}
