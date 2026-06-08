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

//! Traits, types and structs to support a bounded `BTreeMap` whose bound is *not* strictly
//! enforced on decoding.

use crate::{storage::StorageDecodeLength, traits::Get};
use alloc::collections::BTreeMap;
use codec::{
	Compact, Decode, DecodeLength, DecodeWithMemTracking, Encode, Error, Input, MaxEncodedLen,
};
use core::{borrow::Borrow, marker::PhantomData, ops::Deref};

/// A weakly bounded map based on a B-Tree.
///
/// This is the map counterpart of [`WeakBoundedVec`](super::weak_bounded_vec::WeakBoundedVec): it
/// behaves like a [`BoundedBTreeMap`](super::bounded_btree_map::BoundedBTreeMap), except that the
/// bound `S` is *not* strictly enforced when decoding. Decoding a map with more entries than `S`
/// succeeds (and logs a warning), instead of failing.
///
/// This is useful when the bound `S` is runtime-dynamic and may shrink: an on-chain value that was
/// valid under a larger bound can still be decoded under the smaller one, instead of becoming
/// undecodable (which would silently look like an absent value). All mutating operations still
/// respect the bound, so the map is compacted back to within `S` on the next mutation.
#[derive(Encode, scale_info::TypeInfo)]
#[scale_info(skip_type_params(S))]
pub struct WeakBoundedBTreeMap<K, V, S>(BTreeMap<K, V>, PhantomData<S>);

impl<K, V, S> WeakBoundedBTreeMap<K, V, S>
where
	S: Get<u32>,
{
	/// Get the bound of the type in `usize`.
	pub fn bound() -> usize {
		S::get() as usize
	}
}

impl<K, V, S> WeakBoundedBTreeMap<K, V, S>
where
	K: Ord,
	S: Get<u32>,
{
	/// Create `Self` from `t` without any checks.
	fn unchecked_from(t: BTreeMap<K, V>) -> Self {
		Self(t, Default::default())
	}

	/// Create `Self` from `t` without any checks. Logs warnings if the bound is not being
	/// respected. The additional scope can be used to indicate where a potential overflow is
	/// happening.
	pub fn force_from(t: BTreeMap<K, V>, scope: Option<&'static str>) -> Self {
		if t.len() > Self::bound() {
			log::warn!(
				target: "runtime",
				"length of a weakly bounded btree map in scope {} is not respected.",
				scope.unwrap_or("UNKNOWN"),
			);
		}

		Self::unchecked_from(t)
	}

	/// Exactly the same semantics as `BTreeMap::retain`.
	///
	/// This is a safe `&mut self` borrow because `retain` can only ever decrease the length of the
	/// inner map.
	pub fn retain<F: FnMut(&K, &mut V) -> bool>(&mut self, f: F) {
		self.0.retain(f)
	}

	/// Create a new `WeakBoundedBTreeMap`.
	///
	/// Does not allocate.
	pub fn new() -> Self {
		Self(BTreeMap::new(), PhantomData)
	}

	/// Consume self, and return the inner `BTreeMap`.
	///
	/// This is useful when a mutating API of the inner type is desired, and closure-based mutation
	/// such as provided by [`try_mutate`][Self::try_mutate] is inconvenient.
	pub fn into_inner(self) -> BTreeMap<K, V> {
		self.0
	}

	/// Consumes self and mutates self via the given `mutate` function.
	///
	/// If the outcome of mutation is within bounds, `Some(Self)` is returned. Else, `None` is
	/// returned.
	///
	/// This is essentially a *consuming* shorthand [`Self::into_inner`] -> `...` ->
	/// [`Self::try_from`].
	pub fn try_mutate(mut self, mut mutate: impl FnMut(&mut BTreeMap<K, V>)) -> Option<Self> {
		mutate(&mut self.0);
		(self.0.len() <= Self::bound()).then(move || self)
	}

	/// Clears the map, removing all elements.
	pub fn clear(&mut self) {
		self.0.clear()
	}

	/// Return a mutable reference to the value corresponding to the key.
	///
	/// The key may be any borrowed form of the map's key type, but the ordering on the borrowed
	/// form _must_ match the ordering on the key type.
	pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
	where
		K: Borrow<Q>,
		Q: Ord + ?Sized,
	{
		self.0.get_mut(key)
	}

	/// Exactly the same semantics as [`BTreeMap::insert`], but returns an `Err` (and is a noop) if
	/// the new length of the map exceeds `S`.
	///
	/// In the `Err` case, returns the inserted pair so it can be further used without cloning.
	pub fn try_insert(&mut self, key: K, value: V) -> Result<Option<V>, (K, V)> {
		if self.len() < Self::bound() || self.0.contains_key(&key) {
			Ok(self.0.insert(key, value))
		} else {
			Err((key, value))
		}
	}

	/// Remove a key from the map, returning the value at the key if the key was previously in the
	/// map.
	///
	/// The key may be any borrowed form of the map's key type, but the ordering on the borrowed
	/// form _must_ match the ordering on the key type.
	pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
	where
		K: Borrow<Q>,
		Q: Ord + ?Sized,
	{
		self.0.remove(key)
	}

	/// Remove a key from the map, returning the value at the key if the key was previously in the
	/// map.
	///
	/// The key may be any borrowed form of the map's key type, but the ordering on the borrowed
	/// form _must_ match the ordering on the key type.
	pub fn remove_entry<Q>(&mut self, key: &Q) -> Option<(K, V)>
	where
		K: Borrow<Q>,
		Q: Ord + ?Sized,
	{
		self.0.remove_entry(key)
	}

	/// Gets a mutable iterator over the entries of the map, sorted by key.
	///
	/// See [`BTreeMap::iter_mut`] for more information.
	pub fn iter_mut(&mut self) -> alloc::collections::btree_map::IterMut<'_, K, V> {
		self.0.iter_mut()
	}

	/// Returns true if this map is full.
	pub fn is_full(&self) -> bool {
		self.len() >= Self::bound()
	}
}

