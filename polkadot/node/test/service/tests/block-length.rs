// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! End-to-end coverage for the relay-chain `BlockLength` parameter.
//!
//! `polkadot-test-runtime` overrides `BlockLength` to 10 MiB in the same way the production
//! relay-chain runtimes (Westend, Rococo) do. The Normal-dispatch-class cap is therefore
//! `NORMAL_DISPATCH_RATIO * 10 MiB = 7.5 MiB`. Under the previous 5 MiB default the Normal
//! cap was 3.75 MiB, so a 7 MiB `System::remark_with_event` would have been rejected at
//! pool validation with `InvalidTransaction::ExhaustsResources`. Accepting it here proves
//! the new parameter is wired through `frame_system::CheckWeight` end-to-end.

use polkadot_test_service::*;
use sp_keyring::Sr25519Keyring;
use std::time::Duration;

const REMARK_PAYLOAD_SIZE: usize = 7 * 1024 * 1024;
const TEST_TIMEOUT: Duration = Duration::from_secs(120);

#[tokio::test(flavor = "multi_thread")]
async fn relay_chain_accepts_extrinsic_above_legacy_normal_cap() {
	let alice_config = node_config(
		|| {},
		tokio::runtime::Handle::current(),
		Sr25519Keyring::Alice,
		Vec::new(),
		true,
	);
	let alice = run_validator_node(alice_config, None).await;

	let remark = vec![0u8; REMARK_PAYLOAD_SIZE];
	let call = polkadot_test_runtime::RuntimeCall::System(
		frame_system::Call::remark_with_event { remark },
	);

	tokio::time::timeout(TEST_TIMEOUT, alice.send_extrinsic(call, Sr25519Keyring::Bob))
		.await
		.expect("RPC submission must complete within the timeout")
		.expect(
			"7 MiB Normal-class extrinsic must be accepted under the 10 MiB \
			 BlockLength (would fail under the legacy 5 MiB cap)",
		);
}
