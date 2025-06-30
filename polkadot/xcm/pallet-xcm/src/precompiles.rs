// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use crate::{Config, VersionedLocation, VersionedXcm, Weight, WeightInfo};
use alloc::vec::Vec;
use codec::{DecodeAll, DecodeLimit, Encode};
use core::{fmt, marker::PhantomData, num::NonZero};
use pallet_revive::{
	precompiles::{
		alloy::{self, sol_types::SolValue},
		AddressMatcher, Error, Ext, Precompile,
	},
	DispatchInfo, Origin,
};
use tracing::error;
use xcm::MAX_XCM_DECODE_DEPTH;
use xcm_executor::traits::WeightBounds;

alloy::sol!("src/precompiles/IXcm.sol");
use IXcm::IXcmCalls;

const LOG_TARGET: &str = "xcm::precompiles";

fn revert(error: &impl fmt::Debug, message: &str) -> Error {
	error!(target: LOG_TARGET, ?error, "{}", message);
	Error::Revert(message.into())
}

pub struct XcmPrecompile<T>(PhantomData<T>);

impl<Runtime> Precompile for XcmPrecompile<Runtime>
where
	Runtime: crate::Config + pallet_revive::Config,
{
	type T = Runtime;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZero::new(10).unwrap());
	const HAS_CONTRACT_INFO: bool = false;
	type Interface = IXcm::IXcmCalls;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		let origin = env.caller();
		let frame_origin = match origin {
			Origin::Root => frame_system::RawOrigin::Root.into(),
			Origin::Signed(account_id) =>
				frame_system::RawOrigin::Signed(account_id.clone()).into(),
		};

		match input {
			IXcmCalls::xcmSend(IXcm::xcmSendCall { destination, message }) => {
				let _ = env.charge(<Runtime as Config>::WeightInfo::send())?;

				let final_destination = VersionedLocation::decode_all(&mut &destination[..])
					.map_err(|error| {
						revert(&error, "XCM send failed: Invalid destination format")
					})?;

				let final_message = VersionedXcm::<()>::decode_all_with_depth_limit(
					MAX_XCM_DECODE_DEPTH,
					&mut &message[..],
				)
				.map_err(|error| revert(&error, "XCM send failed: Invalid message format"))?;

				crate::Pallet::<Runtime>::send(
					frame_origin,
					final_destination.into(),
					final_message.into(),
				)
				.map(|message_id| message_id.encode())
				.map_err(|error| {
					revert(
						&error,
						"XCM send failed: destination or message format may be incompatible",
					)
				})
			},
			IXcmCalls::xcmExecute(IXcm::xcmExecuteCall { message, weight }) => {
				let max_weight = Weight::from_parts(weight.refTime, weight.proofSize);
				let weight_to_charge =
					max_weight.saturating_add(<Runtime as Config>::WeightInfo::execute());
				let charged_amount = env.charge(weight_to_charge)?;

				let final_message = VersionedXcm::decode_all_with_depth_limit(
					MAX_XCM_DECODE_DEPTH,
					&mut &message[..],
				)
				.map_err(|error| revert(&error, "XCM execute failed: Invalid message format"))?;

				let result = crate::Pallet::<Runtime>::execute(
					frame_origin,
					final_message.into(),
					max_weight,
				);

				let pre = DispatchInfo {
					call_weight: weight_to_charge,
					extension_weight: Weight::zero(),
					..Default::default()
				};

				// Adjust gas using actual weight or fallback to initially charged weight
				let actual_weight = frame_support::dispatch::extract_actual_weight(&result, &pre);
				env.adjust_gas(charged_amount, actual_weight);

				result.map(|post_dispatch_info| post_dispatch_info.encode()).map_err(|error| {
					revert(
							&error,
							"XCM execute failed: message may be invalid or execution constraints not satisfied"
						)
				})
			},
			IXcmCalls::weighMessage(IXcm::weighMessageCall { message }) => {
				let _ = env.charge(<Runtime as Config>::WeightInfo::weigh_message())?;

				let converted_message = VersionedXcm::decode_all_with_depth_limit(
					MAX_XCM_DECODE_DEPTH,
					&mut &message[..],
				)
				.map_err(|error| revert(&error, "XCM weightMessage: Invalid message format"))?;

				let mut final_message = converted_message.try_into().map_err(|error| {
					revert(&error, "XCM weightMessage: Conversion to Xcm failed")
				})?;

				let weight = <<Runtime>::Weigher>::weight(&mut final_message, Weight::MAX)
					.map_err(|error| {
						revert(&error, "XCM weightMessage: Failed to calculate weight")
					})?;

				let final_weight =
					IXcm::Weight { proofSize: weight.proof_size(), refTime: weight.ref_time() };

				Ok(final_weight.abi_encode())
			},
		}
	}
}

