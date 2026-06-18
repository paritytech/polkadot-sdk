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

use frame_support::{traits::Get, weights::Weight};
use sp_runtime::BoundedVec;
use xcm::latest::{prelude::*, AssetTransferFilter};

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum AssetTypes {
	Balances,
	Unknown,
}

pub trait AssetMatcher {
	fn classify(asset: &Asset) -> AssetTypes;
	fn max_assets() -> u64;
}

pub fn weigh_assets_list<M: AssetMatcher>(assets: &Assets, known_weight: Weight) -> Weight {
	assets
		.inner()
		.iter()
		.map(M::classify)
		.map(|asset_type| match asset_type {
			AssetTypes::Balances => known_weight,
			AssetTypes::Unknown => Weight::MAX,
		})
		.fold(Weight::zero(), |acc, x| acc.saturating_add(x))
}

pub fn weigh_assets_filter<M: AssetMatcher>(assets: &AssetFilter, known_weight: Weight) -> Weight {
	match assets {
		AssetFilter::Definite(definite) => definite
			.inner()
			.iter()
			.map(M::classify)
			.map(|asset_type| match asset_type {
				AssetTypes::Balances => known_weight,
				AssetTypes::Unknown => Weight::MAX,
			})
			.fold(Weight::zero(), |acc, x| acc.saturating_add(x)),
		AssetFilter::Wild(AllOf { .. } | AllOfCounted { .. }) => known_weight,
		AssetFilter::Wild(AllCounted(count)) => {
			known_weight.saturating_mul(M::max_assets().min(*count as u64))
		},
		AssetFilter::Wild(All) => known_weight.saturating_mul(M::max_assets()),
	}
}

pub fn weigh_initiate_transfer<MaxFilters: Get<u32>>(
	remote_fees: &Option<AssetTransferFilter>,
	assets: &BoundedVec<AssetTransferFilter, MaxFilters>,
	base_weight: Weight,
	weigh_filter: impl Fn(&AssetFilter, Weight) -> Weight,
) -> Weight {
	let mut weight = if let Some(remote_fees) = remote_fees {
		weigh_filter(remote_fees.inner(), base_weight)
	} else {
		Weight::zero()
	};
	for asset_filter in assets {
		let extra = weigh_filter(asset_filter.inner(), base_weight);
		weight = weight.saturating_add(extra);
	}
	weight
}

pub fn weigh_hints<HintVariants: Get<u32>>(
	hints: &BoundedVec<Hint, HintVariants>,
	asset_claimer_weight: Weight,
) -> Weight {
	let mut weight = Weight::zero();
	for hint in hints {
		match hint {
			AssetClaimer { .. } => {
				weight = weight.saturating_add(asset_claimer_weight);
			},
		}
	}
	weight
}
