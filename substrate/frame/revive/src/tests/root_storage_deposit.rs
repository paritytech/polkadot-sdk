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

//! Storage deposits under `Origin::Root` (paritytech/contract-issues#284).
//!
//! A scenario whose outcome does not depend on `Config::Deposit` is defined at file scope, and
//! each module below wraps it for one implementation. [`crate::deposit_payment::PGasDeposit`]
//! runs through the regular test runtime and `()` through the minimal [`native::TestNative`]
//! runtime. Where the two implementations differ, both modules spell the scenario out under the
//! same name, so a reader comparing them sees the difference in the assertions.
//! `root_write_then_unrelated_signer_clears` is the one such case. The same removal pays the
//! remover under `()` and pays them nothing under `PGasDeposit`.
//!
//! With `DepositPerByte = 1` and `DepositPerItem = 2`, one `RootStorage` slot costs
//! 64 bytes + 1 item = 66, so `write(_, 2)` costs 132 and `write(_, 50)` costs 3300.

use crate::{
	AccountInfo, AddressMapper, Code, Config, HoldReason, OriginFor, StorageDeposit,
	TransactionLimits,
	deposit_payment::Deposit,
	test_utils::{
		ALICE, BOB, CHARLIE, WEIGHT_LIMIT,
		builder::{BareCallBuilder, BareInstantiateBuilder, Contract},
	},
	tests::DepositPerByte,
};
use alloy_core::sol_types::SolCall;
use frame_support::traits::fungible::{Inspect, Mutate};
use frame_system::RawOrigin;
use pallet_revive_fixtures::{FixtureType, RootStorage, compile_module_with_type};
use sp_core::H160;
use sp_runtime::{AccountId32, DispatchError};

const TWO_SLOTS: u128 = 132;
const FIFTY_SLOTS: u128 = 3300;
const FIFTY_SLOTS_BYTES: u32 = 3200;

trait Runtime: Config<Balance = u128> + frame_system::Config<AccountId = AccountId32> {}
impl<T: Config<Balance = u128> + frame_system::Config<AccountId = AccountId32>> Runtime for T {}

/// Deposit accounting of one contract: what `ContractInfo` records versus what is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Snap {
	storage_items: u32,
	storage_bytes: u32,
	byte_deposit: u128,
	item_deposit: u128,
	base_deposit: u128,
	/// `ContractInfo::total_deposit`: base + byte + item deposit.
	recorded: u128,
	/// `T::Deposit::total_on_hold(StorageDepositReserve, contract)`.
	held: u128,
}

fn snap<T: Runtime>(addr: &H160) -> Snap {
	let info = AccountInfo::<T>::load_contract(addr).expect("contract exists");
	let account = T::AddressMapper::to_account_id(addr);
	Snap {
		storage_items: info.storage_items,
		storage_bytes: info.storage_bytes,
		byte_deposit: info.storage_byte_deposit,
		item_deposit: info.storage_item_deposit,
		base_deposit: info.storage_base_deposit(),
		recorded: info.total_deposit(),
		held: <T::Deposit as Deposit<T>>::total_on_hold(
			HoldReason::StorageDepositReserve,
			&account,
		),
	}
}

fn write(from: u64, n: u64) -> Vec<u8> {
	RootStorage::writeCall { from, n }.abi_encode()
}

fn clear(from: u64, n: u64) -> Vec<u8> {
	RootStorage::clearCall { from, n }.abi_encode()
}

fn write_then_clear(from: u64, n: u64) -> Vec<u8> {
	RootStorage::writeThenClearCall { from, n }.abi_encode()
}

fn destroy(beneficiary: H160) -> Vec<u8> {
	RootStorage::destroyCall { beneficiary: beneficiary.0.into() }.abi_encode()
}

fn root<T: Runtime>() -> OriginFor<T> {
	RawOrigin::Root.into()
}

fn signed<T: Runtime>(who: &AccountId32) -> OriginFor<T> {
	RawOrigin::Signed(who.clone()).into()
}

/// Outcome of one call.
#[derive(Debug)]
struct Step {
	/// `Ok(did_revert)` or the dispatch error.
	result: Result<bool, DispatchError>,
	/// What the meter reports as charged or refunded to the origin.
	storage_deposit: StorageDeposit<u128>,
	/// The deposit the call needed at its peak.
	max_storage_deposit: StorageDeposit<u128>,
	/// Change of the signer's free native balance (0 for Root).
	signer_delta: i128,
	snap: Snap,
}

