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

use sp_runtime::{traits::Saturating, RuntimeDebug};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	mem,
	prelude::*,
};
use xcm::latest::{
	Asset, AssetFilter, AssetId, AssetInstance, Assets,
	Fungibility::{Fungible, NonFungible},
	InteriorLocation, Location, Reanchorable,
	WildAsset::{All, AllCounted, AllOf, AllOfCounted},
	WildFungibility::{Fungible as WildFungible, NonFungible as WildNonFungible},
};

/// Map of non-wildcard fungible and non-fungible assets held in the holding register.
#[derive(Default, Clone, RuntimeDebug, Eq, PartialEq)]
pub struct AssetsInHolding {
	/// The fungible assets.
	pub fungible: BTreeMap<AssetId, u128>,

	/// The non-fungible assets.
	// TODO: Consider BTreeMap<AssetId, BTreeSet<AssetInstance>>
	//   or even BTreeMap<AssetId, SortedVec<AssetInstance>>
	pub non_fungible: BTreeSet<(AssetId, AssetInstance)>,
}

impl From<Asset> for AssetsInHolding {
	fn from(asset: Asset) -> AssetsInHolding {
		let mut result = Self::default();
		result.subsume(asset);
		result
	}
}

impl From<Vec<Asset>> for AssetsInHolding {
	fn from(assets: Vec<Asset>) -> AssetsInHolding {
		let mut result = Self::default();
		for asset in assets.into_iter() {
			result.subsume(asset)
		}
		result
	}
}

impl From<Assets> for AssetsInHolding {
	fn from(assets: Assets) -> AssetsInHolding {
		assets.into_inner().into()
	}
}

impl From<AssetsInHolding> for Vec<Asset> {
	fn from(a: AssetsInHolding) -> Self {
		a.into_assets_iter().collect()
	}
}

impl From<AssetsInHolding> for Assets {
	fn from(a: AssetsInHolding) -> Self {
		a.into_assets_iter().collect::<Vec<Asset>>().into()
	}
}

/// An error emitted by `take` operations.
#[derive(Debug)]
pub enum TakeError {
	/// There was an attempt to take an asset without saturating (enough of) which did not exist.
	AssetUnderflow(Asset),
}

impl AssetsInHolding {
	/// New value, containing no assets.
	pub fn new() -> Self {
		Self::default()
	}

	/// Total number of distinct assets.
	pub fn len(&self) -> usize {
		self.fungible.len() + self.non_fungible.len()
	}

	/// Returns `true` if `self` contains no assets.
	pub fn is_empty(&self) -> bool {
		self.fungible.is_empty() && self.non_fungible.is_empty()
	}

