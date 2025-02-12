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

//! Cross-Consensus Message format asset data structures.
//!
//! This encompasses four types for representing assets:
//! - `Asset`: A description of a single asset, either an instance of a non-fungible or some amount
//!   of a fungible.
//! - `Assets`: A collection of `Asset`s. These are stored in a `Vec` and sorted with fungibles
//!   first.
//! - `Wild`: A single asset wildcard, this can either be "all" assets, or all assets of a specific
//!   kind.
//! - `AssetFilter`: A combination of `Wild` and `Assets` designed for efficiently filtering an XCM
//!   holding account.

use super::{InteriorLocation, Location, Reanchorable};
use crate::{
	v3::{
		AssetId as OldAssetId, AssetInstance as OldAssetInstance, Fungibility as OldFungibility,
		MultiAsset as OldAsset, MultiAssetFilter as OldAssetFilter, MultiAssets as OldAssets,
		WildFungibility as OldWildFungibility, WildMultiAsset as OldWildAsset,
	},
	v5::{
		Asset as NewAsset, AssetFilter as NewAssetFilter, AssetId as NewAssetId,
		AssetInstance as NewAssetInstance, Assets as NewAssets, Fungibility as NewFungibility,
		WildAsset as NewWildAsset, WildFungibility as NewWildFungibility,
	},
};
use alloc::{vec, vec::Vec};
use bounded_collections::{BoundedVec, ConstU32};
use codec::{self as codec, Decode, Encode, MaxEncodedLen};
use core::cmp::Ordering;
use scale_info::TypeInfo;

