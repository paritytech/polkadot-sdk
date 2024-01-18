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

//! XCM `Location` datatype.

use super::{traits::Reanchorable, Junction, Junctions};
use crate::{v3::MultiLocation as OldLocation, VersionedLocation};
use core::{
	convert::{TryFrom, TryInto},
	result,
};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// A relative path between state-bearing consensus systems.
///
/// A location in a consensus system is defined as an *isolatable state machine* held within global
/// consensus. The location in question need not have a sophisticated consensus algorithm of its
/// own; a single account within Ethereum, for example, could be considered a location.
///
/// A very-much non-exhaustive list of types of location include:
/// - A (normal, layer-1) block chain, e.g. the Bitcoin mainnet or a parachain.
/// - A layer-0 super-chain, e.g. the Polkadot Relay chain.
/// - A layer-2 smart contract, e.g. an ERC-20 on Ethereum.
/// - A logical functional component of a chain, e.g. a single instance of a pallet on a Frame-based
///   Substrate chain.
/// - An account.
///
/// A `Location` is a *relative identifier*, meaning that it can only be used to define the
/// relative path between two locations, and cannot generally be used to refer to a location
/// universally. It is comprised of an integer number of parents specifying the number of times to
/// "escape" upwards into the containing consensus system and then a number of *junctions*, each
/// diving down and specifying some interior portion of state (which may be considered a
/// "sub-consensus" system).
///
/// This specific `Location` implementation uses a `Junctions` datatype which is a Rust `enum`
/// in order to make pattern matching easier. There are occasions where it is important to ensure
/// that a value is strictly an interior location, in those cases, `Junctions` may be used.
///
/// The `Location` value of `Null` simply refers to the interpreting consensus system.
#[derive(
	Clone,
	Decode,
	Encode,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Debug,
	TypeInfo,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub struct Location {
	/// The number of parent junctions at the beginning of this `Location`.
	pub parents: u8,
	/// The interior (i.e. non-parent) junctions that this `Location` contains.
	pub interior: Junctions,
}

impl Default for Location {
	fn default() -> Self {
		Self { parents: 0, interior: Junctions::Here }
	}
}

/// A relative location which is constrained to be an interior location of the context.
///
/// See also `Location`.
pub type InteriorLocation = Junctions;

impl Location {
	/// Creates a new `Location` with the given number of parents and interior junctions.
	pub fn new(parents: u8, interior: impl Into<Junctions>) -> Location {
		Location { parents, interior: interior.into() }
	}

	/// Consume `self` and return the equivalent `VersionedLocation` value.
	pub const fn into_versioned(self) -> VersionedLocation {
		VersionedLocation::V4(self)
	}

	/// Creates a new `Location` with 0 parents and a `Here` interior.
	///
	/// The resulting `Location` can be interpreted as the "current consensus system".
	pub const fn here() -> Location {
		Location { parents: 0, interior: Junctions::Here }
	}

	/// Creates a new `Location` which evaluates to the parent context.
	pub const fn parent() -> Location {
		Location { parents: 1, interior: Junctions::Here }
	}

	/// Creates a new `Location` with `parents` and an empty (`Here`) interior.
	pub const fn ancestor(parents: u8) -> Location {
		Location { parents, interior: Junctions::Here }
	}

	/// Whether the `Location` has no parents and has a `Here` interior.
	pub fn is_here(&self) -> bool {
		self.parents == 0 && self.interior.len() == 0
	}

	/// Remove the `NetworkId` value in any interior `Junction`s.
	pub fn remove_network_id(&mut self) {
		self.interior.remove_network_id();
	}

	/// Return a reference to the interior field.
	pub fn interior(&self) -> &Junctions {
		&self.interior
	}

	/// Return a mutable reference to the interior field.
	pub fn interior_mut(&mut self) -> &mut Junctions {
		&mut self.interior
	}

