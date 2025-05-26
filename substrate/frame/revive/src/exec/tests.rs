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

/// These tests exercise the executive layer.
///
/// In these tests the VM/loader are mocked. Instead of dealing with wasm bytecode they use
/// simple closures. This allows you to tackle executive logic more thoroughly without writing
/// a wasm VM code.
#[cfg(test)]
use super::*;
use crate::{
	exec::ExportedFunction::*,
	gas::GasMeter,
	test_utils::*,
	tests::{
		test_utils::{get_balance, place_contract, set_balance},
		ExtBuilder, RuntimeCall, RuntimeEvent as MetaEvent, Test, TestFilter,
	},
	AddressMapper, Error,
};
use assert_matches::assert_matches;
use frame_support::{assert_err, assert_ok, parameter_types};
use frame_system::{AccountInfo, EventRecord, Phase};
use pallet_revive_uapi::ReturnFlags;
use pretty_assertions::assert_eq;
use sp_io::hashing::keccak_256;
use sp_runtime::{traits::Hash, DispatchError};
use std::{cell::RefCell, collections::hash_map::HashMap, rc::Rc};

type System = frame_system::Pallet<Test>;

type MockStack<'a> = Stack<'a, Test, MockExecutable>;

parameter_types! {
	static Loader: MockLoader = MockLoader::default();
}

fn events() -> Vec<Event<Test>> {
	System::events()
		.into_iter()
		.filter_map(|meta| match meta.event {
			MetaEvent::Contracts(contract_event) => Some(contract_event),
			_ => None,
		})
		.collect()
}

struct MockCtx<'a> {
	ext: &'a mut MockStack<'a>,
	input_data: Vec<u8>,
}

#[derive(Clone)]
struct MockExecutable {
	func: Rc<dyn for<'a> Fn(MockCtx<'a>, &Self) -> ExecResult + 'static>,
	constructor: Rc<dyn for<'a> Fn(MockCtx<'a>, &Self) -> ExecResult + 'static>,
	code_hash: H256,
	code_info: CodeInfo<Test>,
}

#[derive(Default, Clone)]
pub struct MockLoader {
	map: HashMap<H256, MockExecutable>,
	counter: u64,
}

impl MockLoader {
	fn code_hashes() -> Vec<H256> {
		Loader::get().map.keys().copied().collect()
	}

	fn insert(
		func_type: ExportedFunction,
		f: impl Fn(MockCtx, &MockExecutable) -> ExecResult + 'static,
	) -> H256 {
		Loader::mutate(|loader| {
			// Generate code hashes from contract index value.
			let hash = H256(keccak_256(&loader.counter.to_le_bytes()));
			loader.counter += 1;
			if func_type == ExportedFunction::Constructor {
				loader.map.insert(
					hash,
					MockExecutable {
						func: Rc::new(|_, _| exec_success()),
						constructor: Rc::new(f),
						code_hash: hash,
						code_info: CodeInfo::<Test>::new(ALICE),
					},
				);
			} else {
				loader.map.insert(
					hash,
					MockExecutable {
						func: Rc::new(f),
						constructor: Rc::new(|_, _| exec_success()),
						code_hash: hash,
						code_info: CodeInfo::<Test>::new(ALICE),
					},
				);
			}
			hash
		})
	}

	fn insert_both(
		constructor: impl Fn(MockCtx, &MockExecutable) -> ExecResult + 'static,
		call: impl Fn(MockCtx, &MockExecutable) -> ExecResult + 'static,
	) -> H256 {
		Loader::mutate(|loader| {
			// Generate code hashes from contract index value.
			let hash = H256(keccak_256(&loader.counter.to_le_bytes()));
			loader.counter += 1;
			loader.map.insert(
				hash,
				MockExecutable {
					func: Rc::new(call),
					constructor: Rc::new(constructor),
					code_hash: hash,
					code_info: CodeInfo::<Test>::new(ALICE),
				},
			);
			hash
		})
	}
}

impl Executable<Test> for MockExecutable {
	fn from_storage(
		code_hash: H256,
		_gas_meter: &mut GasMeter<Test>,
	) -> Result<Self, DispatchError> {
		Loader::mutate(|loader| {
			loader.map.get(&code_hash).cloned().ok_or(Error::<Test>::CodeNotFound.into())
		})
	}

	fn execute<E: Ext<T = Test>>(
		self,
		ext: &mut E,
		function: ExportedFunction,
		input_data: Vec<u8>,
	) -> ExecResult {
		// # Safety
		//
		// We know that we **always** call execute with a `MockStack` in this test.
		//
		// # Note
		//
		// The transmute is necessary because `execute` has to be generic over all
		// `E: Ext`. However, `MockExecutable` can't be generic over `E` as it would
		// constitute a cycle.
		let ext = unsafe { mem::transmute(ext) };
		if function == ExportedFunction::Constructor {
			(self.constructor)(MockCtx { ext, input_data }, &self)
		} else {
			(self.func)(MockCtx { ext, input_data }, &self)
		}
	}

	fn code(&self) -> &[u8] {
		// The mock executable doesn't have code", so we return the code hash.
		self.code_hash.as_ref()
	}

	fn code_hash(&self) -> &H256 {
		&self.code_hash
	}

	fn code_info(&self) -> &CodeInfo<Test> {
		&self.code_info
	}
}

fn exec_success() -> ExecResult {
	Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() })
}

fn exec_trapped() -> ExecResult {
	Err(ExecError { error: <Error<Test>>::ContractTrapped.into(), origin: ErrorOrigin::Callee })
}

#[test]
fn it_works() {
	parameter_types! {
		static TestData: Vec<usize> = vec![0];
	}

	let value = Default::default();
	let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
	let exec_ch = MockLoader::insert(Call, |_ctx, _executable| {
		TestData::mutate(|data| data.push(1));
		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, exec_ch);
		let mut storage_meter =
			storage::meter::Meter::new(&Origin::from_account_id(ALICE), 0, value).unwrap();

		assert_matches!(
			MockStack::run_call(
				Origin::from_account_id(ALICE),
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				value.into(),
				vec![],
				false,
			),
			Ok(_)
		);
	});

	assert_eq!(TestData::get(), vec![0, 1]);
}

#[test]
fn transfer_works() {
	// This test verifies that a contract is able to transfer
	// some funds to another account.
	ExtBuilder::default().build().execute_with(|| {
		set_balance(&ALICE, 100);
		set_balance(&BOB, 0);

		let origin = Origin::from_account_id(ALICE);
		MockStack::transfer(&origin, &ALICE, &BOB, 55u64.into()).unwrap();

		let min_balance = <Test as Config>::Currency::minimum_balance();
		assert_eq!(get_balance(&ALICE), 45 - min_balance);
		assert_eq!(get_balance(&BOB), 55 + min_balance);
	});
}

#[test]
fn transfer_to_nonexistent_account_works() {
	// This test verifies that a contract is able to transfer
	// some funds to a nonexistant account and that those transfers
	// are not able to reap accounts.
	ExtBuilder::default().build().execute_with(|| {
		let ed = <Test as Config>::Currency::minimum_balance();
		let value = 1024;

		// Transfers to nonexistant accounts should work
		set_balance(&ALICE, ed * 2);
		set_balance(&BOB, ed + value);

		assert_ok!(MockStack::transfer(
			&Origin::from_account_id(ALICE),
			&BOB,
			&CHARLIE,
			value.into()
		));
		assert_eq!(get_balance(&ALICE), ed);
		assert_eq!(get_balance(&BOB), ed);
		assert_eq!(get_balance(&CHARLIE), ed + value);

		// Do not reap the origin account
		set_balance(&ALICE, ed);
		set_balance(&BOB, ed + value);
		assert_err!(
			MockStack::transfer(&Origin::from_account_id(ALICE), &BOB, &DJANGO, value.into()),
			<Error<Test>>::TransferFailed
		);

		// Do not reap the sender account
		set_balance(&ALICE, ed * 2);
		set_balance(&BOB, value);
		assert_err!(
			MockStack::transfer(&Origin::from_account_id(ALICE), &BOB, &EVE, value.into()),
			<Error<Test>>::TransferFailed
		);
		// The ED transfer would work. But it should only be executed with the actual transfer
		assert!(!System::account_exists(&EVE));
	});
}

#[test]
fn correct_transfer_on_call() {
	let value = 55;

	let success_ch = MockLoader::insert(Call, move |ctx, _| {
		assert_eq!(ctx.ext.value_transferred(), U256::from(value));
		Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() })
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, success_ch);
		set_balance(&ALICE, 100);
		let balance = get_balance(&BOB_FALLBACK);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, value).unwrap();

		let _ = MockStack::run_call(
			origin.clone(),
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			value.into(),
			vec![],
			false,
		)
		.unwrap();

		assert_eq!(get_balance(&ALICE), 100 - value);
		assert_eq!(get_balance(&BOB_FALLBACK), balance + value);
	});
}

