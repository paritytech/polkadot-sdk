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
	AccountInfo, BalanceWithDust, Code, Config, Pallet,
	access_list::{
		Access, AccessEntry, CallItems, CodeLoadItems, InstantiateItems, StorageOp, TransferItems,
		Warmth,
	},
	test_utils::{ALICE, ALICE_ADDR, EVE_ADDR, builder::Contract},
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

/// The entries one or more accesses declare, each with its operation.
#[derive(Default)]
struct DeclaredEntries {
	/// Each entry the accesses declare, with its operation: a write if any of them writes it.
	entries: BTreeMap<AccessEntry, StorageOp>,
	shared: SharedEntries,
}

impl DeclaredEntries {
	/// Returns the entries one access declares.
	fn of(access: impl Access) -> Self {
		Self::default().and(access)
	}

	/// Adds the entries `access` declares.
	fn and(mut self, access: impl Access) -> Self {
		access.expand(|entry, op| {
			self.insert_or_update(entry, op);
			Warmth::new_cold_non_revertible()
		});
		self
	}

	/// Lists `entry` as declared by one access for each operation in `ops`.
	fn shared_by(mut self, entry: AccessEntry, ops: impl IntoIterator<Item = StorageOp>) -> Self {
		let expected = self.shared.expected.entry(entry).or_default();
		for op in ops {
			expected.add(op);
		}
		self
	}

	/// Adds `entry` with `op`; an entry declared as both a read and a write stays a write.
	/// Counts every declaration in `shared`.
	fn insert_or_update(&mut self, entry: AccessEntry, op: StorageOp) {
		self.shared.counted.entry(entry.clone()).or_default().add(op);
		let declared_op = self.entries.entry(entry).or_insert(op);
		if !declared_op.covers(op) {
			*declared_op = op;
		}
	}
}

/// The operations the accesses declare on each entry, checked against `shared_by`.
#[derive(Default)]
struct SharedEntries {
	/// How many accesses declare each entry, as a read and as a write.
	counted: BTreeMap<AccessEntry, OpCounts>,
	/// The same counts, as the test expects them for each entry listed with `shared_by`.
	expected: BTreeMap<AccessEntry, OpCounts>,
}

impl SharedEntries {
	/// Asserts that each entry is declared by one access, or with the operations `shared_by`
	/// lists.
	fn assert_as_listed(&self) {
		for (entry, &counted) in &self.counted {
			let Some(&expected) = self.expected.get(entry) else {
				assert_eq!(
					counted.total(),
					1,
					"{entry:?} is declared as {counted:?}: list the operations with `shared_by`",
				);
				continue;
			};
			assert!(
				counted.total() <= expected.total(),
				"{entry:?} is declared as {counted:?}, but the test expects {expected:?}: \
				 list the operations with `shared_by`",
			);
			assert!(
				counted.total() >= expected.total(),
				"{entry:?} is declared as {counted:?}, but the test expects {expected:?}: \
				 an access no longer declares it",
			);
			assert_eq!(
				counted, expected,
				"{entry:?}: an access declares it with another operation than the test expects",
			);
		}
		for (entry, expected) in &self.expected {
			assert!(
				self.counted.contains_key(entry),
				"{entry:?} is not declared, but the test expects {expected:?}: an access no \
				 longer declares it",
			);
		}
	}
}

/// How many accesses declare an entry as a read, and how many as a write.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct OpCounts {
	reads: u32,
	writes: u32,
}

impl OpCounts {
	/// Counts one more access that declares the entry with `op`.
	fn add(&mut self, op: StorageOp) {
		match op {
			StorageOp::Read => self.reads += 1,
			StorageOp::Write => self.writes += 1,
		}
	}

	/// Returns how many accesses declare the entry.
	fn total(self) -> u32 {
		self.reads + self.writes
	}
}

/// What one operation read from the trie and wrote to the overlay.
#[derive(Debug, PartialEq)]
struct ReadsAndWrites {
	/// Keys the access list tracks, as entries.
	read: BTreeSet<AccessEntry>,
	written: BTreeSet<AccessEntry>,
	/// Keys the access list does not track, named by their pallet and storage item.
	other_read: BTreeSet<String>,
	other_written: BTreeSet<String>,
}

