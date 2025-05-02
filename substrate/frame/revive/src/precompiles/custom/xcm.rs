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

use crate::Origin;
use crate::{
	precompiles::{AddressMatcher, Error, Ext, ExtWithInfo, Precompile},
	Config,
};
use alloc::vec::Vec;
use alloy_core::sol;
use codec::{DecodeAll, Encode};
use core::{marker::PhantomData, num::NonZero};
use log::trace;
use xcm_builder::{ExecuteController, ExecuteControllerWeightInfo, SendController};
pub use IXcm::IXcmCalls;

pub struct XCMPrecompile<T>(PhantomData<T>);

sol! {
    /// @title Defines all functions that can be used to interact with XCM
    /// @author brianspha
    /// @dev Parameters MUST use SCALE codec serialisation
    interface IXcm {
        /// @notice Execute an XCM message locally with the caller's origin
        /// @param message The XCM message to send
        function xcmExecute(bytes calldata message) external;

        /// @notice Send an XCM message to a destination chain
        /// @param destination The destination location, encoded according to the XCM format
        /// @param message The XCM message to send
        function xcmSend(bytes calldata destination, bytes calldata message) external;
    }
}

impl<T: Config> Precompile for XCMPrecompile<T> {
	type T = T;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZero::new(10).unwrap());
	// We don't need to set this to true since we are forwarding any state changing ops to the xcm pallet
	const HAS_CONTRACT_INFO: bool = false;
	type Interface = IXcm::IXcmCalls;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		let origin = env.origin();
		let xcm_origin = match origin {
			Origin::Root => frame_system::RawOrigin::Root.into(),
			Origin::Signed(account_id) => frame_system::RawOrigin::Signed(account_id.clone()).into(),
		};

		match input {
			IXcmCalls::xcmSend(IXcm::xcmSendCall { destination, message }) => {
				let final_destination =
					xcm::VersionedLocation::decode_all(&mut &destination[..])
						.map_err(|_| Error::Revert("Invalid destination format".into()))?;

				let final_message = xcm::VersionedXcm::<()>::decode_all(&mut &message[..])
					.map_err(|_| Error::Revert("Invalid message format".into()))?;

				<<T as Config>::Xcm>::send(
					xcm_origin,
					final_destination.into(),
					final_message.into(),
				)
				.map(|message_id| {
					trace!("message_id.encode(): {:?}", &message_id.encode());
					message_id.encode()
				})
				.map_err(|_| {
					Error::Revert(
						"XCM send failed: destination or message format may be incompatible".into(),
					)
				})
			},
			IXcmCalls::xcmExecute(IXcm::xcmExecuteCall { message }) => {
				let final_message = xcm::VersionedXcm::decode_all(&mut &message[..])
					.map_err(|_| Error::Revert("Invalid message format".into()))?;

				let weight_limit =
					<<T as Config>::Xcm as ExecuteController<_, _>>::WeightInfo::execute();
				
				<<T as Config>::Xcm>::execute(xcm_origin, final_message.into(), weight_limit)
					.map(|results| results.encode())
					.map_err(|_| Error::Revert(
						"XCM execute failed: message may be invalid or execution constraints not satisfied".into()
					))
			},
		}
	}

	fn call_with_info(
		_address: &[u8; 20],
		_input: &Self::Interface,
		_env: &mut impl ExtWithInfo<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		Err(Error::Revert("call_with_info not implemented for XCMPrecompile".into()))
	}
}