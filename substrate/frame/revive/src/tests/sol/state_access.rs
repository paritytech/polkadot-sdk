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

//! Checks what an operation reads and writes against the entries its `Access` types name.

use crate::{
	BalanceWithDust, Code, Config, Pallet,
	access_list::{
		Access, AccessEntry, CallItems, CodeLoadItems, StorageOp, TransferItems, merged_entries,
	},
	call_builder::entry_of_key,
	test_utils::{ALICE, ALICE_ADDR, builder::Contract},
	tests::{ExtBuilder, Test, builder},
};
use alloy_core::sol_types::SolCall;
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{Callee, Caller, FixtureType, compile_module_with_type};
use pretty_assertions::assert_eq;
use sp_core::{H160, H256};
use sp_trie::recorder::Recorder;
use std::collections::{BTreeMap, BTreeSet};
use test_case::test_case;

/// A deployed contract and the hash of its runtime code.
struct DeployedContract {
	addr: H160,
	account_id: <Test as frame_system::Config>::AccountId,
	code_hash: H256,
}

/// What one operation read from the trie and wrote to the overlay.
struct ReadsAndWrites {
	/// Keys the access list tracks, as entries.
	read: BTreeSet<AccessEntry>,
	written: BTreeSet<AccessEntry>,
	/// Keys it does not track and no benchmark pays for, named by their storage item.
	other_read: BTreeSet<String>,
	other_written: BTreeSet<String>,
}

impl ReadsAndWrites {
	/// Splits recorded keys into the entries the access list tracks and labels for the rest,
	/// dropping the keys a call is already charged for.
	fn from_keys(read: &[Vec<u8>], written: &[Vec<u8>]) -> Self {
		let covered = keys_the_benchmarks_cover();
		let split = |keys: &[Vec<u8>]| {
			let mut entries = BTreeSet::new();
			let mut others = BTreeSet::new();
			for key in keys {
				match entry_of_key::<Test>(key) {
					Some(entry) => {
						entries.insert(entry);
					},
					None if covered.contains(key) => {},
					None => {
						others.insert(label_key(key));
					},
				}
			}
			(entries, others)
		};
		let (read, other_read) = split(read);
		let (written, other_written) = split(written);
		Self { read, written, other_read, other_written }
	}
}

/// Returns the keys a call is already charged for, so that only the unpaid ones are reported.
fn keys_the_benchmarks_cover() -> BTreeSet<Vec<u8>> {
	use frame_support::{
		storage::transactional::TRANSACTION_LEVEL_KEY, traits::WhitelistedStorageKeys,
	};
	crate::tests::AllPalletsWithSystem::whitelisted_storage_keys()
		.into_iter()
		.map(|whitelisted| whitelisted.key)
		// The benchmark macro whitelists the transactional layer itself, outside any pallet.
		.chain(core::iter::once(TRANSACTION_LEVEL_KEY.to_vec()))
		// Not whitelisted, but the `call` benchmark declares it: one read when the stack is built.
		.chain(core::iter::once(pallet_timestamp::Now::<Test>::hashed_key().to_vec()))
		.collect()
}

/// Returns the name of a key as its pallet and storage item, or its raw bytes when no pallet
/// claims it.
fn label_key(key: &[u8]) -> String {
	use frame_support::traits::StorageInfoTrait;
	use sp_core::storage::well_known_keys::is_default_child_storage_key;
	if is_default_child_storage_key(key) {
		return "child trie root".to_string();
	}
	crate::tests::AllPalletsWithSystem::storage_info()
		.into_iter()
		.find(|info| key.starts_with(&info.prefix))
		.map(|info| {
			format!(
				"{}::{}",
				String::from_utf8_lossy(&info.pallet_name),
				String::from_utf8_lossy(&info.storage_name)
			)
		})
		.unwrap_or_else(|| format!("{}", sp_core::hexdisplay::HexDisplay::from(&key)))
}

/// Deploys both contracts, funds the caller, and commits, so that reading any of it afterwards
/// goes to the trie and is recorded.
fn deploy_and_commit(
	caller_code: &[u8],
	target_code: &[u8],
) -> (sp_io::TestExternalities, DeployedContract, DeployedContract) {
	let mut ext = ExtBuilder::default().build();
	let (caller, target) = ext.execute_with(|| {
		let deploy = |code: &[u8], salt: [u8; 32]| {
			let Contract { addr, account_id } =
				builder::bare_instantiate(Code::Upload(code.to_vec()))
					.salt(Some(salt))
					.build_and_unwrap_contract();
			let code_hash = crate::tests::test_utils::get_contract(&addr).code_hash;
			DeployedContract { addr, account_id, code_hash }
		};
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let caller = deploy(caller_code, [1; 32]);
		let _ = <Test as Config>::Currency::set_balance(&caller.account_id, 100_000_000_000);
		(caller, deploy(target_code, [2; 32]))
	});
	ext.commit_all().unwrap();
	(ext, caller, target)
}

