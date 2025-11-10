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
use codec::Decode;

use core::{convert::TryFrom, fmt, marker::PhantomData, num::NonZero};
use frame_support::traits::{schedule::DispatchTime, BoundedInline, Get};
use sp_runtime::traits::SaturatedConversion;

use pallet_referenda::{
	BlockNumberFor, BoundedCallOf, PalletsOriginOf, ReferendumCount, ReferendumInfoFor, TracksInfo,
};
use pallet_revive::{
	frame_system,
	precompiles::{
		alloy::{self, sol_types::SolValue},
		AddressMatcher, Error, Ext, Precompile,
	},
	ExecOrigin,
};

use tracing::{error, info};

alloy::sol!("src/interfaces/IReferenda.sol");
use frame_support::{dispatch::DispatchInfo, weights::Weight};
use IReferenda::IReferendaCalls;
const LOG_TARGET: &str = "referenda::precompiles";
pub type RuntimeOriginFor<T> = <T as frame_system::Config>::RuntimeOrigin;
pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

// ========== Private Helper Functions ==========

fn decode_proposal_origin<T>(origin_bytes: &[u8]) -> Result<PalletsOriginOf<T>, Error>
where
	T: frame_system::Config,
	PalletsOriginOf<T>: Decode,
{
	PalletsOriginOf::<T>::decode(&mut &origin_bytes[..]).map_err(|e| {
		error!(target: LOG_TARGET, ?e, "Failed to decode proposal origin");
		Error::Revert("Referenda Precompile: Invalid origin encoding".into())
	})
}

fn revert(error: &impl fmt::Debug, message: &str) -> Error {
	error!(target: LOG_TARGET, ?error, "{}", message);
	Error::Revert(message.into())
}

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
		_ => Err(Error::Revert("Referenda Precompile: Invalid timing variant".into())),
	}
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
	proposal_origin: PalletsOriginOf<Runtime>,
	proposal: BoundedCallOf<Runtime, Instance>,
	dispatch_time: DispatchTime<BlockNumberFor<Runtime, Instance>>,
) -> Result<u32, Error>
where
	Runtime: pallet_referenda::Config<Instance>,
	Instance: 'static,
{
	// Dispatch the call with the correct types
	let result = pallet_referenda::Pallet::<Runtime, Instance>::submit(
		transaction_origin,
		Box::new(proposal_origin),
		proposal,
		dispatch_time,
	);

	match result {
		Ok(_) => {
			let referendum_index = ReferendumCount::<Runtime, Instance>::get().saturating_sub(1);
			Ok(referendum_index)
		},
		Err(e) => Err(revert(&e, "Referenda Precompile: Submission failed")),
	}
}

pub struct ReferendaPrecompile<T>(PhantomData<T>);

