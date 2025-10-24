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
use core::{fmt, marker::PhantomData, num::NonZero};
use frame_support::dispatch::RawOrigin;
use pallet_referenda::Config;
use pallet_revive::{
	precompiles::{
		alloy::{self, sol_types::SolValue},
		AddressMatcher, Error, Ext, Precompile,
	},
	DispatchInfo, ExecOrigin as Origin, Weight,
};
use tracing::error;

alloy::sol!("src/interfaces/IReferenda.sol");
use IReferenda::IReferendaCalls;

const LOG_TARGET: &str = "referenda::precompiles";

fn revert(error: &impl fmt::Debug, message: &str) -> Error {
	error!(target: LOG_TARGET, ?error, "{}", message);
	Error::Revert(message.into())
}

pub struct ReferendaPrecompile<T>(PhantomData<T>);
impl<Runtime> Precompile for ReferendaPrecompile<Runtime>
where
	Runtime: crate::Config + pallet_revive::Config,
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
		let origin = env.caller();

		// match calls
		match input {
			IReferendaCalls::submitLookup(_) | IReferendaCalls::submitInline(_)
				if env.is_read_only() =>
			{
				Err(Error::Error(pallet_revive::Error::<Self::T>::StateChangeDenied.into()))
			},

			// Add a catch-all for now
			_ => Ok(Vec::new()),
		}
	}
}
