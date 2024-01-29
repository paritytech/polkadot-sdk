// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Various implementations for the `MatchesFungible` trait.

use frame_support::traits::Get;
use sp_std::marker::PhantomData;
use xcm::latest::{
	Asset, AssetId, AssetInstance,
	Fungibility::{Fungible, NonFungible},
	Location,
};
use xcm_executor::traits::{MatchesFungible, MatchesNonFungible};

/// Converts a `Asset` into balance `B` if its id is equal to that
/// given by `T`'s `Get`.
///
/// # Example
///
/// ```
/// use xcm::latest::{Location, Parent};
/// use staging_xcm_builder::IsConcrete;
/// use xcm_executor::traits::MatchesFungible;
///
/// frame_support::parameter_types! {
/// 	pub TargetLocation: Location = Parent.into();
/// }
///
/// # fn main() {
/// let asset = (Parent, 999).into();
/// // match `asset` if it is a concrete asset in `TargetLocation`.
/// assert_eq!(<IsConcrete<TargetLocation> as MatchesFungible<u128>>::matches_fungible(&asset), Some(999));
/// # }
/// ```
pub struct IsConcrete<T>(PhantomData<T>);
impl<T: Get<Location>, B: TryFrom<u128>> MatchesFungible<B> for IsConcrete<T> {
	fn matches_fungible(a: &Asset) -> Option<B> {
		match (&a.id, &a.fun) {
			(AssetId(ref id), Fungible(ref amount)) if id == &T::get() => (*amount).try_into().ok(),
			_ => None,
		}
	}
}
impl<T: Get<Location>, I: TryFrom<AssetInstance>> MatchesNonFungible<I> for IsConcrete<T> {
	fn matches_nonfungible(a: &Asset) -> Option<I> {
		match (&a.id, &a.fun) {
			(AssetId(id), NonFungible(instance)) if id == &T::get() => (*instance).try_into().ok(),
			_ => None,
		}
	}
}