fn call_with_limit<T: Runtime>(
	signer: Option<&AccountId32>,
	addr: H160,
	data: Vec<u8>,
	deposit_limit: u128,
) -> Step {
	let origin = signer.map_or_else(root::<T>, signed::<T>);
	let before = signer.map(|s| T::Currency::balance(s)).unwrap_or_default();
	let res = BareCallBuilder::<T>::bare_call(origin, addr)
		.data(data)
		.transaction_limits(TransactionLimits::WeightAndDeposit {
			weight_limit: WEIGHT_LIMIT,
			deposit_limit,
		})
		.build();
	let after = signer.map(|s| T::Currency::balance(s)).unwrap_or_default();
	Step {
		result: res.result.map(|r| r.did_revert()),
		storage_deposit: res.storage_deposit,
		max_storage_deposit: res.max_storage_deposit,
		signer_delta: after as i128 - before as i128,
		snap: snap::<T>(&addr),
	}
}

/// Call `addr` as `signer`, or as Root if `signer` is `None`.
fn call<T: Runtime>(signer: Option<&AccountId32>, addr: H160, data: Vec<u8>) -> Step {
	call_with_limit::<T>(signer, addr, data, crate::test_utils::deposit_limit::<T>())
}

fn instantiate<T: Runtime>(who: &AccountId32, code: Vec<u8>) -> Contract<T> {
	BareInstantiateBuilder::<T>::bare_instantiate(signed::<T>(who), Code::Upload(code))
		.build_and_unwrap_contract()
}

/// ALICE instantiates `RootStorage` and writes two slots, then Root writes fifty.
///
/// Checks that the Root write is unbacked and returns the contract and its base deposit.
fn alice_two_root_fifty<T: Runtime>(fixture_type: FixtureType) -> (Contract<T>, u128) {
	let (code, _) = compile_module_with_type("RootStorage", fixture_type).unwrap();
	T::Currency::set_balance(&ALICE, 100_000_000_000);
	T::Currency::set_balance(&BOB, 100_000_000_000);

	let contract = instantiate::<T>(&ALICE, code);
	let addr = contract.addr;
	let s0 = snap::<T>(&addr);
	let base = s0.base_deposit;
	assert_eq!(s0.held, base, "invariant holds after instantiate");

	let alice_write = call::<T>(Some(&ALICE), addr, write(1000, 2));
	assert_eq!(alice_write.result, Ok(false));
	assert_eq!(alice_write.storage_deposit, StorageDeposit::Charge(TWO_SLOTS));
	assert_eq!(alice_write.snap.held, base + TWO_SLOTS);
	assert_eq!(alice_write.snap.recorded, base + TWO_SLOTS, "signed write is paid");

	let root_write = call::<T>(None, addr, write(0, 50));
	assert_eq!(root_write.result, Ok(false));
	assert_eq!(root_write.storage_deposit, StorageDeposit::Charge(0));
	assert_eq!(root_write.max_storage_deposit, StorageDeposit::Charge(FIFTY_SLOTS));
	assert_eq!(
		root_write.snap,
		Snap {
			storage_bytes: alice_write.snap.storage_bytes + FIFTY_SLOTS_BYTES,
			storage_items: alice_write.snap.storage_items + 50,
			..alice_write.snap
		},
	);
	assert_eq!(root_write.snap.recorded, root_write.snap.held);

	(contract, base)
}

