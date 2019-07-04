// Copyright 2018-2019 Parity Technologies (UK) Ltd.
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
// along with Substrate. If not, see <http://www.gnu.org/licenses/>.

// TODO: #1417 Add more integration tests
// also remove the #![allow(unused)] below.

#![allow(unused)]

use crate::account_db::{AccountDb, DirectAccountDb, OverlayAccountDb};
use crate::{
	BalanceOf, ComputeDispatchFee, ContractAddressFor, ContractInfo, ContractInfoOf, GenesisConfig,
	Module, RawAliveContractInfo, RawEvent, Trait, TrieId, TrieIdFromParentCounter, TrieIdGenerator,
};
use assert_matches::assert_matches;
use hex_literal::*;
use parity_codec::{Decode, Encode, KeyedVec};
use runtime_io;
use runtime_io::with_externalities;
use runtime_primitives::testing::{Digest, DigestItem, Header, UintAuthorityId, H256};
use runtime_primitives::traits::{BlakeTwo256, IdentityLookup};
use runtime_primitives::BuildStorage;
use srml_support::{
	assert_ok, assert_err, impl_outer_dispatch, impl_outer_event, impl_outer_origin, parameter_types,
	storage::child,	StorageMap, StorageValue, traits::{Currency, Get},
};
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use substrate_primitives::storage::well_known_keys;
use substrate_primitives::Blake2Hasher;
use system::{self, EventRecord, Phase};
use {balances, wabt};

mod contract {
	// Re-export contents of the root. This basically
	// needs to give a name for the current crate.
	// This hack is required for `impl_outer_event!`.
	pub use super::super::*;
	use srml_support::impl_outer_event;
}
impl_outer_event! {
	pub enum MetaEvent for Test {
		balances<T>, contract<T>,
	}
}
impl_outer_origin! {
	pub enum Origin for Test { }
}
impl_outer_dispatch! {
	pub enum Call for Test where origin: Origin {
		balances::Balances,
		contract::Contract,
	}
}

thread_local! {
	static EXISTENTIAL_DEPOSIT: RefCell<u64> = RefCell::new(0);
	static TRANSFER_FEE: RefCell<u64> = RefCell::new(0);
	static CREATION_FEE: RefCell<u64> = RefCell::new(0);
	static BLOCK_GAS_LIMIT: RefCell<u64> = RefCell::new(0);
}

pub struct ExistentialDeposit;
impl Get<u64> for ExistentialDeposit {
	fn get() -> u64 { EXISTENTIAL_DEPOSIT.with(|v| *v.borrow()) }
}

pub struct TransferFee;
impl Get<u64> for TransferFee {
	fn get() -> u64 { TRANSFER_FEE.with(|v| *v.borrow()) }
}

pub struct CreationFee;
impl Get<u64> for CreationFee {
	fn get() -> u64 { CREATION_FEE.with(|v| *v.borrow()) }
}

