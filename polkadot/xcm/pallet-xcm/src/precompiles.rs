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

use crate::{Call, PhantomData, VersionedXcm, VersionedLocation, Weight};
use alloc::vec::Vec;
use alloy::{sol_types::SolValue};
use pallet_revive::{precompiles::*, Origin};
use xcm_executor::traits::WeightBounds;
use tracing::log::error;
use codec::{DecodeAll, Encode};
use core::num::NonZero;

alloy::sol!("src/precompiles/IXcm.sol");
use IXcm::*;

/// XCM precompile.
pub struct Xcm<Runtime> {
	_phantom: PhantomData<Runtime>,
}

impl<Runtime> Precompile for Xcm<Runtime>
where
	Runtime: crate::Config + pallet_revive::Config,
	Call<Runtime>: Into<<Runtime as pallet_revive::Config>::RuntimeCall>,
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
				let final_destination = VersionedLocation::decode_all(&mut &destination[..])
					.map_err(|e| {
					error!("XCM send failed: Invalid destination format. Error: {e:?}");
					Error::Revert("Invalid destination format".into())
				})?;

				let final_message = VersionedXcm::<()>::decode_all(&mut &message[..])
					.map_err(|e| {
						error!("XCM send failed: Invalid message format. Error: {e:?}");
						Error::Revert("Invalid message format".into())
					})?;

				// let weight = <<T as Config>::SendController<_>>::WeightInfo::send();
				// env.gas_meter_mut().charge(RuntimeCosts::CallRuntime(weight))?; // TODO: Charge gas

				crate::Pallet::<Runtime>::send(
					frame_origin,
					final_destination.into(),
					final_message.into(),
				)
				.map(|message_id| message_id.encode())
				.map_err(|e| {
					error!(
						"XCM send failed: destination or message format may be incompatible. \
						Error: {e:?}"
					);
					Error::Revert(
						"XCM send failed: destination or message format may be incompatible".into(),
					)
				})
			},
			IXcmCalls::xcmExecute(IXcm::xcmExecuteCall { message, weight }) => {
				let final_message =
					VersionedXcm::decode_all(&mut &message[..]).map_err(|e| {
						error!("XCM execute failed: Invalid message format. Error: {e:?}");
						Error::Revert("Invalid message format".into())
					})?;

				let weight = Weight::from_parts(weight.refTime, weight.proofSize);
				// env.gas_meter_mut().charge(RuntimeCosts::CallXcmExecute(weight.clone()))?; // TODO: Charge gas

				crate::Pallet::<Runtime>::execute(
					frame_origin,
					final_message.into(),
					weight
				)
				.map(|results| results.encode())
				.map_err(|e| {
					error!(
						"XCM execute failed: message may be invalid or execution \
						constraints not satisfied. Error: {e:?}"
					);
					Error::Revert(
						"XCM execute failed: message may be invalid or execution \
						constraints not satisfied"
							.into(),
					)
				})
			},
			IXcmCalls::weightMessage(IXcm::weightMessageCall { message }) => {
				let converted_message =
					VersionedXcm::decode_all(&mut &message[..]).map_err(|error| {
						error!("XCM weightMessage: Invalid message format. Error: {error:?}");
						Error::Revert("XCM weightMessage: Invalid message format".into())
					})?;

				let mut final_message = converted_message.try_into().map_err(|e| {
					error!("XCM weightMessage: Conversion to Xcm failed with Error: {e:?}");
					Error::Revert("XCM weightMessage: Conversion to Xcm failed".into())
				})?;

				let weight =
					<<Runtime>::Weigher>::weight(&mut final_message).map_err(|e| {
						error!("XCM weightMessage: Failed to calculate weight. Error: {e:?}");
						Error::Revert("XCM weightMessage: Failed to calculate weight".into())
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
	use super::*;
	use crate::{
		mock::{
			new_test_ext, RuntimeOrigin, System, Test,
		},
		precompiles::alloy::hex,
	};
	use alloy::primitives::U256;
	use frame_support::{assert_ok, traits::Currency};
	use pallet_revive::DepositLimit;
	use sp_core::H160;
	use sp_runtime::Weight;

    pub const GAS_LIMIT: Weight = Weight::from_parts(100_000_000_000, 3 * 1024 * 1024);

    fn to_fixed_non_zero(precompile_id: u16) -> H160 {
        let mut address = [0u8; 20];
        address[16] = (precompile_id >> 8) as u8;
        address[17] = (precompile_id & 0xFF) as u8;
    
        H160::from(address)
    }

    #[test]
	fn weight_message_works() {
        new_test_ext().execute_with(|| {
            // Create a simple XCM message
            let message = VersionedXcm::V4(xcm::v4::Xcm::<()>(vec![
                xcm::v4::Instruction::ClearOrigin,
                xcm::v4::Instruction::DescendOrigin(xcm::v4::Junctions::Here),
            ]));

            // Encode the message for the precompile call
            let message_bytes = message.encode();

            let weight_params = IXcm::weightMessageCall { message: VersionedXcm::V4(message.clone()).encode().into() };
            let weight_call = IXcm::IXcmCalls::weightMessage(weight_params);
            // Call the precompile
            let xcm_weight_results = pallet_revive::Pallet::<Test>::bare_call(
                RuntimeOrigin::signed(1),
                to_fixed_non_zero(10), // XCM precompile address
                0u32.into(),
                GAS_LIMIT,
                DepositLimit::Balance(deposit_limit::<T>()),,
                weight_call,
            );

            let weight_result = match xcm_weight_results.result {
                Ok(value) => value,
                Err(_) => ExecReturnValue { flags: ReturnFlags::REVERT, data: Vec::new() },
            };

            let weight: IXcm::Weight =
			IXcm::Weight::abi_decode(&weight_result.data[..], true).expect("Failed to weight");

            // Verify the weight components
            assert!(weight.refTime > 0);
            assert!(weight.proofSize > 0);
        });
    }
}