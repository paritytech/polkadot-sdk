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

use alloc::{boxed::Box, vec::Vec};
use codec::{Decode, DecodeAll, Encode, WrapperTypeDecode};

use core::{fmt, marker::PhantomData, num::NonZero};
use frame_support::{
	dispatch::RawOrigin,
	sp_runtime::traits::StaticLookup,
	traits::{schedule::DispatchTime, Bounded},
	traits::{Currency, Get, Polling},
};

use pallet_referenda::{BlockNumberFor, BoundedCallOf, Call, Config, ReferendumCount};
use pallet_revive::{
	frame_system,
	precompiles::{
		alloy::{self, sol_types::SolValue},
		AddressMatcher, Error, Ext, Precompile,
	},
	DispatchInfo, ExecOrigin, Weight,
};

use tracing::{error, info};
// use frame_support::dispatch::{ extract_actual_weight};
use frame_support::traits::OriginTrait;

alloy::sol!("src/interfaces/IReferenda.sol");
use IReferenda::IReferendaCalls;
// use sp_core::H256;
type BalanceOf<T> = <<T as pallet_referenda::Config>::Currency as Currency<
	<T as pallet_revive::frame_system::Config>::AccountId,
>>::Balance;
const LOG_TARGET: &str = "referenda::precompiles";
pub type RuntimeOriginFor<T> = <T as frame_system::Config>::RuntimeOrigin;

// ========== Private Helper Functions ==========

fn decode_proposal_origin<T>(origin_bytes: &[u8]) -> Result<RuntimeOriginFor<T>, Error>
where
	T: frame_system::Config,
	RuntimeOriginFor<T>: Decode,
{
	RuntimeOriginFor::<T>::decode(&mut &origin_bytes[..]).map_err(|e| {
		error!(target: LOG_TARGET, ?e, "Failed to decode proposal origin");
		Error::Revert("Invalid origin encoding".into())
	})
}

fn revert(error: &impl fmt::Debug, message: &str) -> Error {
	error!(target: LOG_TARGET, ?error, "{}", message);
	Error::Revert(message.into())
}
// Get the NEXT referendum index (the one that will be assigned)
fn get_next_referendum_index<T: Config>() -> u32 {
	ReferendumCount::<T>::get()
}
// Convert timing enum to DispatchTime
/// Convert timing enum to DispatchTime
fn convert_timing_to_dispatch<T, I>(
	timing: IReferenda::Timing,
	enactment_moment: u32,
) -> Result<DispatchTime<BlockNumberFor<T, I>>, Error>
where
	T: pallet_referenda::Config<I>,
	I: 'static,
{
	let moment: BlockNumberFor<T, I> = enactment_moment.into();

	match timing {
		IReferenda::Timing::AtBlock => Ok(DispatchTime::At(moment)),
		IReferenda::Timing::AfterBlock => Ok(DispatchTime::After(moment)),
		_ => Err(Error::Revert("Invalid timing variant".into())),
	}
}

/// Decode origin bytes into a RuntimeOrigin
/// This handles various origin types (Signed, Root, Custom)
fn decode_origin<T>(origin_bytes: &[u8]) -> Result<Box<RuntimeOriginFor<T>>, Error>
where
	T: frame_system::Config,
	RuntimeOriginFor<T>: Decode,
{
	let origin = RuntimeOriginFor::<T>::decode(&mut &origin_bytes[..]).map_err(|e| {
		error!(target: LOG_TARGET, ?e, "Failed to decode origin");
		Error::Revert("Invalid origin encoding".into())
	})?;

	Ok(Box::new(origin))
}