pub struct BlockGasLimit;
impl Get<u64> for BlockGasLimit {
	fn get() -> u64 { BLOCK_GAS_LIMIT.with(|v| *v.borrow()) }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Test;
impl system::Trait for Test {
	type Origin = Origin;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = MetaEvent;
}
parameter_types! {
	pub const BalancesTransactionBaseFee: u64 = 0;
	pub const BalancesTransactionByteFee: u64 = 0;
}
impl balances::Trait for Test {
	type Balance = u64;
	type OnFreeBalanceZero = Contract;
	type OnNewAccount = ();
	type Event = MetaEvent;
	type TransactionPayment = ();
	type DustRemoval = ();
	type TransferPayment = ();
	type ExistentialDeposit = ExistentialDeposit;
	type TransferFee = TransferFee;
	type CreationFee = CreationFee;
	type TransactionBaseFee = BalancesTransactionBaseFee;
	type TransactionByteFee = BalancesTransactionByteFee;
}
impl timestamp::Trait for Test {
	type Moment = u64;
	type OnTimestampSet = ();
}
parameter_types! {
	pub const SignedClaimHandicap: u64 = 2;
	pub const TombstoneDeposit: u64 = 16;
	pub const StorageSizeOffset: u32 = 8;
	pub const RentByteFee: u64 = 4;
	pub const RentDepositOffset: u64 = 10_000;
	pub const SurchargeReward: u64 = 150;
	pub const TransactionBaseFee: u64 = 2;
	pub const TransactionByteFee: u64 = 6;
	pub const ContractFee: u64 = 21;
	pub const CallBaseFee: u64 = 135;
	pub const CreateBaseFee: u64 = 175;
	pub const MaxDepth: u32 = 100;
}
impl Trait for Test {
	type Currency = Balances;
	type Call = Call;
	type DetermineContractAddress = DummyContractAddressFor;
	type Event = MetaEvent;
	type ComputeDispatchFee = DummyComputeDispatchFee;
	type TrieIdGenerator = DummyTrieIdGenerator;
	type GasPayment = ();
	type SignedClaimHandicap = SignedClaimHandicap;
	type TombstoneDeposit = TombstoneDeposit;
	type StorageSizeOffset = StorageSizeOffset;
	type RentByteFee = RentByteFee;
	type RentDepositOffset = RentDepositOffset;
	type SurchargeReward = SurchargeReward;
	type TransferFee = TransferFee;
	type CreationFee = CreationFee;
	type TransactionBaseFee = TransactionBaseFee;
	type TransactionByteFee = TransactionByteFee;
	type ContractFee = ContractFee;
	type CallBaseFee = CallBaseFee;
	type CreateBaseFee = CreateBaseFee;
	type MaxDepth = MaxDepth;
	type BlockGasLimit = BlockGasLimit;
}

type Balances = balances::Module<Test>;
type Contract = Module<Test>;
type System = system::Module<Test>;

pub struct DummyContractAddressFor;
impl ContractAddressFor<H256, u64> for DummyContractAddressFor {
	fn contract_address_for(_code_hash: &H256, _data: &[u8], origin: &u64) -> u64 {
		*origin + 1
	}
}

pub struct DummyTrieIdGenerator;
impl TrieIdGenerator<u64> for DummyTrieIdGenerator {
	fn trie_id(account_id: &u64) -> TrieId {
		use substrate_primitives::storage::well_known_keys;

		let new_seed = super::AccountCounter::mutate(|v| {
			*v = v.wrapping_add(1);
			*v
		});

		// TODO: see https://github.com/paritytech/substrate/issues/2325
		let mut res = vec![];
		res.extend_from_slice(well_known_keys::CHILD_STORAGE_KEY_PREFIX);
		res.extend_from_slice(b"default:");
		res.extend_from_slice(&new_seed.to_le_bytes());
		res.extend_from_slice(&account_id.to_le_bytes());
		res
	}
}

pub struct DummyComputeDispatchFee;
impl ComputeDispatchFee<Call, u64> for DummyComputeDispatchFee {
	fn compute_dispatch_fee(call: &Call) -> u64 {
		69
	}
}

const ALICE: u64 = 1;
const BOB: u64 = 2;
const CHARLIE: u64 = 3;
const DJANGO: u64 = 4;

pub struct ExtBuilder {
	existential_deposit: u64,
	gas_price: u64,
	block_gas_limit: u64,
	transfer_fee: u64,
	creation_fee: u64,
}
impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			existential_deposit: 0,
			gas_price: 2,
			block_gas_limit: 100_000_000,
			transfer_fee: 0,
			creation_fee: 0,
		}
	}
}
impl ExtBuilder {
	pub fn existential_deposit(mut self, existential_deposit: u64) -> Self {
		self.existential_deposit = existential_deposit;
		self
	}
	pub fn gas_price(mut self, gas_price: u64) -> Self {
		self.gas_price = gas_price;
		self
	}
	pub fn block_gas_limit(mut self, block_gas_limit: u64) -> Self {
		self.block_gas_limit = block_gas_limit;
		self
	}
	pub fn transfer_fee(mut self, transfer_fee: u64) -> Self {
		self.transfer_fee = transfer_fee;
		self
	}
	pub fn creation_fee(mut self, creation_fee: u64) -> Self {
		self.creation_fee = creation_fee;
		self
	}
	pub fn set_associated_consts(&self) {
		EXISTENTIAL_DEPOSIT.with(|v| *v.borrow_mut() = self.existential_deposit);
		TRANSFER_FEE.with(|v| *v.borrow_mut() = self.transfer_fee);
		CREATION_FEE.with(|v| *v.borrow_mut() = self.creation_fee);
		BLOCK_GAS_LIMIT.with(|v| *v.borrow_mut() = self.block_gas_limit);
	}
	pub fn build(self) -> runtime_io::TestExternalities<Blake2Hasher> {
		self.set_associated_consts();
		let mut t = system::GenesisConfig::default().build_storage::<Test>().unwrap().0;
		t.extend(
			balances::GenesisConfig::<Test> {
				balances: vec![],
				vesting: vec![],
			}
			.build_storage()
			.unwrap()
			.0,
		);
		t.extend(
			GenesisConfig::<Test> {
				current_schedule: Default::default(),
				gas_price: self.gas_price,
			}
			.build_storage()
			.unwrap()
			.0,
		);
		runtime_io::TestExternalities::new(t)
	}
}

// Perform a simple transfer to a non-existent account supplying way more gas than needed.
// Then we check that the all unused gas is refunded.
#[test]
fn refunds_unused_gas() {
	with_externalities(&mut ExtBuilder::default().gas_price(2).build(), || {
		Balances::deposit_creating(&ALICE, 100_000_000);

		assert_ok!(Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, Vec::new()));

		// 2 * 135 - gas price multiplied by the call base fee.
		assert_eq!(Balances::free_balance(&ALICE), 100_000_000 - (2 * 135));
	});
}

