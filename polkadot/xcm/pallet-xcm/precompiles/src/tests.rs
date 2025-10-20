// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.
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

use super::*;
use crate::{
	mock::*,
	IXcm::{self, weighMessageCall},
	VersionedLocation, VersionedXcm,
};
use frame_support::traits::Currency;
use pallet_revive::{
	precompiles::{
		alloy::{
			hex,
			sol_types::{SolInterface, SolValue},
		},
		H160,
	},
	test_utils::builder::{BareInstantiateBuilder, Contract},
	Code, ExecConfig, U256,
};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_runtime::traits::AccountIdConversion;
use xcm::{prelude::*, v3, v4};

alloy::sol!("src/fixtures/CallToXcmPrecompile.sol");

const BOB: AccountId = AccountId::new([1u8; 32]);
const CHARLIE: AccountId = AccountId::new([2u8; 32]);
const SEND_AMOUNT: u128 = 10;
const CUSTOM_INITIAL_BALANCE: u128 = 200_000_000_000_000u128;

const CALL_TO_XCM_PRECOMPILE_PVM: &[u8] = include_bytes!("fixtures/CallToXcmPrecompile.pvm");

#[test]
fn test_xcm_send_precompile_works() {
	use codec::Encode;

	let balances = vec![
		(ALICE, CUSTOM_INITIAL_BALANCE),
		(ParaId::from(SOME_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let xcm_precompile_addr = H160::from(
			hex::const_decode_to_array(b"00000000000000000000000000000000000A0000").unwrap(),
		);

		let sender: Location = AccountId32 { network: None, id: ALICE.into() }.into();
		let message = Xcm(vec![
			ReserveAssetDeposited((Parent, SEND_AMOUNT).into()),
			ClearOrigin,
			buy_execution((Parent, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: sender.clone() },
		]);

		let versioned_dest: VersionedLocation = RelayLocation::get().into();
		let versioned_message: VersionedXcm<()> = VersionedXcm::from(message.clone());

		let xcm_send_params = IXcm::sendCall {
			destination: versioned_dest.encode().into(),
			message: versioned_message.encode().into(),
		};
		let call = IXcm::IXcmCalls::send(xcm_send_params);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);
		assert!(result.result.is_ok());
		let sent_message = Xcm(Some(DescendOrigin(sender.clone().try_into().unwrap()))
			.into_iter()
			.chain(message.0.clone().into_iter())
			.collect());
		assert_eq!(sent_xcm(), vec![(Here.into(), sent_message)]);
	});
}

#[test]
fn test_xcm_send_precompile_to_parachain() {
	use codec::Encode;

	let balances = vec![
		(ALICE, CUSTOM_INITIAL_BALANCE),
		(ParaId::from(SOME_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let xcm_precompile_addr = H160::from(
			hex::const_decode_to_array(b"00000000000000000000000000000000000A0000").unwrap(),
		);

		let sender: Location = AccountId32 { network: None, id: ALICE.into() }.into();
		let message = Xcm(vec![
			ReserveAssetDeposited((Parent, SEND_AMOUNT).into()),
			ClearOrigin,
			buy_execution((Parent, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: sender.clone() },
		]);

		let destination: VersionedLocation = Parachain(SOME_PARA_ID).into();
		let versioned_message: VersionedXcm<()> = VersionedXcm::from(message.clone());

		let xcm_send_params = IXcm::sendCall {
			destination: destination.encode().into(),
			message: versioned_message.encode().into(),
		};
		let call = IXcm::IXcmCalls::send(xcm_send_params);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);

		assert!(result.result.is_ok());
		let sent_message = Xcm(Some(DescendOrigin(sender.clone().try_into().unwrap()))
			.into_iter()
			.chain(message.0.clone().into_iter())
			.collect());
		assert_eq!(sent_xcm(), vec![(Parachain(SOME_PARA_ID).into(), sent_message)]);
	});
}

#[test]
fn test_xcm_send_precompile_fails() {
	use codec::Encode;

	let balances = vec![
		(ALICE, CUSTOM_INITIAL_BALANCE),
		(ParaId::from(SOME_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let xcm_precompile_addr = H160::from(
			hex::const_decode_to_array(b"00000000000000000000000000000000000A0000").unwrap(),
		);

		let sender: Location = AccountId32 { network: None, id: ALICE.into() }.into();
		let message = Xcm(vec![
			ReserveAssetDeposited((Parent, SEND_AMOUNT).into()),
			buy_execution((Parent, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: sender },
		]);

		let destination: VersionedLocation = VersionedLocation::from(Location::ancestor(8));
		let versioned_message: VersionedXcm<()> = VersionedXcm::from(message.clone());

		let xcm_send_params = IXcm::sendCall {
			destination: destination.encode().into(),
			message: versioned_message.encode().into(),
		};
		let call = IXcm::IXcmCalls::send(xcm_send_params);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);
		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("XcmSendPrecompile call failed with error: {err:?}"),
		};
		assert!(return_value.did_revert());
	});
}

#[test]
fn send_fails_on_old_location_version() {
	use codec::Encode;

	let balances = vec![
		(ALICE, CUSTOM_INITIAL_BALANCE),
		(ParaId::from(SOME_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let xcm_precompile_addr = H160::from(
			hex::const_decode_to_array(b"00000000000000000000000000000000000A0000").unwrap(),
		);

		let sender: Location = AccountId32 { network: None, id: ALICE.into() }.into();
		let message = Xcm(vec![
			ReserveAssetDeposited((Parent, SEND_AMOUNT).into()),
			ClearOrigin,
			buy_execution((Parent, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: sender.clone() },
		]);

		// V4 location is old and will fail.
		let destination: VersionedLocation =
			VersionedLocation::V4(v4::Junction::Parachain(SOME_PARA_ID).into());
		let versioned_message: VersionedXcm<RuntimeCall> = VersionedXcm::from(message.clone());

		let xcm_send_params = IXcm::sendCall {
			destination: destination.encode().into(),
			message: versioned_message.encode().into(),
		};
		let call = IXcm::IXcmCalls::send(xcm_send_params);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);
		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("XcmSendPrecompile call failed with error: {err:?}"),
		};
		assert!(return_value.did_revert());

		// V3 also fails.
		let destination: VersionedLocation =
			VersionedLocation::V3(v3::Junction::Parachain(SOME_PARA_ID).into());
		let versioned_message: VersionedXcm<RuntimeCall> = VersionedXcm::from(message);

		let xcm_send_params = IXcm::sendCall {
			destination: destination.encode().into(),
			message: versioned_message.encode().into(),
		};
		let call = IXcm::IXcmCalls::send(xcm_send_params);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);
		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("XcmSendPrecompile call failed with error: {err:?}"),
		};
		assert!(return_value.did_revert());
	});
}

