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

use alloc::vec::Vec;
use alloy_core::sol;
use codec::{DecodeAll, Encode};
use core::{marker::PhantomData, num::NonZero};
use log::error;
use sp_runtime::Weight;
use xcm_builder::{ExecuteController, SendController};
use xcm_executor::traits::WeightBounds;
use crate::Origin;
use crate::{
	precompiles::{AddressMatcher, Error, Ext, ExtWithInfo, Precompile},
	Config,
};

pub use IXcm::IXcmCalls;

pub struct XcmPrecompile<T>(PhantomData<T>);

sol! {
	/// @title Defines all functions that can be used to interact with XCM
	/// @author Tiago Bandeira
	/// @dev Parameters MUST use SCALE codec serialisation
	interface IXcm {
		struct Weight {
			uint64 refTime;
			uint64 proofSize;
		}
		
		/// @notice Execute an XCM message locally with the caller's origin
		/// @param message The XCM message to send
		/// @param maxWeight The maximum amount of weight to be used to execute the message
		function xcmExecute(bytes calldata message, Weight calldata weight) external;

		/// @notice Send an XCM message to a destination chain
		/// @param destination The destination location, encoded according to the XCM format
		/// @param message The XCM message to send
		function xcmSend(bytes calldata destination, bytes calldata message) external;

		/// @notice Given a message estimate the weight cost
		/// @param message The XCM message to send
		/// @returns weight estimated for sending the message
		function weightMessage(bytes calldata message) external view returns(Weight weight);
	}
}

impl<T: Config> Precompile for XcmPrecompile<T> {
	type T = T;
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
			Origin::Signed(account_id) => {
				frame_system::RawOrigin::Signed(account_id.clone()).into()
			},
		};

		match input {
			IXcmCalls::xcmSend(IXcm::xcmSendCall { destination, message }) => {
				let final_destination = xcm::VersionedLocation::decode_all(&mut &destination[..])
					.map_err(|e| {
						error!("XCM send failed: Invalid destination format. Error: {e:?}");
						Error::Revert("Invalid destination format".into())
					})?;

				let final_message = xcm::VersionedXcm::<()>::decode_all(&mut &message[..])
					.map_err(|e| {
						error!("XCM send failed: Invalid message format. Error: {e:?}");
						Error::Revert("Invalid message format".into())
					})?;

				<<T as Config>::Xcm>::send(
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
						"XCM send failed: destination or message format may be incompatible"
							.into(),
					)
				})
			},
			IXcmCalls::xcmExecute(IXcm::xcmExecuteCall { message, weight }) => {
				let final_message = xcm::VersionedXcm::decode_all(&mut &message[..])
					.map_err(|e| {
						error!("XCM execute failed: Invalid message format. Error: {e:?}");
						Error::Revert("Invalid message format".into())
					})?;

				let weight = Weight::from_parts(weight.refTime, weight.proofSize);

				<<T as Config>::Xcm>::execute(frame_origin, final_message.into(), weight)
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
				let converted_message = xcm::VersionedXcm::decode_all(&mut &message[..])
					.map_err(|error| {
						error!("XCM weightMessage: Invalid message format. Error: {error:?}");
						Error::Revert("XCM weightMessage: Invalid message format".into())
					})?;
					
				let mut final_message = converted_message.try_into().map_err(|e| {
					error!("XCM weightMessage: Conversion to Xcm failed with Error: {e:?}");
					Error::Revert("XCM weightMessage: Conversion to Xcm failed".into())
				})?;
				
				let weight = <<T as Config>::XcmWeigher>::weight(&mut final_message)
					.map_err(|e| {
						error!("XCM weightMessage: Failed to calculate weight. Error: {e:?}");
						Error::Revert("XCM weightMessage: Failed to calculate weight".into())
					})?;

				Ok(weight.encode())
			},
		}
	}

	fn call_with_info(
		_address: &[u8; 20],
		_input: &Self::Interface,
		_env: &mut impl ExtWithInfo<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		Err(Error::Revert(
			"call_with_info not implemented for XcmPrecompile".into(),
		))
	}
}