#[test]
fn correct_transfer_on_delegate_call() {
	let value = 35;

	let success_ch = MockLoader::insert(Call, move |ctx, _| {
		assert_eq!(ctx.ext.value_transferred(), U256::from(value));
		Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() })
	});

	let delegate_ch = MockLoader::insert(Call, move |ctx, _| {
		assert_eq!(ctx.ext.value_transferred(), U256::from(value));
		ctx.ext.delegate_call(Weight::zero(), U256::zero(), CHARLIE_ADDR, Vec::new())?;
		Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() })
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, delegate_ch);
		place_contract(&CHARLIE, success_ch);
		set_balance(&ALICE, 100);
		let balance = get_balance(&BOB_FALLBACK);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 55).unwrap();

		assert_ok!(MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			value.into(),
			vec![],
			false,
		));

		assert_eq!(get_balance(&ALICE), 100 - value);
		assert_eq!(get_balance(&BOB_FALLBACK), balance + value);
	});
}

#[test]
fn delegate_call_missing_contract() {
	let missing_ch = MockLoader::insert(Call, move |_ctx, _| {
		Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() })
	});

	let delegate_ch = MockLoader::insert(Call, move |ctx, _| {
		ctx.ext.delegate_call(Weight::zero(), U256::zero(), CHARLIE_ADDR, Vec::new())?;
		Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() })
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, delegate_ch);
		set_balance(&ALICE, 100);

		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 55).unwrap();

		// contract code missing should still succeed to mimic EVM behavior.
		assert_ok!(MockStack::run_call(
			origin.clone(),
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		));

		// add missing contract code
		place_contract(&CHARLIE, missing_ch);
		assert_ok!(MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		));
	});
}

#[test]
fn changes_are_reverted_on_failing_call() {
	// This test verifies that changes are reverted on a call which fails (or equally, returns
	// a non-zero status code).

	let return_ch = MockLoader::insert(Call, |_, _| {
		Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data: Vec::new() })
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, return_ch);
		set_balance(&ALICE, 100);
		let balance = get_balance(&BOB);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 55).unwrap();

		let output = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			55u64.into(),
			vec![],
			false,
		)
		.unwrap();

		assert!(output.did_revert());
		assert_eq!(get_balance(&ALICE), 100);
		assert_eq!(get_balance(&BOB), balance);
	});
}

#[test]
fn balance_too_low() {
	// This test verifies that a contract can't send value if it's
	// balance is too low.
	let from = ALICE;
	let origin = Origin::from_account_id(ALICE);
	let dest = BOB;

	ExtBuilder::default().build().execute_with(|| {
		set_balance(&from, 0);

		let result = MockStack::transfer(&origin, &from, &dest, 100u64.into());

		assert_eq!(result, Err(Error::<Test>::TransferFailed.into()));
		assert_eq!(get_balance(&from), 0);
		assert_eq!(get_balance(&dest), 0);
	});
}

#[test]
fn output_is_returned_on_success() {
	// Verifies that if a contract returns data with a successful exit status, this data
	// is returned from the execution context.
	let return_ch = MockLoader::insert(Call, |_, _| {
		Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: vec![1, 2, 3, 4] })
	});

	ExtBuilder::default().build().execute_with(|| {
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		place_contract(&BOB, return_ch);

		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		);

		let output = result.unwrap();
		assert!(!output.did_revert());
		assert_eq!(output.data, vec![1, 2, 3, 4]);
	});
}

#[test]
fn output_is_returned_on_failure() {
	// Verifies that if a contract returns data with a failing exit status, this data
	// is returned from the execution context.
	let return_ch = MockLoader::insert(Call, |_, _| {
		Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![1, 2, 3, 4] })
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, return_ch);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		);

		let output = result.unwrap();
		assert!(output.did_revert());
		assert_eq!(output.data, vec![1, 2, 3, 4]);
	});
}

#[test]
fn input_data_to_call() {
	let input_data_ch = MockLoader::insert(Call, |ctx, _| {
		assert_eq!(ctx.input_data, &[1, 2, 3, 4]);
		exec_success()
	});

	// This one tests passing the input data into a contract via call.
	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, input_data_ch);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![1, 2, 3, 4],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn input_data_to_instantiate() {
	let input_data_ch = MockLoader::insert(Constructor, |ctx, _| {
		assert_eq!(ctx.input_data, &[1, 2, 3, 4]);
		exec_success()
	});

	// This one tests passing the input data into a contract via instantiate.
	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.build()
		.execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();
			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			let executable = MockExecutable::from_storage(input_data_ch, &mut gas_meter).unwrap();
			set_balance(&ALICE, min_balance * 10_000);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, deposit_limit::<Test>(), min_balance).unwrap();

			let result = MockStack::run_instantiate(
				ALICE,
				executable,
				&mut gas_meter,
				&mut storage_meter,
				min_balance.into(),
				vec![1, 2, 3, 4],
				Some(&[0; 32]),
				false,
			);
			assert_matches!(result, Ok(_));
		});
}

#[test]
fn max_depth() {
	// This test verifies that when we reach the maximal depth creation of an
	// yet another context fails.
	parameter_types! {
		static ReachedBottom: bool = false;
	}
	let value = Default::default();
	let recurse_ch = MockLoader::insert(Call, |ctx, _| {
		// Try to call into yourself.
		let r = ctx.ext.call(
			Weight::zero(),
			U256::zero(),
			&BOB_ADDR,
			U256::zero(),
			vec![],
			true,
			false,
		);

		ReachedBottom::mutate(|reached_bottom| {
			if !*reached_bottom {
				// We are first time here, it means we just reached bottom.
				// Verify that we've got proper error and set `reached_bottom`.
				assert_eq!(r, Err(Error::<Test>::MaxCallDepthReached.into()));
				*reached_bottom = true;
			} else {
				// We just unwinding stack here.
				assert_matches!(r, Ok(_));
			}
		});

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		set_balance(&BOB, 1);
		place_contract(&BOB, recurse_ch);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, value).unwrap();

		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			value.into(),
			vec![],
			false,
		);

		assert_matches!(result, Ok(_));
	});
}

#[test]
fn caller_returns_proper_values() {
	parameter_types! {
		static WitnessedCallerBob: Option<H160> = None;
		static WitnessedCallerCharlie: Option<H160> = None;
	}

	let bob_ch = MockLoader::insert(Call, |ctx, _| {
		// Record the caller for bob.
		WitnessedCallerBob::mutate(|caller| {
			let origin = ctx.ext.caller();
			*caller = Some(<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_address(
				&origin.account_id().unwrap(),
			));
		});

		// Call into CHARLIE contract.
		assert_matches!(
			ctx.ext.call(
				Weight::zero(),
				U256::zero(),
				&CHARLIE_ADDR,
				U256::zero(),
				vec![],
				true,
				false
			),
			Ok(_)
		);
		exec_success()
	});
	let charlie_ch = MockLoader::insert(Call, |ctx, _| {
		// Record the caller for charlie.
		WitnessedCallerCharlie::mutate(|caller| {
			let origin = ctx.ext.caller();
			*caller = Some(<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_address(
				&origin.account_id().unwrap(),
			));
		});
		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, bob_ch);
		place_contract(&CHARLIE, charlie_ch);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		);

		assert_matches!(result, Ok(_));
	});

	assert_eq!(WitnessedCallerBob::get(), Some(ALICE_ADDR));
	assert_eq!(WitnessedCallerCharlie::get(), Some(BOB_ADDR));
}

#[test]
fn origin_returns_proper_values() {
	parameter_types! {
		static WitnessedCallerBob: Option<H160> = None;
		static WitnessedCallerCharlie: Option<H160> = None;
	}

	let bob_ch = MockLoader::insert(Call, |ctx, _| {
		// Record the origin for bob.
		WitnessedCallerBob::mutate(|witness| {
			let origin = ctx.ext.origin();
			*witness =
				Some(<Test as Config>::AddressMapper::to_address(&origin.account_id().unwrap()));
		});

		// Call into CHARLIE contract.
		assert_matches!(
			ctx.ext.call(
				Weight::zero(),
				U256::zero(),
				&CHARLIE_ADDR,
				U256::zero(),
				vec![],
				true,
				false
			),
			Ok(_)
		);
		exec_success()
	});
	let charlie_ch = MockLoader::insert(Call, |ctx, _| {
		// Record the origin for charlie.
		WitnessedCallerCharlie::mutate(|witness| {
			let origin = ctx.ext.origin();
			*witness =
				Some(<Test as Config>::AddressMapper::to_address(&origin.account_id().unwrap()));
		});
		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, bob_ch);
		place_contract(&CHARLIE, charlie_ch);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		);

		assert_matches!(result, Ok(_));
	});

	assert_eq!(WitnessedCallerBob::get(), Some(ALICE_ADDR));
	assert_eq!(WitnessedCallerCharlie::get(), Some(ALICE_ADDR));
}

