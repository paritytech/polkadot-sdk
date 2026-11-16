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

//! Checks what an operation reads and writes in the main trie against the entries each test
//! declares for it.

use crate::{
	AccountInfo, BalanceOf, BalanceWithDust, Code, Config, Pallet,
	access_list::{
		Access, AccessEntry, CallItems, CodeLoadItems, InstantiateItems, StorageOp, TransferItems,
		Warmth,
	},
	test_utils::{ALICE, ALICE_ADDR, EVE_ADDR, builder::Contract},
	tests::{
		AllPalletsWithSystem, ExtBuilder, RuntimeOrigin, Test, builder, test_utils::get_contract,
	},
};
use alloy_core::sol_types::SolCall;
use frame_support::traits::{StorageInfo, StorageInfoTrait, fungible::Mutate};
use pallet_revive_fixtures::{Callee, Caller, Factory, FixtureType, compile_module_with_type};
use pretty_assertions::assert_eq;
use sp_core::{H160, H256};
use sp_trie::recorder::Recorder;
use std::collections::{BTreeMap, BTreeSet};
use test_case::{test_case, test_matrix};

/// A deployed contract and the hash of its runtime code.
struct DeployedContract {
	addr: H160,
	account_id: <Test as frame_system::Config>::AccountId,
	code_hash: H256,
}

/// The entries one or more accesses declare.
#[derive(Default)]
struct DeclaredEntries {
	/// How many accesses declare each entry, as a read and as a write.
	declarations: BTreeMap<AccessEntry, OpCounts>,
	/// For each entry that more than one access declares, how many accesses the test expects to
	/// declare it as a read and as a write.
	declared_more_than_once: BTreeMap<AccessEntry, OpCounts>,
}

impl DeclaredEntries {
	/// Returns the entries one access declares.
	fn of(access: impl Access) -> Self {
		Self::default().and(access)
	}

	/// Adds the entries `access` declares.
	fn and(mut self, access: impl Access) -> Self {
		access.expand(|entry, op| {
			self.declarations.entry(entry).or_default().add(op);
			Warmth::new_cold_non_revertible()
		});
		self
	}

	/// Adds the operation each access declares for `entry`, when more than one access declares it.
	fn declared_more_than_once(
		mut self,
		entry: AccessEntry,
		ops: impl IntoIterator<Item = StorageOp>,
	) -> Self {
		let expected = self.declared_more_than_once.entry(entry).or_default();
		for op in ops {
			expected.add(op);
		}
		self
	}

	/// Asserts that `declared_more_than_once` adds exactly the entries that more than one access
	/// declares, with the operations they declare.
	fn assert_declared_counts(&self) {
		let mut declared_more_than_once = BTreeMap::new();
		for (entry, counts) in &self.declarations {
			if counts.total() > 1 {
				declared_more_than_once.insert(entry.clone(), *counts);
			}
		}
		assert_eq!(declared_more_than_once, self.declared_more_than_once);
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
///
/// Reads made by walking a key prefix (`next_key`) are missing, because the recorder keeps no key.
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
		declared.assert_declared_counts();
		let mut read = BTreeSet::new();
		let mut written = BTreeSet::new();
		for (entry, counts) in declared.declarations {
			if counts.writes > 0 {
				written.insert(entry.clone());
			}
			read.insert(entry);
		}
		Self { read, written, other_read: BTreeSet::new(), other_written: BTreeSet::new() }
	}