#[test]
fn account_removal_removes_storage() {
	with_externalities(
		&mut ExtBuilder::default().existential_deposit(100).build(),
		|| {
			let trie_id1 = <Test as Trait>::TrieIdGenerator::trie_id(&1);
			let trie_id2 = <Test as Trait>::TrieIdGenerator::trie_id(&2);
			let key1 = &[1; 32];
			let key2 = &[2; 32];

			// Set up two accounts with free balance above the existential threshold.
			{
				Balances::deposit_creating(&1, 110);
				ContractInfoOf::<Test>::insert(1, &ContractInfo::Alive(RawAliveContractInfo {
					trie_id: trie_id1.clone(),
					storage_size: <Test as Trait>::StorageSizeOffset::get(),
					deduct_block: System::block_number(),
					code_hash: H256::repeat_byte(1),
					rent_allowance: 40,
					last_write: None,
				}));

				let mut overlay = OverlayAccountDb::<Test>::new(&DirectAccountDb);
				overlay.set_storage(&1, key1.clone(), Some(b"1".to_vec()));
				overlay.set_storage(&1, key2.clone(), Some(b"2".to_vec()));
				DirectAccountDb.commit(overlay.into_change_set());

				Balances::deposit_creating(&2, 110);
				ContractInfoOf::<Test>::insert(2, &ContractInfo::Alive(RawAliveContractInfo {
					trie_id: trie_id2.clone(),
					storage_size: <Test as Trait>::StorageSizeOffset::get(),
					deduct_block: System::block_number(),
					code_hash: H256::repeat_byte(2),
					rent_allowance: 40,
					last_write: None,
				}));

				let mut overlay = OverlayAccountDb::<Test>::new(&DirectAccountDb);
				overlay.set_storage(&2, key1.clone(), Some(b"3".to_vec()));
				overlay.set_storage(&2, key2.clone(), Some(b"4".to_vec()));
				DirectAccountDb.commit(overlay.into_change_set());
			}

			// Transfer funds from account 1 of such amount that after this transfer
			// the balance of account 1 will be below the existential threshold.
			//
			// This should lead to the removal of all storage associated with this account.
			assert_ok!(Balances::transfer(Origin::signed(1), 2, 20));

			// Verify that all entries from account 1 is removed, while
			// entries from account 2 is in place.
			{
				assert!(<dyn AccountDb<Test>>::get_storage(&DirectAccountDb, &1, Some(&trie_id1), key1).is_none());
				assert!(<dyn AccountDb<Test>>::get_storage(&DirectAccountDb, &1, Some(&trie_id1), key2).is_none());

				assert_eq!(
					<dyn AccountDb<Test>>::get_storage(&DirectAccountDb, &2, Some(&trie_id2), key1),
					Some(b"3".to_vec())
				);
				assert_eq!(
					<dyn AccountDb<Test>>::get_storage(&DirectAccountDb, &2, Some(&trie_id2), key2),
					Some(b"4".to_vec())
				);
			}
		},
	);
}

const CODE_RETURN_FROM_START_FN: &str = r#"
(module
	(import "env" "ext_return" (func $ext_return (param i32 i32)))
	(import "env" "ext_deposit_event" (func $ext_deposit_event (param i32 i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(start $start)
	(func $start
		(call $ext_deposit_event
			(i32.const 0) ;; The topics buffer
			(i32.const 0) ;; The topics buffer's length
			(i32.const 8) ;; The data buffer
			(i32.const 4) ;; The data buffer's length
		)
		(call $ext_return
			(i32.const 8)
			(i32.const 4)
		)
		(unreachable)
	)

	(func (export "call")
		(unreachable)
	)
	(func (export "deploy"))

	(data (i32.const 8) "\01\02\03\04")
)
"#;
const HASH_RETURN_FROM_START_FN: [u8; 32] = hex!("66c45bd7c473a1746e1d241176166ef53b1f207f56c5e87d1b6650140704181b");

#[test]
fn instantiate_and_call_and_deposit_event() {
	let wasm = wabt::wat2wasm(CODE_RETURN_FROM_START_FN).unwrap();

	with_externalities(
		&mut ExtBuilder::default().existential_deposit(100).build(),
		|| {
			Balances::deposit_creating(&ALICE, 1_000_000);

			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, wasm));

			// Check at the end to get hash on error easily
			let creation = Contract::create(
				Origin::signed(ALICE),
				100,
				100_000,
				HASH_RETURN_FROM_START_FN.into(),
				vec![],
			);

			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(balances::RawEvent::NewAccount(1, 1_000_000)),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_RETURN_FROM_START_FN.into())),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(
						balances::RawEvent::NewAccount(BOB, 100)
					),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Transfer(ALICE, BOB, 100)),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Contract(BOB, vec![1, 2, 3, 4])),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Instantiated(ALICE, BOB)),
					topics: vec![],
				}
			]);

			assert_ok!(creation);
			assert!(ContractInfoOf::<Test>::exists(BOB));
		},
	);
}

const CODE_DISPATCH_CALL: &str = r#"
(module
	(import "env" "ext_dispatch_call" (func $ext_dispatch_call (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func (export "call")
		(call $ext_dispatch_call
			(i32.const 8) ;; Pointer to the start of encoded call buffer
			(i32.const 11) ;; Length of the buffer
		)
	)
	(func (export "deploy"))

	(data (i32.const 8) "\00\00\03\00\00\00\00\00\00\00\C8")
)
"#;
const HASH_DISPATCH_CALL: [u8; 32] = hex!("49dfdcaf9c1553be10634467e95b8e71a3bc15a4f8bf5563c0312b0902e0afb9");

#[test]
fn dispatch_call() {
	// This test can fail due to the encoding changes. In case it becomes too annoying
	// let's rewrite so as we use this module controlled call or we serialize it in runtime.
	let encoded = Encode::encode(&Call::Balances(balances::Call::transfer(CHARLIE, 50)));
	assert_eq!(&encoded[..], &hex!("00000300000000000000C8")[..]);

	let wasm = wabt::wat2wasm(CODE_DISPATCH_CALL).unwrap();

	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			Balances::deposit_creating(&ALICE, 1_000_000);

			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, wasm));

			// Let's keep this assert even though it's redundant. If you ever need to update the
			// wasm source this test will fail and will show you the actual hash.
			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(balances::RawEvent::NewAccount(1, 1_000_000)),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_DISPATCH_CALL.into())),
					topics: vec![],
				},
			]);

			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				100,
				100_000,
				HASH_DISPATCH_CALL.into(),
				vec![],
			));

			assert_ok!(Contract::call(
				Origin::signed(ALICE),
				BOB, // newly created account
				0,
				100_000,
				vec![],
			));

			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(balances::RawEvent::NewAccount(1, 1_000_000)),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_DISPATCH_CALL.into())),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(
						balances::RawEvent::NewAccount(BOB, 100)
					),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Transfer(ALICE, BOB, 100)),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Instantiated(ALICE, BOB)),
					topics: vec![],
				},

				// Dispatching the call.
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(
						balances::RawEvent::NewAccount(CHARLIE, 50)
					),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(
						balances::RawEvent::Transfer(BOB, CHARLIE, 50, 0)
					),
					topics: vec![],
				},

				// Event emited as a result of dispatch.
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Dispatched(BOB, true)),
					topics: vec![],
				}
			]);
		},
	);
}