#[test]
fn send_fails_on_old_xcm_version() {
	use codec::Encode;

	let balances = vec![
		(ALICE, CUSTOM_INITIAL_BALANCE),
		(ParaId::from(SOME_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let xcm_precompile_addr = H160::from(
			hex::const_decode_to_array(b"00000000000000000000000000000000000A0000").unwrap(),
		);

		let sender: Location = AccountId32 { network: None, id: ALICE.into() }.into();
		let message = Xcm(vec![
			ReserveAssetDeposited((Parent, SEND_AMOUNT).into()),
			ClearOrigin,
			buy_execution((Parent, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: sender.clone() },
		]);
		// V4 is old and fails.
		let v4_message: v4::Xcm<RuntimeCall> = message.try_into().unwrap();

		let destination: VersionedLocation = Parachain(SOME_PARA_ID).into();
		let versioned_message: VersionedXcm<RuntimeCall> = VersionedXcm::V4(v4_message.clone());

		let xcm_send_params = IXcm::sendCall {
			destination: destination.encode().into(),
			message: versioned_message.encode().into(),
		};
		let call = IXcm::IXcmCalls::send(xcm_send_params);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);
		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("XcmSendPrecompile call failed with error: {err:?}"),
		};
		assert!(return_value.did_revert());

		// With V3 it also fails.
		let v3_message: v3::Xcm<RuntimeCall> = v4_message.try_into().unwrap();

		let destination: VersionedLocation = Parachain(SOME_PARA_ID).into();
		let versioned_message: VersionedXcm<RuntimeCall> = VersionedXcm::V3(v3_message);

		let xcm_send_params = IXcm::sendCall {
			destination: destination.encode().into(),
			message: versioned_message.encode().into(),
		};
		let call = IXcm::IXcmCalls::send(xcm_send_params);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);
		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("XcmSendPrecompile call failed with error: {err:?}"),
		};
		assert!(return_value.did_revert());
	});
}