/// A general identifier for an instance of a non-fungible asset class.
#[derive(
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	Debug,
	TypeInfo,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub enum AssetInstance {
	/// Undefined - used if the non-fungible asset class has only one instance.
	Undefined,

	/// A compact index. Technically this could be greater than `u128`, but this implementation
	/// supports only values up to `2**128 - 1`.
	Index(#[codec(compact)] u128),

	/// A 4-byte fixed-length datum.
	Array4([u8; 4]),

	/// An 8-byte fixed-length datum.
	Array8([u8; 8]),

	/// A 16-byte fixed-length datum.
	Array16([u8; 16]),

	/// A 32-byte fixed-length datum.
	Array32([u8; 32]),
}

impl TryFrom<OldAssetInstance> for AssetInstance {
	type Error = ();
	fn try_from(value: OldAssetInstance) -> Result<Self, Self::Error> {
		use OldAssetInstance::*;
		Ok(match value {
			Undefined => Self::Undefined,
			Index(n) => Self::Index(n),
			Array4(n) => Self::Array4(n),
			Array8(n) => Self::Array8(n),
			Array16(n) => Self::Array16(n),
			Array32(n) => Self::Array32(n),
		})
	}
}

impl TryFrom<NewAssetInstance> for AssetInstance {
	type Error = ();
	fn try_from(value: NewAssetInstance) -> Result<Self, Self::Error> {
		use NewAssetInstance::*;
		Ok(match value {
			Undefined => Self::Undefined,
			Index(n) => Self::Index(n),
			Array4(n) => Self::Array4(n),
			Array8(n) => Self::Array8(n),
			Array16(n) => Self::Array16(n),
			Array32(n) => Self::Array32(n),
		})
	}
}

impl From<()> for AssetInstance {
	fn from(_: ()) -> Self {
		Self::Undefined
	}
}

impl From<[u8; 4]> for AssetInstance {
	fn from(x: [u8; 4]) -> Self {
		Self::Array4(x)
	}
}

impl From<[u8; 8]> for AssetInstance {
	fn from(x: [u8; 8]) -> Self {
		Self::Array8(x)
	}
}

impl From<[u8; 16]> for AssetInstance {
	fn from(x: [u8; 16]) -> Self {
		Self::Array16(x)
	}
}

impl From<[u8; 32]> for AssetInstance {
	fn from(x: [u8; 32]) -> Self {
		Self::Array32(x)
	}
}

impl From<u8> for AssetInstance {
	fn from(x: u8) -> Self {
		Self::Index(x as u128)
	}
}

impl From<u16> for AssetInstance {
	fn from(x: u16) -> Self {
		Self::Index(x as u128)
	}
}

impl From<u32> for AssetInstance {
	fn from(x: u32) -> Self {
		Self::Index(x as u128)
	}
}

impl From<u64> for AssetInstance {
	fn from(x: u64) -> Self {
		Self::Index(x as u128)
	}
}

impl TryFrom<AssetInstance> for () {
	type Error = ();
	fn try_from(x: AssetInstance) -> Result<Self, ()> {
		match x {
			AssetInstance::Undefined => Ok(()),
			_ => Err(()),
		}
	}
}

impl TryFrom<AssetInstance> for [u8; 4] {
	type Error = ();
	fn try_from(x: AssetInstance) -> Result<Self, ()> {
		match x {
			AssetInstance::Array4(x) => Ok(x),
			_ => Err(()),
		}
	}
}

impl TryFrom<AssetInstance> for [u8; 8] {
	type Error = ();
	fn try_from(x: AssetInstance) -> Result<Self, ()> {
		match x {
			AssetInstance::Array8(x) => Ok(x),
			_ => Err(()),
		}
	}
}

impl TryFrom<AssetInstance> for [u8; 16] {
	type Error = ();
	fn try_from(x: AssetInstance) -> Result<Self, ()> {
		match x {
			AssetInstance::Array16(x) => Ok(x),
			_ => Err(()),
		}
	}
}

impl TryFrom<AssetInstance> for [u8; 32] {
	type Error = ();
	fn try_from(x: AssetInstance) -> Result<Self, ()> {
		match x {
			AssetInstance::Array32(x) => Ok(x),
			_ => Err(()),
		}
	}
}

impl TryFrom<AssetInstance> for u8 {
	type Error = ();
	fn try_from(x: AssetInstance) -> Result<Self, ()> {
		match x {
			AssetInstance::Index(x) => x.try_into().map_err(|_| ()),
			_ => Err(()),
		}
	}
}

impl TryFrom<AssetInstance> for u16 {
	type Error = ();
	fn try_from(x: AssetInstance) -> Result<Self, ()> {
		match x {
			AssetInstance::Index(x) => x.try_into().map_err(|_| ()),
			_ => Err(()),
		}
	}
}

impl TryFrom<AssetInstance> for u32 {
	type Error = ();
	fn try_from(x: AssetInstance) -> Result<Self, ()> {
		match x {
			AssetInstance::Index(x) => x.try_into().map_err(|_| ()),
			_ => Err(()),
		}
	}
}

impl TryFrom<AssetInstance> for u64 {
	type Error = ();
	fn try_from(x: AssetInstance) -> Result<Self, ()> {
		match x {
			AssetInstance::Index(x) => x.try_into().map_err(|_| ()),
			_ => Err(()),
		}
	}
}

impl TryFrom<AssetInstance> for u128 {
	type Error = ();
	fn try_from(x: AssetInstance) -> Result<Self, ()> {
		match x {
			AssetInstance::Index(x) => Ok(x),
			_ => Err(()),
		}
	}
}

impl TryFrom<NewFungibility> for Fungibility {
	type Error = ();
	fn try_from(value: NewFungibility) -> Result<Self, Self::Error> {
		use NewFungibility::*;
		Ok(match value {
			Fungible(n) => Self::Fungible(n),
			NonFungible(i) => Self::NonFungible(i.try_into()?),
		})
	}
}

/// Classification of whether an asset is fungible or not, along with a mandatory amount or
/// instance.
#[derive(
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Debug,
	Encode,
	TypeInfo,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub enum Fungibility {
	/// A fungible asset; we record a number of units, as a `u128` in the inner item.
	Fungible(#[codec(compact)] u128),
	/// A non-fungible asset. We record the instance identifier in the inner item. Only one asset
	/// of each instance identifier may ever be in existence at once.
	NonFungible(AssetInstance),
}

#[derive(Decode)]
enum UncheckedFungibility {
	Fungible(#[codec(compact)] u128),
	NonFungible(AssetInstance),
}

impl Decode for Fungibility {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		match UncheckedFungibility::decode(input)? {
			UncheckedFungibility::Fungible(a) if a != 0 => Ok(Self::Fungible(a)),
			UncheckedFungibility::NonFungible(i) => Ok(Self::NonFungible(i)),
			UncheckedFungibility::Fungible(_) =>
				Err("Fungible asset of zero amount is not allowed".into()),
		}
	}
}

impl Fungibility {
	pub fn is_kind(&self, w: WildFungibility) -> bool {
		use Fungibility::*;
		use WildFungibility::{Fungible as WildFungible, NonFungible as WildNonFungible};
		matches!((self, w), (Fungible(_), WildFungible) | (NonFungible(_), WildNonFungible))
	}
}

impl From<i32> for Fungibility {
	fn from(amount: i32) -> Fungibility {
		debug_assert_ne!(amount, 0);
		Fungibility::Fungible(amount as u128)
	}
}

impl From<u128> for Fungibility {
	fn from(amount: u128) -> Fungibility {
		debug_assert_ne!(amount, 0);
		Fungibility::Fungible(amount)
	}
}

impl<T: Into<AssetInstance>> From<T> for Fungibility {
	fn from(instance: T) -> Fungibility {
		Fungibility::NonFungible(instance.into())
	}
}

impl TryFrom<OldFungibility> for Fungibility {
	type Error = ();
	fn try_from(value: OldFungibility) -> Result<Self, Self::Error> {
		use OldFungibility::*;
		Ok(match value {
			Fungible(n) => Self::Fungible(n),
			NonFungible(i) => Self::NonFungible(i.try_into()?),
		})
	}
}

/// Classification of whether an asset is fungible or not.
#[derive(
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub enum WildFungibility {
	/// The asset is fungible.
	Fungible,
	/// The asset is not fungible.
	NonFungible,
}

impl TryFrom<OldWildFungibility> for WildFungibility {
	type Error = ();
	fn try_from(value: OldWildFungibility) -> Result<Self, Self::Error> {
		use OldWildFungibility::*;
		Ok(match value {
			Fungible => Self::Fungible,
			NonFungible => Self::NonFungible,
		})
	}
}

impl TryFrom<NewWildFungibility> for WildFungibility {
	type Error = ();
	fn try_from(value: NewWildFungibility) -> Result<Self, Self::Error> {
		use NewWildFungibility::*;
		Ok(match value {
			Fungible => Self::Fungible,
			NonFungible => Self::NonFungible,
		})
	}
}

/// Location to identify an asset.
#[derive(
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub struct AssetId(pub Location);

impl<T: Into<Location>> From<T> for AssetId {
	fn from(x: T) -> Self {
		Self(x.into())
	}
}

impl TryFrom<OldAssetId> for AssetId {
	type Error = ();
	fn try_from(old: OldAssetId) -> Result<Self, ()> {
		use OldAssetId::*;
		Ok(match old {
			Concrete(l) => Self(l.try_into()?),
			Abstract(_) => return Err(()),
		})
	}
}

impl TryFrom<NewAssetId> for AssetId {
	type Error = ();
	fn try_from(new: NewAssetId) -> Result<Self, Self::Error> {
		Ok(Self(new.0.try_into()?))
	}
}

impl AssetId {
	/// Prepend a `Location` to an asset id, giving it a new root location.
	pub fn prepend_with(&mut self, prepend: &Location) -> Result<(), ()> {
		self.0.prepend_with(prepend.clone()).map_err(|_| ())?;
		Ok(())
	}

	/// Use the value of `self` along with a `fun` fungibility specifier to create the corresponding
	/// `Asset` value.
	pub fn into_asset(self, fun: Fungibility) -> Asset {
		Asset { fun, id: self }
	}

	/// Use the value of `self` along with a `fun` fungibility specifier to create the corresponding
	/// `WildAsset` wildcard (`AllOf`) value.
	pub fn into_wild(self, fun: WildFungibility) -> WildAsset {
		WildAsset::AllOf { fun, id: self }
	}
}

impl Reanchorable for AssetId {
	type Error = ();

	/// Mutate the asset to represent the same value from the perspective of a new `target`
	/// location. The local chain's location is provided in `context`.
	fn reanchor(&mut self, target: &Location, context: &InteriorLocation) -> Result<(), ()> {
		self.0.reanchor(target, context)?;
		Ok(())
	}

	fn reanchored(mut self, target: &Location, context: &InteriorLocation) -> Result<Self, ()> {
		match self.reanchor(target, context) {
			Ok(()) => Ok(self),
			Err(()) => Err(()),
		}
	}
}

/// Either an amount of a single fungible asset, or a single well-identified non-fungible asset.
#[derive(
	Clone,
	Eq,
	PartialEq,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub struct Asset {
	/// The overall asset identity (aka *class*, in the case of a non-fungible).
	pub id: AssetId,
	/// The fungibility of the asset, which contains either the amount (in the case of a fungible
	/// asset) or the *instance ID*, the secondary asset identifier.
	pub fun: Fungibility,
}

impl PartialOrd for Asset {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for Asset {
	fn cmp(&self, other: &Self) -> Ordering {
		match (&self.fun, &other.fun) {
			(Fungibility::Fungible(..), Fungibility::NonFungible(..)) => Ordering::Less,
			(Fungibility::NonFungible(..), Fungibility::Fungible(..)) => Ordering::Greater,
			_ => (&self.id, &self.fun).cmp(&(&other.id, &other.fun)),
		}
	}
}

impl<A: Into<AssetId>, B: Into<Fungibility>> From<(A, B)> for Asset {
	fn from((id, fun): (A, B)) -> Asset {
		Asset { fun: fun.into(), id: id.into() }
	}
}

impl Asset {
	pub fn is_fungible(&self, maybe_id: Option<AssetId>) -> bool {
		use Fungibility::*;
		matches!(self.fun, Fungible(..)) && maybe_id.map_or(true, |i| i == self.id)
	}

	pub fn is_non_fungible(&self, maybe_id: Option<AssetId>) -> bool {
		use Fungibility::*;
		matches!(self.fun, NonFungible(..)) && maybe_id.map_or(true, |i| i == self.id)
	}

	/// Prepend a `Location` to a concrete asset, giving it a new root location.
	pub fn prepend_with(&mut self, prepend: &Location) -> Result<(), ()> {
		self.id.prepend_with(prepend)
	}

	/// Returns true if `self` is a super-set of the given `inner` asset.
	pub fn contains(&self, inner: &Asset) -> bool {
		use Fungibility::*;
		if self.id == inner.id {
			match (&self.fun, &inner.fun) {
				(Fungible(a), Fungible(i)) if a >= i => return true,
				(NonFungible(a), NonFungible(i)) if a == i => return true,
				_ => (),
			}
		}
		false
	}
}

impl Reanchorable for Asset {
	type Error = ();

	/// Mutate the location of the asset identifier if concrete, giving it the same location
	/// relative to a `target` context. The local context is provided as `context`.
	fn reanchor(&mut self, target: &Location, context: &InteriorLocation) -> Result<(), ()> {
		self.id.reanchor(target, context)
	}

	/// Mutate the location of the asset identifier if concrete, giving it the same location
	/// relative to a `target` context. The local context is provided as `context`.
	fn reanchored(mut self, target: &Location, context: &InteriorLocation) -> Result<Self, ()> {
		self.id.reanchor(target, context)?;
		Ok(self)
	}
}

impl TryFrom<OldAsset> for Asset {
	type Error = ();
	fn try_from(old: OldAsset) -> Result<Self, ()> {
		Ok(Self { id: old.id.try_into()?, fun: old.fun.try_into()? })
	}
}

impl TryFrom<NewAsset> for Asset {
	type Error = ();
	fn try_from(new: NewAsset) -> Result<Self, Self::Error> {
		Ok(Self { id: new.id.try_into()?, fun: new.fun.try_into()? })
	}
}

/// A `Vec` of `Asset`s.
///
/// There are a number of invariants which the construction and mutation functions must ensure are
/// maintained:
/// - It may contain no items of duplicate asset class;
/// - All items must be ordered;
/// - The number of items should grow no larger than `MAX_ITEMS_IN_ASSETS`.
#[derive(
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Debug,
	Encode,
	TypeInfo,
	Default,
	serde::Serialize,
	serde::Deserialize,
)]
pub struct Assets(Vec<Asset>);

/// Maximum number of items we expect in a single `Assets` value. Note this is not (yet)
/// enforced, and just serves to provide a sensible `max_encoded_len` for `Assets`.
pub const MAX_ITEMS_IN_ASSETS: usize = 20;

impl MaxEncodedLen for Assets {
	fn max_encoded_len() -> usize {
		Asset::max_encoded_len() * MAX_ITEMS_IN_ASSETS
	}
}

impl Decode for Assets {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let bounded_instructions =
			BoundedVec::<Asset, ConstU32<{ MAX_ITEMS_IN_ASSETS as u32 }>>::decode(input)?;
		Self::from_sorted_and_deduplicated(bounded_instructions.into_inner())
			.map_err(|()| "Out of order".into())
	}
}