#[test]
fn is_contract_returns_proper_values() {
	let bob_ch = MockLoader::insert(Call, |ctx, _| {
		// Verify that BOB is a contract
		assert!(ctx.ext.is_contract(&BOB_ADDR));
		// Verify that ALICE is not a contract
		assert!(!ctx.ext.is_contract(&ALICE_ADDR));
		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, bob_ch);

		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn to_account_id_returns_proper_values() {
	let bob_code_hash = MockLoader::insert(Call, |ctx, _| {
		let alice_account_id = <Test as Config>::AddressMapper::to_account_id(&ALICE_ADDR);
		assert_eq!(ctx.ext.to_account_id(&ALICE_ADDR), alice_account_id);

		const UNMAPPED_ADDR: H160 = H160([99u8; 20]);
		let mut unmapped_fallback_account_id = [0xEE; 32];
		unmapped_fallback_account_id[..20].copy_from_slice(UNMAPPED_ADDR.as_bytes());
		assert_eq!(
			ctx.ext.to_account_id(&UNMAPPED_ADDR),
			AccountId32::new(unmapped_fallback_account_id)
		);

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, bob_code_hash);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![0],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn code_hash_returns_proper_values() {
	let bob_code_hash = MockLoader::insert(Call, |ctx, _| {
		// ALICE is not a contract but account exists so it returns hash of empty data
		assert_eq!(ctx.ext.code_hash(&ALICE_ADDR), EMPTY_CODE_HASH);
		// BOB is a contract (this function) and hence it has a code_hash.
		// `MockLoader` uses contract index to generate the code hash.
		assert_eq!(ctx.ext.code_hash(&BOB_ADDR), H256(keccak_256(&0u64.to_le_bytes())));
		// [0xff;20] doesn't exist and returns hash zero
		assert!(ctx.ext.code_hash(&H160([0xff; 20])).is_zero());

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		// add alice account info to test case EOA code hash
		frame_system::Account::<Test>::insert(
			<Test as Config>::AddressMapper::to_account_id(&ALICE_ADDR),
			AccountInfo { consumers: 1, providers: 1, ..Default::default() },
		);
		place_contract(&BOB, bob_code_hash);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		// ALICE (not contract) -> BOB (contract)
		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![0],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn own_code_hash_returns_proper_values() {
	let bob_ch = MockLoader::insert(Call, |ctx, _| {
		let code_hash = ctx.ext.code_hash(&BOB_ADDR);
		assert_eq!(*ctx.ext.own_code_hash(), code_hash);
		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, bob_ch);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		// ALICE (not contract) -> BOB (contract)
		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![0],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn caller_is_origin_returns_proper_values() {
	let code_charlie = MockLoader::insert(Call, |ctx, _| {
		// BOB is not the origin of the stack call
		assert!(!ctx.ext.caller_is_origin());
		exec_success()
	});

	let code_bob = MockLoader::insert(Call, |ctx, _| {
		// ALICE is the origin of the call stack
		assert!(ctx.ext.caller_is_origin());
		// BOB calls CHARLIE
		ctx.ext
			.call(Weight::zero(), U256::zero(), &CHARLIE_ADDR, U256::zero(), vec![], true, false)
			.map(|_| ctx.ext.last_frame_output().clone())
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_bob);
		place_contract(&CHARLIE, code_charlie);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		// ALICE -> BOB (caller is origin) -> CHARLIE (caller is not origin)
		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![0],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn root_caller_succeeds() {
	let code_bob = MockLoader::insert(Call, |ctx, _| {
		// root is the origin of the call stack.
		assert!(ctx.ext.caller_is_root());
		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_bob);
		let origin = Origin::Root;
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		// root -> BOB (caller is root)
		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![0],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn root_caller_does_not_succeed_when_value_not_zero() {
	let code_bob = MockLoader::insert(Call, |ctx, _| {
		// root is the origin of the call stack.
		assert!(ctx.ext.caller_is_root());
		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_bob);
		let origin = Origin::Root;
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		// root -> BOB (caller is root)
		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			1u64.into(),
			vec![0],
			false,
		);
		assert_matches!(result, Err(_));
	});
}

#[test]
fn root_caller_succeeds_with_consecutive_calls() {
	let code_charlie = MockLoader::insert(Call, |ctx, _| {
		// BOB is not root, even though the origin is root.
		assert!(!ctx.ext.caller_is_root());
		exec_success()
	});

	let code_bob = MockLoader::insert(Call, |ctx, _| {
		// root is the origin of the call stack.
		assert!(ctx.ext.caller_is_root());
		// BOB calls CHARLIE.
		ctx.ext
			.call(Weight::zero(), U256::zero(), &CHARLIE_ADDR, U256::zero(), vec![], true, false)
			.map(|_| ctx.ext.last_frame_output().clone())
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_bob);
		place_contract(&CHARLIE, code_charlie);
		let origin = Origin::Root;
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		// root -> BOB (caller is root) -> CHARLIE (caller is not root)
		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![0],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn address_returns_proper_values() {
	let bob_ch = MockLoader::insert(Call, |ctx, _| {
		// Verify that address matches BOB.
		assert_eq!(ctx.ext.address(), BOB_ADDR);

		// Call into charlie contract.
		assert_matches!(
			ctx.ext.call(
				Weight::zero(),
				U256::zero(),
				&CHARLIE_ADDR,
				U256::zero(),
				vec![],
				true,
				false
			),
			Ok(_)
		);
		exec_success()
	});
	let charlie_ch = MockLoader::insert(Call, |ctx, _| {
		assert_eq!(ctx.ext.address(), CHARLIE_ADDR);
		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, bob_ch);
		place_contract(&CHARLIE, charlie_ch);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		);

		assert_matches!(result, Ok(_));
	});
}

#[test]
fn refuse_instantiate_with_value_below_existential_deposit() {
	let dummy_ch = MockLoader::insert(Constructor, |_, _| exec_success());

	ExtBuilder::default().existential_deposit(15).build().execute_with(|| {
		let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
		let executable = MockExecutable::from_storage(dummy_ch, &mut gas_meter).unwrap();
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		assert_matches!(
			MockStack::run_instantiate(
				ALICE,
				executable,
				&mut gas_meter,
				&mut storage_meter,
				U256::zero(), // <- zero value
				vec![],
				Some(&[0; 32]),
				false,
			),
			Err(_)
		);
	});
}

#[test]
fn instantiation_work_with_success_output() {
	let dummy_ch = MockLoader::insert(Constructor, |_, _| {
		Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: vec![80, 65, 83, 83] })
	});

	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.existential_deposit(15)
		.build()
		.execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();
			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			let executable = MockExecutable::from_storage(dummy_ch, &mut gas_meter).unwrap();
			set_balance(&ALICE, min_balance * 1000);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, min_balance * 100, min_balance).unwrap();

			let instantiated_contract_address = assert_matches!(
				MockStack::run_instantiate(
					ALICE,
					executable,
					&mut gas_meter,
					&mut storage_meter,
					min_balance.into(),
					vec![],
					Some(&[0 ;32]),
					false,
				),
				Ok((address, ref output)) if output.data == vec![80, 65, 83, 83] => address
			);
			let instantiated_contract_id =
				<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_fallback_account_id(
					&instantiated_contract_address,
				);

			// Check that the newly created account has the expected code hash and
			// there are instantiation event.
			assert_eq!(
				ContractInfo::<Test>::load_code_hash(&instantiated_contract_id).unwrap(),
				dummy_ch
			);
		});
}

#[test]
fn instantiation_fails_with_failing_output() {
	let dummy_ch = MockLoader::insert(Constructor, |_, _| {
		Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![70, 65, 73, 76] })
	});

	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.existential_deposit(15)
		.build()
		.execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();
			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			let executable = MockExecutable::from_storage(dummy_ch, &mut gas_meter).unwrap();
			set_balance(&ALICE, min_balance * 1000);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, min_balance * 100, min_balance).unwrap();

			let instantiated_contract_address = assert_matches!(
				MockStack::run_instantiate(
					ALICE,
					executable,
					&mut gas_meter,
					&mut storage_meter,
					min_balance.into(),
					vec![],
					Some(&[0; 32]),
					false,
				),
				Ok((address, ref output)) if output.data == vec![70, 65, 73, 76] => address
			);

			let instantiated_contract_id =
				<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_fallback_account_id(
					&instantiated_contract_address,
				);

			// Check that the account has not been created.
			assert!(ContractInfo::<Test>::load_code_hash(&instantiated_contract_id).is_none());
			assert!(events().is_empty());
		});
}

#[test]
fn instantiation_from_contract() {
	let dummy_ch = MockLoader::insert(Call, |_, _| exec_success());
	let instantiated_contract_address = Rc::new(RefCell::new(None::<H160>));
	let instantiator_ch = MockLoader::insert(Call, {
		let instantiated_contract_address = Rc::clone(&instantiated_contract_address);
		move |ctx, _| {
			// Instantiate a contract and save it's address in `instantiated_contract_address`.
			let (address, output) = ctx
				.ext
				.instantiate(
					Weight::MAX,
					U256::MAX,
					dummy_ch,
					<Test as Config>::Currency::minimum_balance().into(),
					vec![],
					Some(&[48; 32]),
				)
				.map(|address| (address, ctx.ext.last_frame_output().clone()))
				.unwrap();

			*instantiated_contract_address.borrow_mut() = Some(address);
			Ok(output)
		}
	});

	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.existential_deposit(15)
		.build()
		.execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();
			set_balance(&ALICE, min_balance * 100);
			place_contract(&BOB, instantiator_ch);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, min_balance * 10, min_balance * 10).unwrap();

			assert_matches!(
				MockStack::run_call(
					origin,
					BOB_ADDR,
					&mut GasMeter::<Test>::new(GAS_LIMIT),
					&mut storage_meter,
					(min_balance * 10).into(),
					vec![],
					false,
				),
				Ok(_)
			);

			let instantiated_contract_address =
				*instantiated_contract_address.borrow().as_ref().unwrap();

			let instantiated_contract_id =
				<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_fallback_account_id(
					&instantiated_contract_address,
				);

			// Check that the newly created account has the expected code hash and
			// there are instantiation event.
			assert_eq!(
				ContractInfo::<Test>::load_code_hash(&instantiated_contract_id).unwrap(),
				dummy_ch
			);
		});
}

