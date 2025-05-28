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

//! The purpose of the `pallet-derivatives` is to cover the following derivative asset support
//! scenarios:
//! 1. The `pallet-derivatives` can serve as an API for creating and destroying derivatives.
//! 2. It can store a mapping between the foreign original ID (e.g., XCM `AssetId` or `(AssetId,
//!    AssetInstance)`) and the local derivative ID.
//!
//! The scenarios can be combined.
//!
//! ### Motivation
//!
//! The motivation differs depending on the scenario in question.
//!
//! #### The first scenario
//!
//! The `pallet-derivatives` can be helpful when another pallet, which hosts the derivative assets,
//! doesn't provide a good enough way to create new assets in the context of them being derivatives.
//!
//! This mainly concerns derivative NFT collections because they should be creatable by an
//! unprivileged user, in contrast to how fungible derivative assets are usually registered using a
//! privileged origin (root or some collective).
//!
//! Fungible derivatives require a privileged origin to be registered since they could be used as
//! fee payment assets. Conversely, Derivative NFT collections contain unique derivative objects
//! that don't affect the chain's fee system. They don't represent a payment asset but rather some
//! logical entity that can interact with the given chain's functionality (e.g., NFT
//! fractionalization). These interactions can be the reason why a user might want to transfer an
//! NFT.
//!
//! Requiring a privileged origin in this case is raising an unreasonable barrier for NFT
//! interoperability between chains.
//!
//! However, a local NFT-hosting pallet might not provide a way for a regular user to create a
//! derivative collection without giving that user ownership and collection configuration
//! capabilities (e.g., `pallet-nfts` creates a collection owned by the transaction signer and
//! always requires the signer to supply the collection's initial configuration). A regular user
//! should not be able to influence the properties of a derivative collection and, of course, should
//! not be its owner. The chain itself should manage such a collection in some way (e.g., by
//! providing ownership to the reserve location's sovereign account).
//!
//! In this case, an intermediate API is needed to create derivative NFT collections safely. The
//! `pallet-derivatives` provides such an API. The `create_derivative` and `destroy_derivative` take
//! only the original ID value as a parameter, and the pallet's config completely defines the actual
//! logic.
//!
//! NOTE: Currently, only the bare minimum data can be assigned to the derivative collections since
//! the only thing available during the creation is its original ID. The transaction signer is
//! untrusted, so we can't allow them to provide additional data. However, in the future, this
//! pallet might be configured to send an XCM program with the `ReportMetadata` instruction (XCM v6)
//! during the `create_derivative` execution to fetch the metadata from the reserve origin itself.
//!
//! #### The second scenario
//!
//! Saving the mapping between the original ID and the derivative ID is needed when their types
//! differ and the derivative ID value can't be deterministically deduced from the original ID.
//!
//! This situation can arise in the following cases:
//! * The original ID type is incompatible with a derivative ID type.
//! For example, let `pallet-nfts` instance host derivative NFT collections. We can't set the
//! `CollectionId` (the derivative ID type) to XCM `AssetId` (the original ID type)
//! because `pallet-nfts` requires `CollectionId` to be incrementable.
//! * It is desired to have a continuous ID space for all objects, both derivative and local.
//! For instance, one might want to reuse the existing pallet combinations (like `pallet-nfts`
//! instance + `pallet-nfts-fractionalization` instance) without adding new pallet instances between
//! the one hosting NFTs and many special logic pallets. In this case, the original ID type would be
//! `(AssetId, AssetInstance)`, and the derivative ID type can be anything.

#![recursion_limit = "256"]
// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	pallet_prelude::*,
	traits::tokens::asset_ops::{
		common_strategies::{CheckOrigin, DeriveAndReportId},
		AssetDefinition, Create, Destroy,
	},
};
use frame_system::pallet_prelude::*;
use sp_runtime::DispatchResult;
use xcm_builder::unique_instances::{
	derivatives::{DerivativesRegistry, IterDerivativesRegistry},
	DerivativesExtra,
};

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

/// The log target of this pallet.
pub const LOG_TARGET: &'static str = "runtime::xcm::derivatives";

/// A helper type representing the intention to store
/// the mapping between the original and the given derivative.
pub struct SaveMappingTo<Derivative>(pub Derivative);