impl TryFrom<OldAssets> for Assets {
	type Error = ();
	fn try_from(old: OldAssets) -> Result<Self, ()> {
		let v = old
			.into_inner()
			.into_iter()
			.map(Asset::try_from)
			.collect::<Result<Vec<_>, ()>>()?;
		Ok(Assets(v))
	}
}

impl TryFrom<NewAssets> for Assets {
	type Error = ();
	fn try_from(new: NewAssets) -> Result<Self, Self::Error> {
		let v = new
			.into_inner()
			.into_iter()
			.map(Asset::try_from)
			.collect::<Result<Vec<_>, ()>>()?;
		Ok(Assets(v))
	}
}

impl From<Vec<Asset>> for Assets {
	fn from(mut assets: Vec<Asset>) -> Self {
		let mut res = Vec::with_capacity(assets.len());
		if !assets.is_empty() {
			assets.sort();
			let mut iter = assets.into_iter();
			if let Some(first) = iter.next() {
				let last = iter.fold(first, |a, b| -> Asset {
					match (a, b) {
						(
							Asset { fun: Fungibility::Fungible(a_amount), id: a_id },
							Asset { fun: Fungibility::Fungible(b_amount), id: b_id },
						) if a_id == b_id => Asset {
							id: a_id,
							fun: Fungibility::Fungible(a_amount.saturating_add(b_amount)),
						},
						(
							Asset { fun: Fungibility::NonFungible(a_instance), id: a_id },
							Asset { fun: Fungibility::NonFungible(b_instance), id: b_id },
						) if a_id == b_id && a_instance == b_instance =>
							Asset { fun: Fungibility::NonFungible(a_instance), id: a_id },
						(to_push, to_remember) => {
							res.push(to_push);
							to_remember
						},
					}
				});
				res.push(last);
			}
		}
		Self(res)
	}
}

