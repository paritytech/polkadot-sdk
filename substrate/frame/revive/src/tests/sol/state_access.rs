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

//! Checks what an operation reads and writes in the main trie, and the entries it warms, against
//! the entries each test declares for it.

use crate::{
	AccountInfo, BalanceOf, BalanceWithDust, Code, Config, Pallet, Weight,
	access_list::{
		Access, AccessEntry, AccessList, CallItems, CodeLoadItems, InstantiateItems, StorageOp,
		TransferItems, Warmth,
	},
	evm::fees::InfoT,
	test_utils::{
		ALICE, ALICE_ADDR, EVE_ADDR,
		builder::{BareCallBuilder, Contract},
	},
	tests::{
		AllPalletsWithSystem, ExtBuilder, RuntimeOrigin, Test, builder, test_utils::get_contract,
	},
};
use alloy_core::sol_types::SolCall;
use frame_support::traits::{
	StorageInfo, StorageInfoTrait,
	fungible::{Balanced, Mutate},
};
use pallet_revive_fixtures::{Callee, Caller, Factory, FixtureType, compile_module_with_type};
use pretty_assertions::assert_eq;
use sp_core::{H160, H256, U256};
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

	/// Adds `entry` with `op`, for an entry the runtime charges on its own, outside any access.
	fn and_entry(mut self, entry: AccessEntry, op: StorageOp) -> Self {
		self.declarations.entry(entry).or_default().add(op);
		self
	}

	/// Adds the operation each access declares for `entry`, when more than one access declares it.
	fn declared_more_than_once(
		mut self,
		entry: AccessEntry,
		ops: impl IntoIterator<Item = StorageOp>,
	) -> Self {
		let expected_counts = self.declared_more_than_once.entry(entry).or_default();
		for op in ops {
			expected_counts.add(op);
		}
		self
	}

	/// Asserts that the entries added with `declared_more_than_once` are exactly the ones more
	/// than one access declares, with the same number of reads and writes for each.
	fn assert_declared_counts(&self) {
		let mut declared_more_than_once = BTreeMap::new();
		for (entry, counts) in &self.declarations {
			if counts.total() > 1 {
				declared_more_than_once.insert(entry.clone(), *counts);
			}
		}
		assert_eq!(declared_more_than_once, self.declared_more_than_once);
	}

	/// Returns the entries the accesses warm, each as a read or a write.
	fn warmed(&self) -> BTreeMap<AccessEntry, StorageOp> {
		let mut warmed = BTreeMap::new();
		for (entry, counts) in &self.declarations {
			let op = if counts.writes > 0 { StorageOp::Write } else { StorageOp::Read };
			warmed.insert(entry.clone(), op);
		}
		warmed
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
	fn from_keys(read_keys: &[Vec<u8>], written_keys: &[Vec<u8>]) -> Self {
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
		let (read, other_read) = split(read_keys);
		let (written, other_written) = split(written_keys);
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
		// An Ethereum transaction pays deposits from the fee its signer paid up front.
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));
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

/// Commits the setup, then runs `operation` and returns what it read, wrote and warmed.
fn reads_and_writes_of<R>(
	ext: &mut sp_io::TestExternalities,
	operation: impl FnOnce() -> R,
) -> (ReadsAndWrites, BTreeMap<AccessEntry, StorageOp>, R) {
	ext.commit_all().unwrap();
	// Clears earlier entries, so `warmed` holds only the operation's.
	AccessList::take_final_entries();
	let recorder = Recorder::<sp_core::Blake2Hasher>::default();
	let outcome = ext.execute_with_recorder(recorder.clone(), operation);
	let warmed = AccessList::take_final_entries();

	let mut recorded = recorder.recorded_keys();
	let read_keys = recorded
		.remove(ext.backend.root())
		.expect("the operation read the trie")
		.into_keys()
		.map(|key| key.to_vec())
		.collect::<Vec<_>>();
	assert_eq!(recorded, Default::default(), "contract storage is out of scope");

	let written_keys = ext
		.overlayed_changes()
		.changes()
		.map(|(key, _)| key.clone())
		.collect::<Vec<_>>();
	assert_eq!(ext.overlayed_changes().children().count(), 0, "contract storage is out of scope");

	// `from_keys` reads `OriginalAccount`, and storage can be read only inside `execute_with`.
	(ext.execute_with(|| ReadsAndWrites::from_keys(&read_keys, &written_keys)), warmed, outcome)
}