	/// Splits recorded keys into the entries the access list tracks and, for the rest, the pallet
	/// and storage item they belong to. Keys the benchmarks already cover are dropped.
	fn from_keys(read: &[Vec<u8>], written: &[Vec<u8>]) -> Self {
		let covered = keys_the_benchmarks_cover();
		let storage_items = AllPalletsWithSystem::storage_info();
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

/// Returns the keys the pallets whitelist for benchmarks, plus the keys the tested operations
/// read that the benchmark macro whitelists or a benchmark measures.
fn keys_the_benchmarks_cover() -> BTreeSet<Vec<u8>> {
	use frame_support::{
		storage::transactional::TRANSACTION_LEVEL_KEY, traits::WhitelistedStorageKeys,
	};
	AllPalletsWithSystem::whitelisted_storage_keys()
		.into_iter()
		.map(|whitelisted| whitelisted.key)
		// The benchmark macro whitelists the transactional layer itself, outside any pallet.
		.chain(core::iter::once(TRANSACTION_LEVEL_KEY.to_vec()))
		// Not whitelisted, but the `call` benchmark measures its one read when the stack is built.
		.chain(core::iter::once(pallet_timestamp::Now::<Test>::hashed_key().to_vec()))
		.collect()
}

/// Returns the name of a key as its pallet and storage item, or the key in hex when no pallet
/// claims it.
fn label_key(key: &[u8], storage_items: &[StorageInfo]) -> String {
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

/// Deploys one contract per code and funds them.
fn deploy_contracts<const N: usize>(
	contract_codes: [Vec<u8>; N],
) -> (sp_io::TestExternalities, [DeployedContract; N]) {
	let mut ext = ExtBuilder::default().build();
	let contracts = ext.execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		contract_codes.map(|code| {
			let Contract { addr, account_id } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
			// Funded so that a contract calling another one can move value.
			let _ = <Test as Config>::Currency::set_balance(&account_id, 100_000_000_000);
			let code_hash = get_contract(&addr).code_hash;
			DeployedContract { addr, account_id, code_hash }
		})
	});
	(ext, contracts)
}

/// Commits the setup, then runs `operation` and returns what it read and wrote.
fn reads_and_writes_of<R>(
	ext: &mut sp_io::TestExternalities,
	operation: impl FnOnce() -> R,
) -> (ReadsAndWrites, R) {
	ext.commit_all().unwrap();
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

	// `from_keys` reads `OriginalAccount`, and storage can be read only inside `execute_with`.
	(ext.execute_with(|| ReadsAndWrites::from_keys(&read, &written)), outcome)
}

/// Deploys both codes, calls the caller with the data `payload` builds for the target, checks
/// that the caller and its nested call succeeded, and returns the call's reads and writes along
/// with the two deployed contracts.
fn reads_and_writes_of_call(
	caller_code: Vec<u8>,
	target_code: Vec<u8>,
	payload: impl FnOnce(&DeployedContract) -> Vec<u8>,
) -> (ReadsAndWrites, DeployedContract, DeployedContract) {
	let (mut ext, [caller, target]) = deploy_contracts([caller_code, target_code]);
	let payload = payload(&target);
	let (reads_writes, nested_call) = reads_and_writes_of(&mut ext, || {
		let result = builder::bare_call(caller.addr).data(payload).build_and_unwrap_result();
		assert!(!result.did_revert(), "the caller returned {:?}", result.data);
		Caller::normalCall::abi_decode_returns(&result.data).unwrap()
	});
	assert!(nested_call.success, "the nested call returned {:?}", nested_call.output);
	(reads_writes, caller, target)
}

#[test_case(FixtureType::Solc,   false; "solc")]
#[test_case(FixtureType::Resolc, false; "resolc")]
#[test_case(FixtureType::Solc,   true;  "solc through a delegation")]
#[test_case(FixtureType::Resolc, true;  "resolc through a delegation")]
fn the_top_frame_accesses_what_it_declares(fixture_type: FixtureType, through_delegation: bool) {
	let (code, _) = compile_module_with_type("Callee", fixture_type).unwrap();
	let (mut ext, [contract]) = deploy_contracts([code]);
	let target = if through_delegation {
		ext.execute_with(|| {
			AccountInfo::<Test>::set_delegation(&EVE_ADDR, Some(contract.addr), &ALICE).unwrap();
		});
		EVE_ADDR
	} else {
		contract.addr
	};
	// `whoSender` reads no contract storage, which the harness does not cover.
	let (top_frame, ()) = reads_and_writes_of(&mut ext, || {
		let result = builder::bare_call(target)
			.data(Callee::whoSenderCall {}.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "the target returned {:?}", result.data);
	});

	assert_eq!(
		top_frame,
		ReadsAndWrites::from_declared(
			DeclaredEntries::of(CallItems::new(target, false))
				// A delegated account runs the code of the contract it delegates to.
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
	let (mut ext, [contract]) = deploy_contracts([code]);

	let value = BalanceWithDust::new_unchecked::<Test>(1, u32::from(dust));
	let (top_frame, ()) = reads_and_writes_of(&mut ext, || {
		let result = builder::bare_call(contract.addr)
			.evm_value(Pallet::<Test>::convert_native_to_evm(value))
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "the contract returned {:?}", result.data);
	});

	// Dust balances are stored in the account infos, so only a dust transfer writes them.
	let transfer_info_op = if dust { StorageOp::Write } else { StorageOp::Read };
	assert_eq!(
		top_frame,
		ReadsAndWrites::from_declared(
			DeclaredEntries::of(CallItems::new(contract.addr, false))
				.and(CodeLoadItems::new(contract.code_hash))
				.and(TransferItems { from: ALICE_ADDR, to: contract.addr, dust })
				.declared_more_than_once(
					AccessEntry::AccountInfo { address: contract.addr },
					[StorageOp::Write, transfer_info_op],
				)
		),
	);
}

/// How the caller contract calls the target.
enum NestedCallKind {
	Plain,
	Delegate,
	Static,
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[FixtureType::Solc, FixtureType::Resolc],
	[NestedCallKind::Plain, NestedCallKind::Delegate, NestedCallKind::Static]
)]
fn a_nested_call_accesses_what_it_declares(
	caller_type: FixtureType,
	target_type: FixtureType,
	kind: NestedCallKind,
) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (target_code, _) = compile_module_with_type("Callee", target_type).unwrap();
	let (accessed, caller, target) = reads_and_writes_of_call(caller_code, target_code, |target| {
		let data = Callee::echoCall { _data: 42 }.abi_encode().into();
		match kind {
			NestedCallKind::Plain => Caller::normalCall {
				_callee: target.addr.0.into(),
				_value: 0,
				_data: data,
				_gas: u64::MAX,
			}
			.abi_encode(),
			NestedCallKind::Delegate => {
				Caller::delegateCall { _callee: target.addr.0.into(), _data: data, _gas: u64::MAX }
					.abi_encode()
			},
			NestedCallKind::Static => Caller::staticCallCall {
				_callee: target.addr.0.into(),
				_data: data,
				_gas: u64::MAX,
			}
			.abi_encode(),
		}
	});