/// ALICE pays for fifty slots, then Root clears them, then CHARLIE (a third party) writes and
/// clears one of the freed slots, in separate transactions and within one transaction.
///
/// The deposit freed by Root stays held on the contract and nobody is refunded. CHARLIE gets
/// back exactly what he paid, so the stranded deposit stays held until termination.
fn signed_write_then_root_clear<T: Runtime>(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("RootStorage", fixture_type).unwrap();
	T::Currency::set_balance(&ALICE, 100_000_000_000);
	T::Currency::set_balance(&CHARLIE, 100_000_000_000);

	let Contract { addr, .. } = instantiate::<T>(&ALICE, code);
	let base = snap::<T>(&addr).base_deposit;

	let alice_write = call::<T>(Some(&ALICE), addr, write(0, 50));
	assert_eq!(alice_write.storage_deposit, StorageDeposit::Charge(FIFTY_SLOTS));
	assert_eq!(alice_write.snap.held, base + FIFTY_SLOTS);
	assert_eq!(alice_write.snap.recorded, base + FIFTY_SLOTS);

	let pallet_account = crate::Pallet::<T>::account_id();
	let pallet_before = T::Currency::balance(&pallet_account);
	let alice_before = T::Currency::balance(&ALICE);
	let root_clear = call::<T>(None, addr, clear(0, 50));
	assert_eq!(root_clear.result, Ok(false));
	assert_eq!(root_clear.storage_deposit, StorageDeposit::Charge(0));
	assert_eq!(root_clear.snap.recorded, base);
	assert_eq!(root_clear.snap.held, base + FIFTY_SLOTS);
	assert_eq!(T::Currency::balance(&pallet_account), pallet_before);
	assert_eq!(T::Currency::balance(&ALICE), alice_before);

	let charlie_write = call::<T>(Some(&CHARLIE), addr, write(0, 1));
	assert_eq!(charlie_write.storage_deposit, StorageDeposit::Charge(66));
	let charlie_clear = call::<T>(Some(&CHARLIE), addr, clear(0, 1));
	assert_eq!(charlie_clear.storage_deposit, StorageDeposit::Refund(66));
	assert_eq!(charlie_write.signer_delta + charlie_clear.signer_delta, 0);
	assert_eq!(charlie_clear.snap, root_clear.snap);

	let charlie_churn = call::<T>(Some(&CHARLIE), addr, write_then_clear(0, 50));
	assert_eq!(charlie_churn.result, Ok(false));
	assert_eq!(charlie_churn.signer_delta, 0);
	assert_eq!(charlie_churn.snap, root_clear.snap);

	// Writing the same slots again charges ALICE a second time; the first payment stays
	// held on the contract until it is terminated.
	let alice_rewrite = call::<T>(Some(&ALICE), addr, write(0, 50));
	assert_eq!(alice_rewrite.signer_delta, -(FIFTY_SLOTS as i128));
	assert_eq!(alice_rewrite.snap.recorded, base + FIFTY_SLOTS);
	assert_eq!(alice_rewrite.snap.held, base + 2 * FIFTY_SLOTS);
}

/// Root writes and clears the same slots, in two transactions and within one transaction. The
/// recorded deposit and the counters end exactly where they started.
fn root_round_trip_is_neutral<T: Runtime>(fixture_type: FixtureType) {
	let (Contract { addr, .. }, _) = alice_two_root_fifty::<T>(fixture_type);

	let root_clear = call::<T>(None, addr, clear(0, 50));
	assert_eq!(root_clear.result, Ok(false));
	let start = root_clear.snap;
	assert_eq!(start.recorded, start.base_deposit + TWO_SLOTS);
	assert_eq!(start.held, start.recorded);

	let root_write = call::<T>(None, addr, write(0, 50));
	let root_clear = call::<T>(None, addr, clear(0, 50));
	assert_eq!(root_write.snap.recorded, start.recorded);
	assert_eq!(root_clear.snap, start);

	let root_churn = call::<T>(None, addr, write_then_clear(0, 50));
	assert_eq!(root_churn.result, Ok(false));
	assert_eq!(root_churn.storage_deposit, StorageDeposit::Charge(0));
	assert_eq!(root_churn.snap, start);

	let alice_clear = call::<T>(Some(&ALICE), addr, clear(1000, 2));
	assert_eq!(alice_clear.storage_deposit, StorageDeposit::Refund(TWO_SLOTS));
	assert_eq!(alice_clear.signer_delta, TWO_SLOTS as i128);
	assert_eq!(alice_clear.snap.recorded, start.base_deposit);
	assert_eq!(alice_clear.snap.held, start.base_deposit);
}

