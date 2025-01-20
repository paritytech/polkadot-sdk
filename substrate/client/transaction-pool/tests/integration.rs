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

#[tokio::test(flavor = "multi_thread")]
async fn test_tryout() {
	let relay_chain = RelaychainConfig {
		default_command: "polkadot".to_owned(),
		chain: "rococo-local".to_owned(),
	};
	let para_chain = ParachainConfig {
		default_command: "polkadot-parachain".to_owned(),
		chain_spec_path: "tests/zombienet/chain-specs/yap-westend-live-2022.json".to_owned(),
		cumulus_based: true,
		id: 2000,
	};
	let small_net =
		zombienet::small_network_yap::SmallNetworkYap::new(relay_chain, para_chain).unwrap();
	let _network = small_net.start().await.unwrap();
}