	/// Returns the number of `Parent` junctions at the beginning of `self`.
	pub const fn parent_count(&self) -> u8 {
		self.parents
	}

	/// Returns the parent count and the interior [`Junctions`] as a tuple.
	///
	/// To be used when pattern matching, for example:
	///
	/// ```rust
	/// # use staging_xcm::v4::{Junctions::*, Junction::*, Location};
	/// fn get_parachain_id(loc: &Location) -> Option<u32> {
	///     match loc.unpack() {
	///         (0, [Parachain(id)]) => Some(*id),
	///         _ => None
	///     }
	/// }
	/// ```
	pub fn unpack(&self) -> (u8, &[Junction]) {
		(self.parents, self.interior.as_slice())
	}

	/// Returns boolean indicating whether `self` contains only the specified amount of
	/// parents and no interior junctions.
	pub const fn contains_parents_only(&self, count: u8) -> bool {
		matches!(self.interior, Junctions::Here) && self.parents == count
	}

	/// Returns the number of parents and junctions in `self`.
	pub fn len(&self) -> usize {
		self.parent_count() as usize + self.interior.len()
	}

	/// Returns the first interior junction, or `None` if the location is empty or contains only
	/// parents.
	pub fn first_interior(&self) -> Option<&Junction> {
		self.interior.first()
	}

	/// Returns last junction, or `None` if the location is empty or contains only parents.
	pub fn last(&self) -> Option<&Junction> {
		self.interior.last()
	}

	/// Splits off the first interior junction, returning the remaining suffix (first item in tuple)
	/// and the first element (second item in tuple) or `None` if it was empty.
	pub fn split_first_interior(self) -> (Location, Option<Junction>) {
		let Location { parents, interior: junctions } = self;
		let (suffix, first) = junctions.split_first();
		let location = Location { parents, interior: suffix };
		(location, first)
	}

	/// Splits off the last interior junction, returning the remaining prefix (first item in tuple)
	/// and the last element (second item in tuple) or `None` if it was empty or if `self` only
	/// contains parents.
	pub fn split_last_interior(self) -> (Location, Option<Junction>) {
		let Location { parents, interior: junctions } = self;
		let (prefix, last) = junctions.split_last();
		let location = Location { parents, interior: prefix };
		(location, last)
	}

	/// Mutates `self`, suffixing its interior junctions with `new`. Returns `Err` with `new` in
	/// case of overflow.
	pub fn push_interior(&mut self, new: impl Into<Junction>) -> result::Result<(), Junction> {
		self.interior.push(new)
	}

	/// Mutates `self`, prefixing its interior junctions with `new`. Returns `Err` with `new` in
	/// case of overflow.
	pub fn push_front_interior(
		&mut self,
		new: impl Into<Junction>,
	) -> result::Result<(), Junction> {
		self.interior.push_front(new)
	}

	/// Consumes `self` and returns a `Location` suffixed with `new`, or an `Err` with
	/// theoriginal value of `self` in case of overflow.
	pub fn pushed_with_interior(
		self,
		new: impl Into<Junction>,
	) -> result::Result<Self, (Self, Junction)> {
		match self.interior.pushed_with(new) {
			Ok(i) => Ok(Location { interior: i, parents: self.parents }),
			Err((i, j)) => Err((Location { interior: i, parents: self.parents }, j)),
		}
	}

	/// Consumes `self` and returns a `Location` prefixed with `new`, or an `Err` with the
	/// original value of `self` in case of overflow.
	pub fn pushed_front_with_interior(
		self,
		new: impl Into<Junction>,
	) -> result::Result<Self, (Self, Junction)> {
		match self.interior.pushed_front_with(new) {
			Ok(i) => Ok(Location { interior: i, parents: self.parents }),
			Err((i, j)) => Err((Location { interior: i, parents: self.parents }, j)),
		}
	}

