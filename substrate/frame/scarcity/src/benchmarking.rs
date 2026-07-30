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
use frame_benchmarking::v2::*;
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo, PostDispatchInfo},
	traits::{Consideration, Get},
};
use frame_system::RawOrigin;
use sp_runtime::{
	traits::{Bounded, Dispatchable, TransactionExtension, TxBaseImplication},
	transaction_validity::TransactionSource,
	DispatchError,
};

use super::*;
use crate::extension::{AsScarcity, AsScarcityInfo};

fn metadata_key<T: Config>(seed: u32) -> MetadataKeyOf<T> {
	let mut bytes = vec![0; T::MaxKeyLen::get() as usize];
	for (destination, source) in bytes.iter_mut().zip(seed.to_le_bytes()) {
		*destination = source;
	}
	bytes.try_into().ok().expect("key uses the configured maximum length")
}

fn metadata_value<T: Config>(byte: u8, len: u32) -> MetadataValueOf<T> {
	vec![byte; len as usize]
		.try_into()
		.ok()
		.expect("value length is benchmark-bounded")
}

fn fund<T: Config>(account: &T::AccountId) {
	T::Consideration::ensure_successful(account, BalanceOf::<T>::max_value() / 4u32.into());
}

fn create_definition<T: Config>(
	owner: &T::AccountId,
) -> Result<(CollectionId, ItemIndex), BenchmarkError> {
	fund::<T>(owner);
	let collection = Pallet::<T>::do_create_collection(owner.clone())?;
	let item = Pallet::<T>::do_define_item(owner.clone(), collection, Vec::new())?;
	Ok((collection, item))
}