impl<T: Into<Asset>> From<T> for Assets {
	fn from(x: T) -> Self {
		Self(vec![x.into()])
	}
}

impl Assets {
	/// A new (empty) value.
	pub fn new() -> Self {
		Self(Vec::new())
	}

	/// Create a new instance of `Assets` from a `Vec<Asset>` whose contents are sorted
	/// and which contain no duplicates.
	///
	/// Returns `Ok` if the operation succeeds and `Err` if `r` is out of order or had duplicates.
	/// If you can't guarantee that `r` is sorted and deduplicated, then use
	/// `From::<Vec<Asset>>::from` which is infallible.
	pub fn from_sorted_and_deduplicated(r: Vec<Asset>) -> Result<Self, ()> {
		if r.is_empty() {
			return Ok(Self(Vec::new()))
		}
		r.iter().skip(1).try_fold(&r[0], |a, b| -> Result<&Asset, ()> {
			if a.id < b.id || a < b && (a.is_non_fungible(None) || b.is_non_fungible(None)) {
				Ok(b)
			} else {
				Err(())
			}
		})?;
		Ok(Self(r))
	}

	/// Create a new instance of `Assets` from a `Vec<Asset>` whose contents are sorted
	/// and which contain no duplicates.
	///
	/// In release mode, this skips any checks to ensure that `r` is correct, making it a
	/// negligible-cost operation. Generally though you should avoid using it unless you have a
	/// strict proof that `r` is valid.
	#[cfg(test)]
	pub fn from_sorted_and_deduplicated_skip_checks(r: Vec<Asset>) -> Self {
		Self::from_sorted_and_deduplicated(r).expect("Invalid input r is not sorted/deduped")
	}
	/// Create a new instance of `Assets` from a `Vec<Asset>` whose contents are sorted
	/// and which contain no duplicates.
	///
	/// In release mode, this skips any checks to ensure that `r` is correct, making it a
	/// negligible-cost operation. Generally though you should avoid using it unless you have a
	/// strict proof that `r` is valid.
	///
	/// In test mode, this checks anyway and panics on fail.
	#[cfg(not(test))]
	pub fn from_sorted_and_deduplicated_skip_checks(r: Vec<Asset>) -> Self {
		Self(r)
	}

