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

use crate::{Config, ExecReturnValue, Origin};
use alloy_sol_types::{sol, SolCall, SolType};
use codec::{DecodeAll, Encode};
use pallet_revive_uapi::ReturnFlags;
use sp_runtime::Weight;
use xcm_builder::{ExecuteController, SendController};
use super::MutatingPrecompile;

pub struct XcmPrecompile;
#[derive(Debug, Clone, PartialEq)]
pub enum XcmFunctionSelector {
	XcmSend,
	XcmExecute,
	UnsupportedXcmSelector,
}

sol! {
	function xcmExecute(bytes);
	function xcmSend(bytes,bytes);
}

type DecodedType = sol!((bytes4, bytes, bytes));

impl XcmFunctionSelector {
	pub fn from_bytes(input: [u8; 4]) -> Self {
		match input {
			xcmExecuteCall::SELECTOR => Self::XcmExecute,
			xcmSendCall::SELECTOR => Self::XcmSend,
			_ => Self::UnsupportedXcmSelector,
		}
	}
}

impl<T: Config> MutatingPrecompile<T> for XcmPrecompile {
	fn execute(input: &[u8], origin: &Origin<T>) -> Result<ExecReturnValue, &'static str> {
		if input.len() < 4 {
			return Err("Input must be at least 4 bytes");
		}
		
		let decoded = DecodedType::abi_decode(input, false)
			.map_err(|_| "Failed to decode function parameters")?;
		
		let selector = XcmFunctionSelector::from_bytes(*decoded.0);
		
		let xcm_origin = match origin {
			Origin::Root => frame_system::RawOrigin::Root.into(),
			Origin::Signed(account_id) => frame_system::RawOrigin::Signed(account_id.clone()).into(),
		};
		
		match selector {
			XcmFunctionSelector::XcmSend => {
				let destination = xcm::VersionedLocation::decode_all(&mut &decoded.1[..])
					.map_err(|_| "Invalid destination format")?;
				let message = xcm::VersionedXcm::<()>::decode_all(&mut &decoded.2[..])
					.map_err(|_| "Invalid message format")?;

				<<T as Config>::Xcm>::send(xcm_origin, destination.into(), message.into())
					.map(|message_id| ExecReturnValue {
						flags: ReturnFlags::empty(),
						data: message_id.encode(),
					})
					.map_err(|_| "XCM send failed: destination or message format may be incompatible")
			},
			XcmFunctionSelector::XcmExecute => {
				let message = xcm::VersionedXcm::decode_all(&mut &decoded.1[..])
					.map_err(|_| "Invalid message format")?;
				
				// TODO: Calculate accurate weights instead of using Weight::MAX
				// Attempted approach below results in contract trapped errors:
				// let weight = <<T as Config>::Xcm as ExecuteController<_,_>>::WeightInfo::execute();
				let weight_limit = Weight::MAX;
				
				<<T as Config>::Xcm>::execute(xcm_origin, message.into(), weight_limit)
					.map(|results| ExecReturnValue {
						flags: ReturnFlags::empty(),
						data: results.encode(),
					})
					.map_err(|_| "XCM execute failed: message may be invalid or execution constraints not satisfied")
			},
			XcmFunctionSelector::UnsupportedXcmSelector => Err("Unsupported XCM function selector"),
		}
	}
}
mod tests {
	use super::*;
	use hex_literal::hex;

	#[test]
	fn test_xcm_selectors() {
		let cases = [
		(
			hex!("c0addb5500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000030501000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003404080004000000070010a5d4e80d0100000101000202020202020202020202020202020202020202020202020202020202020202000000000000000000000000"),
			XcmFunctionSelector::XcmSend,
		),
		(
			hex!("afceee6200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000030501000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003404080004000000070010a5d4e80d0100000101000202020202020202020202020202020202020202020202020202020202020202000000000000000000000000"),
			XcmFunctionSelector::XcmExecute,
		),
		(
			hex!("acceee6200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000030501000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003404080004000000070010a5d4e80d0100000101000202020202020202020202020202020202020202020202020202020202020202000000000000000000000000"),
			XcmFunctionSelector::UnsupportedXcmSelector,
		)
		];
		for (input, expected) in cases {
			let decoded = DecodedType::abi_decode(&input, false)
				.expect("Failed to decode precompiles::xcm::DecodedType data");
			let selector = XcmFunctionSelector::from_bytes(*decoded.0);
			assert_eq!(selector, expected, "Expected {:?} but got {:?}", expected, selector);
		}
	}
}