const CODE_DISPATCH_CALL_THEN_TRAP: &str = r#"
(module
	(import "env" "ext_dispatch_call" (func $ext_dispatch_call (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func (export "call")
		(call $ext_dispatch_call
			(i32.const 8) ;; Pointer to the start of encoded call buffer
			(i32.const 11) ;; Length of the buffer
		)
		(unreachable) ;; trap so that the top level transaction fails
	)
	(func (export "deploy"))

	(data (i32.const 8) "\00\00\03\00\00\00\00\00\00\00\C8")
)
"#;
const HASH_DISPATCH_CALL_THEN_TRAP: [u8; 32] = hex!("55fe5c142dfe2519ca76c7c9b9f05012bd2624b7dcc128d2ce5a7af9d2da1846");

#[test]
fn dispatch_call_not_dispatched_after_top_level_transaction_failure() {
	// This test can fail due to the encoding changes. In case it becomes too annoying
	// let's rewrite so as we use this module controlled call or we serialize it in runtime.
	let encoded = Encode::encode(&Call::Balances(balances::Call::transfer(CHARLIE, 50)));
	assert_eq!(&encoded[..], &hex!("00000300000000000000C8")[..]);

	let wasm = wabt::wat2wasm(CODE_DISPATCH_CALL_THEN_TRAP).unwrap();

	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			Balances::deposit_creating(&ALICE, 1_000_000);

			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, wasm));

			// Let's keep this assert even though it's redundant. If you ever need to update the
			// wasm source this test will fail and will show you the actual hash.
			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(balances::RawEvent::NewAccount(1, 1_000_000)),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_DISPATCH_CALL_THEN_TRAP.into())),
					topics: vec![],
				},
			]);

			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				100,
				100_000,
				HASH_DISPATCH_CALL_THEN_TRAP.into(),
				vec![],
			));

			// Call the newly created contract. The contract is expected to dispatch a call
			// and then trap.
			assert_err!(
				Contract::call(
					Origin::signed(ALICE),
					BOB, // newly created account
					0,
					100_000,
					vec![],
				),
				"during execution"
			);
			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(balances::RawEvent::NewAccount(1, 1_000_000)),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_DISPATCH_CALL_THEN_TRAP.into())),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(
						balances::RawEvent::NewAccount(BOB, 100)
					),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Transfer(ALICE, BOB, 100)),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Instantiated(ALICE, BOB)),
					topics: vec![],
				},
				// ABSENCE of events which would be caused by dispatched Balances::transfer call
			]);
		},
	);
}

const CODE_SET_RENT: &str = r#"
(module
	(import "env" "ext_dispatch_call" (func $ext_dispatch_call (param i32 i32)))
	(import "env" "ext_set_storage" (func $ext_set_storage (param i32 i32 i32 i32)))
	(import "env" "ext_set_rent_allowance" (func $ext_set_rent_allowance (param i32 i32)))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_copy" (func $ext_scratch_copy (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	;; insert a value of 4 bytes into storage
	(func $call_0
		(call $ext_set_storage
			(i32.const 1)
			(i32.const 1)
			(i32.const 0)
			(i32.const 4)
		)
	)

	;; remove the value inserted by call_1
	(func $call_1
		(call $ext_set_storage
			(i32.const 1)
			(i32.const 0)
			(i32.const 0)
			(i32.const 0)
		)
	)

	;; transfer 50 to ALICE
	(func $call_2
		(call $ext_dispatch_call
			(i32.const 68)
			(i32.const 11)
		)
	)

	;; do nothing
	(func $call_else)

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	;; Dispatch the call according to input size
	(func (export "call")
		(local $input_size i32)
		(set_local $input_size
			(call $ext_scratch_size)
		)
		(block $IF_ELSE
			(block $IF_2
				(block $IF_1
					(block $IF_0
						(br_table $IF_0 $IF_1 $IF_2 $IF_ELSE
							(get_local $input_size)
						)
						(unreachable)
					)
					(call $call_0)
					return
				)
				(call $call_1)
				return
			)
			(call $call_2)
			return
		)
		(call $call_else)
	)

	;; Set into storage a 4 bytes value
	;; Set call set_rent_allowance with input
	(func (export "deploy")
		(local $input_size i32)
		(set_local $input_size
			(call $ext_scratch_size)
		)
		(call $ext_set_storage
			(i32.const 0)
			(i32.const 1)
			(i32.const 0)
			(i32.const 4)
		)
		(call $ext_scratch_copy
			(i32.const 0)
			(i32.const 0)
			(get_local $input_size)
		)
		(call $ext_set_rent_allowance
			(i32.const 0)
			(get_local $input_size)
		)
	)

	;; Encoding of 10 in balance
	(data (i32.const 0) "\28")

	;; Encoding of call transfer 50 to CHARLIE
	(data (i32.const 68) "\00\00\03\00\00\00\00\00\00\00\C8")
)
"#;

// Use test_hash_and_code test to get the actual hash if the code changed.
const HASH_SET_RENT: [u8; 32] = hex!("69aedfb4f6c1c398e97f8a5204de0f95ad5e7dc3540960beab11a86c569fbfcf");