type OriginalOf<T, I> = <T as Config<I>>::Original;
type DerivativeOf<T, I> = <T as Config<I>>::Derivative;
type DerivativeExtraOf<T, I> = <T as Config<I>>::DerivativeExtra;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		type WeightInfo: WeightInfo;

		/// The type of an original
		type Original: Member + Parameter + MaxEncodedLen;

		/// The type of a derivative
		type Derivative: Member + Parameter + MaxEncodedLen;

		/// Optional derivative extra data
		type DerivativeExtra: Member + Parameter + MaxEncodedLen;

		/// Derivative creation operation.
		/// Used in the `create_derivative` extrinsic.
		///
		/// Can be configured to save the mapping between the original and the derivative
		/// if it returns `Some(SaveMappingTo(DERIVATIVE))`.
		///
		/// If the extrinsic isn't used, this type can be set to
		/// [AlwaysErrOps](frame_support::traits::tokens::asset_ops::utils::AlwaysErrOps).
		type CreateOp: Create<
			CheckOrigin<
				Self::RuntimeOrigin,
				DeriveAndReportId<Self::Original, Option<SaveMappingTo<Self::Derivative>>>,
			>,
		>;

		/// Derivative destruction operation.
		/// Used in the `destroy_derivative` extrinsic.
		///
		/// If the extrinsic isn't used, this type can be set to
		/// [AlwaysErrOps](frame_support::traits::tokens::asset_ops::utils::AlwaysErrOps).
		type DestroyOp: AssetDefinition<Id = Self::Original>
			+ Destroy<CheckOrigin<Self::RuntimeOrigin>>;
	}

	#[pallet::storage]
	#[pallet::getter(fn original_to_derivative)]
	pub type OriginalToDerivative<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, OriginalOf<T, I>, DerivativeOf<T, I>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn derivative_to_original)]
	pub type DerivativeToOriginal<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, DerivativeOf<T, I>, OriginalOf<T, I>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn derivative_extra)]
	pub type DerivativeExtra<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, DerivativeOf<T, I>, DerivativeExtraOf<T, I>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// A derivative is created.
		DerivativeCreated { original: OriginalOf<T, I> },

		/// A mapping between an original asset ID and a local derivative asset ID is created.
		DerivativeMappingCreated { original: OriginalOf<T, I>, derivative_id: DerivativeOf<T, I> },

		/// A derivative is destroyed.
		DerivativeDestroyed { original: OriginalOf<T, I> },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// A derivative already exists.
		DerivativeAlreadyExists,

		/// Failed to deregister a non-registered derivative.
		NoDerivativeToDeregister,

		/// Failed to find a derivative.
		DerivativeNotFound,

		/// Failed to get the derivative's extra data.
		DerivativeExtraDataNotFound,

		/// Failed to get an original.
		OriginalNotFound,

		/// Invalid asset to register as a derivative
		InvalidAsset,
	}

	#[pallet::call(weight(T::WeightInfo))]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		#[pallet::call_index(0)]
		pub fn create_derivative(
			origin: OriginFor<T>,
			original: OriginalOf<T, I>,
		) -> DispatchResult {
			let maybe_save_mapping = T::CreateOp::create(CheckOrigin::new(
				origin,
				DeriveAndReportId::from(original.clone()),
			))?;

			if let Some(SaveMappingTo(derivative)) = maybe_save_mapping {
				Self::try_register_derivative(&original, &derivative)?;
			}

			Self::deposit_event(Event::<T, I>::DerivativeCreated { original });

			Ok(())
		}

		#[pallet::call_index(1)]
		pub fn destroy_derivative(
			origin: OriginFor<T>,
			original: OriginalOf<T, I>,
		) -> DispatchResult {
			T::DestroyOp::destroy(&original, CheckOrigin::check(origin))?;

			if Self::get_derivative(&original).is_ok() {
				Self::try_deregister_derivative_of(&original)?;
			}

			Ok(())
		}
	}
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

		Self::deposit_event(Event::<T, I>::DerivativeCreated { original: original.clone() });

		Ok(())
	}

	fn try_deregister_derivative_of(original: &OriginalOf<T, I>) -> DispatchResult {
		let derivative = <OriginalToDerivative<T, I>>::take(&original)
			.ok_or(Error::<T, I>::NoDerivativeToDeregister)?;

		<DerivativeToOriginal<T, I>>::remove(&derivative);
		<DerivativeExtra<T, I>>::remove(&derivative);

		Self::deposit_event(Event::<T, I>::DerivativeDestroyed { original: original.clone() });

		Ok(())
	}

	fn get_derivative(original: &OriginalOf<T, I>) -> Result<DerivativeOf<T, I>, DispatchError> {
		<OriginalToDerivative<T, I>>::get(original).ok_or(Error::<T, I>::DerivativeNotFound.into())
	}

	fn get_original(derivative: &DerivativeOf<T, I>) -> Result<OriginalOf<T, I>, DispatchError> {
		<DerivativeToOriginal<T, I>>::get(derivative).ok_or(Error::<T, I>::OriginalNotFound.into())
	}
}

