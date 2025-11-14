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

//! The transfer trait and associated types

use crate::pallet_prelude::{Decode, Encode};
use core::fmt::Debug;
use frame_support::traits::tokens::PaymentStatus;
use scale_info::TypeInfo;
use sp_debug_derive::RuntimeDebug;
use sp_runtime::codec::{FullCodec, MaxEncodedLen};

/// Is intended to be implemented using a `fungible` impl, but can also be implemented with
/// XCM/Asset and made generic over assets.
///
/// It is similar to the `frame_support::traits::tokens::Pay`, but it offers a variable source
/// account for the payment.
pub trait Transfer {
	/// The type by which we measure units of the currency in which we make payments.
	type Balance;
	/// The type by which identify the payer involved in the transfer.
	///
	/// This is usually and AccountId or a Location.
	type Sender;

	/// The type by which we identify the beneficiary involved in the transfer.
	///
	/// This is usually and AccountId or a Location.
	type Beneficiary;

	/// The type for the kinds of asset that are going to be paid.
	///
	/// The unit type can be used here to indicate there's only one kind of asset to do payments
	/// with. When implementing, it should be clear from the context what that asset is.
	type AssetKind;

	/// Asset that is used to pay the xcm execution fees on the remote chain.
	type RemoteFeeAsset;
	/// An identifier given to an individual payment.
	type Id: FullCodec + MaxEncodedLen + TypeInfo + Clone + Eq + PartialEq + Debug + Copy;
	/// An error which could be returned by the Pay type
	type Error: Debug;
	/// Make a payment and return an identifier for later evaluation of success in some off-chain
	/// mechanism (likely an event, but possibly not on this chain).
	fn transfer(
		from: &Self::Sender,
		to: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
		remote_fee: Option<Self::RemoteFeeAsset>,
	) -> Result<Self::Id, Self::Error>;

	/// Check how a payment has proceeded. `id` must have been previously returned by `pay` for
	/// the result of this call to be meaningful.
	fn check_transfer(id: Self::Id) -> PaymentStatus;
	/// Ensure that a call to pay with the given parameters will be successful if done immediately
	/// after this call. Used in benchmarking code.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(
		to: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
	);
	/// Ensure that a call to `check_payment` with the given parameters will return either `Success`
	/// or `Failure`.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(id: Self::Id);
}

/// Status for making a payment via the `Pay::pay` trait function.
#[derive(Encode, Decode, Eq, PartialEq, Clone, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum TransferStatus {
	/// Payment is in progress. Nothing to report yet.
	InProgress,
	/// Payment status is unknowable. It may already have reported the result, or if not then
	/// it will never be reported successful or failed.
	Unknown,
	/// Payment happened successfully.
	Success,
	/// Payment failed. It may safely be retried.
	Failure,
}
