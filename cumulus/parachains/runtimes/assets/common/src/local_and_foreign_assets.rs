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

use frame_support::traits::Get;
use sp_runtime::{
	traits::{Convert, MaybeEquivalence},
	Either,
	Either::{Left, Right},
};
use sp_std::marker::PhantomData;
use xcm::latest::Location;

/// Converts a given [`Location`] to [`Either::Left`] when equal to `Target`, or
/// [`Either::Right`] otherwise.
///
/// Suitable for use as a `Criterion` with [`frame_support::traits::tokens::fungible::UnionOf`].
pub struct TargetFromLeft<Target>(PhantomData<Target>);
impl<Target: Get<Location>> Convert<Location, Either<(), Location>>
	for TargetFromLeft<Target>
{
	fn convert(l: Location) -> Either<(), Location> {
		Target::get().eq(&l).then(|| Left(())).map_or(Right(l), |n| n)
	}
}

/// Converts a given [`Location`] to [`Either::Left`] based on the `Equivalence` criteria.
/// Returns [`Either::Right`] if not equivalent.
///
/// Suitable for use as a `Criterion` with [`frame_support::traits::tokens::fungibles::UnionOf`].
pub struct LocalFromLeft<Equivalence, AssetId>(PhantomData<(Equivalence, AssetId)>);
impl<Equivalence, AssetId> Convert<Location, Either<AssetId, Location>>
	for LocalFromLeft<Equivalence, AssetId>
where
	Equivalence: MaybeEquivalence<Location, AssetId>,
{
	fn convert(l: Location) -> Either<AssetId, Location> {
		match Equivalence::convert(&l) {
			Some(id) => Left(id),
			None => Right(l),
		}
	}
}

pub trait MatchesLocalAndForeignAssetsLocation<L = Location> {
	fn is_local(location: &L) -> bool;
	fn is_foreign(location: &L) -> bool;
}