#[cfg(test)]
mod test {
	use crate::{
		mock::*,
		precompiles::IXcm::{self, weighMessageCall},
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
		DepositLimit,
	};
	use polkadot_parachain_primitives::primitives::Id as ParaId;
	use sp_runtime::traits::AccountIdConversion;
	use xcm::prelude::*;

	const BOB: AccountId = AccountId::new([1u8; 32]);
	const CHARLIE: AccountId = AccountId::new([2u8; 32]);
	const SEND_AMOUNT: u128 = 10;
	const CUSTOM_INITIAL_BALANCE: u128 = 100_000_000_000u128;

	#[test]
	fn test_xcm_send_precompile_works() {
		use codec::Encode;

		let balances = vec![
			(ALICE, CUSTOM_INITIAL_BALANCE),
			(ParaId::from(OTHER_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
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

			let xcm_send_params = IXcm::xcmSendCall {
				destination: versioned_dest.encode().into(),
				message: versioned_message.encode().into(),
			};
			let call = IXcm::IXcmCalls::xcmSend(xcm_send_params);
			let encoded_call = call.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(ALICE),
				xcm_precompile_addr,
				0u128,
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_call,
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
			(ParaId::from(OTHER_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
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

			let destination: VersionedLocation = Parachain(OTHER_PARA_ID).into();
			let versioned_message: VersionedXcm<()> = VersionedXcm::from(message.clone());

			let xcm_send_params = IXcm::xcmSendCall {
				destination: destination.encode().into(),
				message: versioned_message.encode().into(),
			};
			let call = IXcm::IXcmCalls::xcmSend(xcm_send_params);
			let encoded_call = call.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(ALICE),
				xcm_precompile_addr,
				0u128,
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_call,
			);

			assert!(result.result.is_ok());
			let sent_message = Xcm(Some(DescendOrigin(sender.clone().try_into().unwrap()))
				.into_iter()
				.chain(message.0.clone().into_iter())
				.collect());
			assert_eq!(sent_xcm(), vec![(Parachain(OTHER_PARA_ID).into(), sent_message)]);
		});
	}

	#[test]
	fn test_xcm_send_precompile_fails() {
		use codec::Encode;

		let balances = vec![
			(ALICE, CUSTOM_INITIAL_BALANCE),
			(ParaId::from(OTHER_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
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
			let versioned_message: VersionedXcm<RuntimeCall> = VersionedXcm::from(message);

			let xcm_send_params = IXcm::xcmSendCall {
				destination: destination.encode().into(),
				message: versioned_message.encode().into(),
			};
			let call = IXcm::IXcmCalls::xcmSend(xcm_send_params);
			let encoded_call = call.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(ALICE),
				xcm_precompile_addr,
				0u128,
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_call,
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
			(ParaId::from(OTHER_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
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
				0u128,
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_weight_call,
			);

			let weight_result = match xcm_weight_results.result {
				Ok(value) => value,
				Err(err) =>
					panic!("XcmExecutePrecompile Failed to decode weight with error {err:?}"),
			};

			let weight: IXcm::Weight = IXcm::Weight::abi_decode(&weight_result.data[..])
				.expect("XcmExecutePrecompile Failed to decode weight");

			let xcm_execute_params =
				IXcm::xcmExecuteCall { message: message.encode().into(), weight };
			let call = IXcm::IXcmCalls::xcmExecute(xcm_execute_params);
			let encoded_call = call.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(ALICE),
				xcm_precompile_addr,
				0u128,
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_call,
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
				0u128,
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_weight_call,
			);

			let weight_result = match xcm_weight_results.result {
				Ok(value) => value,
				Err(err) =>
					panic!("XcmExecutePrecompile Failed to decode weight with error: {err:?}"),
			};

			let weight: IXcm::Weight = IXcm::Weight::abi_decode(&weight_result.data[..])
				.expect("XcmExecutePrecompile Failed to decode weight");

			let xcm_execute_params =
				IXcm::xcmExecuteCall { message: message.encode().into(), weight };
			let call = IXcm::IXcmCalls::xcmExecute(xcm_execute_params);
			let encoded_call = call.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(ALICE),
				xcm_precompile_addr,
				0u128,
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_call,
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
				0u128,
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_weight_call,
			);

			let weight_result = match xcm_weight_results.result {
				Ok(value) => value,
				Err(err) =>
					panic!("XcmExecutePrecompile Failed to decode weight with error: {err:?}"),
			};

			let weight: IXcm::Weight = IXcm::Weight::abi_decode(&weight_result.data[..])
				.expect("XcmExecutePrecompile Failed to decode weight");

			let xcm_execute_params =
				IXcm::xcmExecuteCall { message: message.encode().into(), weight };
			let call = IXcm::IXcmCalls::xcmExecute(xcm_execute_params);
			let encoded_call = call.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(ALICE),
				xcm_precompile_addr,
				0u128,
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_call,
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
}