	/// Returns the junction at index `i`, or `None` if the location is a parent or if the location
	/// does not contain that many elements.
	pub fn at(&self, i: usize) -> Option<&Junction> {
		let num_parents = self.parents as usize;
		if i < num_parents {
			return None
		}
		self.interior.at(i - num_parents)
	}

	/// Returns a mutable reference to the junction at index `i`, or `None` if the location is a
	/// parent or if it doesn't contain that many elements.
	pub fn at_mut(&mut self, i: usize) -> Option<&mut Junction> {
		let num_parents = self.parents as usize;
		if i < num_parents {
			return None
		}
		self.interior.at_mut(i - num_parents)
	}

	/// Decrements the parent count by 1.
	pub fn dec_parent(&mut self) {
		self.parents = self.parents.saturating_sub(1);
	}

	/// Removes the first interior junction from `self`, returning it
	/// (or `None` if it was empty or if `self` contains only parents).
	pub fn take_first_interior(&mut self) -> Option<Junction> {
		self.interior.take_first()
	}

	/// Removes the last element from `interior`, returning it (or `None` if it was empty or if
	/// `self` only contains parents).
	pub fn take_last(&mut self) -> Option<Junction> {
		self.interior.take_last()
	}

	/// Ensures that `self` has the same number of parents as `prefix`, its junctions begins with
	/// the junctions of `prefix` and that it has a single `Junction` item following.
	/// If so, returns a reference to this `Junction` item.
	///
	/// # Example
	/// ```rust
	/// # use staging_xcm::v4::{Junctions::*, Junction::*, Location};
	/// # fn main() {
	/// let mut m = Location::new(1, [PalletInstance(3), OnlyChild]);
	/// assert_eq!(
	///     m.match_and_split(&Location::new(1, [PalletInstance(3)])),
	///     Some(&OnlyChild),
	/// );
	/// assert_eq!(m.match_and_split(&Location::new(1, Here)), None);
	/// # }
	/// ```
	pub fn match_and_split(&self, prefix: &Location) -> Option<&Junction> {
		if self.parents != prefix.parents {
			return None
		}
		self.interior.match_and_split(&prefix.interior)
	}

	pub fn starts_with(&self, prefix: &Location) -> bool {
		self.parents == prefix.parents && self.interior.starts_with(&prefix.interior)
	}

	/// Mutate `self` so that it is suffixed with `suffix`.
	///
	/// Does not modify `self` and returns `Err` with `suffix` in case of overflow.
	///
	/// # Example
	/// ```rust
	/// # use staging_xcm::v4::{Junctions::*, Junction::*, Location, Parent};
	/// # fn main() {
	/// let mut m: Location = (Parent, Parachain(21), 69u64).into();
	/// assert_eq!(m.append_with((Parent, PalletInstance(3))), Ok(()));
	/// assert_eq!(m, Location::new(1, [Parachain(21), PalletInstance(3)]));
	/// # }
	/// ```
	pub fn append_with(&mut self, suffix: impl Into<Self>) -> Result<(), Self> {
		let prefix = core::mem::replace(self, suffix.into());
		match self.prepend_with(prefix) {
			Ok(()) => Ok(()),
			Err(prefix) => Err(core::mem::replace(self, prefix)),
		}
	}

	/// Consume `self` and return its value suffixed with `suffix`.
	///
	/// Returns `Err` with the original value of `self` and `suffix` in case of overflow.
	///
	/// # Example
	/// ```rust
	/// # use staging_xcm::v4::{Junctions::*, Junction::*, Location, Parent};
	/// # fn main() {
	/// let mut m: Location = (Parent, Parachain(21), 69u64).into();
	/// let r = m.appended_with((Parent, PalletInstance(3))).unwrap();
	/// assert_eq!(r, Location::new(1, [Parachain(21), PalletInstance(3)]));
	/// # }
	/// ```
	pub fn appended_with(mut self, suffix: impl Into<Self>) -> Result<Self, (Self, Self)> {
		match self.append_with(suffix) {
			Ok(()) => Ok(self),
			Err(suffix) => Err((self, suffix)),
		}
	}