impl<K, V, S> Default for WeakBoundedBTreeMap<K, V, S>
where
	K: Ord,
	S: Get<u32>,
{
	fn default() -> Self {
		Self::new()
	}
}

impl<K, V, S> Clone for WeakBoundedBTreeMap<K, V, S>
where
	BTreeMap<K, V>: Clone,
{
	fn clone(&self) -> Self {
		Self(self.0.clone(), PhantomData)
	}
}

impl<K, V, S> core::fmt::Debug for WeakBoundedBTreeMap<K, V, S>
where
	BTreeMap<K, V>: core::fmt::Debug,
	S: Get<u32>,
{
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_tuple("WeakBoundedBTreeMap")
			.field(&self.0)
			.field(&Self::bound())
			.finish()
	}
}

impl<K, V, S1, S2> PartialEq<WeakBoundedBTreeMap<K, V, S1>> for WeakBoundedBTreeMap<K, V, S2>
where
	BTreeMap<K, V>: PartialEq,
	S1: Get<u32>,
	S2: Get<u32>,
{
	fn eq(&self, other: &WeakBoundedBTreeMap<K, V, S1>) -> bool {
		S1::get() == S2::get() && self.0 == other.0
	}
}

impl<K, V, S> Eq for WeakBoundedBTreeMap<K, V, S>
where
	BTreeMap<K, V>: Eq,
	S: Get<u32>,
{
}

impl<K, V, S> PartialEq<BTreeMap<K, V>> for WeakBoundedBTreeMap<K, V, S>
where
	BTreeMap<K, V>: PartialEq,
{
	fn eq(&self, other: &BTreeMap<K, V>) -> bool {
		self.0 == *other
	}
}

impl<K, V, S> PartialOrd for WeakBoundedBTreeMap<K, V, S>
where
	BTreeMap<K, V>: PartialOrd,
	S: Get<u32>,
{
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		self.0.partial_cmp(&other.0)
	}
}

impl<K, V, S> Ord for WeakBoundedBTreeMap<K, V, S>
where
	BTreeMap<K, V>: Ord,
	S: Get<u32>,
{
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.0.cmp(&other.0)
	}
}

impl<K, V, S> IntoIterator for WeakBoundedBTreeMap<K, V, S> {
	type Item = (K, V);
	type IntoIter = alloc::collections::btree_map::IntoIter<K, V>;

	fn into_iter(self) -> Self::IntoIter {
		self.0.into_iter()
	}
}

impl<'a, K, V, S> IntoIterator for &'a WeakBoundedBTreeMap<K, V, S> {
	type Item = (&'a K, &'a V);
	type IntoIter = alloc::collections::btree_map::Iter<'a, K, V>;

	fn into_iter(self) -> Self::IntoIter {
		self.0.iter()
	}
}

impl<'a, K, V, S> IntoIterator for &'a mut WeakBoundedBTreeMap<K, V, S> {
	type Item = (&'a K, &'a mut V);
	type IntoIter = alloc::collections::btree_map::IterMut<'a, K, V>;

	fn into_iter(self) -> Self::IntoIter {
		self.0.iter_mut()
	}
}

