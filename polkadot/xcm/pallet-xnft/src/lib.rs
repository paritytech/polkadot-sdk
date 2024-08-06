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

use frame_support::{
	pallet_prelude::*,
	traits::{
		tokens::asset_ops::{
			common_asset_kinds::Instance,
			common_strategies::{DeriveAndReportId, Owned, PredefinedId},
			AssetDefinition, Create,
		},
		Incrementable,
	},
};
use sp_runtime::DispatchResult;
use sp_std::prelude::*;
use xcm::latest::prelude::*;
use xcm_builder::unique_instances::{derivatives::*, NonFungibleAsset};

pub use pallet::*;

/// The log target of this pallet.
pub const LOG_TARGET: &'static str = "runtime::xnft";

type DerivativeIdParamsOf<T, I> = <T as Config<I>>::DerivativeIdParams;

type DerivativeIdOf<T, I> = <T as Config<I>>::DerivativeId;

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

		type DerivativeIdParams: Member + Parameter + MaxEncodedLen;
		type DerivativeId: Member + Parameter + MaxEncodedLen;
	}

	#[pallet::storage]
	#[pallet::getter(fn foreign_asset_to_derivative_id_params)]
	pub type ForeignAssetToDerivativeIdParams<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128, AssetId, DerivativeIdParamsOf<T, I>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn derivative_id_params_to_foreign_asset)]
	pub type DerivativeIdParamsToForeignAsset<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128, DerivativeIdParamsOf<T, I>, AssetId, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn foreign_nft_to_derivative_id)]
	pub type ForeignNftToDerivativeId<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Blake2_128,
		AssetId,
		Blake2_128,
		AssetInstance,
		DerivativeIdOf<T, I>,
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn derivative_id_to_foreign_nft)]
	pub type DerivativeIdToForeignNft<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128, DerivativeIdOf<T, I>, NonFungibleAsset, OptionQuery>;

	#[pallet::storage]
	pub type NextComposableInstanceIdSuffix<T: Config<I>, I: 'static = ()>
	where
		T::DerivativeId: CompositeDerivativeId<Prefix = T::DerivativeIdParams>,
	= StorageMap<
		_,
		Blake2_128,
		T::DerivativeIdParams,
		<T::DerivativeId as CompositeDerivativeId>::Suffix,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// A derivative instance is registered
		/// and bound to a certain foreign nonfungible asset instance.
		DerivativeInstanceRegistered {
			/// The ID of the derivative instance.
			derivative_id: T::DerivativeId,

			/// The XCM ID of the bound foreign nonfungible asset instance.
			foreign_nonfungible: NonFungibleAsset,
		},

		/// A derivative instance is de-registered.
		DerivativeInstanceDeregistered {
			/// The ID of the derivative instance.
			derivative_id: T::DerivativeId,

			/// The XCM ID of the bound foreign nonfungible asset instance.
			foreign_nonfungible: NonFungibleAsset,
		},
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// Can't perform an operation due to the invalid state of storage.
		InvalidState,

		/// Unable to set the next instance ID suffix.
		CantSetNextInstanceIdSuffix,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {}
}

pub struct DerivativeIdParamsRegistry<XnftPallet>(PhantomData<XnftPallet>);
impl<T: Config<I>, I: 'static> DerivativesRegistry<AssetId, T::DerivativeIdParams>
	for DerivativeIdParamsRegistry<Pallet<T, I>>
{
	fn try_register_derivative(
		foreign_asset_id: &AssetId,
		derivative_id_params: &T::DerivativeIdParams,
	) -> DispatchResult {
		<ForeignAssetToDerivativeIdParams<T, I>>::insert(foreign_asset_id, derivative_id_params);
		<DerivativeIdParamsToForeignAsset<T, I>>::insert(derivative_id_params, foreign_asset_id);

		Ok(())
	}

	fn try_deregister_derivative(derivative_id_params: &T::DerivativeIdParams) -> DispatchResult {
		let foreign_asset_id =
			<DerivativeIdParamsToForeignAsset<T, I>>::take(&derivative_id_params)
				.ok_or(Error::<T, I>::InvalidState)?;

		<ForeignAssetToDerivativeIdParams<T, I>>::remove(&foreign_asset_id);

		Ok(())
	}

	fn get_derivative(original: &AssetId) -> Option<T::DerivativeIdParams> {
		<ForeignAssetToDerivativeIdParams<T, I>>::get(original)
	}

	fn get_original(derivative_id_params: &T::DerivativeIdParams) -> Option<AssetId> {
		<DerivativeIdParamsToForeignAsset<T, I>>::get(derivative_id_params)
	}
}

