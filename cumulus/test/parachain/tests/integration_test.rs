// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

// TODO: this is necessary for the jsonrpsee macro used
#![allow(unused_variables, dead_code)]

use assert_cmd::cargo::cargo_bin;
use async_std::{net, task::sleep};
use codec::Encode;
use futures::{future::FutureExt, join, pin_mut, select};
use jsonrpsee::{raw::RawClient, transport::http::HttpTransportClient};
use polkadot_primitives::parachain::{Info, Scheduling};
use polkadot_primitives::Hash as PHash;
use polkadot_runtime::{Header, Runtime, SignedExtra, SignedPayload};
use polkadot_runtime_common::{parachains, registrar, BlockHashCount, claims};
use serde_json::Value;
use sp_arithmetic::traits::SaturatedConversion;
use sp_runtime::generic;
use sp_version::RuntimeVersion;
use std::{
	collections::HashSet,
	env, fs,
	io::Read,
	path::PathBuf,
	process::{Child, Command, Stdio},
	time::Duration,
};
use substrate_test_runtime_client::AccountKeyring::Alice;
use tempfile::tempdir;

static POLKADOT_ARGS: &[&str] = &["polkadot", "--chain=res/polkadot_chainspec.json"];
static INTEGRATION_TEST_ALLOWED_TIME: Option<&str> = option_env!("INTEGRATION_TEST_ALLOWED_TIME");

jsonrpsee::rpc_api! {
	Author {
		#[rpc(method = "author_submitExtrinsic", positional_params)]
		fn submit_extrinsic(extrinsic: String) -> PHash;
	}

	Chain {
		#[rpc(method = "chain_getFinalizedHead")]
		fn current_block_hash() -> PHash;

		#[rpc(method = "chain_getHeader", positional_params)]
		fn header(hash: PHash) -> Option<Header>;

		#[rpc(method = "chain_getBlockHash", positional_params)]
		fn block_hash(hash: Option<u64>) -> Option<PHash>;
	}

	State {
		#[rpc(method = "state_getRuntimeVersion")]
		fn runtime_version() -> RuntimeVersion;
	}

	System {
		#[rpc(method = "system_networkState")]
		fn network_state() -> Value;
	}
}

// Adapted from
// https://github.com/rust-lang/cargo/blob/485670b3983b52289a2f353d589c57fae2f60f82/tests/testsuite/support/mod.rs#L507
fn target_dir() -> PathBuf {
	env::current_exe()
		.ok()
		.map(|mut path| {
			path.pop();
			if path.ends_with("deps") {
				path.pop();
			}
			path
		})
		.unwrap()
}

struct ChildHelper<'a> {
	name: String,
	child: &'a mut Child,
}

impl<'a> Drop for ChildHelper<'a> {
	fn drop(&mut self) {
		let name = self.name.clone();

		self.terminate();

		let mut stdout = String::new();
		if let Some(reader) = self.child.stdout.as_mut() {
			let _ = reader.read_to_string(&mut stdout);
		}
		eprintln!("process '{}' stdout:\n{}\n", name, stdout,);

		let mut stderr = String::new();
		if let Some(reader) = self.child.stderr.as_mut() {
			let _ = reader.read_to_string(&mut stderr);
		}
		eprintln!("process '{}' stderr:\n{}\n", name, stderr,);
	}
}