	/// Mutate `self` so that it is prefixed with `prefix`.
	///
	/// Does not modify `self` and returns `Err` with `prefix` in case of overflow.
	///
	/// # Example
	/// ```rust
	/// # use staging_xcm::v4::{Junctions::*, Junction::*, Location, Parent};
	/// # fn main() {
	/// let mut m: Location = (Parent, Parent, PalletInstance(3)).into();
	/// assert_eq!(m.prepend_with((Parent, Parachain(21), OnlyChild)), Ok(()));
	/// assert_eq!(m, Location::new(1, [PalletInstance(3)]));
	/// # }
	/// ```
	pub fn prepend_with(&mut self, prefix: impl Into<Self>) -> Result<(), Self> {
		//     prefix     self (suffix)
		// P .. P I .. I  p .. p i .. i
		let mut prefix = prefix.into();
		let prepend_interior = prefix.interior.len().saturating_sub(self.parents as usize);
		let final_interior = self.interior.len().saturating_add(prepend_interior);
		if final_interior > super::junctions::MAX_JUNCTIONS {
			return Err(prefix)
		}
		let suffix_parents = (self.parents as usize).saturating_sub(prefix.interior.len());
		let final_parents = (prefix.parents as usize).saturating_add(suffix_parents);
		if final_parents > 255 {
			return Err(prefix)
		}

		// cancel out the final item on the prefix interior for one of the suffix's parents.
		while self.parents > 0 && prefix.take_last().is_some() {
			self.dec_parent();
		}

		// now we have either removed all suffix's parents or prefix interior.
		// this means we can combine the prefix's and suffix's remaining parents/interior since
		// we know that with at least one empty, the overall order will be respected:
		//     prefix     self (suffix)
		// P .. P   (I)   p .. p i .. i => P + p .. (no I) i
		//  -- or --
		// P .. P I .. I    (p)  i .. i => P (no p) .. I + i

		self.parents = self.parents.saturating_add(prefix.parents);
		for j in prefix.interior.into_iter().rev() {
			self.push_front_interior(j)
				.expect("final_interior no greater than MAX_JUNCTIONS; qed");
		}
		Ok(())
	}

	/// Consume `self` and return its value prefixed with `prefix`.
	///
	/// Returns `Err` with the original value of `self` and `prefix` in case of overflow.
	///
	/// # Example
	/// ```rust
	/// # use staging_xcm::v4::{Junctions::*, Junction::*, Location, Parent};
	/// # fn main() {
	/// let m: Location = (Parent, Parent, PalletInstance(3)).into();
	/// let r = m.prepended_with((Parent, Parachain(21), OnlyChild)).unwrap();
	/// assert_eq!(r, Location::new(1, [PalletInstance(3)]));
	/// # }
	/// ```
	pub fn prepended_with(mut self, prefix: impl Into<Self>) -> Result<Self, (Self, Self)> {
		match self.prepend_with(prefix) {
			Ok(()) => Ok(self),
			Err(prefix) => Err((self, prefix)),
		}
	}

	/// Remove any unneeded parents/junctions in `self` based on the given context it will be
	/// interpreted in.
	pub fn simplify(&mut self, context: &Junctions) {
		if context.len() < self.parents as usize {
			// Not enough context
			return
		}
		while self.parents > 0 {
			let maybe = context.at(context.len() - (self.parents as usize));
			match (self.interior.first(), maybe) {
				(Some(i), Some(j)) if i == j => {
					self.interior.take_first();
					self.parents -= 1;
				},
				_ => break,
			}
		}
	}