	/// Add some asset onto the list, saturating. This is quite a laborious operation since it
	/// maintains the ordering.
	pub fn push(&mut self, a: Asset) {
		for asset in self.0.iter_mut().filter(|x| x.id == a.id) {
			match (&a.fun, &mut asset.fun) {
				(Fungibility::Fungible(amount), Fungibility::Fungible(balance)) => {
					*balance = balance.saturating_add(*amount);
					return
				},
				(Fungibility::NonFungible(inst1), Fungibility::NonFungible(inst2))
					if inst1 == inst2 =>
					return,
				_ => (),
			}
		}
		self.0.push(a);
		self.0.sort();
	}

	/// Returns `true` if this definitely represents no asset.
	pub fn is_none(&self) -> bool {
		self.0.is_empty()
	}

	/// Returns true if `self` is a super-set of the given `inner` asset.
	pub fn contains(&self, inner: &Asset) -> bool {
		self.0.iter().any(|i| i.contains(inner))
	}

	/// Consume `self` and return the inner vec.
	#[deprecated = "Use `into_inner()` instead"]
	pub fn drain(self) -> Vec<Asset> {
		self.0
	}

	/// Consume `self` and return the inner vec.
	pub fn into_inner(self) -> Vec<Asset> {
		self.0
	}

	/// Return a reference to the inner vec.
	pub fn inner(&self) -> &Vec<Asset> {
		&self.0
	}

	/// Return the number of distinct asset instances contained.
	pub fn len(&self) -> usize {
		self.0.len()
	}

	/// Prepend a `Location` to any concrete asset items, giving it a new root location.
	pub fn prepend_with(&mut self, prefix: &Location) -> Result<(), ()> {
		self.0.iter_mut().try_for_each(|i| i.prepend_with(prefix))?;
		self.0.sort();
		Ok(())
	}

	/// Return a reference to an item at a specific index or `None` if it doesn't exist.
	pub fn get(&self, index: usize) -> Option<&Asset> {
		self.0.get(index)
	}
}

impl Reanchorable for Assets {
	type Error = ();

	fn reanchor(&mut self, target: &Location, context: &InteriorLocation) -> Result<(), ()> {
		self.0.iter_mut().try_for_each(|i| i.reanchor(target, context))?;
		self.0.sort();
		Ok(())
	}

	fn reanchored(mut self, target: &Location, context: &InteriorLocation) -> Result<Self, ()> {
		match self.reanchor(target, context) {
			Ok(()) => Ok(self),
			Err(()) => Err(()),
		}
	}
}