/// Dispatch a referenda submit call and extract actual weight
///
/// # Parameters
/// - `transaction_origin`: The origin calling the extrinsic (EVM caller)
/// - `proposal_origin`: The decoded proposal execution origin
/// - `proposal`: The proposal to submit (Lookup or Inline variant)
/// - `dispatch_time`: When to enact the proposal
///
/// # Returns
/// - `referendum_index`: The index of the created referendum
pub fn submit_dispatch<Runtime, Instance>(
	transaction_origin: <Runtime as frame_system::Config>::RuntimeOrigin,
	proposal_origin: <Runtime as frame_system::Config>::RuntimeOrigin,
	proposal: BoundedCallOf<Runtime, Instance>,
	dispatch_time: DispatchTime<BlockNumberFor<Runtime, Instance>>,
) -> Result<u32, Error>
where
	Runtime: pallet_referenda::Config<Instance>,
	Instance: 'static,
{
	// Extract the inner PalletsOrigin from RuntimeOrigin
	let pallets_origin = proposal_origin.caller().clone(); // Returns the INNER origin (PalletsOrigin)

	// Dispatch the call with the correct types
	let result = pallet_referenda::Pallet::<Runtime, Instance>::submit(
		transaction_origin,
		Box::new(pallets_origin),
		proposal,
		dispatch_time,
	);

	match result {
		Ok(_) => {
			let referendum_index =
				pallet_referenda::ReferendumCount::<Runtime, Instance>::get().saturating_sub(1);
			Ok(referendum_index)
		},
		Err(e) => {
			// e is DispatchErrorWithPostInfo<PostDispatchInfo>
			// e.error is the DispatchError
			Err(revert(&e, "Referenda submission failed"))
		},
	}
}
pub struct ReferendaPrecompile<T>(PhantomData<T>);
impl<Runtime> Precompile for ReferendaPrecompile<Runtime>
where
	Runtime: pallet_referenda::Config + pallet_revive::Config, //+ pallet_custom_origins::Config,
	RuntimeOriginFor<Runtime>: Decode + WrapperTypeDecode,     // Add this bound
{
	type T = Runtime;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZero::new(11).unwrap());
	const HAS_CONTRACT_INFO: bool = false;
	type Interface = IReferenda::IReferendaCalls;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		// READ INPUTS from EVM call data
		let exec_origin = env.caller();
		// match calls
		match input {
			IReferendaCalls::submitLookup(_) | IReferendaCalls::submitInline(_)
				if env.is_read_only() =>
			{
				Err(Error::Error(pallet_revive::Error::<Self::T>::StateChangeDenied.into()))
			},
			IReferendaCalls::submitLookup(IReferenda::submitLookupCall {
				origin,
				hash,
				preimageLength: preimage_length,
				timing,
				enactmentMoment: enactment_moment,
			}) => {
				info!(target: LOG_TARGET, ?origin, ?hash, ?preimage_length, ?enactment_moment, "submitLookup");

				// 1. Convert EVM caller to transaction origin
				let transaction_origin: RuntimeOriginFor<Runtime> = match &&exec_origin {
					ExecOrigin::Signed(account_id) => {
						frame_system::RawOrigin::Signed(account_id.clone()).into()
					},
					ExecOrigin::Root => frame_system::RawOrigin::Root.into(),
				};

				// 2. Charge gas
				env.charge(<<Runtime as pallet_referenda::Config>::WeightInfo  as pallet_referenda::WeightInfo>::submit())?;

				// 3. Decode proposal origin
				let proposal_origin = decode_proposal_origin::<Runtime>(&origin)?;

				// 4. Convert timing
				let dispatch_time =
					convert_timing_to_dispatch::<Runtime, ()>(*timing, *enactment_moment)?;

				// 5. Convert hash
				let hash_bytes: [u8; 32] = *hash.as_ref();
				let preimage_hash =
					<Runtime as frame_system::Config>::Hash::decode(&mut &hash_bytes[..])
						.map_err(|_| Error::Revert("Invalid hash format".into()))?;

				// 6. Build lookup proposal
				let proposal = BoundedCallOf::<Runtime, ()>::Lookup {
					hash: preimage_hash,
					len: *preimage_length,
				};

				// 7. Submit referendum and get the actual created index
				let referendum_index = submit_dispatch::<Runtime, ()>(
					transaction_origin,
					proposal_origin,
					proposal,
					dispatch_time,
				)?;
				Ok(referendum_index.abi_encode())
			},

			// TODO: Implement submitInline
			IReferendaCalls::submitInline(IReferenda::submitInlineCall {
				origin,
				proposal,
				timing,
				enactmentMoment: enactment_moment,
			}) => {
				// Placeholder implementation
				Err(Error::Revert("submitInline: Not yet implemented".into()))
			},
			// Add a catch-all for now
			_ => Ok(Vec::new()),
		}
	}
}
