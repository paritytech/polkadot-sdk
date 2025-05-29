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

use crate::{Call, Config, VersionedLocation, VersionedXcm, Weight, WeightInfo};
use alloc::vec::Vec;
use alloy::sol_types::SolValue;
use codec::{DecodeAll, Encode};
use core::{marker::PhantomData, num::NonZero};
use pallet_revive::{precompiles::*, Origin};
use tracing::log::error;
use xcm_executor::traits::WeightBounds;

alloy::sol!("src/precompiles/IXcm.sol");
use IXcm::*;

pub struct XcmPrecompile<T>(PhantomData<T>);

impl<Runtime> Precompile for XcmPrecompile<Runtime>
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
				let weight = <Runtime as Config>::WeightInfo::send();
				let _ = env.gas_meter_mut().charge(RuntimeCosts::CallRuntime(weight));

				let final_destination = VersionedLocation::decode_all(&mut &destination[..])
					.map_err(|e| {
						error!("XCM send failed: Invalid destination format. Error: {e:?}");
						Error::Revert("Invalid destination format".into())
					})?;

				let final_message =
					VersionedXcm::<()>::decode_all(&mut &message[..]).map_err(|e| {
						error!("XCM send failed: Invalid message format. Error: {e:?}");
						Error::Revert("Invalid message format".into())
					})?;

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
				let final_message = VersionedXcm::decode_all(&mut &message[..]).map_err(|e| {
					error!("XCM execute failed: Invalid message format. Error: {e:?}");
					Error::Revert("Invalid message format".into())
				})?;

				let weight = Weight::from_parts(weight.refTime, weight.proofSize);
				let _ = env.gas_meter_mut().charge(RuntimeCosts::CallRuntime(weight));

				crate::Pallet::<Runtime>::execute(frame_origin, final_message.into(), weight)
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
			IXcmCalls::weighMessage(IXcm::weighMessageCall { message }) => {
				let converted_message =
					VersionedXcm::decode_all(&mut &message[..]).map_err(|error| {
						error!("XCM weightMessage: Invalid message format. Error: {error:?}");
						Error::Revert("XCM weightMessage: Invalid message format".into())
					})?;

				let mut final_message = converted_message.try_into().map_err(|e| {
					error!("XCM weightMessage: Conversion to Xcm failed with Error: {e:?}");
					Error::Revert("XCM weightMessage: Conversion to Xcm failed".into())
				})?;

				let weight = <<Runtime>::Weigher>::weight(&mut final_message).map_err(|e| {
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
