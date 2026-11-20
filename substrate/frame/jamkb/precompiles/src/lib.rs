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

//! # pallet-jamkb precompile (prototype)
//!
//! Exposes `pallet-jamkb` to contracts at a fixed address, as described in
//! `designs/jamkb-management/jamkb-management.md` §3.2.1. Each function calls the pallet method
//! that holds the logic shared with the dispatchable of the same name; the call origin is the
//! account that calls the precompile. No governance call is exposed, and a caller without an
//! account id, such as Root, is rejected.
//!
//! Every call is rejected as a delegate call, and a state-changing call in a read-only frame. On
//! failure a call reverts with the pallet error name for a module error, and with the error's
//! `Debug` form otherwise. The precompile emits no Solidity events; a contract follows its
//! operation with `operationState`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{format, vec::Vec};
use core::{marker::PhantomData, num::NonZero};
use frame_support::traits::Get;
use pallet_jamkb::{AllocationMode, AllocationState, OperationState, WeightInfo as _};
use pallet_revive::precompiles::{
	alloy::{sol, sol_types::SolCall},
	AddressMatcher, Error, Ext, Precompile,
};
use sp_runtime::{DispatchError, ModuleError};

sol!(
	#[sol(all_derives = true)]
	"IJamkb.sol"
);

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// The precompile's address suffix: `0x…0903_0000`.
pub const JAMKB_PRECOMPILE: u16 = 0x0903;

/// The pallet-jamkb precompile.
pub struct Jamkb<T>(PhantomData<T>);

type JamkbPallet<T> = pallet_jamkb::Pallet<T>;
type JamkbWeights<T> = <T as pallet_jamkb::Config>::WeightInfo;

fn ensure_not_read_only<T: pallet_revive::Config>(env: &impl Ext<T = T>) -> Result<(), Error> {
	if env.is_read_only() {
		return Err(pallet_revive::Error::<T>::StateChangeDenied.into());
	}
	Ok(())
}

fn caller<T: pallet_revive::Config>(env: &impl Ext<T = T>) -> Result<T::AccountId, Error> {
	env.caller()
		.account_id()
		.cloned()
		.map_err(|_| Error::Revert("caller has no account id".into()))
}

/// Reverts with the pallet error name, or with the error itself for a non-module error. A
/// revert rolls back the call's storage changes: pallet-revive runs every call frame in its own
/// transaction.
fn revert(error: DispatchError) -> Error {
	match error {
		DispatchError::Module(ModuleError { message: Some(name), .. }) => {
			Error::Revert(name.into())
		},
		other => Error::Revert(format!("{other:?}").into()),
	}
}

