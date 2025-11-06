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

//! The Pay trait and associated types.

use codec::{FullCodec, MaxEncodedLen};
use core::fmt::Debug;
use scale_info::TypeInfo;
use sp_core::TypedGet;
use sp_runtime::DispatchError;

use super::{fungible, fungibles, Balance, Preservation::Expendable};

/// Can be implemented by `PayFromAccount` using a `fungible` impl, but can also be implemented with
/// XCM/Asset and made generic over assets.
pub trait Pay {
	/// The type by which we measure units of the currency in which we make payments.
	type Balance: Balance;
	/// The type by which we identify the beneficiaries to whom a payment may be made.
	type Beneficiary;
	/// The type for the kinds of asset that are going to be paid.
	///
	/// The unit type can be used here to indicate there's only one kind of asset to do payments
	/// with. When implementing, it should be clear from the context what that asset is.
	type AssetKind;
	/// An identifier given to an individual payment.
	type Id: FullCodec + MaxEncodedLen + TypeInfo + Clone + Eq + PartialEq + Debug + Copy;
	/// An error which could be returned by the Pay type
	type Error: Debug;
	/// Make a payment and return an identifier for later evaluation of success in some off-chain
	/// mechanism (likely an event, but possibly not on this chain).
	fn pay(
		who: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error>;
	/// Check how a payment has proceeded. `id` must have been previously returned by `pay` for
	/// the result of this call to be meaningful.
	fn check_payment(id: Self::Id) -> PaymentStatus;
	/// Ensure that a call to pay with the given parameters will be successful if done immediately
	/// after this call. Used in benchmarking code.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(
		who: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
	);
	/// Ensure that a call to `check_payment` with the given parameters will return either `Success`
	/// or `Failure`.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(id: Self::Id);
}

/// Status for making a payment via the `Pay::pay` trait function.
pub type PaymentStatus = super::transfer::TransferStatus;

/// Simple implementation of `Pay` which makes a payment from a "pot" - i.e. a single account.
pub struct PayFromAccount<F, A>(core::marker::PhantomData<(F, A)>);
impl<A, F> Pay for PayFromAccount<F, A>
where
	A: TypedGet,
	F: fungible::Mutate<A::Type>,
	A::Type: Eq,
{
	type Balance = F::Balance;
	type Beneficiary = A::Type;
	type AssetKind = ();
	type Id = ();
	type Error = DispatchError;
	fn pay(
		who: &Self::Beneficiary,
		_: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		<F as fungible::Mutate<_>>::transfer(&A::get(), who, amount, Expendable)?;
		Ok(())
	}
	fn check_payment(_: ()) -> PaymentStatus {
		PaymentStatus::Success
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(_: &Self::Beneficiary, _: Self::AssetKind, amount: Self::Balance) {
		<F as fungible::Mutate<_>>::mint_into(&A::get(), amount).unwrap();
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(_: Self::Id) {}
}

/// Simple implementation of `Pay` for assets which makes a payment from a "pot" - i.e. a single
/// account.
pub struct PayAssetFromAccount<F, A>(core::marker::PhantomData<(F, A)>);
impl<A, F> frame_support::traits::tokens::Pay for PayAssetFromAccount<F, A>
where
	A: TypedGet,
	F: fungibles::Mutate<A::Type> + fungibles::Create<A::Type>,
	A::Type: Eq,
{
	type Balance = F::Balance;
	type Beneficiary = A::Type;
	type AssetKind = F::AssetId;
	type Id = ();
	type Error = DispatchError;
	fn pay(
		who: &Self::Beneficiary,
		asset: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		<F as fungibles::Mutate<_>>::transfer(asset, &A::get(), who, amount, Expendable)?;
		Ok(())
	}
	fn check_payment(_: ()) -> PaymentStatus {
		PaymentStatus::Success
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(_: &Self::Beneficiary, asset: Self::AssetKind, amount: Self::Balance) {
		<F as fungibles::Create<_>>::create(asset.clone(), A::get(), true, amount).unwrap();
		<F as fungibles::Mutate<_>>::mint_into(asset, &A::get(), amount).unwrap();
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(_: Self::Id) {}
}

/// A variant of `Pay` that includes the payment `Source`.
pub trait PayWithSource {
	/// The type by which we measure units of the currency in which we make payments.
	type Balance: Balance;
	/// The type by which we identify the sources from whom a payment may be made.
	type Source;
	/// The type by which we identify the beneficiaries to whom a payment may be made.
	type Beneficiary;
	/// The type for the kinds of asset that are going to be paid.
	///
	/// The unit type can be used here to indicate there's only one kind of asset to do payments
	/// with. When implementing, it should be clear from the context what that asset is.
	type AssetKind;
	/// An identifier given to an individual payment.
	type Id: FullCodec + MaxEncodedLen + TypeInfo + Clone + Eq + PartialEq + Debug + Copy;
	/// An error which could be returned by the Pay type
	type Error: Debug;
	/// Make a payment and return an identifier for later evaluation of success in some off-chain
	/// mechanism (likely an event, but possibly not on this chain).
	fn pay(
		source: &Self::Source,
		beneficiary: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error>;
	/// Check how a payment has proceeded. `id` must have been previously returned by `pay` for
	/// the result of this call to be meaningful. Once this returns anything other than
	/// `InProgress` for some `id` it must return `Unknown` rather than the actual result
	/// value.
	fn check_payment(id: Self::Id) -> PaymentStatus;
	/// Ensure that a call to pay with the given parameters will be successful if done immediately
	/// after this call. Used in benchmarking code.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(
		source: &Self::Source,
		beneficiary: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
	);
	/// Ensure that a call to `check_payment` with the given parameters will return either `Success`
	/// or `Failure`.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(id: Self::Id);
}

/// Implementation of the `PayWithSource` trait using multiple fungible asset classes (e.g.,
/// `pallet_assets`)
pub struct PayWithFungibles<F, A>(core::marker::PhantomData<(F, A)>);
impl<A, F> frame_support::traits::tokens::PayWithSource for PayWithFungibles<F, A>
where
	A: Eq + Clone,
	F: fungibles::Mutate<A> + fungibles::Create<A>,
{
	type Balance = F::Balance;
	type Source = A;
	type Beneficiary = A;
	type AssetKind = F::AssetId;
	type Id = ();
	type Error = DispatchError;
	fn pay(
		source: &Self::Source,
		beneficiary: &Self::Beneficiary,
		asset: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		<F as fungibles::Mutate<_>>::transfer(asset, source, beneficiary, amount, Expendable)?;
		Ok(())
	}
	fn check_payment(_: ()) -> PaymentStatus {
		PaymentStatus::Success
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(
		source: &Self::Source,
		_: &Self::Beneficiary,
		asset: Self::AssetKind,
		amount: Self::Balance,
	) {
		use sp_runtime::traits::Zero;

		if F::total_issuance(asset.clone()).is_zero() {
			let _ = <F as fungibles::Create<_>>::create(
				asset.clone(),
				source.clone(),
				true,
				1u32.into(),
			);
		}
		<F as fungibles::Mutate<_>>::mint_into(asset, &source, amount).unwrap();
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(_: Self::Id) {}
}