impl<Runtime> Precompile for ReferendaPrecompile<Runtime>
where
	Runtime: pallet_referenda::Config + pallet_revive::Config,
	PalletsOriginOf<Runtime>: Decode,
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
			IReferendaCalls::submitLookup(_)
			| IReferendaCalls::submitInline(_)
			| IReferendaCalls::placeDecisionDeposit(_)
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

				// 2. Charge gas (worst-case weight for submitLookup)
				env.charge(<crate::weights::SubstrateWeight<Runtime> as WeightInfo>::submit_lookup_worst_case())?;

				// 3. Decode proposal origin
				let proposal_origin = decode_proposal_origin::<Runtime>(&origin)?;

				// 4. Convert timing
				let dispatch_time =
					convert_timing_to_dispatch::<Runtime, ()>(*timing, *enactment_moment)?;

				// 5. Convert hash
				let hash_bytes: [u8; 32] = *hash.as_ref();
				let preimage_hash =
					<Runtime as frame_system::Config>::Hash::decode(&mut &hash_bytes[..])
						.map_err(|_| Error::Revert("Referenda Precompile: Invalid hash format".into()))?;

				// 6. Build lookup proposal
				let proposal = BoundedCallOf::<Runtime, ()>::Lookup {
					hash: preimage_hash,
					len: *preimage_length,
				};

				// 7. Submit referendum
				let referendum_index = submit_dispatch::<Runtime, ()>(
					transaction_origin,
					proposal_origin,
					proposal,
					dispatch_time,
				)?;
				Ok(referendum_index.abi_encode())
			},

			IReferendaCalls::submitInline(IReferenda::submitInlineCall {
				origin,
				proposal,
				timing,
				enactmentMoment: enactment_moment,
			}) => {
				info!(target: LOG_TARGET, ?origin,  ?enactment_moment, "submitInline");

				// 1. Convert EVM caller to transaction origin
				let transaction_origin: RuntimeOriginFor<Runtime> = match &&exec_origin {
					ExecOrigin::Signed(account_id) => {
						frame_system::RawOrigin::Signed(account_id.clone()).into()
					},
					ExecOrigin::Root => frame_system::RawOrigin::Root.into(),
				};

				// 2. Charge gas (worst-case weight for submitInline)
				env.charge(<crate::weights::SubstrateWeight<Runtime> as WeightInfo>::submit_inline_worst_case())?;

				// 3. Decode proposal origin
				let proposal_origin = decode_proposal_origin::<Runtime>(&origin)?;

				// 4. Convert timing
				let dispatch_time =
					convert_timing_to_dispatch::<Runtime, ()>(*timing, *enactment_moment)?;

				// 5. Build inline proposal
				// Convert Vec<u8> to BoundedInline (max 128 bytes)
				let bounded_proposal = BoundedInline::try_from(proposal.to_vec())
					.map_err(|_| Error::Revert("Referenda Precompile: Proposal exceeds 128 byte limit".into()))?;
				let proposal_inline = BoundedCallOf::<Runtime, ()>::Inline(bounded_proposal);

				// 6. Submit referendum
				let referendum_index = submit_dispatch::<Runtime, ()>(
					transaction_origin,
					proposal_origin,
					proposal_inline,
					dispatch_time,
				)?;
				Ok(referendum_index.abi_encode())
			},
			IReferendaCalls::placeDecisionDeposit(IReferenda::placeDecisionDepositCall {
				referendumIndex: index,
			}) => {
				info!(target: LOG_TARGET, ?index, "placeDecisionDeposit");
				// 1. Convert EVM caller to transaction origin
				let origin: RuntimeOriginFor<Runtime> = match &&exec_origin {
					ExecOrigin::Signed(account_id) => {
						frame_system::RawOrigin::Signed(account_id.clone()).into()
					},
					ExecOrigin::Root => frame_system::RawOrigin::Root.into(),
				};

				// 2. Pre-charge worst-case weight
				let weight_to_charge =
					<crate::weights::SubstrateWeight<Runtime> as WeightInfo>::place_decision_deposit_worst_case();
				let charged_amount = env.charge(weight_to_charge)?;

				// 3. Place deposit
				let result =
					pallet_referenda::Pallet::<Runtime>::place_decision_deposit(origin, *index);

				// 4. Extract actual weight and adjust gas (refund unused)
				let pre = DispatchInfo {
					call_weight: weight_to_charge,
					extension_weight: Weight::zero(),
					..Default::default()
				};
				let actual_weight = frame_support::dispatch::extract_actual_weight(&result, &pre);
				env.adjust_gas(charged_amount, actual_weight);

				// 5. Handle result
				match result {
					Ok(_) => Ok(Vec::new()),
					Err(e) => Err(revert(&e, "Referenda Precompile: Place decision deposit failed")),
				}
			},
			IReferendaCalls::submissionDeposit(IReferenda::submissionDepositCall) => {
				// Charge gas for submissionDeposit (read-only operation)
				env.charge(
					<crate::weights::SubstrateWeight<Runtime> as WeightInfo>::submission_deposit(),
				)?;

				let submission_deposit =
					<Runtime as pallet_referenda::Config>::SubmissionDeposit::get();
				let deposit_u128: u128 = submission_deposit.saturated_into();
				Ok(deposit_u128.abi_encode())
			},
			IReferendaCalls::decisionDeposit(IReferenda::decisionDepositCall {
				referendumIndex: index,
			}) => {
				// Charge worst-case upfront (before any storage reads)
				let max_charge =
					<crate::weights::SubstrateWeight<Runtime> as WeightInfo>::decision_deposit_ongoing_no_deposit();
				let charged_amount = env.charge(max_charge)?;

				// Get the referendum info to find the track
				let referendum_info = ReferendumInfoFor::<Runtime, ()>::get(*index);

				// Calculate actual weight and deposit amount based on path
				let (actual_weight, decision_deposit_amount) = match &referendum_info {
					Some(pallet_referenda::ReferendumInfo::Ongoing(status)) => {
						// Check if deposit is already placed
						if status.decision_deposit.is_some() {
							// Lighter path - no track lookup needed
							let weight =
								<crate::weights::SubstrateWeight<Runtime> as WeightInfo>::decision_deposit_ongoing_with_deposit();
							(weight, 0u128)
						} else {
							// Heavier path - needs track lookup (already charged worst-case)
							let weight =
								<crate::weights::SubstrateWeight<Runtime> as WeightInfo>::decision_deposit_ongoing_no_deposit();
							let track =
								<Runtime as pallet_referenda::Config>::Tracks::info(status.track)
									.ok_or(Error::Revert("Referenda Precompile: Track not found".into()))?;
							let deposit = track.decision_deposit.saturated_into::<u128>();
							(weight, deposit)
						}
					},
					// For completed referenda, return 0 (nothing left to place)
					Some(_) => {
						let weight =
							<crate::weights::SubstrateWeight<Runtime> as WeightInfo>::decision_deposit_not_found_or_completed();
						(weight, 0u128)
					},
					// Referendum doesn't exist - return 0 (nothing left to place)
					None => {
						let weight =
							<crate::weights::SubstrateWeight<Runtime> as WeightInfo>::decision_deposit_not_found_or_completed();
						(weight, 0u128)
					},
				};

				// Refund unused gas
				env.adjust_gas(charged_amount, actual_weight);

				Ok(decision_deposit_amount.abi_encode())
			},
			_ => Ok(Vec::new()),
		}
	}
}