/// A wildcard representing a set of assets.
#[derive(
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub enum WildAsset {
	/// All assets in Holding.
	All,
	/// All assets in Holding of a given fungibility and ID.
	AllOf { id: AssetId, fun: WildFungibility },
	/// All assets in Holding, up to `u32` individual assets (different instances of non-fungibles
	/// are separate assets).
	AllCounted(#[codec(compact)] u32),
	/// All assets in Holding of a given fungibility and ID up to `count` individual assets
	/// (different instances of non-fungibles are separate assets).
	AllOfCounted {
		id: AssetId,
		fun: WildFungibility,
		#[codec(compact)]
		count: u32,
	},
}

impl TryFrom<OldWildAsset> for WildAsset {
	type Error = ();
	fn try_from(old: OldWildAsset) -> Result<WildAsset, ()> {
		use OldWildAsset::*;
		Ok(match old {
			AllOf { id, fun } => Self::AllOf { id: id.try_into()?, fun: fun.try_into()? },
			All => Self::All,
			AllOfCounted { id, fun, count } =>
				Self::AllOfCounted { id: id.try_into()?, fun: fun.try_into()?, count },
			AllCounted(count) => Self::AllCounted(count),
		})
	}
}

impl TryFrom<NewWildAsset> for WildAsset {
	type Error = ();
	fn try_from(new: NewWildAsset) -> Result<Self, ()> {
		use NewWildAsset::*;
		Ok(match new {
			AllOf { id, fun } => Self::AllOf { id: id.try_into()?, fun: fun.try_into()? },
			AllOfCounted { id, fun, count } =>
				Self::AllOfCounted { id: id.try_into()?, fun: fun.try_into()?, count },
			All => Self::All,
			AllCounted(count) => Self::AllCounted(count),
		})
	}
}

impl WildAsset {
	/// Returns true if `self` is a super-set of the given `inner` asset.
	pub fn contains(&self, inner: &Asset) -> bool {
		use WildAsset::*;
		match self {
			AllOfCounted { count: 0, .. } | AllCounted(0) => false,
			AllOf { fun, id } | AllOfCounted { id, fun, .. } =>
				inner.fun.is_kind(*fun) && &inner.id == id,
			All | AllCounted(_) => true,
		}
	}

	/// Returns true if the wild element of `self` matches `inner`.
	///
	/// Note that for `Counted` variants of wildcards, then it will disregard the count except for
	/// always returning `false` when equal to 0.
	#[deprecated = "Use `contains` instead"]
	pub fn matches(&self, inner: &Asset) -> bool {
		self.contains(inner)
	}

	/// Mutate the asset to represent the same value from the perspective of a new `target`
	/// location. The local chain's location is provided in `context`.
	pub fn reanchor(&mut self, target: &Location, context: &InteriorLocation) -> Result<(), ()> {
		use WildAsset::*;
		match self {
			AllOf { ref mut id, .. } | AllOfCounted { ref mut id, .. } =>
				id.reanchor(target, context),
			All | AllCounted(_) => Ok(()),
		}
	}

	/// Maximum count of assets allowed to match, if any.
	pub fn count(&self) -> Option<u32> {
		use WildAsset::*;
		match self {
			AllOfCounted { count, .. } | AllCounted(count) => Some(*count),
			All | AllOf { .. } => None,
		}
	}

	/// Explicit limit on number of assets allowed to match, if any.
	pub fn limit(&self) -> Option<u32> {
		self.count()
	}

	/// Consume self and return the equivalent version but counted and with the `count` set to the
	/// given parameter.
	pub fn counted(self, count: u32) -> Self {
		use WildAsset::*;
		match self {
			AllOfCounted { fun, id, .. } | AllOf { fun, id } => AllOfCounted { fun, id, count },
			All | AllCounted(_) => AllCounted(count),
		}
	}
}

impl<A: Into<AssetId>, B: Into<WildFungibility>> From<(A, B)> for WildAsset {
	fn from((id, fun): (A, B)) -> WildAsset {
		WildAsset::AllOf { fun: fun.into(), id: id.into() }
	}
}

/// `Asset` collection, defined either by a number of `Assets` or a single wildcard.
#[derive(
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub enum AssetFilter {
	/// Specify the filter as being everything contained by the given `Assets` inner.
	Definite(Assets),
	/// Specify the filter as the given `WildAsset` wildcard.
	Wild(WildAsset),
}

impl<T: Into<WildAsset>> From<T> for AssetFilter {
	fn from(x: T) -> Self {
		Self::Wild(x.into())
	}
}

impl From<Asset> for AssetFilter {
	fn from(x: Asset) -> Self {
		Self::Definite(vec![x].into())
	}
}

impl From<Vec<Asset>> for AssetFilter {
	fn from(x: Vec<Asset>) -> Self {
		Self::Definite(x.into())
	}
}

impl From<Assets> for AssetFilter {
	fn from(x: Assets) -> Self {
		Self::Definite(x)
	}
}

impl AssetFilter {
	/// Returns true if `inner` would be matched by `self`.
	///
	/// Note that for `Counted` variants of wildcards, then it will disregard the count except for
	/// always returning `false` when equal to 0.
	pub fn matches(&self, inner: &Asset) -> bool {
		match self {
			AssetFilter::Definite(ref assets) => assets.contains(inner),
			AssetFilter::Wild(ref wild) => wild.contains(inner),
		}
	}

	/// Mutate the location of the asset identifier if concrete, giving it the same location
	/// relative to a `target` context. The local context is provided as `context`.
	pub fn reanchor(&mut self, target: &Location, context: &InteriorLocation) -> Result<(), ()> {
		match self {
			AssetFilter::Definite(ref mut assets) => assets.reanchor(target, context),
			AssetFilter::Wild(ref mut wild) => wild.reanchor(target, context),
		}
	}

	/// Maximum count of assets it is possible to match, if known.
	pub fn count(&self) -> Option<u32> {
		use AssetFilter::*;
		match self {
			Definite(x) => Some(x.len() as u32),
			Wild(x) => x.count(),
		}
	}

	/// Explicit limit placed on the number of items, if any.
	pub fn limit(&self) -> Option<u32> {
		use AssetFilter::*;
		match self {
			Definite(_) => None,
			Wild(x) => x.limit(),
		}
	}
}

impl TryFrom<NewAssetFilter> for AssetFilter {
	type Error = ();
	fn try_from(new: NewAssetFilter) -> Result<AssetFilter, Self::Error> {
		use NewAssetFilter::*;
		Ok(match new {
			Definite(x) => Self::Definite(x.try_into()?),
			Wild(x) => Self::Wild(x.try_into()?),
		})
	}
}

impl TryFrom<OldAssetFilter> for AssetFilter {
	type Error = ();
	fn try_from(old: OldAssetFilter) -> Result<AssetFilter, ()> {
		Ok(match old {
			OldAssetFilter::Definite(x) => Self::Definite(x.try_into()?),
			OldAssetFilter::Wild(x) => Self::Wild(x.try_into()?),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::super::prelude::*;

	#[test]
	fn conversion_works() {
		let _: Assets = (Here, 1u128).into();
	}

	#[test]
	fn from_sorted_and_deduplicated_works() {
		use super::*;
		use alloc::vec;

		let empty = vec![];
		let r = Assets::from_sorted_and_deduplicated(empty);
		assert_eq!(r, Ok(Assets(vec![])));

		let dup_fun = vec![(Here, 100).into(), (Here, 10).into()];
		let r = Assets::from_sorted_and_deduplicated(dup_fun);
		assert!(r.is_err());

		let dup_nft = vec![(Here, *b"notgood!").into(), (Here, *b"notgood!").into()];
		let r = Assets::from_sorted_and_deduplicated(dup_nft);
		assert!(r.is_err());

		let good_fun = vec![(Here, 10).into(), (Parent, 10).into()];
		let r = Assets::from_sorted_and_deduplicated(good_fun.clone());
		assert_eq!(r, Ok(Assets(good_fun)));

		let bad_fun = vec![(Parent, 10).into(), (Here, 10).into()];
		let r = Assets::from_sorted_and_deduplicated(bad_fun);
		assert!(r.is_err());

		let good_nft = vec![(Here, ()).into(), (Here, *b"good").into()];
		let r = Assets::from_sorted_and_deduplicated(good_nft.clone());
		assert_eq!(r, Ok(Assets(good_nft)));

		let bad_nft = vec![(Here, *b"bad!").into(), (Here, ()).into()];
		let r = Assets::from_sorted_and_deduplicated(bad_nft);
		assert!(r.is_err());

		let mixed_good = vec![(Here, 10).into(), (Here, *b"good").into()];
		let r = Assets::from_sorted_and_deduplicated(mixed_good.clone());
		assert_eq!(r, Ok(Assets(mixed_good)));

		let mixed_bad = vec![(Here, *b"bad!").into(), (Here, 10).into()];
		let r = Assets::from_sorted_and_deduplicated(mixed_bad);
		assert!(r.is_err());
	}

	#[test]
	fn reanchor_preserves_sorting() {
		use super::*;
		use alloc::vec;

		let reanchor_context: Junctions = Parachain(2000).into();
		let dest = Location::new(1, []);

		let asset_1: Asset = (Location::new(0, [PalletInstance(50), GeneralIndex(1)]), 10).into();
		let mut asset_1_reanchored = asset_1.clone();
		assert!(asset_1_reanchored.reanchor(&dest, &reanchor_context).is_ok());
		assert_eq!(
			asset_1_reanchored,
			(Location::new(0, [Parachain(2000), PalletInstance(50), GeneralIndex(1)]), 10).into()
		);

		let asset_2: Asset = (Location::new(1, []), 10).into();
		let mut asset_2_reanchored = asset_2.clone();
		assert!(asset_2_reanchored.reanchor(&dest, &reanchor_context).is_ok());
		assert_eq!(asset_2_reanchored, (Location::new(0, []), 10).into());

		let asset_3: Asset = (Location::new(1, [Parachain(1000)]), 10).into();
		let mut asset_3_reanchored = asset_3.clone();
		assert!(asset_3_reanchored.reanchor(&dest, &reanchor_context).is_ok());
		assert_eq!(asset_3_reanchored, (Location::new(0, [Parachain(1000)]), 10).into());

		let mut assets: Assets = vec![asset_1.clone(), asset_2.clone(), asset_3.clone()].into();
		assert_eq!(assets.clone(), vec![asset_1.clone(), asset_2.clone(), asset_3.clone()].into());

		// decoding respects limits and sorting
		assert!(assets.using_encoded(|mut enc| Assets::decode(&mut enc).map(|_| ())).is_ok());

		assert!(assets.reanchor(&dest, &reanchor_context).is_ok());
		assert_eq!(assets.0, vec![asset_2_reanchored, asset_3_reanchored, asset_1_reanchored]);

		// decoding respects limits and sorting
		assert!(assets.using_encoded(|mut enc| Assets::decode(&mut enc).map(|_| ())).is_ok());
	}

	#[test]
	fn prepend_preserves_sorting() {
		use super::*;
		use alloc::vec;

		let prefix = Location::new(0, [Parachain(1000)]);

		let asset_1: Asset = (Location::new(0, [PalletInstance(50), GeneralIndex(1)]), 10).into();
		let mut asset_1_prepended = asset_1.clone();
		assert!(asset_1_prepended.prepend_with(&prefix).is_ok());
		// changes interior X2->X3
		assert_eq!(
			asset_1_prepended,
			(Location::new(0, [Parachain(1000), PalletInstance(50), GeneralIndex(1)]), 10).into()
		);

		let asset_2: Asset = (Location::new(1, [PalletInstance(50), GeneralIndex(1)]), 10).into();
		let mut asset_2_prepended = asset_2.clone();
		assert!(asset_2_prepended.prepend_with(&prefix).is_ok());
		// changes parent
		assert_eq!(
			asset_2_prepended,
			(Location::new(0, [PalletInstance(50), GeneralIndex(1)]), 10).into()
		);

		let asset_3: Asset = (Location::new(2, [PalletInstance(50), GeneralIndex(1)]), 10).into();
		let mut asset_3_prepended = asset_3.clone();
		assert!(asset_3_prepended.prepend_with(&prefix).is_ok());
		// changes parent
		assert_eq!(
			asset_3_prepended,
			(Location::new(1, [PalletInstance(50), GeneralIndex(1)]), 10).into()
		);

		// `From` impl does sorting.
		let mut assets: Assets = vec![asset_1, asset_2, asset_3].into();
		// decoding respects limits and sorting
		assert!(assets.using_encoded(|mut enc| Assets::decode(&mut enc).map(|_| ())).is_ok());

		// let's do `prepend_with`
		assert!(assets.prepend_with(&prefix).is_ok());
		assert_eq!(assets.0, vec![asset_2_prepended, asset_1_prepended, asset_3_prepended]);

		// decoding respects limits and sorting
		assert!(assets.using_encoded(|mut enc| Assets::decode(&mut enc).map(|_| ())).is_ok());
	}

	#[test]
	fn decoding_respects_limit() {
		use super::*;

		// Having lots of one asset will work since they are deduplicated
		let lots_of_one_asset: Assets =
			vec![(GeneralIndex(1), 1u128).into(); MAX_ITEMS_IN_ASSETS + 1].into();
		let encoded = lots_of_one_asset.encode();
		assert!(Assets::decode(&mut &encoded[..]).is_ok());

		// Fewer assets than the limit works
		let mut few_assets: Assets = Vec::new().into();
		for i in 0..MAX_ITEMS_IN_ASSETS {
			few_assets.push((GeneralIndex(i as u128), 1u128).into());
		}
		let encoded = few_assets.encode();
		assert!(Assets::decode(&mut &encoded[..]).is_ok());

		// Having lots of different assets will not work
		let mut too_many_different_assets: Assets = Vec::new().into();
		for i in 0..MAX_ITEMS_IN_ASSETS + 1 {
			too_many_different_assets.push((GeneralIndex(i as u128), 1u128).into());
		}
		let encoded = too_many_different_assets.encode();
		assert!(Assets::decode(&mut &encoded[..]).is_err());
	}
}