/// Additions under Root count against the deposit limit at the list price, as they do for a
/// signed call.
fn root_limit_is_enforced<T: Runtime>(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("RootStorage", fixture_type).unwrap();
	T::Currency::set_balance(&ALICE, 100_000_000_000);
	T::Currency::set_balance(&BOB, 100_000_000_000);
	let Contract { addr, .. } = instantiate::<T>(&ALICE, code);
	let start = snap::<T>(&addr);
	let exhausted = Err(crate::Error::<T>::StorageDepositLimitExhausted.into());

	for signer in [None, Some(&BOB)] {
		let below = call_with_limit::<T>(signer, addr, write(0, 50), FIFTY_SLOTS - 1);
		assert_eq!(below.result, exhausted, "signer {signer:?}");
		assert_eq!(below.snap, start, "signer {signer:?}");

		// The nested `write` frame exceeds the limit at its end, and the caller reverts.
		let churn_below =
			call_with_limit::<T>(signer, addr, write_then_clear(0, 50), FIFTY_SLOTS - 1);
		assert_eq!(churn_below.result, Ok(true), "signer {signer:?}");
		assert_eq!(churn_below.snap, start, "signer {signer:?}");
	}

	let root_churn = call_with_limit::<T>(None, addr, write_then_clear(0, 50), FIFTY_SLOTS);
	assert_eq!(root_churn.result, Ok(false));
	assert_eq!(root_churn.max_storage_deposit, StorageDeposit::Charge(FIFTY_SLOTS));
	assert_eq!(root_churn.snap, start);

	let root_exact = call_with_limit::<T>(None, addr, write(0, 50), FIFTY_SLOTS);
	assert_eq!(root_exact.result, Ok(false));
	assert_eq!(root_exact.snap.recorded, start.recorded);
	assert_eq!(root_exact.snap.held, start.held);
}

/// Root creates a `RootStorage` child through a nested CREATE and writes two slots into it.
/// The child records no deposit, base included, and holds nothing. A later signed write is
/// paid in full and recorded.
fn root_created_child_records_zero_base<T: Runtime>(fixture_type: FixtureType) {
	use alloy_core::primitives::Address;
	use pallet_revive_fixtures::RootStorageDeployer;

	T::Currency::set_balance(&ALICE, 100_000_000_000_000);
	if fixture_type == FixtureType::Resolc {
		let (child_code, _) = compile_module_with_type("RootStorage", fixture_type).unwrap();
		crate::Pallet::<T>::upload_code(signed::<T>(&ALICE), child_code, u128::MAX).unwrap();
	}
	let (code, _) = compile_module_with_type("RootStorageDeployer", fixture_type).unwrap();
	let Contract { addr, account_id } = instantiate::<T>(&ALICE, code);
	T::Currency::set_balance(&account_id, 100_000_000_000_000);
	let deployer_before = snap::<T>(&addr);

	let result = BareCallBuilder::<T>::bare_call(root::<T>(), addr)
		.data(RootStorageDeployer::deployCall { n: 2 }.abi_encode())
		.build();
	assert_eq!(result.storage_deposit, StorageDeposit::Charge(0));
	let output = result.result.unwrap();
	assert!(!output.did_revert());
	let child: Address = RootStorageDeployer::deployCall::abi_decode_returns(&output.data).unwrap();
	let child = H160::from_slice(child.as_slice());

	let created = snap::<T>(&child);
	assert_eq!(
		created,
		Snap {
			storage_items: 2,
			storage_bytes: 128,
			byte_deposit: 0,
			item_deposit: 0,
			base_deposit: 0,
			recorded: 0,
			held: 0,
		},
	);
	assert_eq!(snap::<T>(&addr), deployer_before);

	let alice_write = call::<T>(Some(&ALICE), child, write(100, 1));
	assert_eq!(alice_write.storage_deposit, StorageDeposit::Charge(66));
	assert_eq!(alice_write.snap.recorded, 66);
	assert_eq!(alice_write.snap.held, 66);

	// Her refund is pro rata over all three slots, Root's two unbacked ones included. The
	// remainder stays recorded and held for the slots that remain.
	let alice_clear = call::<T>(Some(&ALICE), child, clear(100, 1));
	assert_eq!(alice_clear.storage_deposit, StorageDeposit::Refund(21));
	assert_eq!(alice_clear.snap.recorded, 45);
	assert_eq!(alice_clear.snap.held, 45);
}