impl<T: Config<I>, I: 'static> IterDerivativesRegistry<OriginalOf<T, I>, DerivativeOf<T, I>>
	for Pallet<T, I>
{
	fn iter_originals() -> impl Iterator<Item = OriginalOf<T, I>> {
		<OriginalToDerivative<T, I>>::iter_keys()
	}

	fn iter_derivatives() -> impl Iterator<Item = DerivativeOf<T, I>> {
		<OriginalToDerivative<T, I>>::iter_values()
	}

	fn iter() -> impl Iterator<Item = (OriginalOf<T, I>, DerivativeOf<T, I>)> {
		<OriginalToDerivative<T, I>>::iter()
	}
}

impl<T: Config<I>, I: 'static> DerivativesExtra<DerivativeOf<T, I>, DerivativeExtraOf<T, I>>
	for Pallet<T, I>
{
	fn get_derivative_extra(derivative: &DerivativeOf<T, I>) -> Option<DerivativeExtraOf<T, I>> {
		<DerivativeExtra<T, I>>::get(derivative)
	}

	fn set_derivative_extra(
		derivative: &DerivativeOf<T, I>,
		extra: Option<DerivativeExtraOf<T, I>>,
	) -> DispatchResult {
		ensure!(
			<DerivativeToOriginal<T, I>>::contains_key(derivative),
			Error::<T, I>::DerivativeNotFound,
		);

		<DerivativeExtra<T, I>>::set(derivative, extra);

		Ok(())
	}
}

pub trait WeightInfo {
	fn create_derivative() -> Weight;
	fn destroy_derivative() -> Weight;
}

pub struct TestWeightInfo;
impl WeightInfo for TestWeightInfo {
	fn create_derivative() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn destroy_derivative() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}
}

/// The `NoStoredMapping` adapter calls the `CreateOp` (which should take the `Original` value and
/// return a `Derivative` one) and returns `None`, indicating that the mapping between the original
/// and the derivative shouldn't be saved.
pub struct NoStoredMapping<CreateOp>(PhantomData<CreateOp>);
impl<RuntimeOrigin, CreateOp, Original, Derivative>
	Create<
		CheckOrigin<RuntimeOrigin, DeriveAndReportId<Original, Option<SaveMappingTo<Derivative>>>>,
	> for NoStoredMapping<CreateOp>
where
	CreateOp: Create<CheckOrigin<RuntimeOrigin, DeriveAndReportId<Original, Derivative>>>,
{
	fn create(
		strategy: CheckOrigin<
			RuntimeOrigin,
			DeriveAndReportId<Original, Option<SaveMappingTo<Derivative>>>,
		>,
	) -> Result<Option<SaveMappingTo<Derivative>>, DispatchError> {
		let CheckOrigin(origin, strategy) = strategy;

		CreateOp::create(CheckOrigin::new(origin, DeriveAndReportId::from(strategy.params)))?;

		Ok(None)
	}
}

/// The `StoreMapping` adapter obtains a `Derivative` value by calling the `CreateOp`
/// (which should take the `Original` value and return a `Derivative` one),
/// and returns `Some(SaveMappingTo(DERIVATIVE_VALUE))`, indicating that the mapping should be
/// saved.
pub struct StoreMapping<CreateOp>(PhantomData<CreateOp>);
impl<RuntimeOrigin, CreateOp, Original, Derivative>
	Create<
		CheckOrigin<RuntimeOrigin, DeriveAndReportId<Original, Option<SaveMappingTo<Derivative>>>>,
	> for StoreMapping<CreateOp>
where
	CreateOp: Create<CheckOrigin<RuntimeOrigin, DeriveAndReportId<Original, Derivative>>>,
{
	fn create(
		strategy: CheckOrigin<
			RuntimeOrigin,
			DeriveAndReportId<Original, Option<SaveMappingTo<Derivative>>>,
		>,
	) -> Result<Option<SaveMappingTo<Derivative>>, DispatchError> {
		let CheckOrigin(origin, strategy) = strategy;

		let derivative =
			CreateOp::create(CheckOrigin::new(origin, DeriveAndReportId::from(strategy.params)))?;

		Ok(Some(SaveMappingTo(derivative)))
	}
}

/// Gets the `InvalidAsset` error from the given `pallet-derivatives` instance.
pub struct InvalidAssetError<Pallet>(PhantomData<Pallet>);
impl<T: Config<I>, I: 'static> TypedGet for InvalidAssetError<Pallet<T, I>> {
	type Type = Error<T, I>;

	fn get() -> Self::Type {
		Error::<T, I>::InvalidAsset
	}
}
