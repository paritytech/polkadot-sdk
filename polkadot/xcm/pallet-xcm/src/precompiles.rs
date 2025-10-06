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
use codec::{DecodeAll, DecodeLimit};
use core::{fmt, marker::PhantomData, num::NonZero};
use pallet_revive::{
	precompiles::{
		alloy::{self, sol_types::SolValue},
		AddressMatcher, Error, Ext, Precompile,
	},
	DispatchInfo, Origin,
};
use tracing::error;
use xcm::{v5, IdentifyVersion, MAX_XCM_DECODE_DEPTH};
use xcm_executor::traits::WeightBounds;

alloy::sol!("src/precompiles/IXcm.sol");
use IXcm::IXcmCalls;

const LOG_TARGET: &str = "xcm::precompiles";

fn revert(error: &impl fmt::Debug, message: &str) -> Error {
	error!(target: LOG_TARGET, ?error, "{}", message);
	Error::Revert(message.into())
}

// We don't allow XCM versions older than 5.
fn ensure_xcm_version<V: IdentifyVersion>(input: &V) -> Result<(), Error> {
	let version = input.identify_version();
	if version < v5::VERSION {
		return Err(Error::Revert("Only XCM version 5 and onwards are supported.".into()));
	}
	Ok(())
}

fn decode_xcm_message<Runtime>(
	message: &[u8],
) -> Result<VersionedXcm<<Runtime as crate::Config>::RuntimeCall>, Error>
where
	Runtime: crate::Config,
{
	VersionedXcm::decode_all_with_depth_limit(MAX_XCM_DECODE_DEPTH, &mut &message[..])
		.map_err(|error| revert(&error, "XCM execute failed: Invalid message format"))
}

fn weigh_xcm_message<Runtime>(
	message: &VersionedXcm<<Runtime as crate::Config>::RuntimeCall>,
) -> Result<Weight, Error>
where
	Runtime: crate::Config,
{
	let mut final_message = message
		.clone()
		.try_into()
		.map_err(|error| revert(&error, "XCM weighMessage: Conversion to Xcm failed"))?;

	<<Runtime>::Weigher>::weight(&mut final_message, Weight::MAX)
		.map_err(|error| revert(&error, "XCM weighMessage: Failed to calculate weight"))
}

fn execute_xcm_with_weight<Runtime>(
	env: &mut impl Ext<T = Runtime>,
	frame_origin: <Runtime as frame_system::Config>::RuntimeOrigin,
	message: VersionedXcm<<Runtime as crate::Config>::RuntimeCall>,
	max_weight: Weight,
	weight_to_charge: Weight,
) -> Result<Vec<u8>, Error>
where
	Runtime: crate::Config + pallet_revive::Config,
{
	let charged_amount = env.charge(weight_to_charge)?;

	let result = crate::Pallet::<Runtime>::execute(frame_origin, message.into(), max_weight);

	let pre = DispatchInfo {
		call_weight: weight_to_charge,
		extension_weight: Weight::zero(),
		..Default::default()
	};

	// Adjust gas using actual weight or fallback to initially charged weight
	let actual_weight = frame_support::dispatch::extract_actual_weight(&result, &pre);
	env.adjust_gas(charged_amount, actual_weight);

	result.map(|_| Vec::new()).map_err(|error| {
		revert(
			&error,
			"XCM execute failed: message may be invalid or execution constraints not satisfied",
		)
	})
}