#[test]
fn test_xcm_execute_precompile_works() {
	use codec::Encode;

	let balances = vec![
		(ALICE, CUSTOM_INITIAL_BALANCE),
		(ParaId::from(SOME_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let xcm_precompile_addr = H160::from(
			hex::const_decode_to_array(b"00000000000000000000000000000000000A0000").unwrap(),
		);

		let dest: Location = Junction::AccountId32 { network: None, id: BOB.into() }.into();
		assert_eq!(Balances::total_balance(&ALICE), CUSTOM_INITIAL_BALANCE);

		let message: VersionedXcm<RuntimeCall> = VersionedXcm::from(Xcm(vec![
			WithdrawAsset((Here, SEND_AMOUNT).into()),
			buy_execution((Here, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: dest },
		]));

		let weight_params = weighMessageCall { message: message.encode().into() };
		let weight_call = IXcm::IXcmCalls::weighMessage(weight_params);
		let encoded_weight_call = weight_call.abi_encode();

		let xcm_weight_results = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_weight_call,
			ExecConfig::new_substrate_tx(),
		);

		let weight_result = match xcm_weight_results.result {
			Ok(value) => value,
			Err(err) => panic!("XcmExecutePrecompile Failed to decode weight with error {err:?}"),
		};

		let weight: IXcm::Weight = IXcm::Weight::abi_decode(&weight_result.data[..])
			.expect("XcmExecutePrecompile Failed to decode weight");

		let xcm_execute_params = IXcm::execute_0Call { message: message.encode().into(), weight };
		let call = IXcm::IXcmCalls::execute_0(xcm_execute_params);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);

		assert!(result.result.is_ok());
		assert_eq!(Balances::total_balance(&ALICE), CUSTOM_INITIAL_BALANCE - SEND_AMOUNT);
		assert_eq!(Balances::total_balance(&BOB), SEND_AMOUNT);
	});
}

#[test]
fn test_xcm_execute_precompile_different_beneficiary() {
	use codec::Encode;

	let balances = vec![(ALICE, CUSTOM_INITIAL_BALANCE), (CHARLIE, CUSTOM_INITIAL_BALANCE)];
	new_test_ext_with_balances(balances).execute_with(|| {
		let xcm_precompile_addr = H160::from(
			hex::const_decode_to_array(b"00000000000000000000000000000000000A0000").unwrap(),
		);

		let dest: Location = Junction::AccountId32 { network: None, id: CHARLIE.into() }.into();
		assert_eq!(Balances::total_balance(&ALICE), CUSTOM_INITIAL_BALANCE);

		let message: VersionedXcm<RuntimeCall> = VersionedXcm::from(Xcm(vec![
			WithdrawAsset((Here, SEND_AMOUNT).into()),
			buy_execution((Here, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: dest },
		]));

		let weight_params = weighMessageCall { message: message.encode().into() };
		let weight_call = IXcm::IXcmCalls::weighMessage(weight_params);
		let encoded_weight_call = weight_call.abi_encode();

		let xcm_weight_results = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_weight_call,
			ExecConfig::new_substrate_tx(),
		);

		let weight_result = match xcm_weight_results.result {
			Ok(value) => value,
			Err(err) => panic!("XcmExecutePrecompile Failed to decode weight with error: {err:?}"),
		};

		let weight: IXcm::Weight = IXcm::Weight::abi_decode(&weight_result.data[..])
			.expect("XcmExecutePrecompile Failed to decode weight");

		let xcm_execute_params = IXcm::execute_0Call { message: message.encode().into(), weight };
		let call = IXcm::IXcmCalls::execute_0(xcm_execute_params);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);

		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("XcmExecutePrecompile call failed with error: {err:?}"),
		};

		assert!(!return_value.did_revert());
		assert_eq!(Balances::total_balance(&ALICE), CUSTOM_INITIAL_BALANCE - SEND_AMOUNT);
		assert_eq!(Balances::total_balance(&CHARLIE), CUSTOM_INITIAL_BALANCE + SEND_AMOUNT);
	});
}