/// Deploys both codes, calls the caller with the data `build_payload` returns for the target,
/// checks that the caller and its nested call succeeded, and returns what the call read, wrote
/// and warmed, along with the two deployed contracts.
fn reads_and_writes_of_call(
	caller_code: Vec<u8>,
	target_code: Vec<u8>,
	build_payload: impl FnOnce(&DeployedContract) -> Vec<u8>,
) -> (ReadsAndWrites, BTreeMap<AccessEntry, StorageOp>, DeployedContract, DeployedContract) {
	let (mut ext, [caller, target]) = deploy_contracts([caller_code, target_code]);
	let payload = build_payload(&target);
	let (accessed, warmed, nested_call_result) = reads_and_writes_of(&mut ext, || {
		let caller_result = call_with_dispatchable_config(caller.addr, TransactionKind::Substrate)
			.data(payload)
			.build_and_unwrap_result();
		assert!(!caller_result.did_revert(), "the caller returned {:?}", caller_result.data);
		Caller::normalCall::abi_decode_returns(&caller_result.data).unwrap()
	});
	assert!(nested_call_result.success, "the nested call returned {:?}", nested_call_result.output);
	(accessed, warmed, caller, target)
}

/// The kind of transaction a call runs in.
#[derive(Clone, Copy)]
enum TransactionKind {
	Substrate,
	Ethereum,
}

/// Returns a call to `dest` with the config of the dispatchable for `transaction_kind`, `call` or
/// `eth_call`, so the access list starts with the first frame's entries.
fn call_with_dispatchable_config(
	dest: H160,
	transaction_kind: TransactionKind,
) -> BareCallBuilder<Test> {
	let exec_config = match transaction_kind {
		TransactionKind::Substrate => Pallet::<Test>::substrate_call_exec_config(),
		TransactionKind::Ethereum => {
			Pallet::<Test>::eth_call_exec_config(U256::one(), 0, Weight::zero())
		},
	};
	builder::bare_call(dest).exec_config(exec_config)
}

#[test_case(false; "no dust")]
#[test_case(true;  "dust")]
fn a_transfer_accesses_what_it_declares(dust: bool) {
	use crate::evm::transfer_with_dust;
	use frame_support::traits::tokens::Preservation;

	// `transfer_with_dust` runs no contract code, so it does not depend on the backend.
	let (from_code, _) = compile_module_with_type("Caller", FixtureType::Solc).unwrap();
	let (to_code, _) = compile_module_with_type("Callee", FixtureType::Solc).unwrap();
	let (mut ext, [from, to]) = deploy_contracts([from_code, to_code]);

	let value = BalanceWithDust::new_unchecked::<Test>(1, u32::from(dust));
	let (accessed, warmed, ()) = reads_and_writes_of(&mut ext, || {
		transfer_with_dust::<Test>(&from.account_id, &to.account_id, value, Preservation::Preserve)
			.unwrap();
	});

	// `transfer_with_dust` creates no access list.
	assert_eq!(warmed, BTreeMap::new());
	let declared = DeclaredEntries::of(TransferItems { from: from.addr, to: to.addr, dust });
	let mut expected = ReadsAndWrites::from_declared(declared);
	if !dust {
		// A transfer without dust does not touch the receiver's info.
		// `CallItems` declares it as a write before each transfer, so this read finds it warm.
		assert!(
			expected.read.remove(&AccessEntry::AccountInfo { address: to.addr }),
			"`TransferItems` does not declare the receiver's `AccountInfo`",
		);
	}
	assert_eq!(accessed, expected);
}

#[test_case(FixtureType::Solc,   false; "solc")]
#[test_case(FixtureType::Resolc, false; "resolc")]
#[test_case(FixtureType::Solc,   true;  "solc through a delegation")]
#[test_case(FixtureType::Resolc, true;  "resolc through a delegation")]
fn the_top_frame_accesses_what_it_declares(fixture_type: FixtureType, through_delegation: bool) {
	let (code, _) = compile_module_with_type("Callee", fixture_type).unwrap();
	let (mut ext, [contract]) = deploy_contracts([code]);
	let target_addr = if through_delegation {
		ext.execute_with(|| {
			AccountInfo::<Test>::set_delegation(&EVE_ADDR, Some(contract.addr), &ALICE).unwrap();
		});
		EVE_ADDR
	} else {
		contract.addr
	};
	// `whoSender` reads no contract storage, which the harness does not cover.
	let (accessed, warmed, ()) = reads_and_writes_of(&mut ext, || {
		let result = call_with_dispatchable_config(target_addr, TransactionKind::Substrate)
			.data(Callee::whoSenderCall {}.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "the target returned {:?}", result.data);
	});

	let declared = DeclaredEntries::of(CallItems::new(target_addr, false))
		// A delegated account runs the code of the contract it delegates to.
		.and(CodeLoadItems::new(contract.code_hash));
	assert_eq!(warmed, declared.warmed());
	assert_eq!(accessed, ReadsAndWrites::from_declared(declared));
}