#[test]
fn instantiation_traps() {
	let dummy_ch = MockLoader::insert(Constructor, |_, _| Err("It's a trap!".into()));
	let instantiator_ch = MockLoader::insert(Call, {
		move |ctx, _| {
			// Instantiate a contract and save it's address in `instantiated_contract_address`.
			assert_matches!(
				ctx.ext.instantiate(
					Weight::zero(),
					U256::zero(),
					dummy_ch,
					<Test as Config>::Currency::minimum_balance().into(),
					vec![],
					Some(&[0; 32]),
				),
				Err(ExecError {
					error: DispatchError::Other("It's a trap!"),
					origin: ErrorOrigin::Callee,
				})
			);

			exec_success()
		}
	});

	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.existential_deposit(15)
		.build()
		.execute_with(|| {
			set_balance(&ALICE, 1000);
			set_balance(&BOB_FALLBACK, 100);
			place_contract(&BOB, instantiator_ch);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 200, 0).unwrap();

			assert_matches!(
				MockStack::run_call(
					origin,
					BOB_ADDR,
					&mut GasMeter::<Test>::new(GAS_LIMIT),
					&mut storage_meter,
					U256::zero(),
					vec![],
					false,
				),
				Ok(_)
			);
		});
}

#[test]
fn termination_from_instantiate_fails() {
	let terminate_ch = MockLoader::insert(Constructor, |ctx, _| {
		ctx.ext.terminate(&ALICE_ADDR)?;
		exec_success()
	});

	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.existential_deposit(15)
		.build()
		.execute_with(|| {
			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			let executable = MockExecutable::from_storage(terminate_ch, &mut gas_meter).unwrap();
			set_balance(&ALICE, 10_000);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 100).unwrap();

			assert_eq!(
				MockStack::run_instantiate(
					ALICE,
					executable,
					&mut gas_meter,
					&mut storage_meter,
					100u64.into(),
					vec![],
					Some(&[0; 32]),
					false,
				),
				Err(ExecError {
					error: Error::<Test>::TerminatedInConstructor.into(),
					origin: ErrorOrigin::Callee
				})
			);

			assert_eq!(&events(), &[]);
		});
}

#[test]
fn in_memory_changes_not_discarded() {
	// Call stack: BOB -> CHARLIE (trap) -> BOB' (success)
	// This tests verifies some edge case of the contract info cache:
	// We change some value in our contract info before calling into a contract
	// that calls into ourself. This triggers a case where BOBs contract info
	// is written to storage and invalidated by the successful execution of BOB'.
	// The trap of CHARLIE reverts the storage changes to BOB. When the root BOB regains
	// control it reloads its contract info from storage. We check that changes that
	// are made before calling into CHARLIE are not discarded.
	let code_bob = MockLoader::insert(Call, |ctx, _| {
		if ctx.input_data[0] == 0 {
			let info = ctx.ext.contract_info();
			assert_eq!(info.storage_byte_deposit, 0);
			info.storage_byte_deposit = 42;
			assert_eq!(
				ctx.ext
					.call(
						Weight::zero(),
						U256::zero(),
						&CHARLIE_ADDR,
						U256::zero(),
						vec![],
						true,
						false
					)
					.map(|_| ctx.ext.last_frame_output().clone()),
				exec_trapped()
			);
			assert_eq!(ctx.ext.contract_info().storage_byte_deposit, 42);
		}
		exec_success()
	});
	let code_charlie = MockLoader::insert(Call, |ctx, _| {
		assert!(ctx
			.ext
			.call(Weight::zero(), U256::zero(), &BOB_ADDR, U256::zero(), vec![99], true, false)
			.is_ok());
		exec_trapped()
	});

	// This one tests passing the input data into a contract via call.
	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_bob);
		place_contract(&CHARLIE, code_charlie);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![0],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn recursive_call_during_constructor_is_balance_transfer() {
	let code = MockLoader::insert(Constructor, |ctx, _| {
		let account_id = ctx.ext.account_id().clone();
		let addr =
			<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_address(&account_id);
		let balance = ctx.ext.balance();

		// Calling ourselves during the constructor will trigger a balance
		// transfer since no contract exist yet.
		assert_ok!(ctx.ext.call(
			Weight::zero(),
			U256::zero(),
			&addr,
			(balance - 1).into(),
			vec![],
			true,
			false
		));

		// Should also work with call data set as it is ignored when no
		// contract is deployed.
		assert_ok!(ctx.ext.call(
			Weight::zero(),
			U256::zero(),
			&addr,
			1u32.into(),
			vec![1, 2, 3, 4],
			true,
			false
		));
		exec_success()
	});

	// This one tests passing the input data into a contract via instantiate.
	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.build()
		.execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();
			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			let executable = MockExecutable::from_storage(code, &mut gas_meter).unwrap();
			set_balance(&ALICE, min_balance * 10_000);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, deposit_limit::<Test>(), min_balance).unwrap();

			let result = MockStack::run_instantiate(
				ALICE,
				executable,
				&mut gas_meter,
				&mut storage_meter,
				10u64.into(),
				vec![],
				Some(&[0; 32]),
				false,
			);
			assert_matches!(result, Ok(_));
		});
}

#[test]
fn cannot_send_more_balance_than_available_to_self() {
	let code_hash = MockLoader::insert(Call, |ctx, _| {
		let account_id = ctx.ext.account_id().clone();
		let addr =
			<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_address(&account_id);
		let balance = ctx.ext.balance();

		assert_err!(
			ctx.ext.call(
				Weight::zero(),
				U256::zero(),
				&addr,
				(balance + 1).into(),
				vec![],
				true,
				false
			),
			<Error<Test>>::TransferFailed,
		);
		exec_success()
	});

	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.build()
		.execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();
			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			set_balance(&ALICE, min_balance * 10);
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				U256::zero(),
				vec![],
				false,
			)
			.unwrap();
		});
}

#[test]
fn call_reentry_direct_recursion() {
	// call the contract passed as input with disabled reentry
	let code_bob = MockLoader::insert(Call, |ctx, _| {
		let dest = H160::from_slice(ctx.input_data.as_ref());
		ctx.ext
			.call(Weight::zero(), U256::zero(), &dest, U256::zero(), vec![], false, false)
			.map(|_| ctx.ext.last_frame_output().clone())
	});

	let code_charlie = MockLoader::insert(Call, |_, _| exec_success());

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_bob);
		place_contract(&CHARLIE, code_charlie);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		// Calling another contract should succeed
		assert_ok!(MockStack::run_call(
			origin.clone(),
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			CHARLIE_ADDR.as_bytes().to_vec(),
			false,
		));

		// Calling into oneself fails
		assert_err!(
			MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				U256::zero(),
				BOB_ADDR.as_bytes().to_vec(),
				false,
			)
			.map_err(|e| e.error),
			<Error<Test>>::ReentranceDenied,
		);
	});
}