/// Input data for each call in set_rent code
mod call {
	pub fn set_storage_4_byte() -> Vec<u8> { vec![] }
	pub fn remove_storage_4_byte() -> Vec<u8> { vec![0] }
	pub fn transfer() -> Vec<u8> { vec![0, 0] }
	pub fn null() -> Vec<u8> { vec![0, 0, 0] }
}

/// Test correspondence of set_rent code and its hash.
/// Also test that encoded extrinsic in code correspond to the correct transfer
#[test]
fn test_set_rent_code_and_hash() {
	// This test can fail due to the encoding changes. In case it becomes too annoying
	// let's rewrite so as we use this module controlled call or we serialize it in runtime.
	let encoded = Encode::encode(&Call::Balances(balances::Call::transfer(CHARLIE, 50)));
	assert_eq!(&encoded[..], &hex!("00000300000000000000C8")[..]);

	let wasm = wabt::wat2wasm(CODE_SET_RENT).unwrap();

	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			Balances::deposit_creating(&ALICE, 1_000_000);
			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, wasm));

			// If you ever need to update the wasm source this test will fail
			// and will show you the actual hash.
			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(balances::RawEvent::NewAccount(1, 1_000_000)),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_SET_RENT.into())),
					topics: vec![],
				},
			]);
		}
	);
}

#[test]
fn storage_size() {
	let wasm = wabt::wat2wasm(CODE_SET_RENT).unwrap();

	// Storage size
	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			// Create
			Balances::deposit_creating(&ALICE, 1_000_000);
			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, wasm));
			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				30_000,
				100_000, HASH_SET_RENT.into(),
				<Test as balances::Trait>::Balance::from(1_000u32).encode() // rent allowance
			));
			let bob_contract = ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap();
			assert_eq!(bob_contract.storage_size, <Test as Trait>::StorageSizeOffset::get() + 4);

			assert_ok!(Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, call::set_storage_4_byte()));
			let bob_contract = ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap();
			assert_eq!(bob_contract.storage_size, <Test as Trait>::StorageSizeOffset::get() + 4 + 4);

			assert_ok!(Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, call::remove_storage_4_byte()));
			let bob_contract = ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap();
			assert_eq!(bob_contract.storage_size, <Test as Trait>::StorageSizeOffset::get() + 4);
		}
	);
}

#[test]
fn deduct_blocks() {
	let wasm = wabt::wat2wasm(CODE_SET_RENT).unwrap();

	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			// Create
			Balances::deposit_creating(&ALICE, 1_000_000);
			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, wasm));
			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				30_000,
				100_000, HASH_SET_RENT.into(),
				<Test as balances::Trait>::Balance::from(1_000u32).encode() // rent allowance
			));

			// Check creation
			let bob_contract = ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap();
			assert_eq!(bob_contract.rent_allowance, 1_000);

			// Advance 4 blocks
			System::initialize(&5, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());

			// Trigger rent through call
			assert_ok!(Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, call::null()));

			// Check result
			let rent = (8 + 4 - 3) // storage size = size_offset + deploy_set_storage - deposit_offset
				* 4 // rent byte price
				* 4; // blocks to rent
			let bob_contract = ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap();
			assert_eq!(bob_contract.rent_allowance, 1_000 - rent);
			assert_eq!(bob_contract.deduct_block, 5);
			assert_eq!(Balances::free_balance(BOB), 30_000 - rent);

			// Advance 7 blocks more
			System::initialize(&12, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());

			// Trigger rent through call
			assert_ok!(Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, call::null()));

			// Check result
			let rent_2 = (8 + 4 - 2) // storage size = size_offset + deploy_set_storage - deposit_offset
				* 4 // rent byte price
				* 7; // blocks to rent
			let bob_contract = ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap();
			assert_eq!(bob_contract.rent_allowance, 1_000 - rent - rent_2);
			assert_eq!(bob_contract.deduct_block, 12);
			assert_eq!(Balances::free_balance(BOB), 30_000 - rent - rent_2);

			// Second call on same block should have no effect on rent
			assert_ok!(Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, call::null()));

			let bob_contract = ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap();
			assert_eq!(bob_contract.rent_allowance, 1_000 - rent - rent_2);
			assert_eq!(bob_contract.deduct_block, 12);
			assert_eq!(Balances::free_balance(BOB), 30_000 - rent - rent_2);
		}
	);
}

#[test]
fn call_contract_removals() {
	removals(|| {
		// Call on already-removed account might fail, and this is fine.
		Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, call::null());
		true
	});
}

#[test]
fn inherent_claim_surcharge_contract_removals() {
	removals(|| Contract::claim_surcharge(Origin::NONE, BOB, Some(ALICE)).is_ok());
}

#[test]
fn signed_claim_surcharge_contract_removals() {
	removals(|| Contract::claim_surcharge(Origin::signed(ALICE), BOB, None).is_ok());
}