/// Termination releases what is held, not what is recorded. After Root clears storage ALICE
/// paid for, the contract records only its base deposit while the hold still covers her
/// stranded deposit, and the terminating origin receives all of it.
///
/// A record below the hold stays recoverable only because `refund_all` reads the hold and
/// never `ContractInfo`.
fn termination_releases_the_hold_not_the_record<T: Runtime>(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("RootStorage", fixture_type).unwrap();
	T::Currency::set_balance(&ALICE, 100_000_000_000);
	T::Currency::set_balance(&BOB, 100_000_000_000);

	let Contract { addr, .. } = instantiate::<T>(&ALICE, code);
	let base = snap::<T>(&addr).base_deposit;

	let alice_write = call::<T>(Some(&ALICE), addr, write(0, 50));
	assert_eq!(alice_write.storage_deposit, StorageDeposit::Charge(FIFTY_SLOTS));

	let root_clear = call::<T>(None, addr, clear(0, 50));
	assert_eq!(root_clear.snap.recorded, base);
	assert_eq!(root_clear.snap.held, base + FIFTY_SLOTS);

	// BOB terminates and CHARLIE takes the free balance, so BOB's delta is the released
	// deposit alone. The contract is gone afterwards, so this cannot go through `call`.
	let charlie = <T as Config>::AddressMapper::to_address(&CHARLIE);
	let account = <T as Config>::AddressMapper::to_account_id(&addr);
	let before = T::Currency::balance(&BOB);
	let res = BareCallBuilder::<T>::bare_call(signed::<T>(&BOB), addr)
		.data(destroy(charlie))
		.transaction_limits(TransactionLimits::WeightAndDeposit {
			weight_limit: WEIGHT_LIMIT,
			deposit_limit: crate::test_utils::deposit_limit::<T>(),
		})
		.build();
	assert!(!res.result.expect("terminate succeeds").did_revert());

	let released = T::Currency::balance(&BOB) as i128 - before as i128;
	assert_eq!(released, (base + FIFTY_SLOTS) as i128);
	assert!(AccountInfo::<T>::load_contract(&addr).is_none(), "contract is gone");
	assert_eq!(
		<T::Deposit as Deposit<T>>::total_on_hold(HoldReason::StorageDepositReserve, &account),
		0,
	);
}

/// A Root round trip is neutral only while a pool is at or below the list price of the units it
/// covers. After the price per byte drops, ALICE's byte pool sits above that bound, so Root
/// writing and clearing its own slots halves it and strands what ALICE paid. The item price does
/// not move, so the item pool is unchanged by the same round trip.
fn root_round_trip_after_price_decrease_strands_deposit<T: Runtime>(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("RootStorage", fixture_type).unwrap();
	T::Currency::set_balance(&ALICE, 100_000_000_000);
	DepositPerByte::set(2);

	let Contract { addr, .. } = instantiate::<T>(&ALICE, code);
	let base = snap::<T>(&addr).base_deposit;
	let alice_write = call::<T>(Some(&ALICE), addr, write(0, 50));
	assert_eq!(alice_write.storage_deposit, StorageDeposit::Charge(6500));
	assert_eq!((alice_write.snap.byte_deposit, alice_write.snap.item_deposit), (6400, 100));

	DepositPerByte::set(1);
	let root_write = call::<T>(None, addr, write(100, 50));
	assert_eq!((root_write.snap.byte_deposit, root_write.snap.item_deposit), (6400, 100));

	let root_clear = call::<T>(None, addr, clear(100, 50));
	assert_eq!(root_clear.result, Ok(false));
	// Bytes: pro rata leaves 6400 - floor(6400 * 3200/6400) = 3200, and the list price of the
	// 3200 remaining bytes is only 3200, so neither branch keeps the pool at 6400.
	// Items: the item price never moved, so the clamp at 2 * 50 keeps the pool at 100.
	assert_eq!((root_clear.snap.byte_deposit, root_clear.snap.item_deposit), (3200, 100));
	assert_eq!(root_clear.snap.held, base + 6500);

	let alice_clear = call::<T>(Some(&ALICE), addr, clear(0, 50));
	assert_eq!(alice_clear.storage_deposit, StorageDeposit::Refund(3300));
	assert_eq!(alice_clear.signer_delta, 3300);
	assert_eq!(alice_clear.snap.recorded, base);
	assert_eq!(alice_clear.snap.held, base + 3200);
}

