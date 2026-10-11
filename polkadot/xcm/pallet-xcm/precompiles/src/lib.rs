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
use codec::{Decode, DecodeAll};
use core::{fmt, marker::PhantomData, num::NonZero};
use frame_support::{dispatch::RawOrigin, traits::PalletInfoAccess};
use pallet_revive::{
	precompiles::{
		alloy::{self, sol_types::SolValue},
		ensure_not_delegate_call, ensure_not_read_only, AddressMatcher, Error, Ext, Precompile,
	},
	DispatchInfo, ExecOrigin as Origin, Weight,
};
use pallet_xcm::{Config, WeightInfo};
use tracing::debug;
use xcm::{v5, IdentifyVersion, VersionedLocation, VersionedXcm};
use xcm_executor::traits::WeightBounds;

alloy::sol!("src/interface/IXcm.sol");
use IXcm::IXcmCalls;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "xcm::precompiles";
const ERR_UNEXPECTED: &str = "unexpected error";

fn revert(error: &impl fmt::Debug, message: impl fmt::Display) -> Error {
	let reason = alloc::string::ToString::to_string(&message);
	debug!(target: LOG_TARGET, ?error, "{reason}");
	Error::Revert(reason.into())
}

fn xcm_dispatch_reason<Runtime: pallet_xcm::Config>(
	e: frame_support::sp_runtime::DispatchError,
) -> alloc::string::String {
	use frame_support::sp_runtime::DispatchError;
	match e {
		DispatchError::Token(token) => <&'static str>::from(token).into(),
		DispatchError::Arithmetic(arith) => <&'static str>::from(arith).into(),
		DispatchError::Other(msg) => msg.into(),
		DispatchError::Module(module) => match decode_xcm_error::<Runtime>(e) {
			Some(err) => xcm_pallet_reason(err),
			None => module.message.unwrap_or(ERR_UNEXPECTED).into(),
		},
		_ => ERR_UNEXPECTED.into(),
	}
}

fn decode_xcm_error<Runtime: pallet_xcm::Config>(
	e: frame_support::sp_runtime::DispatchError,
) -> Option<pallet_xcm::Error<Runtime>> {
	use frame_support::sp_runtime::DispatchError;
	let DispatchError::Module(module) = e else { return None };
	let index = <pallet_xcm::Pallet<Runtime> as PalletInfoAccess>::index() as u8;
	if module.index != index {
		return None;
	}
	pallet_xcm::Error::<Runtime>::decode(&mut &module.error[..]).ok()
}

#[allow(deprecated)]
fn xcm_pallet_reason<T>(err: pallet_xcm::Error<T>) -> alloc::string::String {
	use pallet_xcm::Error::*;
	match err {
		Unreachable => "destination is unreachable".into(),
		SendFailure => "message could not be sent".into(),
		Filtered => "message was filtered".into(),
		UnweighableMessage => "message weight could not be determined".into(),
		DestinationNotInvertible => "destination cannot be inverted".into(),
		Empty => "assets to send are empty".into(),
		CannotReanchor => "could not re-anchor assets".into(),
		TooManyAssets => "too many assets".into(),
		InvalidOrigin => "origin is invalid for sending".into(),
		BadVersion => "XCM version cannot be interpreted".into(),
		BadLocation => "location cannot be expressed".into(),
		NoSubscription => "subscription was not found".into(),
		AlreadySubscribed => "location is already subscribed".into(),
		CannotCheckOutTeleport => "could not check out assets for teleport".into(),
		LowBalance => "insufficient asset balance".into(),
		TooManyLocks => "too many asset locks".into(),
		AccountNotSovereign => "account is not a sovereign account".into(),
		FeesNotMet => "fees could not be paid".into(),
		LockNotFound => "remote lock was not found".into(),
		InUse => "lock still has consumers".into(),
		InvalidAssetUnknownReserve => "reserve chain could not be determined".into(),
		InvalidAssetUnsupportedReserve => {
			"remote reserve with a different fee reserve is not supported".into()
		},
		TooManyReserves => "too many reserve locations".into(),
		LocalExecutionIncomplete => "local execution incomplete".into(),
		TooManyAuthorizedAliases => "too many authorized aliases".into(),
		ExpiresInPast => "expiry block is in the past".into(),
		AliasNotFound => "alias authorization was not found".into(),
		LocalExecutionIncompleteWithError { index, error } => alloc::format!(
			"local execution incomplete at instruction {index}: {}",
			execution_error_reason(error),
		),
	}
}