	assert_eq!(
		accessed,
		ReadsAndWrites::from_declared(
			DeclaredEntries::of(CallItems::new(caller.addr, false))
				.and(CodeLoadItems::new(caller.code_hash))
				.and(CallItems::new(target.addr, matches!(kind, NestedCallKind::Delegate)))
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
	let (accessed, caller, target) = reads_and_writes_of_call(caller_code, target_code, |target| {
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
				.declared_more_than_once(
					AccessEntry::AccountInfo { address: caller.addr },
					[StorageOp::Write, StorageOp::Read],
				)
				.declared_more_than_once(
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
	let (mut ext, [factory]) = deploy_contracts([factory_code]);
	if child_code_uploaded_up_front {
		// resolc creates the child from an uploaded blob; solc stores the code on the create.
		let (child_code, _) = compile_module_with_type("ReceivingChild", fixture_type).unwrap();
		ext.execute_with(|| {
			Pallet::<Test>::upload_code(
				RuntimeOrigin::signed(ALICE.clone()),
				child_code,
				<BalanceOf<Test>>::MAX,
			)
			.unwrap();
		});
	}

	let (accessed, child) = reads_and_writes_of(&mut ext, || {
		let result = builder::bare_call(factory.addr)
			.data(Factory::createCall {}.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "the factory returned {:?}", result.data);
		let created = Factory::createCall::abi_decode_returns(&result.data).unwrap();
		H160::from_slice(created.as_slice())
	});
	let child_hash = ext.execute_with(|| get_contract(&child).code_hash);

	let declared = DeclaredEntries::of(CallItems::new(factory.addr, false))
		.and(CodeLoadItems::new(factory.code_hash))
		.and(InstantiateItems { address: child, deployer: factory.addr })
		.and(CodeLoadItems::with_info_op(child_hash, StorageOp::Write));
	let mut expected = ReadsAndWrites::from_declared(declared);
	// The instantiate never reads it, but its benchmarks prepay a later call's read.
	assert!(
		expected.read.remove(&AccessEntry::OriginalAccount { address: child }),
		"`InstantiateItems` no longer declares the new contract's `OriginalAccount`",
	);
	// The new contract's deposit is charged to the origin's account, which no access here declares.
	expected.read.insert(AccessEntry::Account { address: ALICE_ADDR });
	expected.written.insert(AccessEntry::Account { address: ALICE_ADDR });

	// The deposit's storage items, which the access list does not track.
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
		// The create stores the child's code without reading it; its benchmark pays for that write.
		// It then warms the code for a later load with `CodeLoadItems`, which declares a read.
		// The code's later writes do not go through the list, so none is overcharged.
		let child_blob = AccessEntry::PristineCode { hash: child_hash };
		assert!(
			expected.read.remove(&child_blob),
			"`CodeLoadItems` no longer declares the child's `PristineCode`",
		);
		expected.written.insert(child_blob);
		// The create's benchmark also pays for holding the code deposit on the pallet's account.
		deposit_items.insert("System::Account".into());
	}
	expected.other_read = deposit_items.clone();
	expected.other_written = deposit_items;
	assert_eq!(accessed, expected);
}

#[test]
fn a_plain_call_to_an_eoa_accesses_what_it_declares() {
	let (mut ext, []) = deploy_contracts([]);
	let eoa = H160::from([0xfe; 20]);

	let (accessed, ()) = reads_and_writes_of(&mut ext, || {
		builder::bare_call(eoa).build_and_unwrap_result();
	});

	let mut expected =
		ReadsAndWrites::from_declared(DeclaredEntries::of(CallItems::new(eoa, false)));
	// No frame runs at an address with no code, so its info is read and never written back.
	// `CallItems` declares that write, so the call overpays by one write.
	assert!(
		expected.written.remove(&AccessEntry::AccountInfo { address: eoa }),
		"`CallItems` no longer declares the EOA's `AccountInfo` as a write",
	);
	assert_eq!(accessed, expected);
}

#[test_case(false; "no dust")]
#[test_case(true;  "dust")]
fn a_transfer_to_an_eoa_accesses_what_it_declares(dust: bool) {
	let (mut ext, []) = deploy_contracts([]);
	let eoa = H160::from([0xfe; 20]);
	let value = BalanceWithDust::new_unchecked::<Test>(1, u32::from(dust));

	let (accessed, ()) = reads_and_writes_of(&mut ext, || {
		builder::bare_call(eoa)
			.evm_value(Pallet::<Test>::convert_native_to_evm(value))
			.build_and_unwrap_result();
	});

	// Dust balances are stored in the account infos, so only a dust transfer writes them.
	let transfer_info_op = if dust { StorageOp::Write } else { StorageOp::Read };
	let mut expected = ReadsAndWrites::from_declared(
		DeclaredEntries::of(CallItems::new(eoa, false))
			.and(TransferItems { from: ALICE_ADDR, to: eoa, dust })
			.declared_more_than_once(
				AccessEntry::AccountInfo { address: eoa },
				[StorageOp::Write, transfer_info_op],
			),
	);
	if !dust {
		// No frame runs at an address with no code, so its info is read and never written back.
		// `CallItems` declares that write, so the call overpays by one write.
		assert!(
			expected.written.remove(&AccessEntry::AccountInfo { address: eoa }),
			"`CallItems` no longer declares the EOA's `AccountInfo` as a write",
		);
	}
	assert_eq!(accessed, expected);
}

#[test_case(false; "no dust")]
#[test_case(true;  "dust")]
fn a_transfer_accesses_what_it_declares(dust: bool) {
	use crate::evm::transfer_with_dust;
	use frame_support::traits::tokens::Preservation;

	// `transfer_with_dust` runs no contract code, so it does not depend on the backend.
	let (caller_code, _) = compile_module_with_type("Caller", FixtureType::Solc).unwrap();
	let (target_code, _) = compile_module_with_type("Callee", FixtureType::Solc).unwrap();
	let (mut ext, [from, to]) = deploy_contracts([caller_code, target_code]);

	let value = BalanceWithDust::new_unchecked::<Test>(1, u32::from(dust));
	let (reads_writes, ()) = reads_and_writes_of(&mut ext, || {
		transfer_with_dust::<Test>(&from.account_id, &to.account_id, value, Preservation::Preserve)
			.unwrap();
	});

	let declared = DeclaredEntries::of(TransferItems { from: from.addr, to: to.addr, dust });
	let mut expected = ReadsAndWrites::from_declared(declared);
	if !dust {
		// A transfer without dust does not touch the receiver's info.
		// `CallItems` declares it as a write before each transfer, so this read finds it warm.
		assert!(
			expected.read.remove(&AccessEntry::AccountInfo { address: to.addr }),
			"`TransferItems` no longer declares the receiver's `AccountInfo`",
		);
	}
	assert_eq!(reads_writes, expected);
}