/// After the price per byte drops, a Root removal shrinks the paid pool only by its pro rata
/// share. Clamping to the new list price would strand deposit ALICE paid.
///
/// Returns the contract and the deposit still held on it, so the `pgas` wrapper can check
/// ALICE's entitlement against that amount.
fn root_clear_after_price_decrease_stays_pro_rata<T: Runtime>(
	fixture_type: FixtureType,
) -> (Contract<T>, u128) {
	let (code, _) = compile_module_with_type("RootStorage", fixture_type).unwrap();
	T::Currency::set_balance(&ALICE, 100_000_000_000);
	DepositPerByte::set(2);

	let contract = instantiate::<T>(&ALICE, code);
	let addr = contract.addr;
	let base = snap::<T>(&addr).base_deposit;
	let alice_write = call::<T>(Some(&ALICE), addr, write(0, 50));
	assert_eq!(alice_write.storage_deposit, StorageDeposit::Charge(6500));
	assert_eq!((alice_write.snap.byte_deposit, alice_write.snap.item_deposit), (6400, 100));

	DepositPerByte::set(1);
	let root_clear = call::<T>(None, addr, clear(0, 1));
	assert_eq!(root_clear.result, Ok(false));
	// Pro rata: 6400 - floor(6400 * 64/3200) = 6272 and 100 - floor(100 * 1/50) = 98.
	// The byte list price of the remainder would be only 3136.
	assert_eq!((root_clear.snap.byte_deposit, root_clear.snap.item_deposit), (6272, 98));
	assert_eq!(root_clear.snap.held, base + 6500);

	let alice_clear = call::<T>(Some(&ALICE), addr, clear(1, 49));
	assert_eq!(alice_clear.storage_deposit, StorageDeposit::Refund(6370));
	assert_eq!(alice_clear.signer_delta, 6370);
	assert_eq!(alice_clear.snap.recorded, base);
	assert_eq!(alice_clear.snap.held, base + 130);

	(contract, alice_clear.snap.held)
}

/// Root terminates a contract whose record is below its hold. Root has no account, so
/// `termination_origin` pays the released deposit to the pallet account. That payment includes
/// the deposit ALICE paid and never got back.
fn root_termination_pays_the_pallet_account<T: Runtime>(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("RootStorage", fixture_type).unwrap();
	T::Currency::set_balance(&ALICE, 100_000_000_000);

	let Contract { addr, .. } = instantiate::<T>(&ALICE, code);
	let base = snap::<T>(&addr).base_deposit;

	let alice_write = call::<T>(Some(&ALICE), addr, write(0, 50));
	assert_eq!(alice_write.storage_deposit, StorageDeposit::Charge(FIFTY_SLOTS));

	// Root clears what ALICE paid for. The record drops to the base and the hold does not move.
	let root_clear = call::<T>(None, addr, clear(0, 50));
	assert_eq!(root_clear.storage_deposit, StorageDeposit::Charge(0));
	assert_eq!(root_clear.snap.recorded, base);
	assert_eq!(root_clear.snap.held, base + FIFTY_SLOTS);

	// CHARLIE takes the free balance, so the pallet account's delta is the released deposit
	// alone. The contract is gone afterwards, so this cannot go through `call`.
	let charlie = <T as Config>::AddressMapper::to_address(&CHARLIE);
	let account = <T as Config>::AddressMapper::to_account_id(&addr);
	let pallet_account = crate::Pallet::<T>::account_id();
	let before = T::Currency::balance(&pallet_account);
	let res = BareCallBuilder::<T>::bare_call(root::<T>(), addr)
		.data(destroy(charlie))
		.transaction_limits(TransactionLimits::WeightAndDeposit {
			weight_limit: WEIGHT_LIMIT,
			deposit_limit: crate::test_utils::deposit_limit::<T>(),
		})
		.build();
	assert!(!res.result.expect("terminate succeeds").did_revert());
	assert_eq!(res.storage_deposit, StorageDeposit::Charge(0));

	let released = T::Currency::balance(&pallet_account) as i128 - before as i128;
	assert_eq!(released, (base + FIFTY_SLOTS) as i128);
	assert!(AccountInfo::<T>::load_contract(&addr).is_none(), "contract is gone");
	assert_eq!(
		<T::Deposit as Deposit<T>>::total_on_hold(HoldReason::StorageDepositReserve, &account),
		0,
	);
}

mod pgas {
	use super::*;
	use crate::{
		NativeDepositOf,
		tests::{ExtBuilder, Test},
	};
	use test_case::test_case;

	fn native_entitlement(contract: &AccountId32, who: &AccountId32) -> u128 {
		NativeDepositOf::<Test>::get(contract, who)
	}

	/// A signer without any native entitlement (BOB) clears Root's storage, then ALICE
	/// clears her own.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_write_then_unrelated_signer_clears(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let (Contract { addr, account_id }, base) = alice_two_root_fifty::<Test>(fixture_type);

