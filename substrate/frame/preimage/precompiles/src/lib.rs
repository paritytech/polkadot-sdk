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

//! Precompiles for pallet-preimage
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use core::{fmt, marker::PhantomData, num::NonZero};
use frame_support::{
	dispatch::RawOrigin,
	sp_runtime::traits::Hash,
};
use pallet_preimage::{Config, WeightInfo};
use pallet_revive::{
	precompiles::{
		alloy::{self},
		AddressMatcher, Error, Ext, Precompile,
	},
	ExecOrigin as Origin, Weight,
};
use codec::{DecodeAll, Encode};
use tracing::error;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

alloy::sol!("src/interface/IPreimage.sol");
use IPreimage::IPreimageCalls;

const LOG_TARGET: &str = "preimage::precompiles";

fn revert(error: &impl fmt::Debug, message: &str) -> Error {
	error!(target: LOG_TARGET, ?error, "{}", message);
	Error::Revert(message.into())
}

pub struct PreimagePrecompile<T>(PhantomData<T>);
impl<T> Precompile for PreimagePrecompile<T>
where
	T: pallet_preimage::Config + pallet_revive::Config,
{
	type T = T;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZero::new(13).unwrap());
	const HAS_CONTRACT_INFO: bool = false;
	type Interface = IPreimage::IPreimageCalls;
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
			IPreimageCalls::notePreimage(_) | IPreimageCalls::unnotePreimage(_)
				if env.is_read_only() =>
				Err(Error::Error(pallet_revive::Error::<Self::T>::StateChangeDenied.into())),
			IPreimageCalls::notePreimage(IPreimage::notePreimageCall { preImage }) => {
				let preimage = preImage.to_vec();
				
				let weight_to_charge =
					<T as Config>::WeightInfo::note_preimage(preimage.len() as u32);
				let charged_amount = env.charge(weight_to_charge)?;

				let hash = T::Hashing::hash(&preimage).encode();

				pallet_preimage::Pallet::<T>::note_preimage(frame_origin, preimage)
					.inspect(|post_info| {
						if post_info.pays_fee == frame_support::dispatch::Pays::No {
							env.adjust_gas(charged_amount, Weight::zero());
						}
					})
					.map(|_| hash)
					.map_err(|error| revert(&error.error, "Preimage: notePreimage failed"))
			},
			IPreimageCalls::unnotePreimage(IPreimage::unnotePreimageCall { hash }) => {
				let _ = env.charge(<T as Config>::WeightInfo::unnote_preimage());

				let runtime_hash = T::Hash::decode_all(&mut &hash[..])
					.map_err(|error| revert(&error, "Preimage: invalid hash format"))?;

				pallet_preimage::Pallet::<T>::unnote_preimage(frame_origin, runtime_hash)
					.map(|_| Vec::new())
					.map_err(|error| revert(&error, "Preimage: unnotePreimage failed"))
			},
		}
	}
}