#[test]
fn call_deny_reentry() {
	let code_bob = MockLoader::insert(Call, |ctx, _| {
		if ctx.input_data[0] == 0 {
			ctx.ext
				.call(
					Weight::zero(),
					U256::zero(),
					&CHARLIE_ADDR,
					U256::zero(),
					vec![],
					false,
					false,
				)
				.map(|_| ctx.ext.last_frame_output().clone())
		} else {
			exec_success()
		}
	});

	// call BOB with input set to '1'
	let code_charlie = MockLoader::insert(Call, |ctx, _| {
		ctx.ext
			.call(Weight::zero(), U256::zero(), &BOB_ADDR, U256::zero(), vec![1], true, false)
			.map(|_| ctx.ext.last_frame_output().clone())
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_bob);
		place_contract(&CHARLIE, code_charlie);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		// BOB -> CHARLIE -> BOB fails as BOB denies reentry.
		assert_err!(
			MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				U256::zero(),
				vec![0],
				false,
			)
			.map_err(|e| e.error),
			<Error<Test>>::ReentranceDenied,
		);
	});
}

#[test]
fn call_runtime_works() {
	let code_hash = MockLoader::insert(Call, |ctx, _| {
		let call = RuntimeCall::System(frame_system::Call::remark_with_event {
			remark: b"Hello World".to_vec(),
		});
		ctx.ext.call_runtime(call).unwrap();
		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		let min_balance = <Test as Config>::Currency::minimum_balance();

		let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
		set_balance(&ALICE, min_balance * 10);
		place_contract(&BOB, code_hash);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		System::reset_events();
		MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut gas_meter,
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		)
		.unwrap();

		let remark_hash = <Test as frame_system::Config>::Hashing::hash(b"Hello World");
		assert_eq!(
			System::events(),
			vec![EventRecord {
				phase: Phase::Initialization,
				event: MetaEvent::System(frame_system::Event::Remarked {
					sender: BOB_FALLBACK,
					hash: remark_hash
				}),
				topics: vec![],
			},]
		);
	});
}

#[test]
fn call_runtime_filter() {
	let code_hash = MockLoader::insert(Call, |ctx, _| {
		use frame_system::Call as SysCall;
		use pallet_balances::Call as BalanceCall;
		use pallet_utility::Call as UtilCall;

		// remark should still be allowed
		let allowed_call =
			RuntimeCall::System(SysCall::remark_with_event { remark: b"Hello".to_vec() });

		// transfers are disallowed by the `TestFiler` (see below)
		let forbidden_call =
			RuntimeCall::Balances(BalanceCall::transfer_allow_death { dest: CHARLIE, value: 22 });

		// simple cases: direct call
		assert_err!(
			ctx.ext.call_runtime(forbidden_call.clone()),
			frame_system::Error::<Test>::CallFiltered
		);

		// as part of a patch: return is OK (but it interrupted the batch)
		assert_ok!(ctx.ext.call_runtime(RuntimeCall::Utility(UtilCall::batch {
			calls: vec![allowed_call.clone(), forbidden_call, allowed_call]
		})),);

		// the transfer wasn't performed
		assert_eq!(get_balance(&CHARLIE), 0);

		exec_success()
	});

	TestFilter::set_filter(|call| match call {
		RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death { .. }) => false,
		_ => true,
	});

	ExtBuilder::default().build().execute_with(|| {
		let min_balance = <Test as Config>::Currency::minimum_balance();

		let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
		set_balance(&ALICE, min_balance * 10);
		place_contract(&BOB, code_hash);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		System::reset_events();
		MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut gas_meter,
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		)
		.unwrap();

		let remark_hash = <Test as frame_system::Config>::Hashing::hash(b"Hello");
		assert_eq!(
			System::events(),
			vec![
				EventRecord {
					phase: Phase::Initialization,
					event: MetaEvent::System(frame_system::Event::Remarked {
						sender: BOB_FALLBACK,
						hash: remark_hash
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: MetaEvent::Utility(pallet_utility::Event::ItemCompleted),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: MetaEvent::Utility(pallet_utility::Event::BatchInterrupted {
						index: 1,
						error: frame_system::Error::<Test>::CallFiltered.into()
					},),
					topics: vec![],
				},
			]
		);
	});
}

#[test]
fn nonce() {
	let fail_code = MockLoader::insert(Constructor, |_, _| exec_trapped());
	let success_code = MockLoader::insert(Constructor, |_, _| exec_success());
	let succ_fail_code = MockLoader::insert(Constructor, move |ctx, _| {
		ctx.ext
			.instantiate(
				Weight::MAX,
				U256::MAX,
				fail_code,
				ctx.ext.minimum_balance() * 100,
				vec![],
				Some(&[0; 32]),
			)
			.ok();
		exec_success()
	});
	let succ_succ_code = MockLoader::insert(Constructor, move |ctx, _| {
		let alice_nonce = System::account_nonce(&ALICE);
		assert_eq!(System::account_nonce(ctx.ext.account_id()), 0);
		assert_eq!(ctx.ext.caller().account_id().unwrap(), &ALICE);
		let addr = ctx
			.ext
			.instantiate(
				Weight::MAX,
				U256::MAX,
				success_code,
				ctx.ext.minimum_balance() * 100,
				vec![],
				Some(&[0; 32]),
			)
			.unwrap();

		let account_id =
			<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_fallback_account_id(&addr);

		assert_eq!(System::account_nonce(&ALICE), alice_nonce);
		assert_eq!(System::account_nonce(ctx.ext.account_id()), 1);
		assert_eq!(System::account_nonce(&account_id), 0);

		// a plain call should not influence the account counter
		ctx.ext
			.call(Weight::zero(), U256::zero(), &addr, U256::zero(), vec![], false, false)
			.unwrap();

		assert_eq!(System::account_nonce(ALICE), alice_nonce);
		assert_eq!(System::account_nonce(ctx.ext.account_id()), 1);
		assert_eq!(System::account_nonce(&account_id), 0);

		exec_success()
	});

	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.build()
		.execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();
			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			let fail_executable = MockExecutable::from_storage(fail_code, &mut gas_meter).unwrap();
			let success_executable =
				MockExecutable::from_storage(success_code, &mut gas_meter).unwrap();
			let succ_fail_executable =
				MockExecutable::from_storage(succ_fail_code, &mut gas_meter).unwrap();
			let succ_succ_executable =
				MockExecutable::from_storage(succ_succ_code, &mut gas_meter).unwrap();
			set_balance(&ALICE, min_balance * 10_000);
			set_balance(&BOB, min_balance * 10_000);
			let origin = Origin::from_account_id(BOB);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, deposit_limit::<Test>(), min_balance * 100)
					.unwrap();

			// fail should not increment
			MockStack::run_instantiate(
				ALICE,
				fail_executable,
				&mut gas_meter,
				&mut storage_meter,
				(min_balance * 100).into(),
				vec![],
				Some(&[0; 32]),
				false,
			)
			.ok();
			assert_eq!(System::account_nonce(&ALICE), 0);

			assert_ok!(MockStack::run_instantiate(
				ALICE,
				success_executable,
				&mut gas_meter,
				&mut storage_meter,
				(min_balance * 100).into(),
				vec![],
				Some(&[0; 32]),
				false,
			));
			assert_eq!(System::account_nonce(&ALICE), 1);

			assert_ok!(MockStack::run_instantiate(
				ALICE,
				succ_fail_executable,
				&mut gas_meter,
				&mut storage_meter,
				(min_balance * 200).into(),
				vec![],
				Some(&[0; 32]),
				false,
			));
			assert_eq!(System::account_nonce(&ALICE), 2);

			assert_ok!(MockStack::run_instantiate(
				ALICE,
				succ_succ_executable,
				&mut gas_meter,
				&mut storage_meter,
				(min_balance * 200).into(),
				vec![],
				Some(&[0; 32]),
				false,
			));
			assert_eq!(System::account_nonce(&ALICE), 3);
		});
}