#[test]
fn test_xcm_execute_precompile_fails() {
	use codec::Encode;

	let balances = vec![(ALICE, CUSTOM_INITIAL_BALANCE), (BOB, CUSTOM_INITIAL_BALANCE)];
	new_test_ext_with_balances(balances).execute_with(|| {
		let xcm_precompile_addr = H160::from(
			hex::const_decode_to_array(b"00000000000000000000000000000000000A0000").unwrap(),
		);

		let dest: Location = Junction::AccountId32 { network: None, id: BOB.into() }.into();
		assert_eq!(Balances::total_balance(&ALICE), CUSTOM_INITIAL_BALANCE);
		let amount_to_send = CUSTOM_INITIAL_BALANCE - ExistentialDeposit::get();
		let assets: Assets = (Here, amount_to_send).into();

		let message: VersionedXcm<RuntimeCall> = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone()),
			buy_execution(assets.inner()[0].clone()),
			DepositAsset { assets: assets.clone().into(), beneficiary: dest },
			WithdrawAsset(assets),
		]));

		let weight_params = weighMessageCall { message: message.encode().into() };
		let weight_call = IXcm::IXcmCalls::weighMessage(weight_params);
		let encoded_weight_call = weight_call.abi_encode();

		let xcm_weight_results = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_weight_call,
			ExecConfig::new_substrate_tx(),
		);

		let weight_result = match xcm_weight_results.result {
			Ok(value) => value,
			Err(err) => panic!("XcmExecutePrecompile Failed to decode weight with error: {err:?}"),
		};

		let weight: IXcm::Weight = IXcm::Weight::abi_decode(&weight_result.data[..])
			.expect("XcmExecutePrecompile Failed to decode weight");

		let xcm_execute_params = IXcm::execute_0Call { message: message.encode().into(), weight };
		let call = IXcm::IXcmCalls::execute_0(xcm_execute_params);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);
		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("XcmExecutePrecompile call failed with error: {err:?}"),
		};
		assert!(return_value.did_revert());
		assert_eq!(Balances::total_balance(&ALICE), CUSTOM_INITIAL_BALANCE);
		assert_eq!(Balances::total_balance(&BOB), CUSTOM_INITIAL_BALANCE);
	});
}