impl ReadsAndWrites {
	/// Builds the expected reads and writes from the `declared` entries.
	fn from_declared(declared: DeclaredEntries) -> Self {
		declared.shared.assert_as_listed();
		let mut read = BTreeSet::new();
		let mut written = BTreeSet::new();
		for (entry, op) in declared.entries {
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
	/// and storage item they belong to. Keys the benchmarks already cover are dropped.
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
		// Not whitelisted, but the `call` benchmark measures it: one read when the stack is built.
		.chain(core::iter::once(pallet_timestamp::Now::<Test>::hashed_key().to_vec()))
		.collect()
}

/// Returns the name of a key as its pallet and storage item, `"child trie root"` for a key that
/// holds a child trie's root, or the key in hex when no pallet claims it.
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
	assert!(
		ext.overlayed_changes().changes().next().is_none(),
		"the setup must be committed before the operation runs",
	);
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

	// Checking whether an account id is mapped reads `OriginalAccount`.
	(ext.execute_with(|| ReadsAndWrites::from_keys(&read, &written)), outcome)
}

/// Deploys both codes, calls the caller with the data `payload` builds for the target, checks
/// that the caller's nested call succeeded, and returns the call's reads and writes along with
/// the two deployed contracts.
fn reads_and_writes_of_call(
	caller_code: &[u8],
	target_code: &[u8],
	payload: impl FnOnce(&DeployedContract) -> Vec<u8>,
) -> (ReadsAndWrites, DeployedContract, DeployedContract) {
	let (mut ext, [caller, target]) = deploy_contracts([caller_code, target_code]);
	let payload = payload(&target);
	let (reads_writes, nested_call) = reads_and_writes_of(&mut ext, || {
		let result = builder::bare_call(caller.addr).data(payload).build_and_unwrap_result();
		Caller::normalCall::abi_decode_returns(&result.data).unwrap()
	});
	assert!(nested_call.success, "the nested call returned {:?}", nested_call.output);
	(reads_writes, caller, target)
}

#[test_case(FixtureType::Solc;   "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn the_top_frame_accesses_what_it_declares(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Callee", fixture_type).unwrap();
	let (mut ext, [contract]) = deploy_contracts([code.as_slice()]);
	// `whoSender` reads no contract storage: the harness covers only the main trie.
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