pub struct DerivativeInstancesRegistry<XnftPallet>(PhantomData<XnftPallet>);
impl<T: Config<I>, I: 'static> DerivativesRegistry<NonFungibleAsset, T::DerivativeId>
	for DerivativeInstancesRegistry<Pallet<T, I>>
{
	fn try_register_derivative(
		foreign_nonfungible @ (original_asset_id, original_asset_instance): &NonFungibleAsset,
		derivative_id: &T::DerivativeId,
	) -> DispatchResult {
		<ForeignNftToDerivativeId<T, I>>::insert(
			original_asset_id,
			original_asset_instance,
			derivative_id,
		);
		<DerivativeIdToForeignNft<T, I>>::insert(derivative_id, foreign_nonfungible);

		<Pallet<T, I>>::deposit_event(Event::<T, I>::DerivativeInstanceRegistered {
			derivative_id: derivative_id.clone(),
			foreign_nonfungible: foreign_nonfungible.clone(),
		});

		Ok(())
	}

	fn try_deregister_derivative(derivative_id: &T::DerivativeId) -> DispatchResult {
		let foreign_nonfungible = <DerivativeIdToForeignNft<T, I>>::take(&derivative_id)
			.ok_or(Error::<T, I>::InvalidState)?;

		<ForeignNftToDerivativeId<T, I>>::remove(&foreign_nonfungible.0, &foreign_nonfungible.1);

		<Pallet<T, I>>::deposit_event(Event::<T, I>::DerivativeInstanceDeregistered {
			derivative_id: derivative_id.clone(),
			foreign_nonfungible,
		});

		Ok(())
	}

	fn get_derivative((asset_id, asset_instance): &NonFungibleAsset) -> Option<T::DerivativeId> {
		<ForeignNftToDerivativeId<T, I>>::get(asset_id, asset_instance)
	}

	fn get_original(derivative: &T::DerivativeId) -> Option<NonFungibleAsset> {
		<DerivativeIdToForeignNft<T, I>>::get(derivative)
	}
}

pub trait CompositeDerivativeId {
	type Prefix: Member + Parameter + MaxEncodedLen;
	type Suffix: Member + Parameter + MaxEncodedLen;

	fn compose(prefix: Self::Prefix, suffix: Self::Suffix) -> Self;
}

impl<Prefix, Suffix> CompositeDerivativeId for (Prefix, Suffix)
where
	Prefix: Member + Parameter + MaxEncodedLen,
	Suffix: Member + Parameter + MaxEncodedLen,
{
	type Prefix = Prefix;
	type Suffix = Suffix;

	fn compose(prefix: Self::Prefix, suffix: Self::Suffix) -> Self {
		(prefix, suffix)
	}
}

pub struct ConcatIncrementableIdOnCreate<XnftPallet, CreateOp>(PhantomData<(XnftPallet, CreateOp)>);
impl<XnftPallet, CreateOp> AssetDefinition<Instance>
	for ConcatIncrementableIdOnCreate<XnftPallet, CreateOp>
where
	CreateOp: AssetDefinition<Instance>,
{
	type Id = CreateOp::Id;
}
impl<T, I, CreateOp>
	Create<Instance, Owned<T::AccountId, DeriveAndReportId<T::DerivativeIdParams, T::DerivativeId>>>
	for ConcatIncrementableIdOnCreate<Pallet<T, I>, CreateOp>
where
	T: Config<I>,
	I: 'static,
	T::DerivativeId: CompositeDerivativeId<Prefix = T::DerivativeIdParams>,
	<T::DerivativeId as CompositeDerivativeId>::Suffix: Incrementable,
	CreateOp: Create<Instance, Owned<T::AccountId, PredefinedId<T::DerivativeId>>>,
{
	fn create(
		strategy: Owned<T::AccountId, DeriveAndReportId<T::DerivativeIdParams, T::DerivativeId>>,
	) -> Result<T::DerivativeId, DispatchError> {
		let Owned { owner, id_assignment, .. } = strategy;
		let derivative_id_params = id_assignment.params;

		let instance_id_suffix = <NextComposableInstanceIdSuffix<T, I>>::get(&derivative_id_params)
			.or(<T::DerivativeId as CompositeDerivativeId>::Suffix::initial_value())
			.ok_or(<Error<T, I>>::CantSetNextInstanceIdSuffix)?;

		let next_instance_id_suffix = instance_id_suffix
			.increment()
			.ok_or(<Error<T, I>>::CantSetNextInstanceIdSuffix)?;

		let derivative_id =
			T::DerivativeId::compose(derivative_id_params.clone(), instance_id_suffix);

		CreateOp::create(Owned::new(owner, PredefinedId::from(derivative_id.clone())))?;

		<NextComposableInstanceIdSuffix<T, I>>::insert(
			derivative_id_params,
			next_instance_id_suffix,
		);

		Ok(derivative_id)
	}
}