	/// A borrowing iterator over the fungible assets.
	pub fn fungible_assets_iter(&self) -> impl Iterator<Item = Asset> + '_ {
		self.fungible
			.iter()
			.map(|(id, &amount)| Asset { fun: Fungible(amount), id: id.clone() })
	}

	/// A borrowing iterator over the non-fungible assets.
	pub fn non_fungible_assets_iter(&self) -> impl Iterator<Item = Asset> + '_ {
		self.non_fungible
			.iter()
			.map(|(id, instance)| Asset { fun: NonFungible(*instance), id: id.clone() })
	}

	/// A consuming iterator over all assets.
	pub fn into_assets_iter(self) -> impl Iterator<Item = Asset> {
		self.fungible
			.into_iter()
			.map(|(id, amount)| Asset { fun: Fungible(amount), id })
			.chain(
				self.non_fungible
					.into_iter()
					.map(|(id, instance)| Asset { fun: NonFungible(instance), id }),
			)
	}

	/// A borrowing iterator over all assets.
	pub fn assets_iter(&self) -> impl Iterator<Item = Asset> + '_ {
		self.fungible_assets_iter().chain(self.non_fungible_assets_iter())
	}

	/// Mutate `self` to contain all given `assets`, saturating if necessary.
	///
	/// NOTE: [`AssetsInHolding`] are always sorted, allowing us to optimize this function from
	/// `O(n^2)` to `O(n)`.
	pub fn subsume_assets(&mut self, mut assets: AssetsInHolding) {
		let mut f_iter = assets.fungible.iter_mut();
		let mut g_iter = self.fungible.iter_mut();
		if let (Some(mut f), Some(mut g)) = (f_iter.next(), g_iter.next()) {
			loop {
				if f.0 == g.0 {
					// keys are equal. in this case, we add `self`'s balance for the asset onto
					// `assets`, balance, knowing that the `append` operation which follows will
					// clobber `self`'s value and only use `assets`'s.
					(*f.1).saturating_accrue(*g.1);
				}
				if f.0 <= g.0 {
					f = match f_iter.next() {
						Some(x) => x,
						None => break,
					};
				}
				if f.0 >= g.0 {
					g = match g_iter.next() {
						Some(x) => x,
						None => break,
					};
				}
			}
		}
		self.fungible.append(&mut assets.fungible);
		self.non_fungible.append(&mut assets.non_fungible);
	}

	/// Mutate `self` to contain the given `asset`, saturating if necessary.
	///
	/// Wildcard values of `asset` do nothing.
	pub fn subsume(&mut self, asset: Asset) {
		match asset.fun {
			Fungible(amount) => {
				self.fungible
					.entry(asset.id)
					.and_modify(|e| *e = e.saturating_add(amount))
					.or_insert(amount);
			},
			NonFungible(instance) => {
				self.non_fungible.insert((asset.id, instance));
			},
		}
	}

	/// Swaps two mutable AssetsInHolding, without deinitializing either one.
	pub fn swapped(&mut self, mut with: AssetsInHolding) -> Self {
		mem::swap(&mut *self, &mut with);
		with
	}

	/// Alter any concretely identified assets by prepending the given `Location`.
	///
	/// WARNING: For now we consider this infallible and swallow any errors. It is thus the caller's
	/// responsibility to ensure that any internal asset IDs are able to be prepended without
	/// overflow.
	pub fn prepend_location(&mut self, prepend: &Location) {
		let mut fungible = Default::default();
		mem::swap(&mut self.fungible, &mut fungible);
		self.fungible = fungible
			.into_iter()
			.map(|(mut id, amount)| {
				let _ = id.prepend_with(prepend);
				(id, amount)
			})
			.collect();
		let mut non_fungible = Default::default();
		mem::swap(&mut self.non_fungible, &mut non_fungible);
		self.non_fungible = non_fungible
			.into_iter()
			.map(|(mut class, inst)| {
				let _ = class.prepend_with(prepend);
				(class, inst)
			})
			.collect();
	}

	/// Mutate the assets to be interpreted as the same assets from the perspective of a `target`
	/// chain. The local chain's `context` is provided.
	///
	/// Any assets which were unable to be reanchored are introduced into `failed_bin`.
	pub fn reanchor(
		&mut self,
		target: &Location,
		context: &InteriorLocation,
		mut maybe_failed_bin: Option<&mut Self>,
	) {
		let mut fungible = Default::default();
		mem::swap(&mut self.fungible, &mut fungible);
		self.fungible = fungible
			.into_iter()
			.filter_map(|(mut id, amount)| match id.reanchor(target, context) {
				Ok(()) => Some((id, amount)),
				Err(()) => {
					maybe_failed_bin.as_mut().map(|f| f.fungible.insert(id, amount));
					None
				},
			})
			.collect();
		let mut non_fungible = Default::default();
		mem::swap(&mut self.non_fungible, &mut non_fungible);
		self.non_fungible = non_fungible
			.into_iter()
			.filter_map(|(mut class, inst)| match class.reanchor(target, context) {
				Ok(()) => Some((class, inst)),
				Err(()) => {
					maybe_failed_bin.as_mut().map(|f| f.non_fungible.insert((class, inst)));
					None
				},
			})
			.collect();
	}

	/// Returns `true` if `asset` is contained within `self`.
	pub fn contains_asset(&self, asset: &Asset) -> bool {
		match asset {
			Asset { fun: Fungible(amount), id } =>
				self.fungible.get(id).map_or(false, |a| a >= amount),
			Asset { fun: NonFungible(instance), id } =>
				self.non_fungible.contains(&(id.clone(), *instance)),
		}
	}

	/// Returns `true` if all `assets` are contained within `self`.
	pub fn contains_assets(&self, assets: &Assets) -> bool {
		assets.inner().iter().all(|a| self.contains_asset(a))
	}

	/// Returns `true` if all `assets` are contained within `self`.
	pub fn contains(&self, assets: &AssetsInHolding) -> bool {
		assets
			.fungible
			.iter()
			.all(|(k, v)| self.fungible.get(k).map_or(false, |a| a >= v)) &&
			self.non_fungible.is_superset(&assets.non_fungible)
	}

	/// Returns an error unless all `assets` are contained in `self`. In the case of an error, the
	/// first asset in `assets` which is not wholly in `self` is returned.
	pub fn ensure_contains(&self, assets: &Assets) -> Result<(), TakeError> {
		for asset in assets.inner().iter() {
			match asset {
				Asset { fun: Fungible(amount), id } => {
					if self.fungible.get(id).map_or(true, |a| a < amount) {
						return Err(TakeError::AssetUnderflow((id.clone(), *amount).into()))
					}
				},
				Asset { fun: NonFungible(instance), id } => {
					let id_instance = (id.clone(), *instance);
					if !self.non_fungible.contains(&id_instance) {
						return Err(TakeError::AssetUnderflow(id_instance.into()))
					}
				},
			}
		}
		return Ok(())
	}

	/// Mutates `self` to its original value less `mask` and returns assets that were removed.
	///
	/// If `saturate` is `true`, then `self` is considered to be masked by `mask`, thereby avoiding
	/// any attempt at reducing it by assets it does not contain. In this case, the function is
	/// infallible. If `saturate` is `false` and `mask` references a definite asset which `self`
	/// does not contain then an error is returned.
	///
	/// The number of unique assets which are removed will respect the `count` parameter in the
	/// counted wildcard variants.
	///
	/// Returns `Ok` with the definite assets token from `self` and mutates `self` to its value
	/// minus `mask`. Returns `Err` in the non-saturating case where `self` did not contain (enough
	/// of) a definite asset to be removed.
	fn general_take(
		&mut self,
		mask: AssetFilter,
		saturate: bool,
	) -> Result<AssetsInHolding, TakeError> {
		let mut taken = AssetsInHolding::new();
		let maybe_limit = mask.limit().map(|x| x as usize);
		match mask {
			// TODO: Counted variants where we define `limit`.
			AssetFilter::Wild(All) | AssetFilter::Wild(AllCounted(_)) => {
				if maybe_limit.map_or(true, |l| self.len() <= l) {
					return Ok(self.swapped(AssetsInHolding::new()))
				} else {
					let fungible = mem::replace(&mut self.fungible, Default::default());
					fungible.into_iter().for_each(|(c, amount)| {
						if maybe_limit.map_or(true, |l| taken.len() < l) {
							taken.fungible.insert(c, amount);
						} else {
							self.fungible.insert(c, amount);
						}
					});
					let non_fungible = mem::replace(&mut self.non_fungible, Default::default());
					non_fungible.into_iter().for_each(|(c, instance)| {
						if maybe_limit.map_or(true, |l| taken.len() < l) {
							taken.non_fungible.insert((c, instance));
						} else {
							self.non_fungible.insert((c, instance));
						}
					});
				}
			},
			AssetFilter::Wild(AllOfCounted { fun: WildFungible, id, .. }) |
			AssetFilter::Wild(AllOf { fun: WildFungible, id }) =>
				if maybe_limit.map_or(true, |l| l >= 1) {
					if let Some((id, amount)) = self.fungible.remove_entry(&id) {
						taken.fungible.insert(id, amount);
					}
				},
			AssetFilter::Wild(AllOfCounted { fun: WildNonFungible, id, .. }) |
			AssetFilter::Wild(AllOf { fun: WildNonFungible, id }) => {
				let non_fungible = mem::replace(&mut self.non_fungible, Default::default());
				non_fungible.into_iter().for_each(|(c, instance)| {
					if c == id && maybe_limit.map_or(true, |l| taken.len() < l) {
						taken.non_fungible.insert((c, instance));
					} else {
						self.non_fungible.insert((c, instance));
					}
				});
			},
			AssetFilter::Definite(assets) => {
				if !saturate {
					self.ensure_contains(&assets)?;
				}
				for asset in assets.into_inner().into_iter() {
					match asset {
						Asset { fun: Fungible(amount), id } => {
							let (remove, amount) = match self.fungible.get_mut(&id) {
								Some(self_amount) => {
									let amount = amount.min(*self_amount);
									*self_amount -= amount;
									(*self_amount == 0, amount)
								},
								None => (false, 0),
							};
							if remove {
								self.fungible.remove(&id);
							}
							if amount > 0 {
								taken.subsume(Asset::from((id, amount)).into());
							}
						},
						Asset { fun: NonFungible(instance), id } => {
							let id_instance = (id, instance);
							if self.non_fungible.remove(&id_instance) {
								taken.subsume(id_instance.into())
							}
						},
					}
				}
			},
		}
		Ok(taken)
	}

	/// Mutates `self` to its original value less `mask` and returns `true` iff it contains at least
	/// `mask`.
	///
	/// Returns `Ok` with the non-wildcard equivalence of `mask` taken and mutates `self` to its
	/// value minus `mask` if `self` contains `asset`, and return `Err` otherwise.
	pub fn saturating_take(&mut self, asset: AssetFilter) -> AssetsInHolding {
		self.general_take(asset, true)
			.expect("general_take never results in error when saturating")
	}

	/// Mutates `self` to its original value less `mask` and returns `true` iff it contains at least
	/// `mask`.
	///
	/// Returns `Ok` with the non-wildcard equivalence of `asset` taken and mutates `self` to its
	/// value minus `asset` if `self` contains `asset`, and return `Err` otherwise.
	pub fn try_take(&mut self, mask: AssetFilter) -> Result<AssetsInHolding, TakeError> {
		self.general_take(mask, false)
	}

	/// Consumes `self` and returns its original value excluding `asset` iff it contains at least
	/// `asset`.
	pub fn checked_sub(mut self, asset: Asset) -> Result<AssetsInHolding, AssetsInHolding> {
		match asset.fun {
			Fungible(amount) => {
				let remove = if let Some(balance) = self.fungible.get_mut(&asset.id) {
					if *balance >= amount {
						*balance -= amount;
						*balance == 0
					} else {
						return Err(self)
					}
				} else {
					return Err(self)
				};
				if remove {
					self.fungible.remove(&asset.id);
				}
				Ok(self)
			},
			NonFungible(instance) =>
				if self.non_fungible.remove(&(asset.id, instance)) {
					Ok(self)
				} else {
					Err(self)
				},
		}
	}

	/// Return the assets in `self`, but (asset-wise) of no greater value than `mask`.
	///
	/// The number of unique assets which are returned will respect the `count` parameter in the
	/// counted wildcard variants of `mask`.
	///
	/// Example:
	///
	/// ```
	/// use staging_xcm_executor::AssetsInHolding;
	/// use xcm::latest::prelude::*;
	/// let assets_i_have: AssetsInHolding = vec![ (Here, 100).into(), (Junctions::from([GeneralIndex(0)]), 100).into() ].into();
	/// let assets_they_want: AssetFilter = vec![ (Here, 200).into(), (Junctions::from([GeneralIndex(0)]), 50).into() ].into();
	///
	/// let assets_we_can_trade: AssetsInHolding = assets_i_have.min(&assets_they_want);
	/// assert_eq!(assets_we_can_trade.into_assets_iter().collect::<Vec<_>>(), vec![
	/// 	(Here, 100).into(), (Junctions::from([GeneralIndex(0)]), 50).into(),
	/// ]);
	/// ```
	pub fn min(&self, mask: &AssetFilter) -> AssetsInHolding {
		let mut masked = AssetsInHolding::new();
		let maybe_limit = mask.limit().map(|x| x as usize);
		if maybe_limit.map_or(false, |l| l == 0) {
			return masked
		}
		match mask {
			AssetFilter::Wild(All) | AssetFilter::Wild(AllCounted(_)) => {
				if maybe_limit.map_or(true, |l| self.len() <= l) {
					return self.clone()
				} else {
					for (c, &amount) in self.fungible.iter() {
						masked.fungible.insert(c.clone(), amount);
						if maybe_limit.map_or(false, |l| masked.len() >= l) {
							return masked
						}
					}
					for (c, instance) in self.non_fungible.iter() {
						masked.non_fungible.insert((c.clone(), *instance));
						if maybe_limit.map_or(false, |l| masked.len() >= l) {
							return masked
						}
					}
				}
			},
			AssetFilter::Wild(AllOfCounted { fun: WildFungible, id, .. }) |
			AssetFilter::Wild(AllOf { fun: WildFungible, id }) =>
				if let Some(&amount) = self.fungible.get(&id) {
					masked.fungible.insert(id.clone(), amount);
				},
			AssetFilter::Wild(AllOfCounted { fun: WildNonFungible, id, .. }) |
			AssetFilter::Wild(AllOf { fun: WildNonFungible, id }) =>
				for (c, instance) in self.non_fungible.iter() {
					if c == id {
						masked.non_fungible.insert((c.clone(), *instance));
						if maybe_limit.map_or(false, |l| masked.len() >= l) {
							return masked
						}
					}
				},
			AssetFilter::Definite(assets) =>
				for asset in assets.inner().iter() {
					match asset {
						Asset { fun: Fungible(amount), id } => {
							if let Some(m) = self.fungible.get(id) {
								masked.subsume((id.clone(), Fungible(*amount.min(m))).into());
							}
						},
						Asset { fun: NonFungible(instance), id } => {
							let id_instance = (id.clone(), *instance);
							if self.non_fungible.contains(&id_instance) {
								masked.subsume(id_instance.into());
							}
						},
					}
				},
		}
		masked
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use xcm::latest::prelude::*;
	#[allow(non_snake_case)]
	/// Concrete fungible constructor
	fn CF(amount: u128) -> Asset {
		(Here, amount).into()
	}
	#[allow(non_snake_case)]
	/// Concrete non-fungible constructor
	fn CNF(instance_id: u8) -> Asset {
		(Here, [instance_id; 4]).into()
	}

	fn test_assets() -> AssetsInHolding {
		let mut assets = AssetsInHolding::new();
		assets.subsume(CF(300));
		assets.subsume(CNF(40));
		assets
	}

	#[test]
	fn subsume_assets_works() {
		let t1 = test_assets();
		let mut t2 = AssetsInHolding::new();
		t2.subsume(CF(300));
		t2.subsume(CNF(50));
		let mut r1 = t1.clone();
		r1.subsume_assets(t2.clone());
		let mut r2 = t1.clone();
		for a in t2.assets_iter() {
			r2.subsume(a)
		}
		assert_eq!(r1, r2);
	}

	#[test]
	fn checked_sub_works() {
		let t = test_assets();
		let t = t.checked_sub(CF(150)).unwrap();
		let t = t.checked_sub(CF(151)).unwrap_err();
		let t = t.checked_sub(CF(150)).unwrap();
		let t = t.checked_sub(CF(1)).unwrap_err();
		let t = t.checked_sub(CNF(41)).unwrap_err();
		let t = t.checked_sub(CNF(40)).unwrap();
		let t = t.checked_sub(CNF(40)).unwrap_err();
		assert_eq!(t, AssetsInHolding::new());
	}

	#[test]
	fn into_assets_iter_works() {
		let assets = test_assets();
		let mut iter = assets.into_assets_iter();
		// Order defined by implementation: CF, CNF
		assert_eq!(Some(CF(300)), iter.next());
		assert_eq!(Some(CNF(40)), iter.next());
		assert_eq!(None, iter.next());
	}

	#[test]
	fn assets_into_works() {
		let mut assets_vec: Vec<Asset> = Vec::new();
		assets_vec.push(CF(300));
		assets_vec.push(CNF(40));
		// Push same group of tokens again
		assets_vec.push(CF(300));
		assets_vec.push(CNF(40));

		let assets: AssetsInHolding = assets_vec.into();
		let mut iter = assets.into_assets_iter();
		// Fungibles add
		assert_eq!(Some(CF(600)), iter.next());
		// Non-fungibles collapse
		assert_eq!(Some(CNF(40)), iter.next());
		assert_eq!(None, iter.next());
	}

	#[test]
	fn min_all_and_none_works() {
		let assets = test_assets();
		let none = Assets::new().into();
		let all = All.into();

		let none_min = assets.min(&none);
		assert_eq!(None, none_min.assets_iter().next());
		let all_min = assets.min(&all);
		assert!(all_min.assets_iter().eq(assets.assets_iter()));
	}

	#[test]
	fn min_counted_works() {
		let mut assets = AssetsInHolding::new();
		assets.subsume(CNF(40));
		assets.subsume(CF(3000));
		assets.subsume(CNF(80));
		let all = WildAsset::AllCounted(6).into();

		let all = assets.min(&all);
		let all = all.assets_iter().collect::<Vec<_>>();
		assert_eq!(all, vec![CF(3000), CNF(40), CNF(80)]);
	}

	#[test]
	fn min_all_concrete_works() {
		let assets = test_assets();
		let fungible = Wild((Here, WildFungible).into());
		let non_fungible = Wild((Here, WildNonFungible).into());

		let fungible = assets.min(&fungible);
		let fungible = fungible.assets_iter().collect::<Vec<_>>();
		assert_eq!(fungible, vec![CF(300)]);
		let non_fungible = assets.min(&non_fungible);
		let non_fungible = non_fungible.assets_iter().collect::<Vec<_>>();
		assert_eq!(non_fungible, vec![CNF(40)]);
	}

	#[test]
	fn min_basic_works() {
		let assets1 = test_assets();

		let mut assets2 = AssetsInHolding::new();
		// This is more then 300, so it should stay at 300
		assets2.subsume(CF(600));
		// This asset should be included
		assets2.subsume(CNF(40));
		let assets2: Assets = assets2.into();

		let assets_min = assets1.min(&assets2.into());
		let assets_min = assets_min.into_assets_iter().collect::<Vec<_>>();
		assert_eq!(assets_min, vec![CF(300), CNF(40)]);
	}

	#[test]
	fn saturating_take_all_and_none_works() {
		let mut assets = test_assets();

		let taken_none = assets.saturating_take(vec![].into());
		assert_eq!(None, taken_none.assets_iter().next());
		let taken_all = assets.saturating_take(All.into());
		// Everything taken
		assert_eq!(None, assets.assets_iter().next());
		let all_iter = taken_all.assets_iter();
		assert!(all_iter.eq(test_assets().assets_iter()));
	}

	#[test]
	fn saturating_take_all_concrete_works() {
		let mut assets = test_assets();
		let fungible = Wild((Here, WildFungible).into());
		let non_fungible = Wild((Here, WildNonFungible).into());

		let fungible = assets.saturating_take(fungible);
		let fungible = fungible.assets_iter().collect::<Vec<_>>();
		assert_eq!(fungible, vec![CF(300)]);
		let non_fungible = assets.saturating_take(non_fungible);
		let non_fungible = non_fungible.assets_iter().collect::<Vec<_>>();
		assert_eq!(non_fungible, vec![CNF(40)]);
	}

	#[test]
	fn saturating_take_basic_works() {
		let mut assets1 = test_assets();

		let mut assets2 = AssetsInHolding::new();
		// This is more then 300, so it takes everything
		assets2.subsume(CF(600));
		// This asset should be taken
		assets2.subsume(CNF(40));
		let assets2: Assets = assets2.into();

		let taken = assets1.saturating_take(assets2.into());
		let taken = taken.into_assets_iter().collect::<Vec<_>>();
		assert_eq!(taken, vec![CF(300), CNF(40)]);
	}

	#[test]
	fn try_take_all_counted_works() {
		let mut assets = AssetsInHolding::new();
		assets.subsume(CNF(40));
		assets.subsume(CF(3000));
		assets.subsume(CNF(80));
		let all = assets.try_take(WildAsset::AllCounted(6).into()).unwrap();
		assert_eq!(Assets::from(all).inner(), &vec![CF(3000), CNF(40), CNF(80)]);
	}

	#[test]
	fn try_take_fungibles_counted_works() {
		let mut assets = AssetsInHolding::new();
		assets.subsume(CNF(40));
		assets.subsume(CF(3000));
		assets.subsume(CNF(80));
		assert_eq!(Assets::from(assets).inner(), &vec![CF(3000), CNF(40), CNF(80),]);
	}

	#[test]
	fn try_take_non_fungibles_counted_works() {
		let mut assets = AssetsInHolding::new();
		assets.subsume(CNF(40));
		assets.subsume(CF(3000));
		assets.subsume(CNF(80));
		assert_eq!(Assets::from(assets).inner(), &vec![CF(3000), CNF(40), CNF(80)]);
	}
}