fn execution_error_reason(error: pallet_xcm::ExecutionError) -> &'static str {
	use pallet_xcm::ExecutionError::*;
	match error {
		Overflow => "Overflow",
		Unimplemented => "Unimplemented",
		UntrustedReserveLocation => "UntrustedReserveLocation",
		UntrustedTeleportLocation => "UntrustedTeleportLocation",
		LocationFull => "LocationFull",
		LocationNotInvertible => "LocationNotInvertible",
		BadOrigin => "BadOrigin",
		InvalidLocation => "InvalidLocation",
		AssetNotFound => "AssetNotFound",
		FailedToTransactAsset => "FailedToTransactAsset",
		NotWithdrawable => "NotWithdrawable",
		LocationCannotHold => "LocationCannotHold",
		ExceedsMaxMessageSize => "ExceedsMaxMessageSize",
		DestinationUnsupported => "DestinationUnsupported",
		Transport => "Transport",
		Unroutable => "Unroutable",
		UnknownClaim => "UnknownClaim",
		FailedToDecode => "FailedToDecode",
		MaxWeightInvalid => "MaxWeightInvalid",
		NotHoldingFees => "NotHoldingFees",
		TooExpensive => "TooExpensive",
		Trap => "Trap",
		ExpectationFalse => "ExpectationFalse",
		PalletNotFound => "PalletNotFound",
		NameMismatch => "NameMismatch",
		VersionIncompatible => "VersionIncompatible",
		HoldingWouldOverflow => "HoldingWouldOverflow",
		ExportError => "ExportError",
		ReanchorFailed => "ReanchorFailed",
		NoDeal => "NoDeal",
		FeesNotMet => "FeesNotMet",
		LockError => "LockError",
		NoPermission => "NoPermission",
		Unanchored => "Unanchored",
		NotDepositable => "NotDepositable",
		TooManyAssets => "TooManyAssets",
		UnhandledXcmVersion => "UnhandledXcmVersion",
		WeightLimitReached => "WeightLimitReached",
		Barrier => "Barrier",
		WeightNotComputable => "WeightNotComputable",
		ExceedsStackLimit => "ExceedsStackLimit",
	}
}

// We don't allow XCM versions older than 5.
fn ensure_xcm_version<V: IdentifyVersion>(input: &V) -> Result<(), Error> {
	let version = input.identify_version();
	if version < v5::VERSION {
		return Err(Error::Revert("Only XCM version 5 and onwards are supported.".into()));
	}
	Ok(())
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
		ensure_not_delegate_call::<Runtime>(env)?;

		let origin = env.caller();
		let frame_origin = match origin {
			Origin::Root => RawOrigin::Root.into(),
			Origin::Signed(account_id) => RawOrigin::Signed(account_id.clone()).into(),
		};

		match input {
			IXcmCalls::send(_) | IXcmCalls::execute(_) if env.is_read_only() => {
				ensure_not_read_only::<Runtime>(env)?;
				Ok(Vec::new())
			},
			IXcmCalls::send(IXcm::sendCall { destination, message }) => {
				// Charged before decoding; see `WeightInfo::decode_xcm`.
				env.charge(
					<Runtime as Config>::WeightInfo::decode_xcm(
						message.len().saturating_add(destination.len()) as u32,
					)
					.saturating_add(<Runtime as Config>::WeightInfo::send()),
				)?;

				let final_destination = VersionedLocation::decode_all(&mut &destination[..])
					.map_err(|error| {
						revert(&error, "XCM send failed: Invalid destination format")
					})?;

				ensure_xcm_version(&final_destination)?;

				let final_message =
					VersionedXcm::<()>::decode_all_with_mem_and_depth_limit(&mut &message[..])
						.map_err(|error| {
							revert(&error, "XCM send failed: Invalid message format")
						})?;

				ensure_xcm_version(&final_message)?;

				pallet_xcm::Pallet::<Runtime>::send(
					frame_origin,
					final_destination.into(),
					final_message.into(),
				)
				.map(|_| Vec::new())
				.map_err(|error| {
					revert(
						&error,
						alloc::format!(
							"XCM send failed: {}",
							xcm_dispatch_reason::<Runtime>(error)
						),
					)
				})
			},
			IXcmCalls::execute(IXcm::executeCall { message, weight }) => {
				// Executing weighs the blob too, via `prepare`. Kept separate from the execution
				// charge below, which gets refunded and would otherwise give this back.
				env.charge(<Runtime as Config>::WeightInfo::weigh_message(message.len() as u32))?;

				let max_weight = Weight::from_parts(weight.refTime, weight.proofSize);
				let weight_to_charge =
					max_weight.saturating_add(<Runtime as Config>::WeightInfo::execute());
				let charged_amount = env.charge(weight_to_charge)?;

				let final_message = VersionedXcm::decode_all_with_mem_and_depth_limit(
					&mut &message[..],
				)
				.map_err(|error| revert(&error, "XCM execute failed: Invalid message format"))?;

				ensure_xcm_version(&final_message)?;

				let result = pallet_xcm::Pallet::<Runtime>::execute(
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

				result.map(|_| Vec::new()).map_err(|error| {
					revert(
						&error,
						alloc::format!(
							"XCM execute failed: {}",
							xcm_dispatch_reason::<Runtime>(error.error),
						),
					)
				})
			},
			IXcmCalls::weighMessage(IXcm::weighMessageCall { message }) => {
				// Charged before decoding; see `WeightInfo::weigh_message`.
				env.charge(<Runtime as Config>::WeightInfo::weigh_message(message.len() as u32))?;

				let converted_message = VersionedXcm::decode_all_with_mem_and_depth_limit(
					&mut &message[..],
				)
				.map_err(|error| revert(&error, "XCM weightMessage: Invalid message format"))?;

				ensure_xcm_version(&converted_message)?;

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