	/// Return the Location subsection identifying the chain that `self` points to.
	pub fn chain_location(&self) -> Location {
		let mut clone = self.clone();
		// start popping junctions until we reach chain identifier
		while let Some(j) = clone.last() {
			if matches!(j, Junction::Parachain(_) | Junction::GlobalConsensus(_)) {
				// return chain subsection
				return clone
			} else {
				(clone, _) = clone.split_last_interior();
			}
		}
		Location::new(clone.parents, Junctions::Here)
	}
}

impl Reanchorable for Location {
	type Error = Self;

	/// Mutate `self` so that it represents the same location from the point of view of `target`.
	/// The context of `self` is provided as `context`.
	///
	/// Does not modify `self` in case of overflow.
	fn reanchor(&mut self, target: &Location, context: &InteriorLocation) -> Result<(), ()> {
		// TODO: https://github.com/paritytech/polkadot/issues/4489 Optimize this.

		// 1. Use our `context` to figure out how the `target` would address us.
		let inverted_target = context.invert_target(target)?;

		// 2. Prepend `inverted_target` to `self` to get self's location from the perspective of
		// `target`.
		self.prepend_with(inverted_target).map_err(|_| ())?;

		// 3. Given that we know some of `target` context, ensure that any parents in `self` are
		// strictly needed.
		self.simplify(target.interior());

		Ok(())
	}

	/// Consume `self` and return a new value representing the same location from the point of view
	/// of `target`. The context of `self` is provided as `context`.
	///
	/// Returns the original `self` in case of overflow.
	fn reanchored(mut self, target: &Location, context: &InteriorLocation) -> Result<Self, Self> {
		match self.reanchor(target, context) {
			Ok(()) => Ok(self),
			Err(()) => Err(self),
		}
	}
}

impl TryFrom<OldLocation> for Option<Location> {
	type Error = ();
	fn try_from(value: OldLocation) -> result::Result<Self, Self::Error> {
		Ok(Some(Location::try_from(value)?))
	}
}

impl TryFrom<OldLocation> for Location {
	type Error = ();
	fn try_from(x: OldLocation) -> result::Result<Self, ()> {
		Ok(Location { parents: x.parents, interior: x.interior.try_into()? })
	}
}

/// A unit struct which can be converted into a `Location` of `parents` value 1.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Parent;
impl From<Parent> for Location {
	fn from(_: Parent) -> Self {
		Location { parents: 1, interior: Junctions::Here }
	}
}

/// A tuple struct which can be converted into a `Location` of `parents` value 1 with the inner
/// interior.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ParentThen(pub Junctions);
impl From<ParentThen> for Location {
	fn from(ParentThen(interior): ParentThen) -> Self {
		Location { parents: 1, interior }
	}
}

/// A unit struct which can be converted into a `Location` of the inner `parents` value.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Ancestor(pub u8);
impl From<Ancestor> for Location {
	fn from(Ancestor(parents): Ancestor) -> Self {
		Location { parents, interior: Junctions::Here }
	}
}

/// A unit struct which can be converted into a `Location` of the inner `parents` value and the
/// inner interior.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct AncestorThen<Interior>(pub u8, pub Interior);
impl<Interior: Into<Junctions>> From<AncestorThen<Interior>> for Location {
	fn from(AncestorThen(parents, interior): AncestorThen<Interior>) -> Self {
		Location { parents, interior: interior.into() }
	}
}

xcm_procedural::impl_conversion_functions_for_location_v4!();

#[cfg(test)]
mod tests {
	use crate::v4::prelude::*;
	use parity_scale_codec::{Decode, Encode};

	#[test]
	fn conversion_works() {
		let x: Location = Parent.into();
		assert_eq!(x, Location { parents: 1, interior: Here });
		//		let x: Location = (Parent,).into();
		//		assert_eq!(x, Location { parents: 1, interior: Here });
		//		let x: Location = (Parent, Parent).into();
		//		assert_eq!(x, Location { parents: 2, interior: Here });
		let x: Location = (Parent, Parent, OnlyChild).into();
		assert_eq!(x, Location { parents: 2, interior: OnlyChild.into() });
		let x: Location = OnlyChild.into();
		assert_eq!(x, Location { parents: 0, interior: OnlyChild.into() });
		let x: Location = (OnlyChild,).into();
		assert_eq!(x, Location { parents: 0, interior: OnlyChild.into() });
	}

