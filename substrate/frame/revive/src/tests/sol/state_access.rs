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
	BalanceOf, Code, Config,
	access_list::{
		Access, AccessEntry, CallItems, CodeLoadItems, StorageOp, TransferItems, merged_entries,
	},
	call_builder::entry_of_key,
	test_utils::{ALICE, builder::Contract},
	tests::{ExtBuilder, Test, builder},
};
use alloy_core::sol_types::SolCall;
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{Callee, Caller, FixtureType, compile_module_with_type};
use pretty_assertions::assert_eq;
use sp_core::{H160, H256, U256};
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
#[derive(Default)]
struct ReadsAndWrites {
	/// Keys the access list tracks, as entries.
	read: BTreeSet<AccessEntry>,
	written: BTreeSet<AccessEntry>,
	/// Keys it does not track and no benchmark whitelists, named by their storage item.
	other_read: BTreeSet<String>,
	other_written: BTreeSet<String>,
}

impl ReadsAndWrites {
	/// Splits recorded keys into the entries the access list tracks and labels for the rest,
	/// dropping the keys every benchmark whitelists.
	fn from_keys(read: &[Vec<u8>], written: &[Vec<u8>]) -> Self {
		let whitelisted = benchmark_whitelisted_keys();
		let split = |keys: &[Vec<u8>]| {
			let mut entries = BTreeSet::new();
			let mut others = BTreeSet::new();
			for key in keys {
				match entry_of_key::<Test>(key) {
					Some(entry) => {
						entries.insert(entry);
					},
					None if whitelisted.contains(key) => {},
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

/// Implements field by field subtraction, leaving what the left touched and the right did not.
impl core::ops::Sub<&ReadsAndWrites> for &ReadsAndWrites {
	type Output = ReadsAndWrites;

	fn sub(self, rhs: &ReadsAndWrites) -> ReadsAndWrites {
		ReadsAndWrites {
			read: &self.read - &rhs.read,
			written: &self.written - &rhs.written,
			other_read: &self.other_read - &rhs.other_read,
			other_written: &self.other_written - &rhs.other_written,
		}
	}
}

/// Returns the keys a benchmark never pays for: those the pallets mark, plus the transactional
/// layer, which the benchmark macro whitelists on its own.
fn benchmark_whitelisted_keys() -> BTreeSet<Vec<u8>> {
	use frame_support::{
		storage::transactional::TRANSACTION_LEVEL_KEY, traits::WhitelistedStorageKeys,
	};
	crate::tests::AllPalletsWithSystem::whitelisted_storage_keys()
		.into_iter()
		.map(|whitelisted| whitelisted.key)
		.chain(core::iter::once(TRANSACTION_LEVEL_KEY.to_vec()))
		.collect()
}

/// Returns the name of a key as its pallet and storage item, or its raw bytes when no pallet
/// claims it.
fn label_key(key: &[u8]) -> String {
	use frame_support::traits::StorageInfoTrait;
	if key.starts_with(b":child_storage:default:") {
		return "child trie".to_string();
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
	assert!(recorded.is_empty(), "the operation read outside the main trie");

	let written = ext
		.overlayed_changes()
		.changes()
		.map(|(key, _)| key.clone())
		.collect::<Vec<_>>();
	assert_eq!(ext.overlayed_changes().children().count(), 0);

	// Hashing the map prefixes needs externalities.
	ext.execute_with(|| ReadsAndWrites::from_keys(&read, &written))
}

/// Returns the reads and writes of calling `caller` with the payload `payload_for` builds, and
/// the two deployed contracts.
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

/// Returns the reads and writes of moving one unit from `caller` to `target`.
fn reads_and_writes_of_transfer(caller_code: &[u8], target_code: &[u8]) -> ReadsAndWrites {
	use frame_support::traits::tokens::Preservation;
	let (mut ext, caller, target) = deploy_and_commit(caller_code, target_code);
	reads_and_writes_of(&mut ext, || {
		<Test as Config>::Currency::transfer(
			&caller.account_id,
			&target.account_id,
			1,
			Preservation::Preserve,
		)
		.unwrap();
	})
}

/// Asserts that a nested call adds exactly the entries its `Access` types declare, on top of what
/// the caller's own call touches. Untracked keys may grow only by what a bare transfer adds, and
/// only when the call `transfers_value`.
fn assert_nested_call_touches_exactly(
	caller_code: &[u8],
	target_code: &[u8],
	transfers_value: bool,
	payload: impl FnOnce(&DeployedContract, &DeployedContract) -> Vec<u8>,
	combined_entries: impl FnOnce(
		&DeployedContract,
		&DeployedContract,
	) -> BTreeMap<AccessEntry, StorageOp>,
	shared_entries: impl FnOnce(&DeployedContract, &DeployedContract) -> BTreeSet<AccessEntry>,
) {
	let (caller_alone, _, _) =
		reads_and_writes_of_call(caller_code, target_code, |_, _| Caller::dataCall {}.abi_encode());
	let (with_nested_call, caller, target) =
		reads_and_writes_of_call(caller_code, target_code, payload);

	let entries = combined_entries(&caller, &target);
	let expected_combined_reads: BTreeSet<AccessEntry> = entries.keys().cloned().collect();
	let expected_combined_writes: BTreeSet<AccessEntry> = entries
		.iter()
		.filter(|(_, op)| matches!(op, StorageOp::Write))
		.map(|(entry, _)| entry.clone())
		.collect();

	let shared = shared_entries(&caller, &target);

	// The subtraction below removes the caller's footprint from the observed and the declared sets
	// alike, so an entry it shares with the nested call goes unchecked. A shared write cannot be
	// checked at all, which is why a value call carrying dust belongs in the bare transfer test.
	let actual_shared_reads: BTreeSet<AccessEntry> =
		expected_combined_reads.intersection(&caller_alone.read).cloned().collect();
	let actual_shared_writes: BTreeSet<AccessEntry> =
		expected_combined_writes.intersection(&caller_alone.written).cloned().collect();
	assert_eq!(actual_shared_reads, shared, "declared reads the caller also touches");
	assert_eq!(actual_shared_writes, BTreeSet::new(), "no write may be shared with the caller");

	let by_nested_call = &with_nested_call - &caller_alone;
	assert_eq!(
		by_nested_call.read,
		&expected_combined_reads - &caller_alone.read,
		"the nested call should read exactly what its `Access` types declare",
	);
	assert_eq!(
		by_nested_call.written,
		&expected_combined_writes - &caller_alone.written,
		"the nested call should write exactly what its `Access` types declare as writes",
	);

	// Both sides are taken beyond what the top call alone touches, `System::Number` for one.
	let allowed = if transfers_value {
		&reads_and_writes_of_transfer(caller_code, target_code) - &caller_alone
	} else {
		ReadsAndWrites::default()
	};
	assert_eq!(
		by_nested_call.other_read, allowed.other_read,
		"reads outside the access list should match",
	);
	assert_eq!(
		by_nested_call.other_written, allowed.other_written,
		"writes outside the access list should match",
	);
}

#[test_case(FixtureType::Solc;   "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn the_top_frame_touches_what_it_declares(fixture_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
	// The second contract this deploys is never reached, so the caller's own code does for it.
	let (caller_alone, caller, _) = reads_and_writes_of_call(&caller_code, &caller_code, |_, _| {
		Caller::dataCall {}.abi_encode()
	});

	let entries = merged_entries([
		CallItems::new(caller.addr, false).entries(),
		CodeLoadItems { hash: caller.code_hash }.entries(),
	]);

	assert_eq!(caller_alone.read, entries.keys().cloned().collect::<BTreeSet<AccessEntry>>(),);

	assert_eq!(
		caller_alone.written,
		entries
			.iter()
			.filter(|(_, op)| matches!(op, StorageOp::Write))
			.map(|(entry, _)| entry.clone())
			.collect::<BTreeSet<AccessEntry>>(),
	);

	// The `call` benchmark declares the timestamp; the getter's slot read looks up the child root.
	let labels =
		|names: &[&str]| names.iter().map(|name| name.to_string()).collect::<BTreeSet<_>>();
	assert_eq!(caller_alone.other_read, labels(&["Timestamp::Now", "child trie"]));
	assert_eq!(caller_alone.other_written, BTreeSet::new());
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn a_plain_call_touches_what_it_declares(caller_type: FixtureType, target_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (target_code, _) = compile_module_with_type("Callee", target_type).unwrap();
	assert_nested_call_touches_exactly(
		&caller_code,
		&target_code,
		false,
		|_, target| {
			Caller::normalCall {
				_callee: target.addr.0.into(),
				_value: 0,
				_data: Callee::echoCall { _data: 42 }.abi_encode().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		},
		|_, target| {
			merged_entries([
				CallItems::new(target.addr, false).entries(),
				CodeLoadItems { hash: target.code_hash }.entries(),
			])
		},
		// The caller's own frame touches none of the above entries.
		|_, _| BTreeSet::new(),
	);
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn a_delegate_call_touches_what_it_declares(caller_type: FixtureType, target_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (target_code, _) = compile_module_with_type("Callee", target_type).unwrap();
	assert_nested_call_touches_exactly(
		&caller_code,
		&target_code,
		false,
		|_, target| {
			Caller::delegateCall {
				_callee: target.addr.0.into(),
				_data: Callee::echoCall { _data: 42 }.abi_encode().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		},
		|_, target| {
			merged_entries([
				CallItems::new(target.addr, true).entries(),
				CodeLoadItems { hash: target.code_hash }.entries(),
			])
		},
		// The caller's own frame touches none of the above entries.
		|_, _| BTreeSet::new(),
	);
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn a_static_call_touches_what_it_declares(caller_type: FixtureType, target_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (target_code, _) = compile_module_with_type("Callee", target_type).unwrap();
	assert_nested_call_touches_exactly(
		&caller_code,
		&target_code,
		false,
		|_, target| {
			Caller::staticCallCall {
				_callee: target.addr.0.into(),
				_data: Callee::echoCall { _data: 42 }.abi_encode().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		},
		|_, target| {
			merged_entries([
				CallItems::new(target.addr, false).entries(),
				CodeLoadItems { hash: target.code_hash }.entries(),
			])
		},
		// The caller's own frame touches none of the above entries.
		|_, _| BTreeSet::new(),
	);
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn a_value_call_touches_what_it_declares(caller_type: FixtureType, target_type: FixtureType) {
	// A target with an empty `receive`, so the value is accepted and the frame persists.
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (target_code, _) = compile_module_with_type("DoNothingReceiver", target_type).unwrap();
	assert_nested_call_touches_exactly(
		&caller_code,
		&target_code,
		true,
		|_, target| {
			Caller::normalCall {
				_callee: target.addr.0.into(),
				_value: 1_000_000,
				_data: Vec::new().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		},
		|caller, target| {
			merged_entries([
				CallItems::new(target.addr, false).entries(),
				CodeLoadItems { hash: target.code_hash }.entries(),
				TransferItems { from: caller.addr, to: target.addr, dust: false }.entries(),
			])
		},
		// The caller's frame reads its own account info to run, and the transfer names it too.
		|caller, _| BTreeSet::from([AccessEntry::AccountInfo { address: caller.addr }]),
	);
}

#[test_case(false; "no dust")]
#[test_case(true; "dust")]
fn a_transfer_touches_what_it_declares(dust: bool) {
	use crate::{BalanceWithDust, evm::transfer_with_dust};
	use frame_support::traits::tokens::Preservation;

	let (caller_code, _) = compile_module_with_type("Caller", FixtureType::Solc).unwrap();
	let (target_code, _) = compile_module_with_type("Callee", FixtureType::Solc).unwrap();
	let (mut ext, from, to) = deploy_and_commit(&caller_code, &target_code);

	// One native unit, plus one wei of dust when asked: the transfer a value call performs.
	let value = U256::from(1_000_000u64 + u64::from(dust));
	let reads_writes = reads_and_writes_of(&mut ext, || {
		let value = BalanceWithDust::<BalanceOf<Test>>::from_value::<Test>(value).unwrap();
		transfer_with_dust::<Test>(&from.account_id, &to.account_id, value, Preservation::Preserve)
			.unwrap();
	});

	let mut expected = TransferItems { from: from.addr, to: to.addr, dust }.entries();
	if !dust {
		// Without dust the transfer never touches the receiver's account info. The access still
		// names it, as a read, to keep one shape for both cases; the call reads that entry anyway.
		expected.remove(&AccessEntry::AccountInfo { address: to.addr });
	}
	assert_eq!(
		reads_writes.read,
		expected.keys().cloned().collect(),
		"the transfer reads exactly the entries its access names",
	);
	assert_eq!(
		reads_writes.written,
		expected
			.iter()
			.filter(|(_, op)| **op == StorageOp::Write)
			.map(|(e, _)| e.clone())
			.collect(),
		"the transfer writes exactly the entries its access names as writes",
	);
}
