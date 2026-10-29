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

//! Checks what an operation reads and writes in the main trie against the entries its `Access`
//! types name.

use crate::{
	BalanceWithDust, Code, Config, Pallet,
	access_list::{
		Access, AccessEntry, CallItems, CodeLoadItems, InstantiateItems, StorageOp, TransferItems,
		Warmth,
	},
	test_utils::{ALICE, ALICE_ADDR, builder::Contract},
	tests::{ExtBuilder, Test, builder},
};
use alloy_core::sol_types::SolCall;
use frame_support::traits::{StorageInfo, StorageInfoTrait, fungible::Mutate};
use pallet_revive_fixtures::{Callee, Caller, Factory, FixtureType, compile_module_with_type};
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

/// The entries accesses have, each with its operation.
#[derive(Default)]
struct DeclaredEntries(BTreeMap<AccessEntry, StorageOp>);

impl DeclaredEntries {
	/// Returns the entries one access has.
	fn of(access: impl Access) -> Self {
		Self::default().and(access)
	}

	/// Adds the entries `access` has.
	fn and(mut self, access: impl Access) -> Self {
		access.expand(|entry, op| {
			self.insert_or_update(entry, op);
			Warmth::cold_non_revertible()
		});
		self
	}

	/// Removes `entry`, returning the operation it had.
	fn remove(&mut self, entry: &AccessEntry) -> Option<StorageOp> {
		self.0.remove(entry)
	}

	/// Inserts or updates `entry` with `op`.
	fn insert_or_update(&mut self, entry: AccessEntry, op: StorageOp) {
		let charged = self.0.entry(entry).or_insert(op);
		if !charged.covers(op) {
			*charged = op;
		}
	}
}

/// What one operation read from the trie and wrote to the overlay.
#[derive(Debug, PartialEq)]
struct ReadsAndWrites {
	/// Keys the access list tracks, as entries.
	read: BTreeSet<AccessEntry>,
	written: BTreeSet<AccessEntry>,
	/// Keys it does not track, named by their pallet and storage item.
	other_read: BTreeSet<String>,
	other_written: BTreeSet<String>,
}

impl ReadsAndWrites {
	/// Builds the expected reads and writes from the declared `entries`.
	fn from_declared(entries: DeclaredEntries) -> Self {
		let mut read = BTreeSet::new();
		let mut written = BTreeSet::new();
		for (entry, op) in entries.0 {
			if op.covers(StorageOp::Write) {
				written.insert(entry.clone());
			}
			read.insert(entry);
		}
		Self { read, written, other_read: BTreeSet::new(), other_written: BTreeSet::new() }
	}

	/// Moves `entry` from the reads to the writes, for a key the operation writes without
	/// reading it first.
	fn write_instead_of_read(&mut self, entry: AccessEntry) {
		assert!(self.read.remove(&entry), "no declared read to move: {entry:?}");
		self.written.insert(entry);
	}

