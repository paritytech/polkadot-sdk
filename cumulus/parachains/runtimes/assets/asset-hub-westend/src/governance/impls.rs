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
#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarks {
	use crate::ParachainSystem;
	use core::marker::PhantomData;
	use cumulus_primitives_core::{ChannelStatus, GetChannelInfo};
	use frame_support::traits::{
		tokens::{Pay, PaymentStatus},
		Get,
	};

	/// Trait for setting up any prerequisites for successful execution of benchmarks.
	pub trait EnsureSuccessful {
		fn ensure_successful();
	}

	/// Implementation of the [`EnsureSuccessful`] trait which opens an HRMP channel between
	/// the Collectives and a parachain with a given ID.
	pub struct OpenHrmpChannel<I>(PhantomData<I>);
	impl<I: Get<u32>> EnsureSuccessful for OpenHrmpChannel<I> {
		fn ensure_successful() {
			if let ChannelStatus::Closed = ParachainSystem::get_channel_status(I::get().into()) {
				ParachainSystem::open_outbound_hrmp_channel_for_benchmarks_or_tests(I::get().into())
			}
		}
	}

	/// Type that wraps a type implementing the [`Pay`] trait to decorate its
	/// [`Pay::ensure_successful`] function with a provided implementation of the
	/// [`EnsureSuccessful`] trait.
	pub struct PayWithEnsure<O, E>(PhantomData<(O, E)>);
	impl<O, E> Pay for PayWithEnsure<O, E>
	where
		O: Pay,
		E: EnsureSuccessful,
	{
		type AssetKind = O::AssetKind;
		type Balance = O::Balance;
		type Beneficiary = O::Beneficiary;
		type Error = O::Error;
		type Id = O::Id;

		fn pay(
			who: &Self::Beneficiary,
			asset_kind: Self::AssetKind,
			amount: Self::Balance,
		) -> Result<Self::Id, Self::Error> {
			O::pay(who, asset_kind, amount)
		}
		fn check_payment(id: Self::Id) -> PaymentStatus {
			O::check_payment(id)
		}
		fn ensure_successful(
			who: &Self::Beneficiary,
			asset_kind: Self::AssetKind,
			amount: Self::Balance,
		) {
			E::ensure_successful();
			O::ensure_successful(who, asset_kind, amount)
		}
		fn ensure_concluded(id: Self::Id) {
			O::ensure_concluded(id)
		}
	}
}