#[benchmarks(
	where
		T::RuntimeCall: From<Call<T>>
			+ Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
			+ GetDispatchInfo,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_collection() {
		let caller: T::AccountId = whitelisted_caller();
		fund::<T>(&caller);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()));

		assert_eq!(NextCollectionId::<T>::get(), 1);
		assert_eq!(Collections::<T>::get(0).map(|info| info.owner), Some(caller));
	}

	#[benchmark]
	fn define_item(m: Linear<0, 100>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		fund::<T>(&caller);
		let collection = Pallet::<T>::do_create_collection(caller.clone())?;
		let metadata = (0..m)
			.map(|index| {
				let key = metadata_key::<T>(index);
				let value = metadata_value::<T>(0x42, T::MaxValueLen::get());
				(key, value)
			})
			.collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), collection, metadata);

		assert!(ItemDefs::<T>::contains_key(collection, 0));
		assert_eq!(ItemMetadata::<T>::iter_prefix((collection, 0)).count(), m as usize);
		Ok(())
	}

	#[benchmark]
	fn mint(m: Linear<0, { T::MaxInstanceMetadata::get() }>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let destination: T::AccountId = account("destination", 0, 0);
		NftsByOwner::<T>::remove(&destination);
		let (collection, item) = create_definition::<T>(&caller)?;
		let metadata = (0..m)
			.map(|index| {
				let key = metadata_key::<T>(index);
				let value = metadata_value::<T>(0x44, T::MaxValueLen::get());
				(key, value)
			})
			.collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), collection, item, destination.clone(), metadata);

		let nft = NftsByOwner::<T>::get(&destination).expect("destination owns the minted NFT");
		assert_eq!(Instances::<T>::get(nft.instance), Some(destination));
		assert_eq!(InstanceMetadataCount::<T>::get(nft.instance), m);
		Ok(())
	}

	#[benchmark]
	fn transfer() -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let destination: T::AccountId = account("destination", 0, 0);
		NftsByOwner::<T>::remove(&destination);
		let (collection, item) = create_definition::<T>(&owner)?;
		Pallet::<T>::do_mint(owner.clone(), collection, item, owner.clone(), Vec::new())?;
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
	fn burn(m: Linear<0, { T::MaxInstanceMetadata::get() }>) -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let (collection, item) = create_definition::<T>(&owner)?;
		let metadata = (0..m)
			.map(|index| {
				let key = metadata_key::<T>(index);
				let value = metadata_value::<T>(0x44, T::MaxValueLen::get());
				(key, value)
			})
			.collect::<Vec<_>>();
		Pallet::<T>::do_mint(owner.clone(), collection, item, owner.clone(), metadata)?;
		let nft = NftsByOwner::<T>::take(&owner).expect("owner has the minted NFT");
		let instance = nft.instance;
		assert!(InstanceDeposits::<T>::contains_key(instance));
		assert_eq!(InstanceMetadataCount::<T>::get(instance), m);
		let origin: T::RuntimeOrigin = Origin::<T>::Nft { owner, nft }.into();

		#[extrinsic_call]
		_(origin);

		assert!(!Instances::<T>::contains_key(instance));
		assert!(!InstanceDeposits::<T>::contains_key(instance));
		assert!(!InstanceMetadataCount::<T>::contains_key(instance));
		assert_eq!(InstanceMetadata::<T>::iter_prefix(instance).count(), 0);
		Ok(())
	}

	#[benchmark]
	fn nominate_collection_owner() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let nominee: T::AccountId = account("nominee", 0, 0);
		fund::<T>(&caller);
		let collection = Pallet::<T>::do_create_collection(caller.clone())?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), collection, Some(nominee.clone()));

		assert_eq!(
			Collections::<T>::get(collection).and_then(|info| info.pending_owner),
			Some(nominee),
		);
		Ok(())
	}

	#[benchmark]
	fn set_collection_metadata() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		fund::<T>(&caller);
		let collection = Pallet::<T>::do_create_collection(caller.clone())?;
		let key = metadata_key::<T>(0);
		let value = metadata_value::<T>(0x22, T::MaxValueLen::get());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), collection, key.clone(), Some(value.clone()));

		assert_eq!(Pallet::<T>::collection_metadata_of(collection, &key), Some(value));
		frame_system::Pallet::<T>::assert_last_event(
			Event::<T>::CollectionMetadataSet { collection, key }.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn set_item_metadata() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let (collection, item) = create_definition::<T>(&caller)?;
		let key = metadata_key::<T>(0);
		let value = metadata_value::<T>(0x22, T::MaxValueLen::get());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), collection, item, key.clone(), Some(value.clone()));

		assert_eq!(Pallet::<T>::item_metadata_of(collection, item, &key), Some(value));
		frame_system::Pallet::<T>::assert_last_event(
			Event::<T>::ItemMetadataSet { collection, item, key }.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn set_instance_metadata() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let destination: T::AccountId = account("destination", 0, 0);
		let (collection, item) = create_definition::<T>(&caller)?;
		let instance =
			Pallet::<T>::do_mint(caller.clone(), collection, item, destination, Vec::new())?;
		let key = metadata_key::<T>(0);
		let value = metadata_value::<T>(0x66, T::MaxValueLen::get());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), instance, key.clone(), Some(value.clone()));

		assert_eq!(Pallet::<T>::instance_metadata_of(instance, &key), Some(value));
		frame_system::Pallet::<T>::assert_last_event(
			Event::<T>::InstanceMetadataSet { instance, key }.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn force_burn(m: Linear<0, { T::MaxInstanceMetadata::get() }>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let destination: T::AccountId = account("destination", 0, 0);
		let (collection, item) = create_definition::<T>(&caller)?;
		let metadata = (0..m)
			.map(|index| {
				let key = metadata_key::<T>(index);
				let value = metadata_value::<T>(0x55, T::MaxValueLen::get());
				(key, value)
			})
			.collect::<Vec<_>>();
		let instance =
			Pallet::<T>::do_mint(caller.clone(), collection, item, destination.clone(), metadata)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), instance);

		assert!(!NftsByOwner::<T>::contains_key(destination));
		assert!(!Instances::<T>::contains_key(instance));
		assert_eq!(InstanceMetadata::<T>::iter_prefix(instance).count(), 0);
		Ok(())
	}

	#[benchmark]
	fn force_transfer() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let from: T::AccountId = account("from", 0, 0);
		let to: T::AccountId = account("to", 0, 0);
		NftsByOwner::<T>::remove(&to);
		let (collection, item) = create_definition::<T>(&caller)?;
		let instance =
			Pallet::<T>::do_mint(caller.clone(), collection, item, from.clone(), Vec::new())?;
		Locked::<T>::insert(&from, LockInfo { retries: u8::MAX, until: u64::MAX });

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), instance, to.clone());

		let nft = NftsByOwner::<T>::get(&to).expect("destination owns the transferred NFT");
		assert_eq!(nft.instance, instance);
		assert_eq!(nft.state_nonce, 1);
		assert_eq!(Instances::<T>::get(instance), Some(to));
		assert!(!NftsByOwner::<T>::contains_key(from.clone()));
		assert!(!Locked::<T>::contains_key(from));
		Ok(())
	}

	#[benchmark]
	fn delete_item() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let (collection, item) = create_definition::<T>(&caller)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), collection, item);

		assert!(!ItemDefs::<T>::contains_key(collection, item));
		assert_eq!(Collections::<T>::get(collection).map(|info| info.item_count), Some(0));
		Ok(())
	}

	#[benchmark]
	fn delete_collection() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		fund::<T>(&caller);
		let collection = Pallet::<T>::do_create_collection(caller.clone())?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), collection);

		assert!(!Collections::<T>::contains_key(collection));
		Ok(())
	}

	#[benchmark]
	fn claim_collection_ownership() -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let new_owner: T::AccountId = account("new_owner", 0, 0);
		fund::<T>(&owner);
		fund::<T>(&new_owner);
		let collection = Pallet::<T>::do_create_collection(owner.clone())?;
		Pallet::<T>::nominate_collection_owner(
			RawOrigin::Signed(owner.clone()).into(),
			collection,
			Some(new_owner.clone()),
		)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(new_owner.clone()), collection);

		let info = Collections::<T>::get(collection).expect("collection remains after claim");
		assert_eq!(info.owner, new_owner);
		assert_eq!(info.pending_owner, None);
		Ok(())
	}

	#[benchmark]
	fn as_scarcity_pipeline() -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let destination: T::AccountId = account("destination", 0, 0);
		NftsByOwner::<T>::remove(&destination);
		let (collection, item) = create_definition::<T>(&owner)?;
		Pallet::<T>::do_mint(owner.clone(), collection, item, owner.clone(), Vec::new())?;
		Locked::<T>::insert(&owner, LockInfo { retries: u8::MAX, until: 0 });
		let nft = NftsByOwner::<T>::get(&owner).expect("owner has the minted NFT");

		let call: T::RuntimeCall = Call::<T>::transfer { to: destination }.into();
		let info = call.get_dispatch_info();
		let extension = AsScarcity::<T>::new(Some(AsScarcityInfo::AsNft {
			instance: nft.instance,
			state_nonce: nft.state_nonce,
		}));
		let origin: T::RuntimeOrigin = RawOrigin::Signed(owner.clone()).into();
		let failed_dispatch = Err(DispatchError::Other("benchmark failure"));

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
			let pre = extension
				.prepare(val, &origin, &call, &info, 0)
				.expect("validated NFT is prepared");
			AsScarcity::<T>::post_dispatch_details(
				pre,
				&info,
				&Default::default(),
				0,
				&failed_dispatch,
			)
			.expect("failed dispatch is restored and locked");
		}

		assert!(NftsByOwner::<T>::contains_key(&owner));
		assert_eq!(Locked::<T>::get(&owner).map(|lock| lock.retries), Some(u8::MAX));
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