fn get_frame_origin<Runtime>(
	origin: &Origin<Runtime>,
) -> <Runtime as frame_system::Config>::RuntimeOrigin
where
	Runtime: frame_system::Config + pallet_revive::Config,
{
	match origin {
		Origin::Root => frame_system::RawOrigin::Root.into(),
		Origin::Signed(account_id) => frame_system::RawOrigin::Signed(account_id.clone()).into(),
	}
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
		let frame_origin = get_frame_origin(&env.caller());

		match input {
			IXcmCalls::send(IXcm::sendCall { destination, message }) => {
				env.charge(<Runtime as Config>::WeightInfo::send())?;

				let final_destination = VersionedLocation::decode_all(&mut &destination[..])
					.map_err(|error| {
						revert(&error, "XCM send failed: Invalid destination format")
					})?;

				ensure_xcm_version(&final_destination)?;

				let final_message = VersionedXcm::<()>::decode_all_with_depth_limit(
					MAX_XCM_DECODE_DEPTH,
					&mut &message[..],
				)
				.map_err(|error| revert(&error, "XCM send failed: Invalid message format"))?;

				ensure_xcm_version(&final_message)?;

				crate::Pallet::<Runtime>::send(
					frame_origin,
					final_destination.into(),
					final_message.into(),
				)
				.map(|_| Vec::new())
				.map_err(|error| {
					revert(
						&error,
						"XCM send failed: destination or message format may be incompatible",
					)
				})
			},
			IXcmCalls::execute_0(IXcm::execute_0Call { message, weight }) => {
				let max_weight = Weight::from_parts(weight.refTime, weight.proofSize);
				let weight_to_charge =
					max_weight.saturating_add(<Runtime as Config>::WeightInfo::execute());

				let final_message = decode_xcm_message::<Runtime>(&message)?;
				ensure_xcm_version(&final_message)?;

				execute_xcm_with_weight(
					env,
					frame_origin,
					final_message,
					max_weight,
					weight_to_charge,
				)
			},
			IXcmCalls::execute_1(IXcm::execute_1Call { message }) => {
				env.charge(<Runtime as Config>::WeightInfo::weigh_message())?;

				let converted_message = decode_xcm_message::<Runtime>(&message)?;
				ensure_xcm_version(&converted_message)?;

				let max_weight = weigh_xcm_message::<Runtime>(&converted_message)?;
				let weight_to_charge =
					max_weight.saturating_add(<Runtime as Config>::WeightInfo::execute());

				execute_xcm_with_weight(
					env,
					frame_origin,
					converted_message,
					max_weight,
					weight_to_charge,
				)
			},
			IXcmCalls::executeAsAccount_0(IXcm::executeAsAccount_0Call { message, weight }) => {
				let max_weight = Weight::from_parts(weight.refTime, weight.proofSize);
				let weight_to_charge =
					max_weight.saturating_add(<Runtime as Config>::WeightInfo::execute());

				let final_message = decode_xcm_message::<Runtime>(&message)?;
				ensure_xcm_version(&final_message)?;

				let frame_origin = get_frame_origin(env.origin());

				execute_xcm_with_weight(
					env,
					frame_origin,
					final_message,
					max_weight,
					weight_to_charge,
				)
			},
			IXcmCalls::executeAsAccount_1(IXcm::executeAsAccount_1Call { message }) => {
				env.charge(<Runtime as Config>::WeightInfo::weigh_message())?;

				let converted_message = decode_xcm_message::<Runtime>(&message)?;
				ensure_xcm_version(&converted_message)?;

				let max_weight = weigh_xcm_message::<Runtime>(&converted_message)?;
				let weight_to_charge =
					max_weight.saturating_add(<Runtime as Config>::WeightInfo::execute());

				let frame_origin = get_frame_origin(env.origin());

				execute_xcm_with_weight(
					env,
					frame_origin,
					converted_message,
					max_weight,
					weight_to_charge,
				)
			},
			IXcmCalls::weighMessage(IXcm::weighMessageCall { message }) => {
				env.charge(<Runtime as Config>::WeightInfo::weigh_message())?;

				let converted_message = decode_xcm_message::<Runtime>(&message)?;
				ensure_xcm_version(&converted_message)?;

				let weight = weigh_xcm_message::<Runtime>(&converted_message)?;

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
				self, hex,
				sol_types::{SolInterface, SolValue},
			},
			H160,
		},
		test_utils::builder::{BareInstantiateBuilder, Contract},
		Code, DepositLimit, U256,
	};
	use polkadot_parachain_primitives::primitives::Id as ParaId;
	use sp_runtime::traits::AccountIdConversion;
	use xcm::{prelude::*, v3, v4};

	alloy::sol!("src/precompiles/fixtures/CallToXcmPrecompile.sol");

	const BOB: AccountId = AccountId::new([1u8; 32]);
	const CHARLIE: AccountId = AccountId::new([2u8; 32]);
	const SEND_AMOUNT: u128 = 10;
	const CUSTOM_INITIAL_BALANCE: u128 = 200_000_000_000_000u128;

	const CALL_TO_XCM_PRECOMPILE_PVM: &[u8] =
		include_bytes!("precompiles/fixtures/CallToXcmPrecompile.pvm");

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
	fn send_fails_on_old_location_version() {
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

			// V4 location is old and will fail.
			let destination: VersionedLocation =
				VersionedLocation::V4(v4::Junction::Parachain(OTHER_PARA_ID).into());
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
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_call,
			);
			let return_value = match result.result {
				Ok(value) => value,
				Err(err) => panic!("XcmSendPrecompile call failed with error: {err:?}"),
			};
			assert!(return_value.did_revert());

			// V3 also fails.
			let destination: VersionedLocation =
				VersionedLocation::V3(v3::Junction::Parachain(OTHER_PARA_ID).into());
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
	fn send_fails_on_old_xcm_version() {
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
			// V4 is old and fails.
			let v4_message: v4::Xcm<RuntimeCall> = message.try_into().unwrap();

			let destination: VersionedLocation = Parachain(OTHER_PARA_ID).into();
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
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_call,
			);
			let return_value = match result.result {
				Ok(value) => value,
				Err(err) => panic!("XcmSendPrecompile call failed with error: {err:?}"),
			};
			assert!(return_value.did_revert());

			// With V3 it also fails.
			let v3_message: v3::Xcm<RuntimeCall> = v4_message.try_into().unwrap();

			let destination: VersionedLocation = Parachain(OTHER_PARA_ID).into();
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
				U256::zero(),
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
				IXcm::execute_0Call { message: message.encode().into(), weight };
			let call = IXcm::IXcmCalls::execute_0(xcm_execute_params);
			let encoded_call = call.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(ALICE),
				xcm_precompile_addr,
				U256::zero(),
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
	fn test_simple_xcm_execute_precompile_works() {
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

			let xcm_execute_params = IXcm::execute_1Call { message: message.encode().into() };
			let call = IXcm::IXcmCalls::execute_1(xcm_execute_params);
			let encoded_call = call.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(ALICE),
				xcm_precompile_addr,
				U256::zero(),
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
				U256::zero(),
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
				IXcm::execute_0Call { message: message.encode().into(), weight };
			let call = IXcm::IXcmCalls::execute_0(xcm_execute_params);
			let encoded_call = call.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(ALICE),
				xcm_precompile_addr,
				U256::zero(),
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
				U256::zero(),
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
				IXcm::execute_0Call { message: message.encode().into(), weight };
			let call = IXcm::IXcmCalls::execute_0(xcm_execute_params);
			let encoded_call = call.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(ALICE),
				xcm_precompile_addr,
				U256::zero(),
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

	#[test]
	fn execute_fails_on_old_version() {
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
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_call,
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
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_call,
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
			(ParaId::from(OTHER_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
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
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_weight_call,
			);

			let result = match xcm_weight_results.result {
				Ok(value) => value,
				Err(err) =>
					panic!("XcmExecutePrecompile Failed to decode weight with error {err:?}"),
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
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_weight_call,
			);

			let result = match xcm_weight_results.result {
				Ok(value) => value,
				Err(err) =>
					panic!("XcmExecutePrecompile Failed to decode weight with error {err:?}"),
			};
			assert!(result.did_revert());
		});
	}

	#[test]
	fn test_xcm_execute_as_account_works() {
		use codec::Encode;

		let balances = vec![
			(ALICE, CUSTOM_INITIAL_BALANCE),
			(ParaId::from(OTHER_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
		];

		new_test_ext_with_balances(balances).execute_with(|| {
			let code = CALL_TO_XCM_PRECOMPILE_PVM.to_vec();

			let Contract { addr: contract_addr, .. } =
				BareInstantiateBuilder::<Test>::bare_instantiate(
					RuntimeOrigin::signed(ALICE),
					Code::Upload(code),
				)
				.storage_deposit_limit(DepositLimit::Balance(CUSTOM_INITIAL_BALANCE / 10))
				.build_and_unwrap_contract();

			let alice_balance_after_deployment = Balances::free_balance(ALICE);
			let bob_initial_balance = Balances::free_balance(BOB);

			let beneficiary: Location =
				Junction::AccountId32 { network: None, id: BOB.into() }.into();
			let transfer_amount = 1_000;
			let message: VersionedXcm<RuntimeCall> = VersionedXcm::from(Xcm(vec![
				WithdrawAsset((Here, transfer_amount).into()),
				buy_execution((Here, transfer_amount)),
				DepositAsset { assets: AllCounted(1).into(), beneficiary },
			]));

			let xcm_execute_as_acc_params = CallToXcmPrecompile::callExecuteAsAccount_1Call {
				message: message.encode().into(),
			};
			let call = CallToXcmPrecompile::CallToXcmPrecompileCalls::callExecuteAsAccount_1(
				xcm_execute_as_acc_params,
			);
			let encoded_call = call.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(ALICE),
				contract_addr,
				U256::zero(),
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_call,
			);

			assert!(result.result.is_ok());
			assert_eq!(
				Balances::free_balance(ALICE),
				alice_balance_after_deployment - transfer_amount,
			);
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
			(ParaId::from(OTHER_PARA_ID).into_account_truncating(), CUSTOM_INITIAL_BALANCE),
		];

		new_test_ext_with_balances(balances).execute_with(|| {
			let code = CALL_TO_XCM_PRECOMPILE_PVM.to_vec();

			// Alice deploys the contract that performs a cross-contract calls to the XCM precompile
			let Contract { addr: contract_addr, account_id: contract_account_id } =
				BareInstantiateBuilder::<Test>::bare_instantiate(
					RuntimeOrigin::signed(ALICE),
					Code::Upload(code),
				)
				.storage_deposit_limit(DepositLimit::Balance(CUSTOM_INITIAL_BALANCE / 10))
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

			let contract_balance_after_funding =
				Balances::free_balance(contract_account_id.clone());

			let beneficiary: Location =
				Junction::AccountId32 { network: None, id: BOB.into() }.into();

			let message: VersionedXcm<RuntimeCall> = VersionedXcm::from(Xcm(vec![
				WithdrawAsset((Here, ALICE_WITHDRAWAL_ATTEMPT).into()),
				buy_execution((Here, ALICE_WITHDRAWAL_ATTEMPT)),
				DepositAsset { assets: AllCounted(1).into(), beneficiary },
			]));

			let xcm_execute_as_acc_params = CallToXcmPrecompile::callExecuteAsAccount_1Call {
				message: message.encode().into(),
			};
			let call = CallToXcmPrecompile::CallToXcmPrecompileCalls::callExecuteAsAccount_1(
				xcm_execute_as_acc_params,
			);
			let encoded_call = call.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(ALICE),
				contract_addr,
				U256::zero(),
				Weight::MAX,
				DepositLimit::UnsafeOnlyForDryRun,
				encoded_call,
			);

			// This should fail because it uses Alice as the origin,
			// so Alice's insufficient balance causes the failure
			assert!(result.result.unwrap().did_revert());

			// Verify balances are unchanged after failed call
			assert_eq!(Balances::free_balance(ALICE), alice_balance_after_deployment);
			assert_eq!(Balances::free_balance(contract_account_id), contract_balance_after_funding);
		});
	}
}
