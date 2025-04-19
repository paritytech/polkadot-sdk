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

//! XCM Precompile
//!
//! This module provides an EVM compatible precompile for interacting with XCM
//!
//! The precompile supports two main operations:
//! - `xcmSend`: Send an XCM message to a specified destination
//! - `xcmExecute`: Execute an XCM message locally
//!
//! These functions can be called from Solidity contracts using standard ABI encoding.

use super::MutatingPrecompile;
use crate::{Config, ExecReturnValue, Origin};
use alloy_sol_types::{sol, SolCall, SolType};
use codec::{DecodeAll, Encode};
use pallet_revive_uapi::ReturnFlags;
use xcm_builder::{ExecuteController, ExecuteControllerWeightInfo, SendController};

/// The XcmPrecompile provides EVM precompile functions for interacting with the XCM.
pub struct XcmPrecompile;

/// The XcmFunctionSelector enum defines the supported function selectors for XCM operations.
///
/// When a smart contract calls the XCM precompile, the first 4 bytes of the calldata
/// (the function selector) determine which operation should be performed. This enum
/// maps those selectors to their corresponding operations.
///
/// The function selectors are derived from the keccak256 hash of the function signatures
/// according to the Solidity ABI specification.
#[derive(Debug, Clone, PartialEq)]
pub enum XcmFunctionSelector {
	/// Selector for the xcmSend function, which sends an XCM message to a remote location
	XcmSend,
	/// Selector for the xcmExecute function, which executes an XCM message locally
	XcmExecute,
	/// Represents an unsupported or invalid function selector
	UnsupportedXcmSelector,
}

sol! {
	// Execute an XCM message locally with the caller's origin
	// @param message The XCM message to execute, encoded according to the XCM format
	function xcmExecute(bytes);
	
	// Send an XCM message to a destination chain
	// @param destination The destination location, encoded according to the XCM format
	// @param message The XCM message to send, encoded according to the XCM format
	function xcmSend(bytes,bytes);
}

/// Type alias for the decoded function call data format.
///
/// This represents the structure of the calldata after ABI decoding:
/// - bytes4: Function selector (first 4 bytes)
/// - bytes: First parameter (destination for xcmSend, message for xcmExecute)
/// - bytes: Second parameter (message for xcmSend, unused for xcmExecute)
type DecodedType = sol!((bytes4, bytes, bytes));

impl XcmFunctionSelector {
	/// Converts raw function selector bytes into the corresponding XcmFunctionSelector.
	///
	/// This function maps the 4-byte function selectors from Solidity ABI encoding
	/// to the appropriate XCM function variant.
	///
	/// # Parameters
	/// * `input` - The 4-byte function selector from the calldata
	///
	/// # Returns
	/// The corresponding XcmFunctionSelector or UnsupportedXcmSelector if not recognized
	pub fn from_bytes(input: [u8; 4]) -> Self {
		match input {
			xcmExecuteCall::SELECTOR => Self::XcmExecute,
			xcmSendCall::SELECTOR => Self::XcmSend,
			_ => Self::UnsupportedXcmSelector,
		}
	}
}

impl<T: Config> MutatingPrecompile<T> for XcmPrecompile {
	/// Executes the XCM precompile with the given input and origin.
	///
	/// This function implements the core logic of the XCM precompile, handling both
	/// xcmSend and xcmExecute operations based on the function selector in the input.
	///
	/// # Parameters
	/// * `input` - The raw calldata from the EVM call, ABI-encoded according to Solidity conventions
	/// * `origin` - The caller's origin within the Substrate runtime
	///
	/// # Returns
	/// * `Ok(ExecReturnValue)` - On successful execution, returns appropriate data:
	///   - For xcmSend: The message ID of the sent message
	///   - For xcmExecute: The execution results i.e Weight
	/// * `Err(&'static str)` - On failure, returns an error message describing the issue
	fn execute(input: &[u8], origin: &Origin<T>) -> Result<ExecReturnValue, &'static str> {
		if input.len() < 4 {
			return Err("Input must be at least 4 bytes");
		}

		let decoded = DecodedType::abi_decode(input, false)
			.map_err(|_| "Failed to decode function parameters")?;

		let selector = XcmFunctionSelector::from_bytes(*decoded.0);

		let xcm_origin = match origin {
			Origin::Root => frame_system::RawOrigin::Root.into(),
			Origin::Signed(account_id) => {
				frame_system::RawOrigin::Signed(account_id.clone()).into()
			},
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
					.map_err(|_| {
						"XCM send failed: destination or message format may be incompatible"
					})
			},
			XcmFunctionSelector::XcmExecute => {
				let message = xcm::VersionedXcm::decode_all(&mut &decoded.1[..])
					.map_err(|_| "Invalid message format")?;

				let weight_limit =
					<<T as Config>::Xcm as ExecuteController<_, _>>::WeightInfo::execute();
				<<T as Config>::Xcm>::execute(xcm_origin, message.into(), weight_limit)
					.map(|results| ExecReturnValue {
						flags: ReturnFlags::empty(),
						data: results.encode(),
					})
					.map_err(|_| {
						"XCM execute failed: message may be invalid or execution constraints not satisfied"
					})
			},
			XcmFunctionSelector::UnsupportedXcmSelector => Err("Unsupported XCM function selector"),
		}
	}
}


mod tests {
	#[warn(unused_imports)]
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
