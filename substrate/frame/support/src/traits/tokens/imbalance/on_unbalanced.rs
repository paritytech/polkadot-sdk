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

//! Trait for handling imbalances.

use frame_support::traits::{fungible, fungibles, misc::TryDrop};
use sp_core::TypedGet;
use sp_std::marker::PhantomData;

/// Handler for when some currency "account" decreased in balance for
/// some reason.
///
/// The only reason at present for an increase would be for validator rewards, but
/// there may be other reasons in the future or for other chains.
///
/// Reasons for decreases include:
///
/// - Someone got slashed.
/// - Someone paid for a transaction to be included.
pub trait OnUnbalanced<Imbalance: TryDrop> {
	/// Handler for some imbalances. The different imbalances might have different origins or
	/// meanings, dependent on the context. Will default to simply calling on_unbalanced for all
	/// of them. Infallible.
	fn on_unbalanceds<B>(amounts: impl Iterator<Item = Imbalance>)
	where
		Imbalance: crate::traits::Imbalance<B>,
	{
		Self::on_unbalanced(amounts.fold(Imbalance::zero(), |i, x| x.merge(i)))
	}

	/// Handler for some imbalance. Infallible.
	fn on_unbalanced(amount: Imbalance) {
		amount.try_drop().unwrap_or_else(Self::on_nonzero_unbalanced)
	}

	/// Actually handle a non-zero imbalance. You probably want to implement this rather than
	/// `on_unbalanced`.
	fn on_nonzero_unbalanced(amount: Imbalance) {
		drop(amount);
	}
}

impl<Imbalance: TryDrop> OnUnbalanced<Imbalance> for () {}

/// Resolves received asset credit to account `A`, implementing [`OnUnbalanced`].
///
/// Credits that cannot be resolved to account `A` are dropped. This may occur if the account for
/// address `A` does not exist and the existential deposit requirement is not met.
pub struct ResolveTo<A, F>(PhantomData<(A, F)>);
impl<A: TypedGet, F: fungible::Balanced<A::Type>> OnUnbalanced<fungible::Credit<A::Type, F>>
	for ResolveTo<A, F>
{
	fn on_nonzero_unbalanced(credit: fungible::Credit<A::Type, F>) {
		let _ = F::resolve(&A::get(), credit).map_err(|c| drop(c));
	}
}

/// Resolves received asset credit to account `A`, implementing [`OnUnbalanced`].
///
/// Credits that cannot be resolved to account `A` are dropped. This may occur if the account for
/// address `A` does not exist and the existential deposit requirement is not met.
pub struct ResolveAssetTo<A, F>(PhantomData<(A, F)>);
impl<A: TypedGet, F: fungibles::Balanced<A::Type>> OnUnbalanced<fungibles::Credit<A::Type, F>>
	for ResolveAssetTo<A, F>
{
	fn on_nonzero_unbalanced(credit: fungibles::Credit<A::Type, F>) {
		let _ = F::resolve(&A::get(), credit).map_err(|c| drop(c));
	}
}