#[test]
fn execute_fails_on_old_version() {
	use codec::Encode;

	let balances = vec![
		(ALICE, CUSTOM_INITIAL_BALANCE),
		(ParaId::from(SOME_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let xcm_precompile_addr = H160::from(
			hex::const_decode_to_array(b"00000000000000000000000000000000000A0000").unwrap(),
		);

		let dest: Location = Junction::AccountId32 { network: None, id: BOB.into() }.into();
		assert_eq!(Balances::total_balance(&ALICE), CUSTOM_INITIAL_BALANCE);

		let message = Xcm(vec![
			WithdrawAsset((Here, SEND_AMOUNT).into()),
			buy_execution((Here, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: dest },
		]);
		let versioned_message = VersionedXcm::from(message.clone());

		let weight_params = weighMessageCall { message: versioned_message.encode().into() };
		let weight_call = IXcm::IXcmCalls::weighMessage(weight_params);
		let encoded_weight_call = weight_call.abi_encode();

		let xcm_weight_results = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_weight_call,
			ExecConfig::new_substrate_tx(),
		);

		let weight_result = match xcm_weight_results.result {
			Ok(value) => value,
			Err(err) => panic!("XcmExecutePrecompile Failed to decode weight with error {err:?}"),
		};

		let weight: IXcm::Weight = IXcm::Weight::abi_decode(&weight_result.data[..])
			.expect("XcmExecutePrecompile Failed to decode weight");

		// Using a V4 message to check that it fails.
		let v4_message: v4::Xcm<RuntimeCall> = message.clone().try_into().unwrap();
		let versioned_message = VersionedXcm::V4(v4_message.clone());

		let xcm_execute_params = IXcm::execute_0Call {
			message: versioned_message.encode().into(),
			weight: weight.clone(),
		};
		let call = IXcm::IXcmCalls::execute_0(xcm_execute_params);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);

		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("XcmExecutePrecompile call failed with error: {err:?}"),
		};
		assert!(return_value.did_revert());
		assert_eq!(Balances::total_balance(&ALICE), CUSTOM_INITIAL_BALANCE);
		assert_eq!(Balances::total_balance(&BOB), 0);

		// Now using a V3 message.
		let v3_message: v3::Xcm<RuntimeCall> = v4_message.try_into().unwrap();
		let versioned_message = VersionedXcm::V3(v3_message);

		let xcm_execute_params =
			IXcm::execute_0Call { message: versioned_message.encode().into(), weight };
		let call = IXcm::IXcmCalls::execute_0(xcm_execute_params);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);

		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("XcmExecutePrecompile call failed with error: {err:?}"),
		};
		assert!(return_value.did_revert());
		assert_eq!(Balances::total_balance(&ALICE), CUSTOM_INITIAL_BALANCE);
		assert_eq!(Balances::total_balance(&BOB), 0);
	});
}

#[test]
fn weight_fails_on_old_version() {
	use codec::Encode;

	let balances = vec![
		(ALICE, CUSTOM_INITIAL_BALANCE),
		(ParaId::from(SOME_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let xcm_precompile_addr = H160::from(
			hex::const_decode_to_array(b"00000000000000000000000000000000000A0000").unwrap(),
		);

		let dest: Location = Junction::AccountId32 { network: None, id: BOB.into() }.into();
		assert_eq!(Balances::total_balance(&ALICE), CUSTOM_INITIAL_BALANCE);

		let message: Xcm<RuntimeCall> = Xcm(vec![
			WithdrawAsset((Here, SEND_AMOUNT).into()),
			buy_execution((Here, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: dest },
		]);
		// V4 version is old, fails.
		let v4_message: v4::Xcm<RuntimeCall> = message.try_into().unwrap();
		let versioned_message = VersionedXcm::V4(v4_message.clone());

		let weight_params = weighMessageCall { message: versioned_message.encode().into() };
		let weight_call = IXcm::IXcmCalls::weighMessage(weight_params);
		let encoded_weight_call = weight_call.abi_encode();

		let xcm_weight_results = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_weight_call,
			ExecConfig::new_substrate_tx(),
		);

		let result = match xcm_weight_results.result {
			Ok(value) => value,
			Err(err) => panic!("XcmExecutePrecompile Failed to decode weight with error {err:?}"),
		};
		assert!(result.did_revert());

		// Now we also try V3.
		let v3_message: v3::Xcm<RuntimeCall> = v4_message.try_into().unwrap();
		let versioned_message = VersionedXcm::V3(v3_message);

		let weight_params = weighMessageCall { message: versioned_message.encode().into() };
		let weight_call = IXcm::IXcmCalls::weighMessage(weight_params);
		let encoded_weight_call = weight_call.abi_encode();

		let xcm_weight_results = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			xcm_precompile_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_weight_call,
			ExecConfig::new_substrate_tx(),
		);

		let result = match xcm_weight_results.result {
			Ok(value) => value,
			Err(err) => panic!("XcmExecutePrecompile Failed to decode weight with error {err:?}"),
		};
		assert!(result.did_revert());
	});
}