	#[test]
	fn simplify_basic_works() {
		let mut location: Location =
			(Parent, Parent, Parachain(1000), PalletInstance(42), GeneralIndex(69)).into();
		let context = [Parachain(1000), PalletInstance(42)].into();
		let expected = GeneralIndex(69).into();
		location.simplify(&context);
		assert_eq!(location, expected);

		let mut location: Location = (Parent, PalletInstance(42), GeneralIndex(69)).into();
		let context = [PalletInstance(42)].into();
		let expected = GeneralIndex(69).into();
		location.simplify(&context);
		assert_eq!(location, expected);

		let mut location: Location = (Parent, PalletInstance(42), GeneralIndex(69)).into();
		let context = [Parachain(1000), PalletInstance(42)].into();
		let expected = GeneralIndex(69).into();
		location.simplify(&context);
		assert_eq!(location, expected);

		let mut location: Location =
			(Parent, Parent, Parachain(1000), PalletInstance(42), GeneralIndex(69)).into();
		let context = [OnlyChild, Parachain(1000), PalletInstance(42)].into();
		let expected = GeneralIndex(69).into();
		location.simplify(&context);
		assert_eq!(location, expected);
	}

	#[test]
	fn simplify_incompatible_location_fails() {
		let mut location: Location =
			(Parent, Parent, Parachain(1000), PalletInstance(42), GeneralIndex(69)).into();
		let context = [Parachain(1000), PalletInstance(42), GeneralIndex(42)].into();
		let expected =
			(Parent, Parent, Parachain(1000), PalletInstance(42), GeneralIndex(69)).into();
		location.simplify(&context);
		assert_eq!(location, expected);

		let mut location: Location =
			(Parent, Parent, Parachain(1000), PalletInstance(42), GeneralIndex(69)).into();
		let context = [Parachain(1000)].into();
		let expected =
			(Parent, Parent, Parachain(1000), PalletInstance(42), GeneralIndex(69)).into();
		location.simplify(&context);
		assert_eq!(location, expected);
	}

	#[test]
	fn reanchor_works() {
		let mut id: Location = (Parent, Parachain(1000), GeneralIndex(42)).into();
		let context = Parachain(2000).into();
		let target = (Parent, Parachain(1000)).into();
		let expected = GeneralIndex(42).into();
		id.reanchor(&target, &context).unwrap();
		assert_eq!(id, expected);
	}

	#[test]
	fn encode_and_decode_works() {
		let m = Location {
			parents: 1,
			interior: [Parachain(42), AccountIndex64 { network: None, index: 23 }].into(),
		};
		let encoded = m.encode();
		assert_eq!(encoded, [1, 2, 0, 168, 2, 0, 92].to_vec());
		let decoded = Location::decode(&mut &encoded[..]);
		assert_eq!(decoded, Ok(m));
	}

	#[test]
	fn match_and_split_works() {
		let m = Location {
			parents: 1,
			interior: [Parachain(42), AccountIndex64 { network: None, index: 23 }].into(),
		};
		assert_eq!(m.match_and_split(&Location { parents: 1, interior: Here }), None);
		assert_eq!(
			m.match_and_split(&Location { parents: 1, interior: [Parachain(42)].into() }),
			Some(&AccountIndex64 { network: None, index: 23 })
		);
		assert_eq!(m.match_and_split(&m), None);
	}