			let bob_clear = call::<Test>(Some(&BOB), addr, clear(0, 50));
			assert_eq!(bob_clear.result, Ok(false));
			// Pro rata over the paid pool: floor(128 * 3200/3328) + floor(4 * 50/52) = 123 + 3.
			assert_eq!(bob_clear.storage_deposit, StorageDeposit::Refund(126));
			// BOB has no entitlement and the contract holds no PGAS, so nothing is paid out.
			assert_eq!(bob_clear.signer_delta, 0);
			assert_eq!(bob_clear.snap.held, base + TWO_SLOTS);
			assert_eq!((bob_clear.snap.byte_deposit, bob_clear.snap.item_deposit), (5, 1));
			assert_eq!(bob_clear.snap.recorded, base + 6);
			assert_eq!(native_entitlement(&account_id, &BOB), 0);
			assert_eq!(native_entitlement(&account_id, &ALICE), base + TWO_SLOTS);

			let alice_clear = call::<Test>(Some(&ALICE), addr, clear(1000, 2));
			assert_eq!(alice_clear.result, Ok(false));
			assert_eq!(alice_clear.storage_deposit, StorageDeposit::Refund(6));
			assert_eq!(alice_clear.signer_delta, 6);
			assert_eq!(alice_clear.snap.recorded, base);
			assert_eq!(alice_clear.snap.held, base + TWO_SLOTS - 6);
			assert_eq!(native_entitlement(&account_id, &ALICE), base + TWO_SLOTS - 6);
		});
	}

	/// A signer with a native entitlement (ALICE) clears Root's storage. Her refund is limited
	/// by the paid pool, so the base deposit stays held.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_write_then_entitled_signer_clears(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let (Contract { addr, account_id }, base) = alice_two_root_fifty::<Test>(fixture_type);

			let alice_clear = call::<Test>(Some(&ALICE), addr, clear(0, 50));
			assert_eq!(alice_clear.result, Ok(false));
			assert_eq!(alice_clear.storage_deposit, StorageDeposit::Refund(126));
			assert_eq!(alice_clear.signer_delta, 126);
			assert_eq!(alice_clear.snap.recorded, base + 6);
			assert_eq!(alice_clear.snap.held, base + 6);
			assert_eq!(native_entitlement(&account_id, &ALICE), base + 6);

			let alice_clear = call::<Test>(Some(&ALICE), addr, clear(1000, 2));
			assert_eq!(alice_clear.storage_deposit, StorageDeposit::Refund(6));
			assert_eq!(alice_clear.snap.recorded, base);
			assert_eq!(alice_clear.snap.held, base);
			assert_eq!(native_entitlement(&account_id, &ALICE), base);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn signed_write_then_root_clear(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			super::signed_write_then_root_clear::<Test>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_round_trip_is_neutral(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			super::root_round_trip_is_neutral::<Test>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_limit_is_enforced(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			super::root_limit_is_enforced::<Test>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn termination_releases_the_hold_not_the_record(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			super::termination_releases_the_hold_not_the_record::<Test>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_termination_pays_the_pallet_account(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			super::root_termination_pays_the_pallet_account::<Test>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_created_child_records_zero_base(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			super::root_created_child_records_zero_base::<Test>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_round_trip_after_price_decrease_strands_deposit(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			super::root_round_trip_after_price_decrease_strands_deposit::<Test>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_clear_after_price_decrease_stays_pro_rata(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let (contract, held) =
				super::root_clear_after_price_decrease_stays_pro_rata::<Test>(fixture_type);
			// ALICE funded the whole hold in native, so her entitlement equals it.
			assert_eq!(native_entitlement(&contract.account_id, &ALICE), held);
		});
	}
}

/// A minimal runtime that uses the default `()` storage-deposit backend.
mod native {
	use super::*;
	use crate::{self as pallet_revive, AccountId32Mapper};
	use frame_support::{derive_impl, traits::ConstU128};
	use sp_runtime::{BuildStorage, traits::IdentityLookup};
	use test_case::test_case;

	frame_support::construct_runtime!(
		pub enum TestNative
		{
			System: frame_system,
			Balances: pallet_balances,
			Timestamp: pallet_timestamp,
			Contracts: pallet_revive,
		}
	);

	type Block = frame_system::mocking::MockBlock<TestNative>;

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for TestNative {
		type Block = Block;
		type AccountId = AccountId32;
		type Lookup = IdentityLookup<Self::AccountId>;
		type AccountData = pallet_balances::AccountData<u128>;
	}

	#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
	impl pallet_balances::Config for TestNative {
		type Balance = u128;
		type ExistentialDeposit = ConstU128<1>;
		type AccountStore = System;
		type RuntimeHoldReason = RuntimeHoldReason;
		type RuntimeFreezeReason = RuntimeFreezeReason;
	}

	#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
	impl pallet_timestamp::Config for TestNative {}

	#[derive_impl(crate::config_preludes::TestDefaultConfig)]
	impl Config for TestNative {
		type Time = Timestamp;
		type AddressMapper = AccountId32Mapper<Self>;
		type Balance = u128;
		type Currency = Balances;
		// Same prices as the main test runtime.
		type DepositPerByte = crate::tests::DepositPerByte;
		type DepositPerItem = crate::tests::DepositPerItem;
		type DepositPerChildTrieItem = crate::tests::DepositPerItem;
		type CodeHashLockupDepositPercent = crate::tests::CodeHashLockupDepositPercent;
		type UploadOrigin = frame_system::EnsureSigned<AccountId32>;
		type InstantiateOrigin = frame_system::EnsureSigned<AccountId32>;
	}

	fn new_test_ext() -> sp_io::TestExternalities {
		sp_tracing::try_init_simple();
		let t = RuntimeGenesisConfig::default().build_storage().unwrap();
		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}

	/// ALICE writes two slots, Root writes fifty, BOB clears Root's fifty, then ALICE clears
	/// her two. Every refund is covered by the hold, which ends at the base deposit.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_write_then_unrelated_signer_clears(fixture_type: FixtureType) {
		new_test_ext().execute_with(|| {
			let (Contract { addr, .. }, base) = alice_two_root_fifty::<TestNative>(fixture_type);

			// Pro rata over the paid pool: floor(128 * 3200/3328) + floor(4 * 50/52) = 123 + 3.
			// Under `()` BOB is paid this out of the deposit ALICE put down.
			let bob_clear = call::<TestNative>(Some(&BOB), addr, clear(0, 50));
			assert_eq!(bob_clear.result, Ok(false));
			assert_eq!(bob_clear.storage_deposit, StorageDeposit::Refund(126));
			assert_eq!(bob_clear.signer_delta, 126);
			assert_eq!(bob_clear.snap.recorded, base + 6);
			assert_eq!(bob_clear.snap.held, base + 6);

			let alice_clear = call::<TestNative>(Some(&ALICE), addr, clear(1000, 2));
			assert_eq!(alice_clear.storage_deposit, StorageDeposit::Refund(6));
			assert_eq!(alice_clear.signer_delta, 6);
			assert_eq!(alice_clear.snap.recorded, base);
			assert_eq!(alice_clear.snap.held, base);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn signed_write_then_root_clear(fixture_type: FixtureType) {
		new_test_ext().execute_with(|| {
			super::signed_write_then_root_clear::<TestNative>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_round_trip_is_neutral(fixture_type: FixtureType) {
		new_test_ext().execute_with(|| {
			super::root_round_trip_is_neutral::<TestNative>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_limit_is_enforced(fixture_type: FixtureType) {
		new_test_ext().execute_with(|| {
			super::root_limit_is_enforced::<TestNative>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_created_child_records_zero_base(fixture_type: FixtureType) {
		new_test_ext().execute_with(|| {
			super::root_created_child_records_zero_base::<TestNative>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn termination_releases_the_hold_not_the_record(fixture_type: FixtureType) {
		new_test_ext().execute_with(|| {
			super::termination_releases_the_hold_not_the_record::<TestNative>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_termination_pays_the_pallet_account(fixture_type: FixtureType) {
		new_test_ext().execute_with(|| {
			super::root_termination_pays_the_pallet_account::<TestNative>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_clear_after_price_decrease_stays_pro_rata(fixture_type: FixtureType) {
		new_test_ext().execute_with(|| {
			super::root_clear_after_price_decrease_stays_pro_rata::<TestNative>(fixture_type);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn root_round_trip_after_price_decrease_strands_deposit(fixture_type: FixtureType) {
		new_test_ext().execute_with(|| {
			super::root_round_trip_after_price_decrease_strands_deposit::<TestNative>(fixture_type);
		});
	}
}
