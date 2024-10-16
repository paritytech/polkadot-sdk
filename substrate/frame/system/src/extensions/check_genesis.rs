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

use crate::{pallet_prelude::BlockNumberFor, Config, Pallet};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{TransactionExtension, Zero},
	transaction_validity::TransactionValidityError,
};

/// Genesis hash check to provide replay protection between different networks.
///
/// # Transaction Validity
///
/// Note that while a transaction with invalid `genesis_hash` will fail to be decoded,
/// the extension does not affect any other fields of `TransactionValidity` directly.
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct CheckGenesis<T: Config + Send + Sync>(core::marker::PhantomData<T>);

impl<T: Config + Send + Sync> core::fmt::Debug for CheckGenesis<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "CheckGenesis")
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut core::fmt::Formatter) -> core::fmt::Result {
		Ok(())
	}
}

impl<T: Config + Send + Sync> CheckGenesis<T> {
	/// Creates new `TransactionExtension` to check genesis hash.
	pub fn new() -> Self {
		Self(core::marker::PhantomData)
	}
}

impl<T: Config + Send + Sync> TransactionExtension<T::RuntimeCall> for CheckGenesis<T> {
	const IDENTIFIER: &'static str = "CheckGenesis";
	type Implicit = T::Hash;
	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		Ok(<Pallet<T>>::block_hash(BlockNumberFor::<T>::zero()))
	}
	type Val = ();
	type Pre = ();
	fn weight(&self, _: &T::RuntimeCall) -> sp_weights::Weight {
		// All transactions will always read the hash of the genesis block, so to avoid
		// charging this multiple times in a block we manually set the proof size to 0.
		<T::ExtensionsWeightInfo as super::WeightInfo>::check_genesis().set_proof_size(0)
	}
	impl_tx_ext_default!(T::RuntimeCall; validate prepare);
}