#[test_case(FixtureType::Solc,   false; "solc no dust")]
#[test_case(FixtureType::Solc,   true;  "solc dust")]
#[test_case(FixtureType::Resolc, false; "resolc no dust")]
#[test_case(FixtureType::Resolc, true;  "resolc dust")]
fn the_top_frame_moving_value_accesses_what_it_declares(fixture_type: FixtureType, dust: bool) {
	let (code, _) = compile_module_with_type("DoNothingReceiver", fixture_type).unwrap();
	let (mut ext, [contract]) = deploy_contracts([code]);

	let value = BalanceWithDust::new_unchecked::<Test>(1, u32::from(dust));
	let (accessed, warmed, ()) = reads_and_writes_of(&mut ext, || {
		let result = call_with_dispatchable_config(contract.addr, TransactionKind::Substrate)
			.evm_value(Pallet::<Test>::convert_native_to_evm(value))
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "the contract returned {:?}", result.data);
	});

	// Dust balances are stored in the account infos, so only a dust transfer writes them.
	let transfer_info_op = if dust { StorageOp::Write } else { StorageOp::Read };
	let declared = DeclaredEntries::of(CallItems::new(contract.addr, false))
		.and(CodeLoadItems::new(contract.code_hash))
		.and(TransferItems { from: ALICE_ADDR, to: contract.addr, dust })
		.declared_more_than_once(
			AccessEntry::AccountInfo { address: contract.addr },
			[StorageOp::Write, transfer_info_op],
		);
	assert_eq!(warmed, declared.warmed());
	assert_eq!(accessed, ReadsAndWrites::from_declared(declared));
}

#[test_case(false; "not delegated")]
#[test_case(true;  "delegated to an EOA")]
fn a_plain_call_to_an_eoa_accesses_what_it_declares(delegated_to_an_eoa: bool) {
	let (mut ext, []) = deploy_contracts([]);
	let eoa = H160::from([0xfe; 20]);
	let delegation_target = H160::from([0xfd; 20]);
	if delegated_to_an_eoa {
		ext.execute_with(|| {
			AccountInfo::<Test>::set_delegation(&eoa, Some(delegation_target), &ALICE).unwrap();
		});
	}

	let (accessed, warmed, ()) = reads_and_writes_of(&mut ext, || {
		call_with_dispatchable_config(eoa, TransactionKind::Substrate).build_and_unwrap_result();
	});

	let mut declared = DeclaredEntries::of(CallItems::new(eoa, false));
	if delegated_to_an_eoa {
		// The check for a chained delegation reads the target's info, outside any access.
		declared = declared
			.and_entry(AccessEntry::AccountInfo { address: delegation_target }, StorageOp::Read);
	}
	assert_eq!(warmed, declared.warmed());
	let mut expected = ReadsAndWrites::from_declared(declared);
	// No frame runs at an address with no code, so its info is read and never written back.
	// `CallItems` declares that write, so the call overpays by one write surcharge.
	assert!(
		expected.written.remove(&AccessEntry::AccountInfo { address: eoa }),
		"`CallItems` does not declare the EOA's `AccountInfo` as a write",
	);
	assert_eq!(accessed, expected);
}