#[test]
fn claim_surcharge_malus() {
	// Test surcharge malus for inherent
	claim_surcharge(4, || Contract::claim_surcharge(Origin::NONE, BOB, Some(ALICE)).is_ok(), true);
	claim_surcharge(3, || Contract::claim_surcharge(Origin::NONE, BOB, Some(ALICE)).is_ok(), true);
	claim_surcharge(2, || Contract::claim_surcharge(Origin::NONE, BOB, Some(ALICE)).is_ok(), true);
	claim_surcharge(1, || Contract::claim_surcharge(Origin::NONE, BOB, Some(ALICE)).is_ok(), false);

	// Test surcharge malus for signed
	claim_surcharge(4, || Contract::claim_surcharge(Origin::signed(ALICE), BOB, None).is_ok(), true);
	claim_surcharge(3, || Contract::claim_surcharge(Origin::signed(ALICE), BOB, None).is_ok(), false);
	claim_surcharge(2, || Contract::claim_surcharge(Origin::signed(ALICE), BOB, None).is_ok(), false);
	claim_surcharge(1, || Contract::claim_surcharge(Origin::signed(ALICE), BOB, None).is_ok(), false);
}

/// Claim surcharge with the given trigger_call at the given blocks.
/// if removes is true then assert that the contract is a tombstonedead
fn claim_surcharge(blocks: u64, trigger_call: impl Fn() -> bool, removes: bool) {
	let wasm = wabt::wat2wasm(CODE_SET_RENT).unwrap();

	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			// Create
			Balances::deposit_creating(&ALICE, 1_000_000);
			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, wasm));
			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				100,
				100_000, HASH_SET_RENT.into(),
				<Test as balances::Trait>::Balance::from(1_000u32).encode() // rent allowance
			));

			// Advance blocks
			System::initialize(&blocks, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());

			// Trigger rent through call
			assert!(trigger_call());

			if removes {
				assert!(ContractInfoOf::<Test>::get(BOB).unwrap().get_tombstone().is_some());
			} else {
				assert!(ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().is_some());
			}
		}
	);
}

/// Test for all kind of removals for the given trigger:
/// * if balance is reached and balance > subsistence threshold
/// * if allowance is exceeded
/// * if balance is reached and balance < subsistence threshold
fn removals(trigger_call: impl Fn() -> bool) {
	let wasm = wabt::wat2wasm(CODE_SET_RENT).unwrap();

	// Balance reached and superior to subsistence threshold
	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			// Create
			Balances::deposit_creating(&ALICE, 1_000_000);
			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, wasm.clone()));
			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				100,
				100_000, HASH_SET_RENT.into(),
				<Test as balances::Trait>::Balance::from(1_000u32).encode() // rent allowance
			));

			let subsistence_threshold = 50 /*existential_deposit*/ + 16 /*tombstone_deposit*/;

			// Trigger rent must have no effect
			assert!(trigger_call());
			assert_eq!(ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap().rent_allowance, 1_000);
			assert_eq!(Balances::free_balance(&BOB), 100);

			// Advance blocks
			System::initialize(&10, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());

			// Trigger rent through call
			assert!(trigger_call());
			assert!(ContractInfoOf::<Test>::get(BOB).unwrap().get_tombstone().is_some());
			assert_eq!(Balances::free_balance(&BOB), subsistence_threshold);

			// Advance blocks
			System::initialize(&20, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());

			// Trigger rent must have no effect
			assert!(trigger_call());
			assert!(ContractInfoOf::<Test>::get(BOB).unwrap().get_tombstone().is_some());
			assert_eq!(Balances::free_balance(&BOB), subsistence_threshold);
		}
	);

	// Allowance exceeded
	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			// Create
			Balances::deposit_creating(&ALICE, 1_000_000);
			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, wasm.clone()));
			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				1_000,
				100_000, HASH_SET_RENT.into(),
				<Test as balances::Trait>::Balance::from(100u32).encode() // rent allowance
			));

			// Trigger rent must have no effect
			assert!(trigger_call());
			assert_eq!(ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap().rent_allowance, 100);
			assert_eq!(Balances::free_balance(&BOB), 1_000);

			// Advance blocks
			System::initialize(&10, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());

			// Trigger rent through call
			assert!(trigger_call());
			assert!(ContractInfoOf::<Test>::get(BOB).unwrap().get_tombstone().is_some());
			// Balance should be initial balance - initial rent_allowance
			assert_eq!(Balances::free_balance(&BOB), 900);

			// Advance blocks
			System::initialize(&20, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());

			// Trigger rent must have no effect
			assert!(trigger_call());
			assert!(ContractInfoOf::<Test>::get(BOB).unwrap().get_tombstone().is_some());
			assert_eq!(Balances::free_balance(&BOB), 900);
		}
	);

	// Balance reached and inferior to subsistence threshold
	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			// Create
			Balances::deposit_creating(&ALICE, 1_000_000);
			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, wasm.clone()));
			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				50+Balances::minimum_balance(),
				100_000, HASH_SET_RENT.into(),
				<Test as balances::Trait>::Balance::from(1_000u32).encode() // rent allowance
			));

			// Trigger rent must have no effect
			assert!(trigger_call());
			assert_eq!(ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap().rent_allowance, 1_000);
			assert_eq!(Balances::free_balance(&BOB), 50 + Balances::minimum_balance());

			// Transfer funds
			assert_ok!(Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, call::transfer()));
			assert_eq!(ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap().rent_allowance, 1_000);
			assert_eq!(Balances::free_balance(&BOB), Balances::minimum_balance());

			// Advance blocks
			System::initialize(&10, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());

			// Trigger rent through call
			assert!(trigger_call());
			assert!(ContractInfoOf::<Test>::get(BOB).is_none());
			assert_eq!(Balances::free_balance(&BOB), Balances::minimum_balance());

			// Advance blocks
			System::initialize(&20, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());

			// Trigger rent must have no effect
			assert!(trigger_call());
			assert!(ContractInfoOf::<Test>::get(BOB).is_none());
			assert_eq!(Balances::free_balance(&BOB), Balances::minimum_balance());
		}
	);
}

