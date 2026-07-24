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

//! Benchmarks for `pallet_scarcity`.

#![cfg(feature = "runtime-benchmarks")]

extern crate alloc;

use alloc::{vec, vec::Vec};
use codec::{Encode, MaxEncodedLen};
use frame_benchmarking::v2::*;
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::{Consideration, Footprint, Get},
	BoundedVec,
};
use frame_system::RawOrigin;
use sp_runtime::{
	traits::{Dispatchable, TransactionExtension, TxBaseImplication},
	transaction_validity::TransactionSource,
};

use super::*;
use crate::extension::{AsScarcity, AsScarcityInfo};

fn stats<T: Config>(len: u32) -> BoundedVec<Stat, T::MaxStats> {
	(0..len)
		.map(|i| Stat { attr: i as AttrId, value: i.into() })
		.collect::<Vec<_>>()
		.try_into()
		.expect("benchmark component is bounded by MaxStats")
}

fn metadata<T: Config>(len: u32) -> BoundedVec<u8, T::MaxMetadata> {
	vec![0xff; len as usize]
		.try_into()
		.expect("benchmark component is bounded by MaxMetadata")
}

fn ensure_collection_consideration<T: Config>(owner: &T::AccountId) {
	let record_size = CollectionInfo { owner: owner.clone(), next_item_index: 0, ticket: () }
		.encoded_size()
		.saturating_add(T::CollectionConsideration::max_encoded_len());
	T::CollectionConsideration::ensure_successful(owner, Footprint::from_parts(1, record_size));
}

fn ensure_item_consideration<T: Config>(
	owner: &T::AccountId,
	kind: Kind,
	next_variant: Option<ItemIndex>,
	stats: &BoundedVec<Stat, T::MaxStats>,
	metadata: &BoundedVec<u8, T::MaxMetadata>,
) {
	let definition_size = ItemDefinition {
		kind,
		next_variant,
		stats: stats.clone(),
		metadata: metadata.clone(),
		supply: 0,
		ticket: (),
	}
	.encoded_size()
	.saturating_add(T::ItemDefConsideration::max_encoded_len());
	T::ItemDefConsideration::ensure_successful(owner, Footprint::from_parts(1, definition_size));
}

fn ensure_instance_consideration<T: Config>(
	owner: &T::AccountId,
	collection: CollectionId,
	item: ItemIndex,
	to: &T::AccountId,
) {
	let nft = Nft {
		instance: NextInstanceId::<T>::get(),
		collection,
		item,
		minted_at: 0,
		last_moved: 0,
		moves: 0,
	};
	T::InstanceConsideration::ensure_successful(
		owner,
		Footprint::from_parts(2, nft.encoded_size().saturating_add(to.encoded_size())),
	);
}

fn create_definition<T: Config>(
	owner: &T::AccountId,
	stats: BoundedVec<Stat, T::MaxStats>,
	metadata: BoundedVec<u8, T::MaxMetadata>,
) -> Result<(CollectionId, ItemIndex), BenchmarkError> {
	ensure_collection_consideration::<T>(owner);
	let collection = Pallet::<T>::do_create_collection(owner.clone())?;
	ensure_item_consideration::<T>(owner, Kind::Normal, None, &stats, &metadata);
	let item = Pallet::<T>::do_define_item(
		owner.clone(),
		collection,
		Kind::Normal,
		None,
		stats,
		metadata,
	)?;
	Ok((collection, item))
}

