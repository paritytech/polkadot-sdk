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

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{DecodeAll, DecodeLimit};
use core::{fmt, marker::PhantomData, num::NonZero};
use frame_support::dispatch::RawOrigin;
use pallet_revive::{
	precompiles::{
		alloy::{self, sol_types::SolValue},
		AddressMatcher, Error, Ext, Precompile,
	},
	DispatchInfo, ExecOrigin as Origin, Weight,
};
use pallet_xcm::{Config, Pallet, WeightInfo};
use tracing::error;
use xcm::{v5, IdentifyVersion, VersionedLocation, VersionedXcm, MAX_XCM_DECODE_DEPTH};
use xcm_executor::traits::WeightBounds;

alloy::sol!("src/interface/IXcm.sol");
use IXcm::IXcmCalls;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

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
		let origin = env.caller();
		let frame_origin = match origin {
			Origin::Root => RawOrigin::Root.into(),
			Origin::Signed(account_id) => RawOrigin::Signed(account_id.clone()).into(),
		};

		match input {
			IXcmCalls::send(_) | IXcmCalls::execute(_) if env.is_read_only() =>
				Err(Error::Error(pallet_revive::Error::<Self::T>::StateChangeDenied.into())),
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