#[test]
fn call_removed_contract() {
	let wasm = wabt::wat2wasm(CODE_SET_RENT).unwrap();

	// Balance reached and superior to subsistence threshold
	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			// Create
			Balances::deposit_creating(&ALICE, 1_000_000);
			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, wasm.clone()));
			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				100,
				100_000, HASH_SET_RENT.into(),
				<Test as balances::Trait>::Balance::from(1_000u32).encode() // rent allowance
			));

			// Calling contract should succeed.
			assert_ok!(Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, call::null()));

			// Advance blocks
			System::initialize(&10, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());

			// Calling contract should remove contract and fail.
			assert_err!(
				Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, call::null()),
				"contract has been evicted"
			);

 			// Subsequent contract calls should also fail.
			assert_err!(
				Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, call::null()),
				"contract has been evicted"
			);
		}
	)
}

const CODE_CHECK_DEFAULT_RENT_ALLOWANCE: &str = r#"
(module
	(import "env" "ext_rent_allowance" (func $ext_rent_allowance))
	(import "env" "ext_scratch_size" (func $ext_scratch_size (result i32)))
	(import "env" "ext_scratch_copy" (func $ext_scratch_copy (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call"))

	(func (export "deploy")
		;; fill the scratch buffer with the rent allowance.
		(call $ext_rent_allowance)

		;; assert $ext_scratch_size == 8
		(call $assert
			(i32.eq
				(call $ext_scratch_size)
				(i32.const 8)
			)
		)

		;; copy contents of the scratch buffer into the contract's memory.
		(call $ext_scratch_copy
			(i32.const 8)		;; Pointer in memory to the place where to copy.
			(i32.const 0)		;; Offset from the start of the scratch buffer.
			(i32.const 8)		;; Count of bytes to copy.
		)

		;; assert that contents of the buffer is equal to <BalanceOf<T>>::max_value().
		(call $assert
			(i64.eq
				(i64.load
					(i32.const 8)
				)
				(i64.const 0xFFFFFFFFFFFFFFFF)
			)
		)
	)
)
"#;
const HASH_CHECK_DEFAULT_RENT_ALLOWANCE: [u8; 32] =
	hex!("4f9ec2b94eea522cfff10b77ef4056c631045c00978a457d283950521ecf07b6");

#[test]
fn default_rent_allowance_on_create() {
	let wasm = wabt::wat2wasm(CODE_CHECK_DEFAULT_RENT_ALLOWANCE).unwrap();

	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			// Create
			Balances::deposit_creating(&ALICE, 1_000_000);
			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, wasm));
			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				30_000,
				100_000,
				HASH_CHECK_DEFAULT_RENT_ALLOWANCE.into(),
				vec![],
			));

			// Check creation
			let bob_contract = ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap();
			assert_eq!(bob_contract.rent_allowance, <BalanceOf<Test>>::max_value());

			// Advance blocks
			System::initialize(&5, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());

			// Trigger rent through call
			assert_ok!(Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, call::null()));

			// Check contract is still alive
			let bob_contract = ContractInfoOf::<Test>::get(BOB).unwrap().get_alive();
			assert!(bob_contract.is_some())
		}
	);
}