#[benchmarks(
	where
		T::RuntimeCall: From<Call<T>> + Dispatchable<Info = DispatchInfo> + GetDispatchInfo,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_collection() {
		let caller: T::AccountId = whitelisted_caller();
		ensure_collection_consideration::<T>(&caller);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()));

		assert_eq!(NextCollectionId::<T>::get(), 1);
		assert_eq!(Collections::<T>::get(0).map(|info| info.owner), Some(caller));
	}

	#[benchmark]
	fn define_item(
		s: Linear<1, { T::MaxStats::get() }>,
		m: Linear<1, { T::MaxMetadata::get() }>,
	) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		ensure_collection_consideration::<T>(&caller);
		let collection = Pallet::<T>::do_create_collection(caller.clone())?;
		let empty_stats = BoundedVec::default();
		let empty_metadata = BoundedVec::default();
		ensure_item_consideration::<T>(&caller, Kind::Special, None, &empty_stats, &empty_metadata);
		let variant = Pallet::<T>::do_define_item(
			caller.clone(),
			collection,
			Kind::Special,
			None,
			empty_stats,
			empty_metadata,
		)?;
		let stats = stats::<T>(s);
		let metadata = metadata::<T>(m);
		ensure_item_consideration::<T>(&caller, Kind::Normal, Some(variant), &stats, &metadata);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), collection, Kind::Normal, Some(variant), stats, metadata);

		let definition =
			ItemDefs::<T>::get(collection, variant + 1).expect("new definition exists");
		assert_eq!(definition.stats.len(), s as usize);
		assert_eq!(definition.metadata.len(), m as usize);
		Ok(())
	}

	#[benchmark]
	fn mint() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let destination: T::AccountId = account("destination", 0, 0);
		NftsByOwner::<T>::remove(&destination);
		let (collection, item) = create_definition::<T>(
			&caller,
			stats::<T>(T::MaxStats::get()),
			metadata::<T>(T::MaxMetadata::get()),
		)?;
		ensure_instance_consideration::<T>(&caller, collection, item, &destination);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), collection, item, destination.clone());

		let nft = NftsByOwner::<T>::get(&destination).expect("destination owns the minted NFT");
		assert_eq!(Instances::<T>::get(nft.instance), Some(destination));
		Ok(())
	}

	#[benchmark]
	fn transfer() -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let destination: T::AccountId = account("destination", 0, 0);
		NftsByOwner::<T>::remove(&destination);
		let (collection, item) =
			create_definition::<T>(&owner, BoundedVec::default(), BoundedVec::default())?;
		ensure_instance_consideration::<T>(&owner, collection, item, &owner);
		Pallet::<T>::do_mint(owner.clone(), collection, item, owner.clone())?;
		let nft = NftsByOwner::<T>::take(&owner).expect("owner has the minted NFT");
		let instance = nft.instance;
		let origin: T::RuntimeOrigin = Origin::<T>::Nft { owner, nft }.into();

		#[extrinsic_call]
		_(origin, destination.clone());

		assert_eq!(NftsByOwner::<T>::get(&destination).map(|nft| nft.instance), Some(instance));
		assert_eq!(Instances::<T>::get(instance), Some(destination));
		Ok(())
	}

	#[benchmark]
	fn burn() -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let (collection, item) =
			create_definition::<T>(&owner, BoundedVec::default(), BoundedVec::default())?;
		ensure_instance_consideration::<T>(&owner, collection, item, &owner);
		Pallet::<T>::do_mint(owner.clone(), collection, item, owner.clone())?;
		let nft = NftsByOwner::<T>::take(&owner).expect("owner has the minted NFT");
		let instance = nft.instance;
		assert!(InstanceDeposits::<T>::contains_key(instance));
		let origin: T::RuntimeOrigin = Origin::<T>::Nft { owner, nft }.into();

		#[extrinsic_call]
		_(origin);

		assert!(!Instances::<T>::contains_key(instance));
		assert!(!InstanceDeposits::<T>::contains_key(instance));
		Ok(())
	}

	#[benchmark]
	fn as_scarcity_pipeline() -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let destination: T::AccountId = account("destination", 0, 0);
		NftsByOwner::<T>::remove(&destination);
		let (collection, item) =
			create_definition::<T>(&owner, BoundedVec::default(), BoundedVec::default())?;
		ensure_instance_consideration::<T>(&owner, collection, item, &owner);
		Pallet::<T>::do_mint(owner.clone(), collection, item, owner.clone())?;
		Locked::<T>::insert(&owner, LockInfo { retries: u8::MAX, until: 0 });

		let call: T::RuntimeCall = Call::<T>::transfer { to: destination }.into();
		let info = call.get_dispatch_info();
		let extension = AsScarcity::<T>::new(Some(AsScarcityInfo::AsNft));
		let origin: T::RuntimeOrigin = RawOrigin::Signed(owner.clone()).into();

		#[block]
		{
			let (_, val, origin) = extension
				.validate(
					origin,
					&call,
					&info,
					0,
					(),
					&TxBaseImplication(()),
					TransactionSource::External,
				)
				.expect("worst-case scarcity transfer validates");
			let _pre = extension
				.prepare(val, &origin, &call, &info, 0)
				.expect("validated NFT is prepared");
		}

		assert!(!NftsByOwner::<T>::contains_key(&owner));
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