#[test_case(false; "no dust")]
#[test_case(true;  "dust")]
fn a_transfer_to_an_eoa_accesses_what_it_declares(dust: bool) {
	let (mut ext, []) = deploy_contracts([]);
	let eoa = H160::from([0xfe; 20]);
	let value = BalanceWithDust::new_unchecked::<Test>(1, u32::from(dust));

	let (accessed, warmed, ()) = reads_and_writes_of(&mut ext, || {
		call_with_dispatchable_config(eoa, TransactionKind::Substrate)
			.evm_value(Pallet::<Test>::convert_native_to_evm(value))
			.build_and_unwrap_result();
	});

	// Dust balances are stored in the account infos, so only a dust transfer writes them.
	let transfer_info_op = if dust { StorageOp::Write } else { StorageOp::Read };
	let declared = DeclaredEntries::of(CallItems::new(eoa, false))
		.and(TransferItems { from: ALICE_ADDR, to: eoa, dust })
		.declared_more_than_once(
			AccessEntry::AccountInfo { address: eoa },
			[StorageOp::Write, transfer_info_op],
		);
	assert_eq!(warmed, declared.warmed());
	let mut expected = ReadsAndWrites::from_declared(declared);
	if !dust {
		// No frame runs at an address with no code, so its info is read and never written back.
		// `CallItems` declares that write, so the call overpays by one write surcharge.
		assert!(
			expected.written.remove(&AccessEntry::AccountInfo { address: eoa }),
			"`CallItems` does not declare the EOA's `AccountInfo` as a write",
		);
	}
	assert_eq!(accessed, expected);
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
	let (accessed, warmed, caller, target) =
		reads_and_writes_of_call(caller_code, target_code, |target| {
			let data = Callee::echoCall { _data: 42 }.abi_encode().into();
			match kind {
				NestedCallKind::Plain => Caller::normalCall {
					_callee: target.addr.0.into(),
					_value: 0,
					_data: data,
					_gas: u64::MAX,
				}
				.abi_encode(),
				NestedCallKind::Delegate => Caller::delegateCall {
					_callee: target.addr.0.into(),
					_data: data,
					_gas: u64::MAX,
				}
				.abi_encode(),
				NestedCallKind::Static => Caller::staticCallCall {
					_callee: target.addr.0.into(),
					_data: data,
					_gas: u64::MAX,
				}
				.abi_encode(),
			}
		});

	let declared = DeclaredEntries::of(CallItems::new(caller.addr, false))
		.and(CodeLoadItems::new(caller.code_hash))
		.and(CallItems::new(target.addr, matches!(kind, NestedCallKind::Delegate)))
		.and(CodeLoadItems::new(target.code_hash));
	assert_eq!(warmed, declared.warmed());
	assert_eq!(accessed, ReadsAndWrites::from_declared(declared));
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
	let (accessed, warmed, caller, target) =
		reads_and_writes_of_call(caller_code, target_code, |target| {
			Caller::normalCall {
				_callee: target.addr.0.into(),
				_value: Pallet::<Test>::convert_native_to_evm(value).low_u64(),
				_data: Vec::new().into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		});

	let declared = DeclaredEntries::of(CallItems::new(caller.addr, false))
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
		);
	assert_eq!(warmed, declared.warmed());
	assert_eq!(accessed, ReadsAndWrites::from_declared(declared));
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[TransactionKind::Substrate, TransactionKind::Ethereum]
)]
fn a_nested_instantiate_accesses_what_it_declares(
	fixture_type: FixtureType,
	transaction_kind: TransactionKind,
) {
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

	let (accessed, warmed, child_addr) = reads_and_writes_of(&mut ext, || {
		let result = call_with_dispatchable_config(factory.addr, transaction_kind)
			.data(Factory::createCall {}.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "the factory returned {:?}", result.data);
		let created = Factory::createCall::abi_decode_returns(&result.data).unwrap();
		H160::from_slice(created.as_slice())
	});
	let child_hash = ext.execute_with(|| get_contract(&child_addr).code_hash);

	let declared = DeclaredEntries::of(CallItems::new(factory.addr, false))
		.and(CodeLoadItems::new(factory.code_hash))
		.and(InstantiateItems { address: child_addr, deployer: factory.addr })
		.and(CodeLoadItems::with_info_op(child_hash, StorageOp::Write));
	assert_eq!(warmed, declared.warmed());
	let mut expected = ReadsAndWrites::from_declared(declared);
	// The instantiate never reads it, but its benchmarks prepay a later call's read.
	assert!(
		expected.read.remove(&AccessEntry::OriginalAccount { address: child_addr }),
		"`InstantiateItems` does not declare the new contract's `OriginalAccount`",
	);
	if matches!(transaction_kind, TransactionKind::Substrate) {
		// A Substrate transaction charges the new contract's deposit to the origin's account. No
		// access declares it, because the signed transaction already touched that account for its
		// nonce and fee. An Ethereum transaction pays the deposit from the prepaid fee.
		expected.read.insert(AccessEntry::Account { address: ALICE_ADDR });
		expected.written.insert(AccessEntry::Account { address: ALICE_ADDR });
	}

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
			"`CodeLoadItems` does not declare the child's `PristineCode`",
		);
		expected.written.insert(child_blob);
		// The create's benchmark also pays for holding the code deposit on the pallet's account.
		deposit_items.insert("System::Account".into());
	}
	expected.other_read = deposit_items.clone();
	expected.other_written = deposit_items;
	// The create reads the deletion queue counter to derive the child's trie id; its benchmark
	// pays for that read.
	expected.other_read.insert("Contracts::DeletionQueueCounter".into());
	assert_eq!(accessed, expected);
}