const CODE_RESTORATION: &str = r#"
(module
	(import "env" "ext_set_storage" (func $ext_set_storage (param i32 i32 i32 i32)))
	(import "env" "ext_dispatch_call" (func $ext_dispatch_call (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func (export "call")
		(call $ext_dispatch_call
			;; Pointer to the start of the encoded call buffer
			(i32.const 200)
			;; The length of the encoded call buffer.
			;;
			;; NB: This is required to keep in sync with the values in `restoration`.
			(i32.const 115)
		)
	)
	(func (export "deploy")
		;; Data to restore
		(call $ext_set_storage
			(i32.const 0)
			(i32.const 1)
			(i32.const 0)
			(i32.const 4)
		)

		;; ACL
		(call $ext_set_storage
			(i32.const 100)
			(i32.const 1)
			(i32.const 0)
			(i32.const 4)
		)
	)

	;; Data to restore
	(data (i32.const 0) "\28")

	;; ACL
	(data (i32.const 100) "\01")

	;; Serialized version of `T::Call` that encodes a call to `restore_to` function. For more
	;; details check out the `ENCODED_CALL_LITERAL`.
	(data (i32.const 200)
		"\01\05\02\00\00\00\00\00\00\00\69\ae\df\b4\f6\c1\c3\98\e9\7f\8a\52\04\de\0f\95\ad\5e\7d\c3"
		"\54\09\60\be\ab\11\a8\6c\56\9f\bf\cf\32\00\00\00\00\00\00\00\08\01\00\00\00\00\00\00\00\00"
		"\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00\00\00\00"
		"\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00"
	)
)
"#;
const HASH_RESTORATION: [u8; 32] = hex!("02988182efba70fe605031f5c55bfa59e47f72c0a4707f22b6b74fffbf7803dc");

#[test]
fn restorations_dirty_storage_and_different_storage() {
	restoration(true, true);
}

#[test]
fn restorations_dirty_storage() {
	restoration(false, true);
}

#[test]
fn restoration_different_storage() {
	restoration(true, false);
}

#[test]
fn restoration_success() {
	restoration(false, false);
}

fn restoration(test_different_storage: bool, test_restore_to_with_dirty_storage: bool) {
	let acl_key = {
		let mut s = [0u8; 32];
		s[0] = 1;
		s
	};

	// This test can fail due to the encoding changes. In case it becomes too annoying
	// let's rewrite so as we use this module controlled call or we serialize it in runtime.
	let encoded = hex::encode(Encode::encode(&Call::Contract(super::Call::restore_to(
		BOB,
		HASH_SET_RENT.into(),
		<Test as balances::Trait>::Balance::from(50u32),
		vec![acl_key, acl_key],
	))));

	// `ENCODED_CALL_LITERAL` is encoded `T::Call` represented as a byte array. There is an exact
	// same copy of this (modulo hex notation differences) in `CODE_RESTORATION`.
	//
	// When this assert is triggered make sure that you update the literals here and in
	// `CODE_RESTORATION`. Hopefully, we switch to automatic injection of the code.
	const ENCODED_CALL_LITERAL: &str =
		"0105020000000000000069aedfb4f6c1c398e97f8a5204de0f95ad5e7dc3540960beab11a86c569fbfcf320000\
		0000000000080100000000000000000000000000000000000000000000000000000000000000010000000000000\
		0000000000000000000000000000000000000000000000000";
	assert_eq!(
		encoded,
		ENCODED_CALL_LITERAL,
		"The literal was changed and requires updating here and in `CODE_RESTORATION`",
	);
	assert_eq!(
		hex::decode(ENCODED_CALL_LITERAL).unwrap().len(),
		115,
		"The size of the literal was changed and requires updating in `CODE_RESTORATION`",
	);

	let restoration_wasm = wabt::wat2wasm(CODE_RESTORATION).unwrap();
	let set_rent_wasm = wabt::wat2wasm(CODE_SET_RENT).unwrap();

	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			Balances::deposit_creating(&ALICE, 1_000_000);
			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, restoration_wasm));
			assert_ok!(Contract::put_code(Origin::signed(ALICE), 100_000, set_rent_wasm));

			// If you ever need to update the wasm source this test will fail
			// and will show you the actual hash.
			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(balances::RawEvent::NewAccount(1, 1_000_000)),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_RESTORATION.into())),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_SET_RENT.into())),
					topics: vec![],
				},
			]);

			// Create an account with address `BOB` with code `HASH_SET_RENT`.
			// The input parameter sets the rent allowance to 0.
			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				30_000,
				100_000,
				HASH_SET_RENT.into(),
				<Test as balances::Trait>::Balance::from(0u32).encode()
			));

			// Check if `BOB` was created successfully and that the rent allowance is
			// set to 0.
			let bob_contract = ContractInfoOf::<Test>::get(BOB).unwrap().get_alive().unwrap();
			assert_eq!(bob_contract.rent_allowance, 0);

			if test_different_storage {
				assert_ok!(Contract::call(
					Origin::signed(ALICE),
					BOB, 0, 100_000,
					call::set_storage_4_byte())
				);
			}

			// Advance 4 blocks, to the 5th.
			System::initialize(&5, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());

			// Call `BOB`, which makes it pay rent. Since the rent allowance is set to 0
			// we expect that it will get removed leaving tombstone.
			assert_err!(
				Contract::call(Origin::signed(ALICE), BOB, 0, 100_000, call::null()),
				"contract has been evicted"
			);
			assert!(ContractInfoOf::<Test>::get(BOB).unwrap().get_tombstone().is_some());

			/// Create another account with the address `DJANGO` with `CODE_RESTORATION`.
			///
			/// Note that we can't use `ALICE` for creating `DJANGO` so we create yet another
			/// account `CHARLIE` and create `DJANGO` with it.
			Balances::deposit_creating(&CHARLIE, 1_000_000);
			assert_ok!(Contract::create(
				Origin::signed(CHARLIE),
				30_000,
				100_000,
				HASH_RESTORATION.into(),
				<Test as balances::Trait>::Balance::from(0u32).encode()
			));

			// Before performing a call to `DJANGO` save its original trie id.
			let django_trie_id = ContractInfoOf::<Test>::get(DJANGO).unwrap()
				.get_alive().unwrap().trie_id;

			if !test_restore_to_with_dirty_storage {
				// Advance 1 block, to the 6th.
				System::initialize(&6, &[0u8; 32].into(), &[0u8; 32].into(), &Default::default());
			}

			// Perform a call to `DJANGO`. This should either perform restoration successfully or
			// fail depending on the test parameters.
			assert_ok!(Contract::call(
				Origin::signed(ALICE),
				DJANGO,
				0,
				100_000,
				vec![],
			));

			if test_different_storage || test_restore_to_with_dirty_storage {
				// Parametrization of the test imply restoration failure. Check that `DJANGO` aka
				// restoration contract is still in place and also that `BOB` doesn't exist.
				assert!(ContractInfoOf::<Test>::get(BOB).unwrap().get_tombstone().is_some());
				let django_contract = ContractInfoOf::<Test>::get(DJANGO).unwrap()
					.get_alive().unwrap();
				assert_eq!(django_contract.storage_size, 16);
				assert_eq!(django_contract.trie_id, django_trie_id);
				assert_eq!(django_contract.deduct_block, System::block_number());
			} else {
				// Here we expect that the restoration is succeeded. Check that the restoration
				// contract `DJANGO` ceased to exist and that `BOB` returned back.
				let bob_contract = ContractInfoOf::<Test>::get(BOB).unwrap()
					.get_alive().unwrap();
				assert_eq!(bob_contract.rent_allowance, 50);
				assert_eq!(bob_contract.storage_size, 12);
				assert_eq!(bob_contract.trie_id, django_trie_id);
				assert_eq!(bob_contract.deduct_block, System::block_number());
				assert!(ContractInfoOf::<Test>::get(DJANGO).is_none());
			}
		}
	);
}