	/// Splits recorded keys into the entries the access list tracks and, for the rest, the pallet
	/// and storage item they belong to. Keys the benchmarks whitelist are dropped.
	fn from_keys(read: &[Vec<u8>], written: &[Vec<u8>]) -> Self {
		let covered = keys_the_benchmarks_cover();
		let storage_items = crate::tests::AllPalletsWithSystem::storage_info();
		let split = |keys: &[Vec<u8>]| {
			let mut entries = BTreeSet::new();
			let mut others = BTreeSet::new();
			for key in keys {
				match AccessEntry::from_storage_key::<Test>(key) {
					Some(entry) => {
						entries.insert(entry);
					},
					None if covered.contains(key) => {},
					None => {
						others.insert(label_key(key, &storage_items));
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

/// Returns the keys the benchmarks cover.
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
fn label_key(key: &[u8], storage_items: &[StorageInfo]) -> String {
	use sp_core::storage::well_known_keys::is_default_child_storage_key;
	if is_default_child_storage_key(key) {
		return "child trie root".to_string();
	}
	storage_items
		.iter()
		.find(|info| key.starts_with(&info.prefix))
		.map(|info| {
			format!(
				"{}::{}",
				String::from_utf8_lossy(&info.pallet_name),
				String::from_utf8_lossy(&info.storage_name)
			)
		})
		.unwrap_or_else(|| sp_core::hexdisplay::HexDisplay::from(&key).to_string())
}

/// Deploys one contract per code, funds them, and commits, so that future reads are captured by
/// the trie recorder.
fn deploy_contracts<const N: usize>(
	contract_codes: [&[u8]; N],
) -> (sp_io::TestExternalities, [DeployedContract; N]) {
	let mut ext = ExtBuilder::default().build();
	let contracts = ext.execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let mut salt = 0u8;
		contract_codes.map(|code| {
			salt += 1;
			let Contract { addr, account_id } =
				builder::bare_instantiate(Code::Upload(code.to_vec()))
					.salt(Some([salt; 32]))
					.build_and_unwrap_contract();
			// Funded so that a contract calling another one can move value.
			let _ = <Test as Config>::Currency::set_balance(&account_id, 100_000_000_000);
			let code_hash = crate::tests::test_utils::get_contract(&addr).code_hash;
			DeployedContract { addr, account_id, code_hash }
		})
	});
	ext.commit_all().unwrap();
	(ext, contracts)
}

/// Runs `operation` on the committed state and returns what it read and wrote.
fn reads_and_writes_of<R>(
	ext: &mut sp_io::TestExternalities,
	operation: impl FnOnce() -> R,
) -> (ReadsAndWrites, R) {
	let recorder = Recorder::<sp_core::Blake2Hasher>::default();
	let outcome = ext.execute_with_recorder(recorder.clone(), operation);

	let mut recorded = recorder.recorded_keys();
	let read = recorded
		.remove(ext.backend.root())
		.expect("the operation read the trie")
		.into_keys()
		.map(|key| key.to_vec())
		.collect::<Vec<_>>();
	assert_eq!(recorded, Default::default(), "contract storage is out of scope");

	let written = ext
		.overlayed_changes()
		.changes()
		.map(|(key, _)| key.clone())
		.collect::<Vec<_>>();
	assert_eq!(ext.overlayed_changes().children().count(), 0, "contract storage is out of scope");

	// Mapping an account id back to its address reads `OriginalAccount`.
	(ext.execute_with(|| ReadsAndWrites::from_keys(&read, &written)), outcome)
}

/// Returns the reads and writes of calling `caller` with `payload`, and the two deployed
/// contracts.
fn reads_and_writes_of_call(
	caller_code: &[u8],
	target_code: &[u8],
	payload: impl FnOnce(&DeployedContract) -> Vec<u8>,
) -> (ReadsAndWrites, DeployedContract, DeployedContract) {
	let (mut ext, [caller, target]) = deploy_contracts([caller_code, target_code]);
	let payload = payload(&target);
	let (reads_writes, ()) = reads_and_writes_of(&mut ext, || {
		builder::bare_call(caller.addr).data(payload).build_and_unwrap_result();
	});
	(reads_writes, caller, target)
}

#[test_case(FixtureType::Solc;   "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn the_top_frame_accesses_what_it_declares(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Callee", fixture_type).unwrap();
	let (mut ext, [contract]) = deploy_contracts([code.as_slice()]);
	// a call that reads no contract storage.
	let (top_frame, ()) = reads_and_writes_of(&mut ext, || {
		builder::bare_call(contract.addr)
			.data(Callee::whoSenderCall {}.abi_encode())
			.build_and_unwrap_result();
	});

	assert_eq!(
		top_frame,
		ReadsAndWrites::from_declared(
			DeclaredEntries::of(CallItems::new(contract.addr, false))
				.and(CodeLoadItems::new(contract.code_hash))
		),
	);
}

#[test_case(FixtureType::Solc,   false; "solc no dust")]
#[test_case(FixtureType::Solc,   true;  "solc dust")]
#[test_case(FixtureType::Resolc, false; "resolc no dust")]
#[test_case(FixtureType::Resolc, true;  "resolc dust")]
fn the_top_frame_moving_value_accesses_what_it_declares(fixture_type: FixtureType, dust: bool) {
	let (code, _) = compile_module_with_type("DoNothingReceiver", fixture_type).unwrap();
	let (mut ext, [contract]) = deploy_contracts([code.as_slice()]);

	let value = BalanceWithDust::new_unchecked::<Test>(1, u32::from(dust));
	let (top_frame, ()) = reads_and_writes_of(&mut ext, || {
		builder::bare_call(contract.addr)
			.evm_value(Pallet::<Test>::convert_native_to_evm(value))
			.build_and_unwrap_result();
	});

	assert_eq!(
		top_frame,
		ReadsAndWrites::from_declared(
			DeclaredEntries::of(CallItems::new(contract.addr, false))
				.and(CodeLoadItems::new(contract.code_hash))
				.and(TransferItems { from: ALICE_ADDR, to: contract.addr, dust })
		),
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
		reads_and_writes_of_call(&caller_code, &target_code, |target| {
			Caller::normalCall {
				_callee: target.addr.0.into(),
				_value: 0,
				_data: Callee::echoCall { _data: 42 }.abi_encode().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		});

	assert_eq!(
		accessed,
		ReadsAndWrites::from_declared(
			DeclaredEntries::of(CallItems::new(caller.addr, false))
				.and(CodeLoadItems::new(caller.code_hash))
				.and(CallItems::new(target.addr, false))
				.and(CodeLoadItems::new(target.code_hash))
		),
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
		reads_and_writes_of_call(&caller_code, &target_code, |target| {
			Caller::delegateCall {
				_callee: target.addr.0.into(),
				_data: Callee::echoCall { _data: 42 }.abi_encode().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		});

	assert_eq!(
		accessed,
		ReadsAndWrites::from_declared(
			DeclaredEntries::of(CallItems::new(caller.addr, false))
				.and(CodeLoadItems::new(caller.code_hash))
				.and(CallItems::new(target.addr, true))
				.and(CodeLoadItems::new(target.code_hash))
		),
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
		reads_and_writes_of_call(&caller_code, &target_code, |target| {
			Caller::staticCallCall {
				_callee: target.addr.0.into(),
				_data: Callee::echoCall { _data: 42 }.abi_encode().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		});

	assert_eq!(
		accessed,
		ReadsAndWrites::from_declared(
			DeclaredEntries::of(CallItems::new(caller.addr, false))
				.and(CodeLoadItems::new(caller.code_hash))
				.and(CallItems::new(target.addr, false))
				.and(CodeLoadItems::new(target.code_hash))
		),
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
		reads_and_writes_of_call(&caller_code, &target_code, |target| {
			Caller::normalCall {
				_callee: target.addr.0.into(),
				_value: Pallet::<Test>::convert_native_to_evm(value).low_u64(),
				_data: Vec::new().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		});

	assert_eq!(
		accessed,
		ReadsAndWrites::from_declared(
			DeclaredEntries::of(CallItems::new(caller.addr, false))
				.and(CodeLoadItems::new(caller.code_hash))
				.and(CallItems::new(target.addr, false))
				.and(CodeLoadItems::new(target.code_hash))
				.and(TransferItems { from: caller.addr, to: target.addr, dust: false })
		),
	);
}

#[test_case(FixtureType::Solc;   "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn a_nested_instantiate_accesses_what_it_declares(fixture_type: FixtureType) {
	let (factory_code, _) = compile_module_with_type("Factory", fixture_type).unwrap();
	let child_code_uploaded_up_front = fixture_type == FixtureType::Resolc;
	let (mut ext, [factory]) = deploy_contracts([factory_code.as_slice()]);
	if child_code_uploaded_up_front {
		// resolc creates the child from an uploaded blob; solc stores the code on the create.
		let (child_code, _) = compile_module_with_type("ReceivingChild", fixture_type).unwrap();
		ext.execute_with(|| {
			crate::Pallet::<Test>::upload_code(
				crate::tests::RuntimeOrigin::signed(ALICE.clone()),
				child_code,
				<crate::BalanceOf<Test>>::MAX,
			)
			.unwrap();
		});
		ext.commit_all().unwrap();
	}

	let (accessed, child) = reads_and_writes_of(&mut ext, || {
		let result = builder::bare_call(factory.addr)
			.data(Factory::createCall {}.abi_encode())
			.build_and_unwrap_result();
		let created = Factory::createCall::abi_decode_returns(&result.data).unwrap();
		H160::from_slice(created.as_slice())
	});
	let child_hash = ext.execute_with(|| crate::tests::test_utils::get_contract(&child).code_hash);

	let mut declared = DeclaredEntries::of(CallItems::new(factory.addr, false))
		.and(CodeLoadItems::new(factory.code_hash))
		.and(InstantiateItems { address: child, deployer: factory.addr })
		.and(CodeLoadItems::with_info_op(child_hash, StorageOp::Write));
	// Warmed by InstantiateItems, never read: seal_instantiate and evm_instantiate pay for it.
	assert!(
		declared.remove(&AccessEntry::OriginalAccount { address: child }).is_some(),
		"the instantiate no longer has the new address's mapping",
	);
	let mut expected = ReadsAndWrites::from_declared(declared);
	// Never warmed: the origin pays the new contract's deposit, which writes its account.
	expected.read.insert(AccessEntry::Account { address: ALICE_ADDR });
	expected.written.insert(AccessEntry::Account { address: ALICE_ADDR });

	if !child_code_uploaded_up_front {
		// The list does not track this write: the create stores the code and is billed cold.
		expected.write_instead_of_read(AccessEntry::CodeBlob { hash: child_hash });
	}

	// The deposit entries, which are not tracked by the access list and always paid cold.
	let mut deposit_items = BTreeSet::from(
		[
			"Assets::Account",
			"Assets::Asset",
			"AssetsFreezer::Freezes",
			"AssetsFreezer::FrozenBalances",
			"Balances::Holds",
			"Contracts::NativeDepositOf",
		]
		.map(String::from),
	);
	if !child_code_uploaded_up_front {
		// The code deposit sits on the pallet's own account.
		deposit_items.insert("System::Account".into());
	}

	expected.other_read = deposit_items.clone();
	expected.other_written = deposit_items;
	assert_eq!(accessed, expected);
}

#[test_case(FixtureType::Solc,   false; "solc no dust")]
#[test_case(FixtureType::Solc,   true;  "solc dust")]
#[test_case(FixtureType::Resolc, false; "resolc no dust")]
#[test_case(FixtureType::Resolc, true;  "resolc dust")]
fn a_transfer_accesses_what_it_declares(fixture_type: FixtureType, dust: bool) {
	use crate::evm::transfer_with_dust;
	use frame_support::traits::tokens::Preservation;

	let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
	let (target_code, _) = compile_module_with_type("Callee", fixture_type).unwrap();
	let (mut ext, [from, to]) = deploy_contracts([caller_code.as_slice(), target_code.as_slice()]);

	let value = BalanceWithDust::new_unchecked::<Test>(1, u32::from(dust));
	let (reads_writes, ()) = reads_and_writes_of(&mut ext, || {
		transfer_with_dust::<Test>(&from.account_id, &to.account_id, value, Preservation::Preserve)
			.unwrap();
	});

	let mut expected = DeclaredEntries::of(TransferItems { from: from.addr, to: to.addr, dust });
	if !dust {
		// The access has the receiver's info; a dust-free transfer never touches it.
		assert!(
			expected.remove(&AccessEntry::AccountInfo { address: to.addr }).is_some(),
			"the transfer no longer has the receiver's info",
		);
	}
	assert_eq!(reads_writes, ReadsAndWrites::from_declared(expected));
}