#[test]
fn set_storage_works() {
	let code_hash = MockLoader::insert(Call, |ctx, _| {
		// Write
		assert_eq!(
			ctx.ext.set_storage(&Key::Fix([1; 32]), Some(vec![1, 2, 3]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_storage(&Key::Fix([2; 32]), Some(vec![4, 5, 6]), true),
			Ok(WriteOutcome::New)
		);
		assert_eq!(ctx.ext.set_storage(&Key::Fix([3; 32]), None, false), Ok(WriteOutcome::New));
		assert_eq!(ctx.ext.set_storage(&Key::Fix([4; 32]), None, true), Ok(WriteOutcome::New));
		assert_eq!(
			ctx.ext.set_storage(&Key::Fix([5; 32]), Some(vec![]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_storage(&Key::Fix([6; 32]), Some(vec![]), true),
			Ok(WriteOutcome::New)
		);

		// Overwrite
		assert_eq!(
			ctx.ext.set_storage(&Key::Fix([1; 32]), Some(vec![42]), false),
			Ok(WriteOutcome::Overwritten(3))
		);
		assert_eq!(
			ctx.ext.set_storage(&Key::Fix([2; 32]), Some(vec![48]), true),
			Ok(WriteOutcome::Taken(vec![4, 5, 6]))
		);
		assert_eq!(ctx.ext.set_storage(&Key::Fix([3; 32]), None, false), Ok(WriteOutcome::New));
		assert_eq!(ctx.ext.set_storage(&Key::Fix([4; 32]), None, true), Ok(WriteOutcome::New));
		assert_eq!(
			ctx.ext.set_storage(&Key::Fix([5; 32]), Some(vec![]), false),
			Ok(WriteOutcome::Overwritten(0))
		);
		assert_eq!(
			ctx.ext.set_storage(&Key::Fix([6; 32]), Some(vec![]), true),
			Ok(WriteOutcome::Taken(vec![]))
		);

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		let min_balance = <Test as Config>::Currency::minimum_balance();

		let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
		set_balance(&ALICE, min_balance * 1000);
		place_contract(&BOB, code_hash);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter =
			storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
		assert_ok!(MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut gas_meter,
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		));
	});
}

#[test]
fn set_storage_varsized_key_works() {
	let code_hash = MockLoader::insert(Call, |ctx, _| {
		// Write
		assert_eq!(
			ctx.ext.set_storage(
				&Key::try_from_var([1; 64].to_vec()).unwrap(),
				Some(vec![1, 2, 3]),
				false
			),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_storage(
				&Key::try_from_var([2; 19].to_vec()).unwrap(),
				Some(vec![4, 5, 6]),
				true
			),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_storage(&Key::try_from_var([3; 19].to_vec()).unwrap(), None, false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_storage(&Key::try_from_var([4; 64].to_vec()).unwrap(), None, true),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext
				.set_storage(&Key::try_from_var([5; 30].to_vec()).unwrap(), Some(vec![]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext
				.set_storage(&Key::try_from_var([6; 128].to_vec()).unwrap(), Some(vec![]), true),
			Ok(WriteOutcome::New)
		);

		// Overwrite
		assert_eq!(
			ctx.ext.set_storage(
				&Key::try_from_var([1; 64].to_vec()).unwrap(),
				Some(vec![42, 43, 44]),
				false
			),
			Ok(WriteOutcome::Overwritten(3))
		);
		assert_eq!(
			ctx.ext.set_storage(
				&Key::try_from_var([2; 19].to_vec()).unwrap(),
				Some(vec![48]),
				true
			),
			Ok(WriteOutcome::Taken(vec![4, 5, 6]))
		);
		assert_eq!(
			ctx.ext.set_storage(&Key::try_from_var([3; 19].to_vec()).unwrap(), None, false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_storage(&Key::try_from_var([4; 64].to_vec()).unwrap(), None, true),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext
				.set_storage(&Key::try_from_var([5; 30].to_vec()).unwrap(), Some(vec![]), false),
			Ok(WriteOutcome::Overwritten(0))
		);
		assert_eq!(
			ctx.ext
				.set_storage(&Key::try_from_var([6; 128].to_vec()).unwrap(), Some(vec![]), true),
			Ok(WriteOutcome::Taken(vec![]))
		);

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		let min_balance = <Test as Config>::Currency::minimum_balance();

		let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
		set_balance(&ALICE, min_balance * 1000);
		place_contract(&BOB, code_hash);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter =
			storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
		assert_ok!(MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut gas_meter,
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		));
	});
}

#[test]
fn get_storage_works() {
	let code_hash = MockLoader::insert(Call, |ctx, _| {
		assert_eq!(
			ctx.ext.set_storage(&Key::Fix([1; 32]), Some(vec![1, 2, 3]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_storage(&Key::Fix([2; 32]), Some(vec![]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(ctx.ext.get_storage(&Key::Fix([1; 32])), Some(vec![1, 2, 3]));
		assert_eq!(ctx.ext.get_storage(&Key::Fix([2; 32])), Some(vec![]));
		assert_eq!(ctx.ext.get_storage(&Key::Fix([3; 32])), None);

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		let min_balance = <Test as Config>::Currency::minimum_balance();

		let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
		set_balance(&ALICE, min_balance * 1000);
		place_contract(&BOB, code_hash);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter =
			storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
		assert_ok!(MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut gas_meter,
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		));
	});
}

#[test]
fn get_storage_size_works() {
	let code_hash = MockLoader::insert(Call, |ctx, _| {
		assert_eq!(
			ctx.ext.set_storage(&Key::Fix([1; 32]), Some(vec![1, 2, 3]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_storage(&Key::Fix([2; 32]), Some(vec![]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(ctx.ext.get_storage_size(&Key::Fix([1; 32])), Some(3));
		assert_eq!(ctx.ext.get_storage_size(&Key::Fix([2; 32])), Some(0));
		assert_eq!(ctx.ext.get_storage_size(&Key::Fix([3; 32])), None);

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		let min_balance = <Test as Config>::Currency::minimum_balance();

		let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
		set_balance(&ALICE, min_balance * 1000);
		place_contract(&BOB, code_hash);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter =
			storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
		assert_ok!(MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut gas_meter,
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		));
	});
}

#[test]
fn get_storage_varsized_key_works() {
	let code_hash = MockLoader::insert(Call, |ctx, _| {
		assert_eq!(
			ctx.ext.set_storage(
				&Key::try_from_var([1; 19].to_vec()).unwrap(),
				Some(vec![1, 2, 3]),
				false
			),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext
				.set_storage(&Key::try_from_var([2; 16].to_vec()).unwrap(), Some(vec![]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.get_storage(&Key::try_from_var([1; 19].to_vec()).unwrap()),
			Some(vec![1, 2, 3])
		);
		assert_eq!(
			ctx.ext.get_storage(&Key::try_from_var([2; 16].to_vec()).unwrap()),
			Some(vec![])
		);
		assert_eq!(ctx.ext.get_storage(&Key::try_from_var([3; 8].to_vec()).unwrap()), None);

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		let min_balance = <Test as Config>::Currency::minimum_balance();

		let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
		set_balance(&ALICE, min_balance * 1000);
		place_contract(&BOB, code_hash);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter =
			storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
		assert_ok!(MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut gas_meter,
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		));
	});
}

#[test]
fn get_storage_size_varsized_key_works() {
	let code_hash = MockLoader::insert(Call, |ctx, _| {
		assert_eq!(
			ctx.ext.set_storage(
				&Key::try_from_var([1; 19].to_vec()).unwrap(),
				Some(vec![1, 2, 3]),
				false
			),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext
				.set_storage(&Key::try_from_var([2; 16].to_vec()).unwrap(), Some(vec![]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.get_storage_size(&Key::try_from_var([1; 19].to_vec()).unwrap()),
			Some(3)
		);
		assert_eq!(
			ctx.ext.get_storage_size(&Key::try_from_var([2; 16].to_vec()).unwrap()),
			Some(0)
		);
		assert_eq!(ctx.ext.get_storage_size(&Key::try_from_var([3; 8].to_vec()).unwrap()), None);

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		let min_balance = <Test as Config>::Currency::minimum_balance();

		let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
		set_balance(&ALICE, min_balance * 1000);
		place_contract(&BOB, code_hash);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter =
			storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
		assert_ok!(MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut gas_meter,
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		));
	});
}

#[test]
fn set_transient_storage_works() {
	let code_hash = MockLoader::insert(Call, |ctx, _| {
		// Write
		assert_eq!(
			ctx.ext.set_transient_storage(&Key::Fix([1; 32]), Some(vec![1, 2, 3]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_transient_storage(&Key::Fix([2; 32]), Some(vec![4, 5, 6]), true),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_transient_storage(&Key::Fix([3; 32]), None, false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_transient_storage(&Key::Fix([4; 32]), None, true),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_transient_storage(&Key::Fix([5; 32]), Some(vec![]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_transient_storage(&Key::Fix([6; 32]), Some(vec![]), true),
			Ok(WriteOutcome::New)
		);

		// Overwrite
		assert_eq!(
			ctx.ext.set_transient_storage(&Key::Fix([1; 32]), Some(vec![42]), false),
			Ok(WriteOutcome::Overwritten(3))
		);
		assert_eq!(
			ctx.ext.set_transient_storage(&Key::Fix([2; 32]), Some(vec![48]), true),
			Ok(WriteOutcome::Taken(vec![4, 5, 6]))
		);
		assert_eq!(
			ctx.ext.set_transient_storage(&Key::Fix([3; 32]), None, false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_transient_storage(&Key::Fix([4; 32]), None, true),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_transient_storage(&Key::Fix([5; 32]), Some(vec![]), false),
			Ok(WriteOutcome::Overwritten(0))
		);
		assert_eq!(
			ctx.ext.set_transient_storage(&Key::Fix([6; 32]), Some(vec![]), true),
			Ok(WriteOutcome::Taken(vec![]))
		);

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_hash);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter =
			storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
		assert_ok!(MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		));
	});
}

#[test]
fn get_transient_storage_works() {
	// Call stack: BOB -> CHARLIE(success) -> BOB' (success)
	let storage_key_1 = &Key::Fix([1; 32]);
	let storage_key_2 = &Key::Fix([2; 32]);
	let storage_key_3 = &Key::Fix([3; 32]);
	let code_bob = MockLoader::insert(Call, |ctx, _| {
		if ctx.input_data[0] == 0 {
			assert_eq!(
				ctx.ext.set_transient_storage(storage_key_1, Some(vec![1, 2]), false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext
					.call(
						Weight::zero(),
						U256::zero(),
						&CHARLIE_ADDR,
						U256::zero(),
						vec![],
						true,
						false,
					)
					.map(|_| ctx.ext.last_frame_output().clone()),
				exec_success()
			);
			assert_eq!(ctx.ext.get_transient_storage(storage_key_1), Some(vec![3]));
			assert_eq!(ctx.ext.get_transient_storage(storage_key_2), Some(vec![]));
			assert_eq!(ctx.ext.get_transient_storage(storage_key_3), None);
		} else {
			assert_eq!(
				ctx.ext.set_transient_storage(storage_key_1, Some(vec![3]), true),
				Ok(WriteOutcome::Taken(vec![1, 2]))
			);
			assert_eq!(
				ctx.ext.set_transient_storage(storage_key_2, Some(vec![]), false),
				Ok(WriteOutcome::New)
			);
		}
		exec_success()
	});
	let code_charlie = MockLoader::insert(Call, |ctx, _| {
		assert!(ctx
			.ext
			.call(Weight::zero(), U256::zero(), &BOB_ADDR, U256::zero(), vec![99], true, false)
			.is_ok());
		// CHARLIE can not read BOB`s storage.
		assert_eq!(ctx.ext.get_transient_storage(storage_key_1), None);
		exec_success()
	});

	// This one tests passing the input data into a contract via call.
	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_bob);
		place_contract(&CHARLIE, code_charlie);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![0],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn get_transient_storage_size_works() {
	let storage_key_1 = &Key::Fix([1; 32]);
	let storage_key_2 = &Key::Fix([2; 32]);
	let storage_key_3 = &Key::Fix([3; 32]);
	let code_hash = MockLoader::insert(Call, |ctx, _| {
		assert_eq!(
			ctx.ext.set_transient_storage(storage_key_1, Some(vec![1, 2, 3]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			ctx.ext.set_transient_storage(storage_key_2, Some(vec![]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(ctx.ext.get_transient_storage_size(storage_key_1), Some(3));
		assert_eq!(ctx.ext.get_transient_storage_size(storage_key_2), Some(0));
		assert_eq!(ctx.ext.get_transient_storage_size(storage_key_3), None);

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_hash);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		assert_ok!(MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		));
	});
}

#[test]
fn rollback_transient_storage_works() {
	// Call stack: BOB -> CHARLIE (trap) -> BOB' (success)
	let storage_key = &Key::Fix([1; 32]);
	let code_bob = MockLoader::insert(Call, |ctx, _| {
		if ctx.input_data[0] == 0 {
			assert_eq!(
				ctx.ext.set_transient_storage(storage_key, Some(vec![1, 2]), false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext
					.call(
						Weight::zero(),
						U256::zero(),
						&CHARLIE_ADDR,
						U256::zero(),
						vec![],
						true,
						false
					)
					.map(|_| ctx.ext.last_frame_output().clone()),
				exec_trapped()
			);
			assert_eq!(ctx.ext.get_transient_storage(storage_key), Some(vec![1, 2]));
		} else {
			let overwritten_length = ctx.ext.get_transient_storage_size(storage_key).unwrap();
			assert_eq!(
				ctx.ext.set_transient_storage(storage_key, Some(vec![3]), false),
				Ok(WriteOutcome::Overwritten(overwritten_length))
			);
			assert_eq!(ctx.ext.get_transient_storage(storage_key), Some(vec![3]));
		}
		exec_success()
	});
	let code_charlie = MockLoader::insert(Call, |ctx, _| {
		assert!(ctx
			.ext
			.call(Weight::zero(), U256::zero(), &BOB_ADDR, U256::zero(), vec![99], true, false)
			.is_ok());
		exec_trapped()
	});

	// This one tests passing the input data into a contract via call.
	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_bob);
		place_contract(&CHARLIE, code_charlie);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![0],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn ecdsa_to_eth_address_returns_proper_value() {
	let bob_ch = MockLoader::insert(Call, |ctx, _| {
		let pubkey_compressed = array_bytes::hex2array_unchecked(
			"028db55b05db86c0b1786ca49f095d76344c9e6056b2f02701a7e7f3c20aabfd91",
		);
		assert_eq!(
			ctx.ext.ecdsa_to_eth_address(&pubkey_compressed).unwrap(),
			array_bytes::hex2array_unchecked::<_, 20>("09231da7b19A016f9e576d23B16277062F4d46A8")
		);
		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, bob_ch);

		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn last_frame_output_works_on_instantiate() {
	let ok_ch = MockLoader::insert(Constructor, move |_, _| {
		Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: vec![127] })
	});
	let revert_ch = MockLoader::insert(Constructor, move |_, _| {
		Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![70] })
	});
	let trap_ch = MockLoader::insert(Constructor, |_, _| Err("It's a trap!".into()));
	let instantiator_ch = MockLoader::insert(Call, {
		move |ctx, _| {
			let value = <Test as Config>::Currency::minimum_balance().into();

			// Successful instantiation should set the output
			let address =
				ctx.ext.instantiate(Weight::MAX, U256::MAX, ok_ch, value, vec![], None).unwrap();
			assert_eq!(
				ctx.ext.last_frame_output(),
				&ExecReturnValue { flags: ReturnFlags::empty(), data: vec![127] }
			);

			// Balance transfers should reset the output
			ctx.ext
				.call(Weight::MAX, U256::MAX, &address, U256::from(1), vec![], true, false)
				.unwrap();
			assert_eq!(ctx.ext.last_frame_output(), &Default::default());

			// Reverted instantiation should set the output
			ctx.ext
				.instantiate(Weight::zero(), U256::zero(), revert_ch, value, vec![], None)
				.unwrap();
			assert_eq!(
				ctx.ext.last_frame_output(),
				&ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![70] }
			);

			// Trapped instantiation should clear the output
			ctx.ext
				.instantiate(Weight::zero(), U256::zero(), trap_ch, value, vec![], None)
				.unwrap_err();
			assert_eq!(
				ctx.ext.last_frame_output(),
				&ExecReturnValue { flags: ReturnFlags::empty(), data: vec![] }
			);

			exec_success()
		}
	});

	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.existential_deposit(15)
		.build()
		.execute_with(|| {
			set_balance(&ALICE, 1000);
			set_balance(&BOB, 100);
			place_contract(&BOB, instantiator_ch);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 200, 0).unwrap();

			MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				U256::zero(),
				vec![],
				false,
			)
			.unwrap()
		});
}

#[test]
fn last_frame_output_works_on_nested_call() {
	// Call stack: BOB -> CHARLIE(revert) -> BOB' (success)
	let code_bob = MockLoader::insert(Call, |ctx, _| {
		if ctx.input_data.is_empty() {
			// We didn't do anything yet
			assert_eq!(
				ctx.ext.last_frame_output(),
				&ExecReturnValue { flags: ReturnFlags::empty(), data: vec![] }
			);

			ctx.ext
				.call(
					Weight::zero(),
					U256::zero(),
					&CHARLIE_ADDR,
					U256::zero(),
					vec![],
					true,
					false,
				)
				.unwrap();
			assert_eq!(
				ctx.ext.last_frame_output(),
				&ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![70] }
			);
		}

		Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: vec![127] })
	});
	let code_charlie = MockLoader::insert(Call, |ctx, _| {
		// We didn't do anything yet
		assert_eq!(
			ctx.ext.last_frame_output(),
			&ExecReturnValue { flags: ReturnFlags::empty(), data: vec![] }
		);

		assert!(ctx
			.ext
			.call(Weight::zero(), U256::zero(), &BOB_ADDR, U256::zero(), vec![99], true, false)
			.is_ok());
		assert_eq!(
			ctx.ext.last_frame_output(),
			&ExecReturnValue { flags: ReturnFlags::empty(), data: vec![127] }
		);

		Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![70] })
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_bob);
		place_contract(&CHARLIE, code_charlie);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![0],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn last_frame_output_is_always_reset() {
	let code_bob = MockLoader::insert(Call, |ctx, _| {
		let invalid_code_hash = H256::from_low_u64_le(u64::MAX);
		let output_revert = || ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![1] };

		// A value of u256::MAX to fail the call on the first condition.
		*ctx.ext.last_frame_output_mut() = output_revert();
		assert_eq!(
			ctx.ext.call(
				Weight::zero(),
				U256::zero(),
				&H160::zero(),
				U256::max_value(),
				vec![],
				true,
				false,
			),
			Err(Error::<Test>::BalanceConversionFailed.into())
		);
		assert_eq!(ctx.ext.last_frame_output(), &Default::default());

		// An unknown code hash should succeed but clear the output.
		*ctx.ext.last_frame_output_mut() = output_revert();
		assert_ok!(ctx.ext.delegate_call(
			Weight::zero(),
			U256::zero(),
			H160([0xff; 20]),
			Default::default()
		));
		assert_eq!(ctx.ext.last_frame_output(), &Default::default());

		// An unknown code hash to fail instantiation on the first condition.
		*ctx.ext.last_frame_output_mut() = output_revert();
		assert_eq!(
			ctx.ext.instantiate(
				Weight::zero(),
				U256::zero(),
				invalid_code_hash,
				U256::zero(),
				vec![],
				None,
			),
			Err(Error::<Test>::CodeNotFound.into())
		);
		assert_eq!(ctx.ext.last_frame_output(), &Default::default());

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		place_contract(&BOB, code_bob);
		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

		let result = MockStack::run_call(
			origin,
			BOB_ADDR,
			&mut GasMeter::<Test>::new(GAS_LIMIT),
			&mut storage_meter,
			U256::zero(),
			vec![],
			false,
		);
		assert_matches!(result, Ok(_));
	});
}

#[test]
fn immutable_data_access_checks_work() {
	let dummy_ch = MockLoader::insert(Constructor, move |ctx, _| {
		// Calls can not store immutable data
		assert_eq!(ctx.ext.get_immutable_data(), Err(Error::<Test>::InvalidImmutableAccess.into()));
		exec_success()
	});
	let instantiator_ch = MockLoader::insert(Call, {
		move |ctx, _| {
			let value = <Test as Config>::Currency::minimum_balance().into();

			assert_eq!(
				ctx.ext.set_immutable_data(vec![0, 1, 2, 3].try_into().unwrap()),
				Err(Error::<Test>::InvalidImmutableAccess.into())
			);

			// Constructors can not access the immutable data
			ctx.ext
				.instantiate(Weight::MAX, U256::MAX, dummy_ch, value, vec![], None)
				.unwrap();

			exec_success()
		}
	});
	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.existential_deposit(15)
		.build()
		.execute_with(|| {
			set_balance(&ALICE, 1000);
			set_balance(&BOB, 100);
			place_contract(&BOB, instantiator_ch);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 200, 0).unwrap();

			MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				U256::zero(),
				vec![],
				false,
			)
			.unwrap()
		});
}

#[test]
fn correct_immutable_data_in_delegate_call() {
	let charlie_ch = MockLoader::insert(Call, |ctx, _| {
		Ok(ExecReturnValue {
			flags: ReturnFlags::empty(),
			data: ctx.ext.get_immutable_data()?.to_vec(),
		})
	});
	let bob_ch = MockLoader::insert(Call, move |ctx, _| {
		// In a regular call, we should witness the callee immutable data
		assert_eq!(
			ctx.ext
				.call(
					Weight::zero(),
					U256::zero(),
					&CHARLIE_ADDR,
					U256::zero(),
					vec![],
					true,
					false,
				)
				.map(|_| ctx.ext.last_frame_output().data.clone()),
			Ok(vec![2]),
		);

		// Also in a delegate call, we should witness the callee immutable data
		assert_eq!(
			ctx.ext
				.delegate_call(Weight::zero(), U256::zero(), CHARLIE_ADDR, Vec::new())
				.map(|_| ctx.ext.last_frame_output().data.clone()),
			Ok(vec![2])
		);

		exec_success()
	});
	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.existential_deposit(15)
		.build()
		.execute_with(|| {
			place_contract(&BOB, bob_ch);
			place_contract(&CHARLIE, charlie_ch);

			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 200, 0).unwrap();

			// Place unique immutable data for each contract
			<ImmutableDataOf<Test>>::insert::<_, ImmutableData>(
				BOB_ADDR,
				vec![1].try_into().unwrap(),
			);
			<ImmutableDataOf<Test>>::insert::<_, ImmutableData>(
				CHARLIE_ADDR,
				vec![2].try_into().unwrap(),
			);

			MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				U256::zero(),
				vec![],
				false,
			)
			.unwrap()
		});
}

#[test]
fn immutable_data_set_overrides() {
	let hash = MockLoader::insert_both(
		move |ctx, _| {
			// Calling `set_immutable_data` the first time should work
			assert_ok!(ctx.ext.set_immutable_data(vec![0, 1, 2, 3].try_into().unwrap()));
			// Calling `set_immutable_data` the second time overrides the original one
			assert_ok!(ctx.ext.set_immutable_data(vec![7, 5].try_into().unwrap()));
			exec_success()
		},
		move |ctx, _| {
			assert_eq!(ctx.ext.get_immutable_data().unwrap().into_inner(), vec![7, 5]);
			exec_success()
		},
	);
	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.existential_deposit(15)
		.build()
		.execute_with(|| {
			set_balance(&ALICE, 1000);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 200, 0).unwrap();
			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);

			let addr = MockStack::run_instantiate(
				ALICE,
				MockExecutable::from_storage(hash, &mut gas_meter).unwrap(),
				&mut gas_meter,
				&mut storage_meter,
				U256::zero(),
				vec![],
				None,
				false,
			)
			.unwrap()
			.0;

			MockStack::run_call(
				origin,
				addr,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				U256::zero(),
				vec![],
				false,
			)
			.unwrap()
		});
}

#[test]
fn immutable_data_set_errors_with_empty_data() {
	let dummy_ch = MockLoader::insert(Constructor, move |ctx, _| {
		// Calling `set_immutable_data` with empty data should error out
		assert_eq!(
			ctx.ext.set_immutable_data(Default::default()),
			Err(Error::<Test>::InvalidImmutableAccess.into())
		);
		exec_success()
	});
	let instantiator_ch = MockLoader::insert(Call, {
		move |ctx, _| {
			let value = <Test as Config>::Currency::minimum_balance().into();
			ctx.ext
				.instantiate(Weight::MAX, U256::MAX, dummy_ch, value, vec![], None)
				.unwrap();

			exec_success()
		}
	});
	ExtBuilder::default()
		.with_code_hashes(MockLoader::code_hashes())
		.existential_deposit(15)
		.build()
		.execute_with(|| {
			set_balance(&ALICE, 1000);
			set_balance(&BOB, 100);
			place_contract(&BOB, instantiator_ch);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 200, 0).unwrap();

			MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				U256::zero(),
				vec![],
				false,
			)
			.unwrap()
		});
}

#[test]
fn block_hash_returns_proper_values() {
	let bob_code_hash = MockLoader::insert(Call, |ctx, _| {
		ctx.ext.block_number = 1u32.into();
		assert_eq!(ctx.ext.block_hash(U256::from(1)), None);
		assert_eq!(ctx.ext.block_hash(U256::from(0)), Some(H256::from([1; 32])));

		ctx.ext.block_number = 300u32.into();
		assert_eq!(ctx.ext.block_hash(U256::from(300)), None);
		assert_eq!(ctx.ext.block_hash(U256::from(43)), None);
		assert_eq!(ctx.ext.block_hash(U256::from(44)), Some(H256::from([2; 32])));

		exec_success()
	});

	ExtBuilder::default().build().execute_with(|| {
		frame_system::BlockHash::<Test>::insert(
			&BlockNumberFor::<Test>::from(0u32),
			<tests::Test as frame_system::Config>::Hash::from([1; 32]),
		);
		frame_system::BlockHash::<Test>::insert(
			&BlockNumberFor::<Test>::from(1u32),
			<tests::Test as frame_system::Config>::Hash::default(),
		);
		frame_system::BlockHash::<Test>::insert(
			&BlockNumberFor::<Test>::from(43u32),
			<tests::Test as frame_system::Config>::Hash::default(),
		);
		frame_system::BlockHash::<Test>::insert(
			&BlockNumberFor::<Test>::from(44u32),
			<tests::Test as frame_system::Config>::Hash::from([2; 32]),
		);
		frame_system::BlockHash::<Test>::insert(
			&BlockNumberFor::<Test>::from(300u32),
			<tests::Test as frame_system::Config>::Hash::default(),
		);

		place_contract(&BOB, bob_code_hash);

		let origin = Origin::from_account_id(ALICE);
		let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
		assert_matches!(
			MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				U256::zero(),
				vec![0],
				false,
			),
			Ok(_)
		);
	});
}