	// Dust balances live in the account infos, so only a dust transfer writes them.
	let transfer_info_op = if dust { StorageOp::Write } else { StorageOp::Read };
	assert_eq!(
		top_frame,
		ReadsAndWrites::from_declared(
			DeclaredEntries::of(CallItems::new(contract.addr, false))
				.and(CodeLoadItems::new(contract.code_hash))
				.and(TransferItems { from: ALICE_ADDR, to: contract.addr, dust })
				.shared_by(
					AccessEntry::AccountInfo { address: contract.addr },
					[StorageOp::Write, transfer_info_op],
				)
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
				.shared_by(
					AccessEntry::AccountInfo { address: caller.addr },
					[StorageOp::Write, StorageOp::Read],
				)
				.shared_by(
					AccessEntry::AccountInfo { address: target.addr },
					[StorageOp::Write, StorageOp::Read],
				)
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

	let declared = DeclaredEntries::of(CallItems::new(factory.addr, false))
		.and(CodeLoadItems::new(factory.code_hash))
		.and(InstantiateItems { address: child, deployer: factory.addr })
		.and(CodeLoadItems::with_info_op(child_hash, StorageOp::Write));
	let mut expected = ReadsAndWrites::from_declared(declared);
	// The instantiate never reads it, but its benches prepay a later call's read.
	assert!(
		expected.read.remove(&AccessEntry::OriginalAccount { address: child }),
		"`InstantiateItems` no longer declares the new contract's `OriginalAccount`",
	);
	// Never warmed: the new contract's deposit is charged to the origin's account.
	expected.read.insert(AccessEntry::Account { address: ALICE_ADDR });
	expected.written.insert(AccessEntry::Account { address: ALICE_ADDR });

	if !child_code_uploaded_up_front {
		// The list does not track this write: the create stores the code and is billed cold.
		expected.write_instead_of_read(AccessEntry::PristineCode { hash: child_hash });
	}

	// The deposit entries, which the access list does not track.
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
		// The code deposit is held on the pallet's own account.
		deposit_items.insert("System::Account".into());
	}
	let deposit_writes = deposit_items.clone();

	expected.other_read = deposit_items;
	expected.other_written = deposit_writes;
	assert_eq!(accessed, expected);
}

#[test]
fn a_plain_call_to_an_eoa_accesses_what_it_declares() {
	let (mut ext, []) = deploy_contracts::<0>([]);
	let eoa = H160::from([0xfe; 20]);

	let (accessed, ()) = reads_and_writes_of(&mut ext, || {
		builder::bare_call(eoa).build_and_unwrap_result();
	});

	let mut expected =
		ReadsAndWrites::from_declared(DeclaredEntries::of(CallItems::new(eoa, false)));
	// No frame runs at an address with no code, so its info is read and never written back.
	// `CallItems` declares that write, so the call overpays by one write.
	assert!(expected.written.remove(&AccessEntry::AccountInfo { address: eoa }));
	assert_eq!(accessed, expected);
}

#[test_case(false; "no dust")]
#[test_case(true;  "dust")]
fn a_transfer_to_an_eoa_accesses_what_it_declares(dust: bool) {
	let (mut ext, []) = deploy_contracts::<0>([]);
	let eoa = H160::from([0xfe; 20]);
	let value = BalanceWithDust::new_unchecked::<Test>(1, u32::from(dust));

	let (accessed, ()) = reads_and_writes_of(&mut ext, || {
		builder::bare_call(eoa)
			.evm_value(Pallet::<Test>::convert_native_to_evm(value))
			.build_and_unwrap_result();
	});

	// Dust balances live in the account infos, so only a dust transfer writes them.
	let transfer_info_op = if dust { StorageOp::Write } else { StorageOp::Read };
	let mut expected = ReadsAndWrites::from_declared(
		DeclaredEntries::of(CallItems::new(eoa, false))
			.and(TransferItems { from: ALICE_ADDR, to: eoa, dust })
			.shared_by(
				AccessEntry::AccountInfo { address: eoa },
				[StorageOp::Write, transfer_info_op],
			),
	);
	if !dust {
		// No frame runs at an address with no code, so its info is read and never written back.
		assert!(expected.written.remove(&AccessEntry::AccountInfo { address: eoa }));
	}
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

	let declared = DeclaredEntries::of(TransferItems { from: from.addr, to: to.addr, dust });
	let mut expected = ReadsAndWrites::from_declared(declared);
	if !dust {
		// A transfer without dust does not touch the receiver's info.
		assert!(
			expected.read.remove(&AccessEntry::AccountInfo { address: to.addr }),
			"`TransferItems` no longer declares the receiver's `AccountInfo`",
		);
	}
	assert_eq!(reads_writes, expected);
}

#[test_case(FixtureType::Solc;   "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn a_call_to_a_delegated_account_accesses_what_it_declares(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Callee", fixture_type).unwrap();
	let (mut ext, [target]) = deploy_contracts([code.as_slice()]);
	ext.execute_with(|| {
		AccountInfo::<Test>::set_delegation(&EVE_ADDR, Some(target.addr), &ALICE).unwrap();
	});
	ext.commit_all().unwrap();

	let (accessed, ()) = reads_and_writes_of(&mut ext, || {
		builder::bare_call(EVE_ADDR)
			.data(Callee::whoSenderCall {}.abi_encode())
			.build_and_unwrap_result();
	});

	assert_eq!(
		accessed,
		ReadsAndWrites::from_declared(
			DeclaredEntries::of(CallItems::new(EVE_ADDR, false))
				// The delegated account runs the target's code.
				.and(CodeLoadItems::new(target.code_hash))
		),
	);
}