/// Runs `operation` on the committed state and returns what it read and wrote.
fn reads_and_writes_of(
	ext: &mut sp_io::TestExternalities,
	operation: impl FnOnce(),
) -> ReadsAndWrites {
	let recorder = Recorder::<sp_core::Blake2Hasher>::default();
	ext.execute_with_recorder(recorder.clone(), operation);

	let mut recorded = recorder.recorded_keys();
	let read = recorded
		.remove(ext.backend.root())
		.expect("the operation read the trie")
		.into_keys()
		.map(|key| key.to_vec())
		.collect::<Vec<_>>();
	assert_eq!(recorded, Default::default());

	let written = ext
		.overlayed_changes()
		.changes()
		.map(|(key, _)| key.clone())
		.collect::<Vec<_>>();
	assert_eq!(ext.overlayed_changes().children().count(), 0);

	// Mapping an account id back to its address reads `OriginalAccount`.
	ext.execute_with(|| ReadsAndWrites::from_keys(&read, &written))
}

/// Returns the reads and writes of calling `caller` with `payload`, and the two deployed
/// contracts.
fn reads_and_writes_of_call(
	caller_code: &[u8],
	target_code: &[u8],
	payload: impl FnOnce(&DeployedContract, &DeployedContract) -> Vec<u8>,
) -> (ReadsAndWrites, DeployedContract, DeployedContract) {
	let (mut ext, caller, target) = deploy_and_commit(caller_code, target_code);
	let payload = payload(&caller, &target);
	let reads_writes = reads_and_writes_of(&mut ext, || {
		builder::bare_call(caller.addr).data(payload).build_and_unwrap_result();
	});
	(reads_writes, caller, target)
}

/// Asserts the top frame accessed exactly `expected` and left no unpaid key outside it.
fn assert_only_declared_entries(
	actual: &ReadsAndWrites,
	expected: BTreeMap<AccessEntry, StorageOp>,
) {
	assert_eq!(actual.read, expected.keys().cloned().collect::<BTreeSet<AccessEntry>>());
	assert_eq!(
		actual.written,
		expected
			.iter()
			.filter(|(_, op)| matches!(op, StorageOp::Write))
			.map(|(entry, _)| entry.clone())
			.collect::<BTreeSet<AccessEntry>>(),
	);
	assert_eq!(actual.other_read, BTreeSet::new());
	assert_eq!(actual.other_written, BTreeSet::new());
}

#[test_case(FixtureType::Solc;   "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn the_top_frame_accesses_what_it_declares(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Callee", fixture_type).unwrap();
	// a call that reads no contract storage.
	let (top_frame, contract, _) =
		reads_and_writes_of_call(&code, &code, |_, _| Callee::whoSenderCall {}.abi_encode());

	assert_only_declared_entries(
		&top_frame,
		merged_entries([
			CallItems::new(contract.addr, false).entries(),
			CodeLoadItems { hash: contract.code_hash }.entries(),
		]),
	);
}