impl<K, V, S> Deref for WeakBoundedBTreeMap<K, V, S>
where
	K: Ord,
{
	type Target = BTreeMap<K, V>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<K, V, S> AsRef<BTreeMap<K, V>> for WeakBoundedBTreeMap<K, V, S>
where
	K: Ord,
{
	fn as_ref(&self) -> &BTreeMap<K, V> {
		&self.0
	}
}

impl<K, V, S> From<WeakBoundedBTreeMap<K, V, S>> for BTreeMap<K, V>
where
	K: Ord,
{
	fn from(map: WeakBoundedBTreeMap<K, V, S>) -> Self {
		map.0
	}
}

impl<K, V, S> TryFrom<BTreeMap<K, V>> for WeakBoundedBTreeMap<K, V, S>
where
	K: Ord,
	S: Get<u32>,
{
	type Error = ();

	fn try_from(value: BTreeMap<K, V>) -> Result<Self, Self::Error> {
		(value.len() <= Self::bound()).then(move || Self(value, PhantomData)).ok_or(())
	}
}

impl<K, V, S> MaxEncodedLen for WeakBoundedBTreeMap<K, V, S>
where
	K: MaxEncodedLen,
	V: MaxEncodedLen,
	S: Get<u32>,
{
	fn max_encoded_len() -> usize {
		Self::bound()
			.saturating_mul(K::max_encoded_len().saturating_add(V::max_encoded_len()))
			.saturating_add(Compact(S::get()).encoded_size())
	}
}

impl<K, V, S> Decode for WeakBoundedBTreeMap<K, V, S>
where
	K: Decode + Ord,
	V: Decode,
	S: Get<u32>,
{
	fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
		// The bound is *not* enforced on decoding: an oversized map (e.g. one that was valid under
		// a larger, now-shrunk bound) is accepted with a warning, instead of failing to decode.
		let inner = BTreeMap::<K, V>::decode(input)?;
		Ok(Self::force_from(inner, Some("decode")))
	}

	fn skip<I: Input>(input: &mut I) -> Result<(), Error> {
		BTreeMap::<K, V>::skip(input)
	}
}

impl<K, V, S> DecodeWithMemTracking for WeakBoundedBTreeMap<K, V, S>
where
	K: DecodeWithMemTracking + Ord,
	V: DecodeWithMemTracking,
	S: Get<u32>,
{
}

impl<K, V, S> DecodeLength for WeakBoundedBTreeMap<K, V, S> {
	fn len(self_encoded: &[u8]) -> Result<usize, Error> {
		// `WeakBoundedBTreeMap<K, V, S>` is stored just as a `BTreeMap<K, V>`, which is stored as a
		// `Compact<u32>` with its length followed by an iteration of its items, so we can reuse the
		// underlying implementation.
		<BTreeMap<K, V> as DecodeLength>::len(self_encoded)
	}
}

impl<K, V, S> codec::EncodeLike<BTreeMap<K, V>> for WeakBoundedBTreeMap<K, V, S> where
	BTreeMap<K, V>: Encode
{
}

impl<K, V, S> StorageDecodeLength for WeakBoundedBTreeMap<K, V, S> {}

#[cfg(test)]
pub mod test {
	use super::*;
	use crate::traits::ConstU32;

	#[test]
	fn try_insert_respects_the_bound() {
		let mut map = WeakBoundedBTreeMap::<u32, u32, ConstU32<3>>::new();
		assert_eq!(map.try_insert(1, 1), Ok(None));
		assert_eq!(map.try_insert(2, 2), Ok(None));
		assert_eq!(map.try_insert(3, 3), Ok(None));
		// Full now: a new key is rejected...
		assert_eq!(map.try_insert(4, 4), Err((4, 4)));
		// ...but updating an existing key is fine.
		assert_eq!(map.try_insert(3, 30), Ok(Some(3)));
		assert_eq!(map.len(), 3);
	}

	#[test]
	fn oversized_map_decodes() {
		// Encode a 5-entry map...
		let unbounded: BTreeMap<u32, u32> = (0..5).map(|i| (i, i)).collect();
		let encoded = unbounded.encode();

		// ...and decode it against a bound of 3. The strict `BoundedBTreeMap` would reject this;
		// the weak variant accepts it and keeps all entries.
		let weak = WeakBoundedBTreeMap::<u32, u32, ConstU32<3>>::decode(&mut &encoded[..])
			.expect("weakly bounded map accepts oversized data");
		assert_eq!(weak.len(), 5);
		assert!(weak.len() > WeakBoundedBTreeMap::<u32, u32, ConstU32<3>>::bound());

		// It encodes identically to the inner `BTreeMap`, so it round-trips on disk.
		assert_eq!(weak.encode(), encoded);
	}

	#[test]
	fn force_from_keeps_all_entries() {
		let unbounded: BTreeMap<u32, u32> = (0..10).map(|i| (i, i)).collect();
		let weak = WeakBoundedBTreeMap::<u32, u32, ConstU32<4>>::force_from(unbounded, None);
		assert_eq!(weak.len(), 10);
	}
}