	#[test]
	fn append_with_works() {
		let acc = AccountIndex64 { network: None, index: 23 };
		let mut m = Location { parents: 1, interior: [Parachain(42)].into() };
		assert_eq!(m.append_with([PalletInstance(3), acc]), Ok(()));
		assert_eq!(
			m,
			Location { parents: 1, interior: [Parachain(42), PalletInstance(3), acc].into() }
		);

		// cannot append to create overly long location
		let acc = AccountIndex64 { network: None, index: 23 };
		let m = Location {
			parents: 254,
			interior: [Parachain(42), OnlyChild, OnlyChild, OnlyChild, OnlyChild].into(),
		};
		let suffix: Location = (PalletInstance(3), acc, OnlyChild, OnlyChild).into();
		assert_eq!(m.clone().append_with(suffix.clone()), Err(suffix));
	}

	#[test]
	fn prepend_with_works() {
		let mut m = Location {
			parents: 1,
			interior: [Parachain(42), AccountIndex64 { network: None, index: 23 }].into(),
		};
		assert_eq!(m.prepend_with(Location { parents: 1, interior: [OnlyChild].into() }), Ok(()));
		assert_eq!(
			m,
			Location {
				parents: 1,
				interior: [Parachain(42), AccountIndex64 { network: None, index: 23 }].into()
			}
		);

		// cannot prepend to create overly long location
		let mut m = Location { parents: 254, interior: [Parachain(42)].into() };
		let prefix = Location { parents: 2, interior: Here };
		assert_eq!(m.prepend_with(prefix.clone()), Err(prefix));

		let prefix = Location { parents: 1, interior: Here };
		assert_eq!(m.prepend_with(prefix.clone()), Ok(()));
		assert_eq!(m, Location { parents: 255, interior: [Parachain(42)].into() });
	}

	#[test]
	fn double_ended_ref_iteration_works() {
		let m: Junctions = [Parachain(1000), Parachain(3), PalletInstance(5)].into();
		let mut iter = m.iter();

		let first = iter.next().unwrap();
		assert_eq!(first, &Parachain(1000));
		let third = iter.next_back().unwrap();
		assert_eq!(third, &PalletInstance(5));
		let second = iter.next_back().unwrap();
		assert_eq!(iter.next(), None);
		assert_eq!(iter.next_back(), None);
		assert_eq!(second, &Parachain(3));

		let res = Here
			.pushed_with(*first)
			.unwrap()
			.pushed_with(*second)
			.unwrap()
			.pushed_with(*third)
			.unwrap();
		assert_eq!(m, res);

		// make sure there's no funny business with the 0 indexing
		let m = Here;
		let mut iter = m.iter();

		assert_eq!(iter.next(), None);
		assert_eq!(iter.next_back(), None);
	}

	#[test]
	fn conversion_from_other_types_works() {
		use crate::v3;
		use core::convert::TryInto;

		fn takes_location<Arg: Into<Location>>(_arg: Arg) {}

		takes_location(Parent);
		takes_location(Here);
		takes_location([Parachain(42)]);
		takes_location((Ancestor(255), PalletInstance(8)));
		takes_location((Ancestor(5), Parachain(1), PalletInstance(3)));
		takes_location((Ancestor(2), Here));
		takes_location(AncestorThen(
			3,
			[Parachain(43), AccountIndex64 { network: None, index: 155 }],
		));
		takes_location((Parent, AccountId32 { network: None, id: [0; 32] }));
		takes_location((Parent, Here));
		takes_location(ParentThen([Parachain(75)].into()));
		takes_location([Parachain(100), PalletInstance(3)]);

		assert_eq!(v3::Location::from(v3::Junctions::Here).try_into(), Ok(Location::here()));
		assert_eq!(v3::Location::from(v3::Parent).try_into(), Ok(Location::parent()));
		assert_eq!(
			v3::Location::from((v3::Parent, v3::Parent, v3::Junction::GeneralIndex(42u128),))
				.try_into(),
			Ok(Location { parents: 2, interior: [GeneralIndex(42u128)].into() }),
		);
	}
}
