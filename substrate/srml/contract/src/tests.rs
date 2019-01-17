// Copyright 2018 Parity Technologies (UK) Ltd.
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

use runtime_io::with_externalities;
use runtime_primitives::testing::{Digest, DigestItem, H256, Header};
use runtime_primitives::traits::{BlakeTwo256, IdentityLookup};
use runtime_primitives::BuildStorage;
use runtime_support::{StorageMap, StorageDoubleMap};
use substrate_primitives::{Blake2Hasher};
use system::{Phase, EventRecord};
use wabt;
use {
	balances, runtime_io, system, ContractAddressFor, GenesisConfig, Module, RawEvent, StorageOf,
	Trait,
};

impl_outer_origin! {
	pub enum Origin for Test {}
}

mod contract {
	// Re-export contents of the root. This basically
	// needs to give a name for the current crate.
	// This hack is required for `impl_outer_event!`.
	pub use super::super::*;
}
impl_outer_event! {
	pub enum MetaEvent for Test {
		balances<T>, contract<T>,
	}
}

#[derive(Clone, Eq, PartialEq)]
pub struct Test;
impl system::Trait for Test {
	type Origin = Origin;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type Digest = Digest;
	type AccountId = u64;
	type Lookup = IdentityLookup<u64>;
	type Header = Header;
	type Event = MetaEvent;
	type Log = DigestItem;
}
impl balances::Trait for Test {
	type Balance = u64;
	type OnFreeBalanceZero = Contract;
	type OnNewAccount = ();
	type EnsureAccountLiquid = ();
	type Event = MetaEvent;
}
impl Trait for Test {
	type Gas = u64;
	type DetermineContractAddress = DummyContractAddressFor;
	type Event = MetaEvent;
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

const ALICE: u64 = 1;
const BOB: u64 = 2;
const CHARLIE: u64 = 3;

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
	pub fn build(self) -> runtime_io::TestExternalities<Blake2Hasher> {
		let mut t = system::GenesisConfig::<Test>::default()
			.build_storage()
			.unwrap()
			.0;
		t.extend(
			balances::GenesisConfig::<Test> {
				balances: vec![],
				transaction_base_fee: 0,
				transaction_byte_fee: 0,
				existential_deposit: self.existential_deposit,
				transfer_fee: self.transfer_fee,
				creation_fee: self.creation_fee,
			}
			.build_storage()
			.unwrap()
			.0,
		);
		t.extend(
			GenesisConfig::<Test> {
				contract_fee: 21,
				call_base_fee: 135,
				create_base_fee: 175,
				gas_price: self.gas_price,
				max_depth: 100,
				block_gas_limit: self.block_gas_limit,
				current_schedule: Default::default(),
			}
			.build_storage()
			.unwrap()
			.0,
		);
		runtime_io::TestExternalities::new(t)
	}
}

#[test]
fn refunds_unused_gas() {
	with_externalities(&mut ExtBuilder::default().build(), || {
		Balances::set_free_balance(&0, 100_000_000);
		Balances::increase_total_stake_by(100_000_000);

		assert_ok!(Contract::call(
			Origin::signed(0),
			1,
			0.into(),
			100_000.into(),
			Vec::new()
		));

		assert_eq!(Balances::free_balance(&0), 100_000_000 - (2 * 135));
	});
}

#[test]
fn account_removal_removes_storage() {
	with_externalities(
		&mut ExtBuilder::default().existential_deposit(100).build(),
		|| {
			// Setup two accounts with free balance above than exsistential threshold.
			{
				Balances::set_free_balance(&1, 110);
				Balances::increase_total_stake_by(110);
				<StorageOf<Test>>::insert(1, b"foo".to_vec(), b"1".to_vec());
				<StorageOf<Test>>::insert(1, b"bar".to_vec(), b"2".to_vec());

				Balances::set_free_balance(&2, 110);
				Balances::increase_total_stake_by(110);
				<StorageOf<Test>>::insert(2, b"hello".to_vec(), b"3".to_vec());
				<StorageOf<Test>>::insert(2, b"world".to_vec(), b"4".to_vec());
			}

			// Transfer funds from account 1 of such amount that after this transfer
			// the balance of account 1 is will be below than exsistential threshold.
			//
			// This should lead to the removal of all storage associated with this account.
			assert_ok!(Balances::transfer(Origin::signed(1), 2, 20.into()));

			// Verify that all entries from account 1 is removed, while
			// entries from account 2 is in place.
			{
				assert_eq!(<StorageOf<Test>>::get(1, b"foo".to_vec()), None);
				assert_eq!(<StorageOf<Test>>::get(1, b"bar".to_vec()), None);

				assert_eq!(
					<StorageOf<Test>>::get(2, b"hello".to_vec()),
					Some(b"3".to_vec())
				);
				assert_eq!(
					<StorageOf<Test>>::get(2, b"world".to_vec()),
					Some(b"4".to_vec())
				);
			}
		},
	);
}

const CODE_RETURN_FROM_START_FN: &str = r#"
(module
	(import "env" "ext_return" (func $ext_return (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	(start $start)
	(func $start
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
const HASH_RETURN_FROM_START_FN: [u8; 32] = hex!("e6411d12daa2a19e4e9c7d8306c31c7d53a352cb8ed84385c8a1d48fc232e708");

#[test]
fn instantiate_and_call() {
	let wasm = wabt::wat2wasm(CODE_RETURN_FROM_START_FN).unwrap();

	with_externalities(
		&mut ExtBuilder::default().existential_deposit(100).build(),
		|| {
			Balances::set_free_balance(&ALICE, 1_000_000);
			Balances::increase_total_stake_by(1_000_000);

			assert_ok!(Contract::put_code(
				Origin::signed(ALICE),
				100_000.into(),
				wasm,
			));

			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				100.into(),
				100_000.into(),
				HASH_RETURN_FROM_START_FN.into(),
				vec![],
			));

			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_RETURN_FROM_START_FN.into())),
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(
						balances::RawEvent::NewAccount(BOB, 100)
					)
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Transfer(ALICE, BOB, 100))
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Instantiated(ALICE, BOB))
				}
			]);
		},
	);
}