#[test_case(false; "no dust")]
#[test_case(true;  "dust")]
fn the_top_frame_moving_value_accesses_what_it_declares(dust: bool) {
	let (code, _) = compile_module_with_type("DoNothingReceiver", FixtureType::Solc).unwrap();
	let (mut ext, contract, _) = deploy_and_commit(&code, &code);

	let value = BalanceWithDust::new_unchecked::<Test>(1, u32::from(dust));
	let top_frame = reads_and_writes_of(&mut ext, || {
		builder::bare_call(contract.addr)
			.evm_value(Pallet::<Test>::convert_native_to_evm(value))
			.build_and_unwrap_result();
	});

	assert_only_declared_entries(
		&top_frame,
		merged_entries([
			CallItems::new(contract.addr, false).entries(),
			CodeLoadItems { hash: contract.code_hash }.entries(),
			TransferItems { from: ALICE_ADDR, to: contract.addr, dust }.entries(),
		]),
	);
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn a_nested_plain_call_accesses_what_it_declares(
	caller_type: FixtureType,
	target_type: FixtureType,
) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (target_code, _) = compile_module_with_type("Callee", target_type).unwrap();
	let (accessed, caller, target) =
		reads_and_writes_of_call(&caller_code, &target_code, |_, target| {
			Caller::normalCall {
				_callee: target.addr.0.into(),
				_value: 0,
				_data: Callee::echoCall { _data: 42 }.abi_encode().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		});

	assert_only_declared_entries(
		&accessed,
		merged_entries([
			CallItems::new(caller.addr, false).entries(),
			CodeLoadItems { hash: caller.code_hash }.entries(),
			CallItems::new(target.addr, false).entries(),
			CodeLoadItems { hash: target.code_hash }.entries(),
		]),
	);
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn a_nested_delegate_call_accesses_what_it_declares(
	caller_type: FixtureType,
	target_type: FixtureType,
) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (target_code, _) = compile_module_with_type("Callee", target_type).unwrap();
	let (accessed, caller, target) =
		reads_and_writes_of_call(&caller_code, &target_code, |_, target| {
			Caller::delegateCall {
				_callee: target.addr.0.into(),
				_data: Callee::echoCall { _data: 42 }.abi_encode().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		});

	assert_only_declared_entries(
		&accessed,
		merged_entries([
			CallItems::new(caller.addr, false).entries(),
			CodeLoadItems { hash: caller.code_hash }.entries(),
			CallItems::new(target.addr, true).entries(),
			CodeLoadItems { hash: target.code_hash }.entries(),
		]),
	);
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn a_nested_static_call_accesses_what_it_declares(
	caller_type: FixtureType,
	target_type: FixtureType,
) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (target_code, _) = compile_module_with_type("Callee", target_type).unwrap();
	let (accessed, caller, target) =
		reads_and_writes_of_call(&caller_code, &target_code, |_, target| {
			Caller::staticCallCall {
				_callee: target.addr.0.into(),
				_data: Callee::echoCall { _data: 42 }.abi_encode().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		});

	assert_only_declared_entries(
		&accessed,
		merged_entries([
			CallItems::new(caller.addr, false).entries(),
			CodeLoadItems { hash: caller.code_hash }.entries(),
			CallItems::new(target.addr, false).entries(),
			CodeLoadItems { hash: target.code_hash }.entries(),
		]),
	);
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn a_nested_value_call_accesses_what_it_declares(
	caller_type: FixtureType,
	target_type: FixtureType,
) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (target_code, _) = compile_module_with_type("DoNothingReceiver", target_type).unwrap();
	let value = BalanceWithDust::new_unchecked::<Test>(1, 0);
	let (accessed, caller, target) =
		reads_and_writes_of_call(&caller_code, &target_code, |_, target| {
			Caller::normalCall {
				_callee: target.addr.0.into(),
				_value: Pallet::<Test>::convert_native_to_evm(value).low_u64(),
				_data: Vec::new().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		});

	assert_only_declared_entries(
		&accessed,
		merged_entries([
			CallItems::new(caller.addr, false).entries(),
			CodeLoadItems { hash: caller.code_hash }.entries(),
			CallItems::new(target.addr, false).entries(),
			CodeLoadItems { hash: target.code_hash }.entries(),
			TransferItems { from: caller.addr, to: target.addr, dust: false }.entries(),
		]),
	);
}

#[test_case(false; "no dust")]
#[test_case(true; "dust")]
fn a_transfer_accesses_what_it_declares(dust: bool) {
	use crate::evm::transfer_with_dust;
	use frame_support::traits::tokens::Preservation;

	let (caller_code, _) = compile_module_with_type("Caller", FixtureType::Solc).unwrap();
	let (target_code, _) = compile_module_with_type("Callee", FixtureType::Solc).unwrap();
	let (mut ext, from, to) = deploy_and_commit(&caller_code, &target_code);

	let value = BalanceWithDust::new_unchecked::<Test>(1, u32::from(dust));
	let reads_writes = reads_and_writes_of(&mut ext, || {
		transfer_with_dust::<Test>(&from.account_id, &to.account_id, value, Preservation::Preserve)
			.unwrap();
	});

	let mut expected = TransferItems { from: from.addr, to: to.addr, dust }.entries();
	if !dust {
		// A dust-free transfer never touches the receiver's info. The access names it either way,
		// which costs only a hot touch since `CallItems` already named that entry.
		expected.remove(&AccessEntry::AccountInfo { address: to.addr });
	}
	assert_only_declared_entries(&reads_writes, expected);
}
