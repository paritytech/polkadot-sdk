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

use zombienet::{Network, ParachainConfig, RelaychainConfig};
use zombienet_sdk::NetworkConfigExt;

#[tokio::test(flavor = "multi_thread")]
// TODO: continue this scenario
async fn send_future_and_then_ready() {
	let net_config = zombienet_configuration::NetworkConfig::load_from_toml(
		"tests/zombienet/network-specs/small_network-yap.toml",
	)
	.unwrap();
	let _net = net_config.spawn_native().await.unwrap();
}