#[test]
fn test_xcm_execute_as_account_works() {
	use codec::Encode;

	let balances = vec![
		(ALICE, CUSTOM_INITIAL_BALANCE),
		(ParaId::from(SOME_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
	];

	new_test_ext_with_balances(balances).execute_with(|| {
		let code = CALL_TO_XCM_PRECOMPILE_PVM.to_vec();

		let Contract { addr: contract_addr, .. } =
			BareInstantiateBuilder::<Test>::bare_instantiate(
				RuntimeOrigin::signed(ALICE),
				Code::Upload(code),
			)
			.storage_deposit_limit(CUSTOM_INITIAL_BALANCE / 10)
			.build_and_unwrap_contract();

		let alice_balance_after_deployment = Balances::free_balance(ALICE);
		let bob_initial_balance = Balances::free_balance(BOB);

		let beneficiary: Location = Junction::AccountId32 { network: None, id: BOB.into() }.into();
		let transfer_amount = 1_000;
		let message: VersionedXcm<RuntimeCall> = VersionedXcm::from(Xcm(vec![
			WithdrawAsset((Here, transfer_amount).into()),
			buy_execution((Here, transfer_amount)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary },
		]));

		let xcm_execute_as_acc_params =
			CallToXcmPrecompile::callExecuteAsAccount_1Call { message: message.encode().into() };
		let call = CallToXcmPrecompile::CallToXcmPrecompileCalls::callExecuteAsAccount_1(
			xcm_execute_as_acc_params,
		);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			contract_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);

		assert!(result.result.is_ok());
		assert_eq!(Balances::free_balance(ALICE), alice_balance_after_deployment - transfer_amount,);
		assert_eq!(Balances::free_balance(BOB), bob_initial_balance + transfer_amount,);
	});
}

#[test]
fn test_xcm_execute_as_account_fails() {
	use codec::Encode;

	const ALICE_WITHDRAWAL_ATTEMPT: u128 = CUSTOM_INITIAL_BALANCE * 2; // More than Alice has

	let balances = vec![
		(ALICE, CUSTOM_INITIAL_BALANCE),
		(BOB, ALICE_WITHDRAWAL_ATTEMPT),
		(ParaId::from(SOME_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
	];

	new_test_ext_with_balances(balances).execute_with(|| {
		let code = CALL_TO_XCM_PRECOMPILE_PVM.to_vec();

		// Alice deploys the contract that performs a cross-contract calls to the XCM precompile
		let Contract { addr: contract_addr, account_id: contract_account_id } =
			BareInstantiateBuilder::<Test>::bare_instantiate(
				RuntimeOrigin::signed(ALICE),
				Code::Upload(code),
			)
			.storage_deposit_limit(CUSTOM_INITIAL_BALANCE / 10)
			.build_and_unwrap_contract();

		let alice_balance_after_deployment = Balances::free_balance(ALICE);
		assert!(alice_balance_after_deployment < ALICE_WITHDRAWAL_ATTEMPT);

		// Not really necessary, just to demonstrate that the contract has enough funds in case
		// `execute` was called instead
		let _ = Balances::transfer_allow_death(
			RuntimeOrigin::signed(BOB),
			contract_account_id.clone(),
			ALICE_WITHDRAWAL_ATTEMPT,
		);

		let contract_balance_after_funding = Balances::free_balance(contract_account_id.clone());

		let beneficiary: Location = Junction::AccountId32 { network: None, id: BOB.into() }.into();

		let message: VersionedXcm<RuntimeCall> = VersionedXcm::from(Xcm(vec![
			WithdrawAsset((Here, ALICE_WITHDRAWAL_ATTEMPT).into()),
			buy_execution((Here, ALICE_WITHDRAWAL_ATTEMPT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary },
		]));

		let xcm_execute_as_acc_params =
			CallToXcmPrecompile::callExecuteAsAccount_1Call { message: message.encode().into() };
		let call = CallToXcmPrecompile::CallToXcmPrecompileCalls::callExecuteAsAccount_1(
			xcm_execute_as_acc_params,
		);
		let encoded_call = call.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			contract_addr,
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);

		// This should fail because it uses Alice as the origin,
		// so Alice's insufficient balance causes the failure
		assert!(result.result.unwrap().did_revert());

		// Verify balances are unchanged after failed call
		assert_eq!(Balances::free_balance(ALICE), alice_balance_after_deployment);
		assert_eq!(Balances::free_balance(contract_account_id), contract_balance_after_funding);
	});
}
