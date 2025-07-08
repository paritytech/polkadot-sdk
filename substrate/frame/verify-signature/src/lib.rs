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

//! Transaction extension which validates a signature against a payload constructed from a call and
//! the rest of the transaction extension pipeline.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod extension;
#[cfg(test)]
mod tests;
pub mod weights;

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
pub use benchmarking::BenchmarkHelper;
use codec::{Decode, Encode};
pub use extension::VerifySignature;
use frame_support::Parameter;
pub use weights::WeightInfo;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use sp_runtime::traits::{IdentifyAccount, Verify};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Signature type that the extension of this pallet can verify.
		type Signature: Verify<Signer = Self::AccountIdentifier>
			+ Parameter
			+ Encode
			+ Decode
			+ Send
			+ Sync;
		/// The account identifier used by this pallet's signature type.
		type AccountIdentifier: IdentifyAccount<AccountId = Self::AccountId>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
		/// Helper to create a signature to be benchmarked.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: BenchmarkHelper<Self::Signature, Self::AccountId>;
	}
}