impl<T> Precompile for Jamkb<T>
where
	T: pallet_jamkb::Config + pallet_revive::Config,
	T::AccountId: From<[u8; 32]> + AsRef<[u8; 32]>,
{
	type T = T;
	type Interface = IJamkb::IJamkbCalls;
	const MATCHER: AddressMatcher =
		AddressMatcher::Fixed(NonZero::new(JAMKB_PRECOMPILE).expect("not zero; qed"));
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		frame_support::ensure!(
			!env.is_delegate_call(),
			pallet_revive::Error::<Self::T>::PrecompileDelegateDenied,
		);
		use IJamkb::IJamkbCalls;
		let read = T::DbWeight::get().reads(1);
		match input {
			IJamkbCalls::executeRelease(IJamkb::executeReleaseCall { allocationId }) => {
				env.charge(JamkbWeights::<T>::execute_release())?;
				ensure_not_read_only(env)?;
				let who = caller(env)?;
				let op =
					JamkbPallet::<T>::do_execute_release(&who, *allocationId).map_err(revert)?;
				Ok(IJamkb::executeReleaseCall::abi_encode_returns(&op))
			},
			IJamkbCalls::redeem(IJamkb::redeemCall { amount, target, memo }) => {
				env.charge(JamkbWeights::<T>::redeem())?;
				ensure_not_read_only(env)?;
				let who = caller(env)?;
				let amount = <T as pallet_jamkb::Config>::Balance::try_from(*amount)
					.map_err(|_| Error::Revert("amount overflow".into()))?;
				let mut bytes = [0u8; 128];
				for (chunk, word) in bytes.chunks_mut(32).zip(memo.iter()) {
					chunk.copy_from_slice(word.as_slice());
				}
				let op =
					JamkbPallet::<T>::do_redeem(who, amount, *target, bytes).map_err(revert)?;
				Ok(IJamkb::redeemCall::abi_encode_returns(&op))
			},
			IJamkbCalls::claim(IJamkb::claimCall { beneficiary }) => {
				env.charge(JamkbWeights::<T>::claim())?;
				ensure_not_read_only(env)?;
				caller(env)?;
				let beneficiary = T::AccountId::from(beneficiary.0);
				let claimed = JamkbPallet::<T>::do_claim(beneficiary).map_err(revert)?;
				Ok(IJamkb::claimCall::abi_encode_returns(&to_u128::<T>(claimed)?))
			},
			IJamkbCalls::cancelAllocation(IJamkb::cancelAllocationCall { allocationId }) => {
				env.charge(JamkbWeights::<T>::cancel_allocation())?;
				ensure_not_read_only(env)?;
				let who = caller(env)?;
				JamkbPallet::<T>::do_cancel_allocation(Some(&who), false, *allocationId)
					.map_err(revert)?;
				Ok(Vec::new())
			},
			IJamkbCalls::allocation(IJamkb::allocationCall { allocationId }) => {
				env.charge(read)?;
				let a = JamkbPallet::<T>::allocation(*allocationId)
					.ok_or_else(|| revert(pallet_jamkb::Error::<T>::UnknownAllocation.into()))?;
				let AllocationMode::Permanent { .. } = a.mode;
				let state = match a.state {
					AllocationState::Approved => IJamkb::AllocationState::Approved,
					AllocationState::Delivering => IJamkb::AllocationState::Delivering,
					AllocationState::Delivered => IJamkb::AllocationState::Delivered,
					AllocationState::Closed => IJamkb::AllocationState::Closed,
				};
				let allocation = IJamkb::Allocation {
					id: a.id,
					state,
					mode: IJamkb::AllocationMode::Permanent,
					target: a.target,
					amount: to_u128::<T>(a.amount)?,
					approver: a.approver.map(|who| *who.as_ref()).unwrap_or_default().into(),
					deliveredAt: a.delivered_at.unwrap_or_default(),
					duration: 0,
					validFrom: a.valid_from,
					expiresAt: a.expires_at,
				};
				Ok(IJamkb::allocationCall::abi_encode_returns(&allocation))
			},
			IJamkbCalls::operationState(IJamkb::operationStateCall { opId }) => {
				env.charge(read)?;
				let state = match JamkbPallet::<T>::operation(*opId).map(|op| op.state) {
					None => IJamkb::OperationState::NotFound,
					Some(OperationState::Requested) => IJamkb::OperationState::Requested,
					Some(OperationState::Confirmed(_)) => IJamkb::OperationState::Confirmed,
					Some(OperationState::Failed(_)) => IJamkb::OperationState::Failed,
				};
				Ok(IJamkb::operationStateCall::abi_encode_returns(&state))
			},
			IJamkbCalls::claimableOf(IJamkb::claimableOfCall { beneficiary }) => {
				env.charge(read)?;
				let who = T::AccountId::from(beneficiary.0);
				let claimable = pallet_jamkb::Unclaimed::<T>::get(&who);
				Ok(IJamkb::claimableOfCall::abi_encode_returns(&to_u128::<T>(claimable)?))
			},
			IJamkbCalls::currentTimeslot(IJamkb::currentTimeslotCall {}) => {
				env.charge(read)?;
				Ok(IJamkb::currentTimeslotCall::abi_encode_returns(&JamkbPallet::<T>::now()))
			},
		}
	}
}

fn to_u128<T: pallet_jamkb::Config>(
	balance: <T as pallet_jamkb::Config>::Balance,
) -> Result<u128, Error> {
	balance.try_into().map_err(|_| Error::Revert("balance overflow".into()))
}