impl<'a> ChildHelper<'a> {
	fn new(name: &str, child: &'a mut Child) -> ChildHelper<'a> {
		ChildHelper {
			name: name.to_string(),
			child,
		}
	}

	fn terminate(&mut self) {
		match self.child.try_wait() {
			Ok(Some(_)) => return,
			Ok(None) => {}
			Err(err) => {
				eprintln!("could not wait for child process to finish: {}", err);
				let _ = self.child.kill();
				let _ = self.child.wait();
				return;
			}
		}

		let _ = self.child.kill();

		let _ = self.child.wait();
	}
}

async fn wait_for_tcp<A: net::ToSocketAddrs + std::fmt::Display>(address: A) {
	while let Err(err) = net::TcpStream::connect(&address).await {
		eprintln!("Waiting for {} to be up ({})...", address, err);
		sleep(Duration::from_secs(2)).await;
	}
}

/// wait for parachain blocks to be produced
async fn wait_for_blocks(number_of_blocks: usize, mut client: &mut RawClient<HttpTransportClient>) {
	let mut previous_blocks = HashSet::with_capacity(number_of_blocks);

	loop {
		let current_block_hash = Chain::block_hash(&mut client, None).await.unwrap().unwrap();

		if previous_blocks.insert(current_block_hash) {
			eprintln!("new parachain block: {}", current_block_hash);

			if previous_blocks.len() == number_of_blocks {
				break;
			}
		}

		sleep(Duration::from_secs(2)).await;
	}
}

#[async_std::test]
#[ignore]
#[cfg(feature = "disabled")]
async fn integration_test() {
	assert!(
		!net::TcpStream::connect("127.0.0.1:27015").await.is_ok(),
		"tcp port is already open 127.0.0.1:27015, this test cannot be run",
	);
	assert!(
		!net::TcpStream::connect("127.0.0.1:27016").await.is_ok(),
		"tcp port is already open 127.0.0.1:27016, this test cannot be run",
	);

	let t1 = sleep(Duration::from_secs(
		INTEGRATION_TEST_ALLOWED_TIME
			.and_then(|x| x.parse().ok())
			.unwrap_or(600),
	))
	.fuse();
	let t2 = async {
		// start alice
		let polkadot_alice_dir = tempdir().unwrap();
		let mut polkadot_alice = Command::new(cargo_bin("cumulus-test-parachain-collator"))
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.args(POLKADOT_ARGS)
			.arg("--base-path")
			.arg(polkadot_alice_dir.path())
			.arg("--alice")
			.arg("--rpc-methods=unsafe")
			.arg("--rpc-port=27015")
			.arg("--port=27115")
			.spawn()
			.unwrap();
		let polkadot_alice_helper = ChildHelper::new("alice", &mut polkadot_alice);

		// start bob
		let polkadot_bob_dir = tempdir().unwrap();
		let mut polkadot_bob = Command::new(cargo_bin("cumulus-test-parachain-collator"))
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.args(POLKADOT_ARGS)
			.arg("--base-path")
			.arg(polkadot_bob_dir.path())
			.arg("--bob")
			.arg("--rpc-methods=unsafe")
			.arg("--rpc-port=27016")
			.arg("--port=27116")
			.spawn()
			.unwrap();
		let polkadot_bob_helper = ChildHelper::new("bob", &mut polkadot_bob);

		// wait for both nodes to be up and running
		join!(
			wait_for_tcp("127.0.0.1:27015"),
			wait_for_tcp("127.0.0.1:27016")
		);

		// export genesis state
		let cmd = Command::new(cargo_bin("cumulus-test-parachain-collator"))
			.arg("export-genesis-state")
			.output()
			.unwrap();
		assert!(cmd.status.success());
		let output = &cmd.stdout;
		let genesis_state = hex::decode(&output[2..output.len() - 1]).unwrap();

		// connect RPC clients
		let transport_client_alice =
			jsonrpsee::transport::http::HttpTransportClient::new("http://127.0.0.1:27015");
		let mut client_alice = jsonrpsee::raw::RawClient::new(transport_client_alice);
		let transport_client_bob =
			jsonrpsee::transport::http::HttpTransportClient::new("http://127.0.0.1:27016");
		let mut client_bob = jsonrpsee::raw::RawClient::new(transport_client_bob);

		// retrieve nodes network id
		let polkadot_alice_id = System::network_state(&mut client_alice).await.unwrap()["peerId"]
			.as_str()
			.unwrap()
			.to_string();
		let polkadot_bob_id = System::network_state(&mut client_bob).await.unwrap()["peerId"]
			.as_str()
			.unwrap()
			.to_string();

		// retrieve runtime version
		let runtime_version = State::runtime_version(&mut client_alice).await.unwrap();

		// get the current block
		let current_block_hash = Chain::block_hash(&mut client_alice, None)
			.await
			.unwrap()
			.unwrap();
		let current_block = Chain::header(&mut client_alice, current_block_hash)
			.await
			.unwrap()
			.unwrap()
			.number
			.saturated_into::<u64>();

		let genesis_block = Chain::block_hash(&mut client_alice, 0)
			.await
			.unwrap()
			.unwrap();

		// create and sign transaction
		let wasm = fs::read(target_dir().join(
			"wbuild/cumulus-test-parachain-runtime/cumulus_test_parachain_runtime.compact.wasm",
		))
		.unwrap();
		let call = pallet_sudo::Call::sudo(Box::new(
			registrar::Call::<Runtime>::register_para(
				100.into(),
				Info {
					scheduling: Scheduling::Always,
				},
				wasm.into(),
				genesis_state.into(),
			)
			.into(),
		));
		let nonce = 0;
		let period = BlockHashCount::get()
			.checked_next_power_of_two()
			.map(|c| c / 2)
			.unwrap_or(2) as u64;
		let tip = 0;
		let extra: SignedExtra = (
			TransactionCallFilter::<IsCallable, polkadot_runtime::Call>::new(),
			frame_system::CheckSpecVersion::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckEra::<Runtime>::from(generic::Era::mortal(period, current_block)),
			frame_system::CheckNonce::<Runtime>::from(nonce),
			frame_system::CheckWeight::<Runtime>::new(),
			pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
			registrar::LimitParathreadCommits::<Runtime>::new(),
			parachains::ValidateDoubleVoteReports::<Runtime>::new(),
			pallet_grandpa::ValidateEquivocationReport::<Runtime>::new(),
			claims::PrevalidateAttests::<Runtime>::new(),
		);
		let raw_payload = SignedPayload::from_raw(
			call.clone().into(),
			extra.clone(),
			(
				(),
				runtime_version.spec_version,
				runtime_version.transaction_version,
				genesis_block,
				current_block_hash,
				(),
				(),
				(),
				(),
				(),
				(),
				(),
			),
		);
		let signature = raw_payload.using_encoded(|e| Alice.sign(e));

		// register parachain
		let ex = polkadot_runtime::UncheckedExtrinsic::new_signed(
			call.into(),
			Alice.into(),
			sp_runtime::MultiSignature::Sr25519(signature),
			extra,
		);
		let _register_block_hash =
			Author::submit_extrinsic(&mut client_alice, format!("0x{}", hex::encode(ex.encode())))
				.await
				.unwrap();

		// run cumulus charlie
		let cumulus_charlie_dir = tempdir().unwrap();
		let mut cumulus_charlie = Command::new(cargo_bin("cumulus-test-parachain-collator"))
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.arg("--base-path")
			.arg(cumulus_charlie_dir.path())
			.arg("--rpc-methods=unsafe")
			.arg("--rpc-port=27017")
			.arg("--port=27117")
			.arg("--")
			.arg(format!(
				"--bootnodes=/ip4/127.0.0.1/tcp/27115/p2p/{}",
				polkadot_alice_id
			))
			.arg(format!(
				"--bootnodes=/ip4/127.0.0.1/tcp/27116/p2p/{}",
				polkadot_bob_id
			))
			.arg("--charlie")
			.spawn()
			.unwrap();
		let cumulus_charlie_helper = ChildHelper::new("cumulus-charlie", &mut cumulus_charlie);
		wait_for_tcp("127.0.0.1:27017").await;

		// connect rpc client to cumulus
		let transport_client_cumulus_charlie =
			jsonrpsee::transport::http::HttpTransportClient::new("http://127.0.0.1:27017");
		let mut client_cumulus_charlie =
			jsonrpsee::raw::RawClient::new(transport_client_cumulus_charlie);

		wait_for_blocks(4, &mut client_cumulus_charlie).await;
	}
	.fuse();

	pin_mut!(t1, t2);

	select! {
		_ = t1 => {
			panic!("the test took too long, maybe no parachain blocks have been produced");
		},
		_ = t2 => {},
	}
}
