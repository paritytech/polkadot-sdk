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

#![recursion_limit = "256"]
// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::pallet_prelude::*;
use sp_runtime::{DispatchError, DispatchResult};
use xcm_builder::unique_instances::derivatives::DerivativesRegistry;

pub use pallet::*;

/// The log target of this pallet.
pub const LOG_TARGET: &'static str = "runtime::xcm::derivatives";

type OriginalOf<T, I> = <T as Config<I>>::Original;

type DerivativeOf<T, I> = <T as Config<I>>::Derivative;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	/// The module configuration trait.
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type Original: Member + Parameter + MaxEncodedLen;
		type Derivative: Member + Parameter + MaxEncodedLen;
	}

	#[pallet::storage]
	#[pallet::getter(fn original_to_derivative)]
	pub type OriginalToDerivative<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128, OriginalOf<T, I>, DerivativeOf<T, I>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn derivative_to_original)]
	pub type DerivativeToOriginal<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128, DerivativeOf<T, I>, OriginalOf<T, I>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// A derivative is registered.
		DerivativeRegistered { original: OriginalOf<T, I>, derivative: DerivativeOf<T, I> },

		/// A derivative is de-registered.
		DerivativeDeregistered { original: OriginalOf<T, I>, derivative: DerivativeOf<T, I> },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// A derivative already exists.
		DerivativeAlreadyExists,

		/// Failed to deregister a non-registered derivative.
		NoDerivativeToDeregister,

		/// Failed to get a derivative for the given original.
		DerivativeNotFound,

		/// Failed to get an original for the given derivative.
		OriginalNotFound,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {}
}

impl<T: Config<I>, I: 'static> DerivativesRegistry<OriginalOf<T, I>, DerivativeOf<T, I>>
	for Pallet<T, I>
{
	fn try_register_derivative(
		original: &OriginalOf<T, I>,
		derivative: &DerivativeOf<T, I>,
	) -> DispatchResult {
		ensure!(
			Self::original_to_derivative(original).is_none(),
			Error::<T, I>::DerivativeAlreadyExists,
		);

		<OriginalToDerivative<T, I>>::insert(original, derivative);
		<DerivativeToOriginal<T, I>>::insert(derivative, original);

		Self::deposit_event(Event::<T, I>::DerivativeRegistered {
			original: original.clone(),
			derivative: derivative.clone(),
		});

		Ok(())
	}

	fn try_deregister_derivative(derivative: &DerivativeOf<T, I>) -> DispatchResult {
		let original = <DerivativeToOriginal<T, I>>::take(&derivative)
			.ok_or(Error::<T, I>::NoDerivativeToDeregister)?;

		<OriginalToDerivative<T, I>>::remove(&original);

		Self::deposit_event(Event::<T, I>::DerivativeDeregistered {
			original: original.clone(),
			derivative: derivative.clone(),
		});

		Ok(())
	}

	fn get_derivative(original: &OriginalOf<T, I>) -> Result<DerivativeOf<T, I>, DispatchError> {
		<OriginalToDerivative<T, I>>::get(original).ok_or(Error::<T, I>::DerivativeNotFound.into())
	}

	fn get_original(derivative: &DerivativeOf<T, I>) -> Result<OriginalOf<T, I>, DispatchError> {
		<DerivativeToOriginal<T, I>>::get(derivative).ok_or(Error::<T, I>::OriginalNotFound.into())
	}
}
