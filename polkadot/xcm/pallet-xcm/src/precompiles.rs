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
use core::{marker::PhantomData, num::NonZero};
use pallet_revive::{
	precompiles::{
		alloy::{self, sol_types::SolValue},
		AddressMatcher, Error, Ext, Precompile, RuntimeCosts,
	},
	DispatchInfo, Origin,
};
use tracing::error;
use xcm::MAX_XCM_DECODE_DEPTH;
use xcm_executor::traits::WeightBounds;

alloy::sol!("src/precompiles/IXcm.sol");
use IXcm::*;

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
						error!(target: "xcm::precompiles", ?error, "XCM send failed: Invalid destination format");
						Error::Revert("XCM send failed: Invalid destination format".into())
					})?;

				let final_message = VersionedXcm::<()>::decode_all_with_depth_limit(
					MAX_XCM_DECODE_DEPTH,
					&mut &message[..],
				)
				.map_err(|error| {
					error!(target: "xcm::precompiles", ?error, "XCM send failed: Invalid message format");
					Error::Revert("XCM send failed: Invalid message format".into())
				})?;

				crate::Pallet::<Runtime>::send(
					frame_origin,
					final_destination.into(),
					final_message.into(),
				)
				.map(|message_id| message_id.encode())
				.map_err(|error| {
					error!(
						target: "xcm::precompiles",
						?error,
						"XCM send failed: destination or message format may be incompatible"

					);
					Error::Revert(
						"XCM send failed: destination or message format may be incompatible".into(),
					)
				})
			},
			IXcmCalls::xcmExecute(IXcm::xcmExecuteCall { message, weight }) => {
				let weight = Weight::from_parts(weight.refTime, weight.proofSize);
				let charged_amount = env.charge(weight)?;

				let final_message = VersionedXcm::decode_all_with_depth_limit(
					MAX_XCM_DECODE_DEPTH,
					&mut &message[..],
				)
				.map_err(|error| {
					error!(target: "xcm::precompiles", ?error, "XCM execute failed: Invalid message format");
					Error::Revert("Invalid message format".into())
				})?;

				let result =
					crate::Pallet::<Runtime>::execute(frame_origin, final_message.into(), weight);

				let pre = DispatchInfo {
					call_weight: weight.clone(),
					extension_weight: Weight::zero(),
					..Default::default()
				};

				// Adjust gas using actual weight or fallback to initially charged weight
				let actual_weight = frame_support::dispatch::extract_actual_weight(&result, &pre);
				env.adjust_gas(charged_amount, RuntimeCosts::Precompile(actual_weight));

				result.map(|post_dispatch_info| post_dispatch_info.encode()).map_err(|error| {
					error!(
						target: "xcm::precompiles",
						?error,
						"XCM execute failed: message may be invalid or execution constraints not satisfied"
					);
					Error::Revert(
							"XCM execute failed: message may be invalid or execution constraints not satisfied"
								.into(),
						)
				})
			},
			IXcmCalls::weighMessage(IXcm::weighMessageCall { message }) => {
				let _ = env.charge(<Runtime as Config>::WeightInfo::weigh_message())?;

				let converted_message = VersionedXcm::decode_all_with_depth_limit(
					MAX_XCM_DECODE_DEPTH,
					&mut &message[..],
				)
				.map_err(|error| {
					error!(target: "xcm::precompiles", ?error, "XCM weightMessage: Invalid message format");
					Error::Revert("XCM weightMessage: Invalid message format".into())
				})?;

				let mut final_message = converted_message.try_into().map_err(|error| {
					error!(target: "xcm::precompiles", ?error, "XCM weightMessage: Conversion to Xcm failed");
					Error::Revert("XCM weightMessage: Conversion to Xcm failed".into())
				})?;

				let weight = <<Runtime>::Weigher>::weight(&mut final_message, Weight::MAX)
					.map_err(|error| {
						error!(target: "xcm::precompiles", ?error, "XCM weightMessage: Failed to calculate weight");
						Error::Revert("XCM weightMessage: Failed to calculate weight".into())
					})?;

				let final_weight =
					IXcm::Weight { proofSize: weight.proof_size(), refTime: weight.ref_time() };

				Ok(final_weight.abi_encode())
			},
		}
	}
}
