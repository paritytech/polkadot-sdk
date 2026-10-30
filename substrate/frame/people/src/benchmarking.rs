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

extern crate alloc;

use alloc::vec;

use super::*;
use crate::extension::{AsPerson, AsPersonInfo};

use core::marker::{Send, Sync};
use frame_benchmarking::{account, v2::*, BenchmarkError};
use frame_support::{
	assert_ok,
	dispatch::RawOrigin,
	pallet_prelude::{Get, Pays},
	traits::{Len, OnIdle, OnPoll},
};
use frame_system::RawOrigin as SystemOrigin;
use sp_runtime::{
	generic::ExtensionVersion,
	traits::{AppendZerosInput, AsTransactionAuthorizedOrigin, DispatchTransaction},
	Weight,
};

const RI_ZERO: RingIndex = 0;
const SEED: u32 = 0;

type SecretOf<T> = <<T as Config>::Crypto as GenerateVerifiable>::Secret;

fn new_member_from<T: Config + Send + Sync>(i: u32, seed: u32) -> (SecretOf<T>, MemberOf<T>) {
	let mut entropy = &(i, seed).encode()[..];
	let mut entropy = AppendZerosInput::new(&mut entropy);
	let secret = T::Crypto::new_secret(Decode::decode(&mut entropy).unwrap());
	let public = T::Crypto::member_from_secret(&secret);
	(secret, public)
}

fn generate_members_for_ring<T: Config + Send + Sync>(
	seed: u32,
) -> Vec<(SecretOf<T>, MemberOf<T>)> {
	(0..T::MaxRingSize::get())
		.map(|i| new_member_from::<T>(i, seed))
		.collect::<Vec<_>>()
}

fn generate_members<T: Config + Send + Sync>(
	seed: u32,
	start: u32,
	end: u32,
) -> Vec<(SecretOf<T>, MemberOf<T>)> {
	(start..end).map(|i| new_member_from::<T>(i, seed)).collect::<Vec<_>>()
}

pub fn recognize_people<T: Config + Send + Sync>(
	members: &[(SecretOf<T>, MemberOf<T>)],
) -> Vec<(PersonalId, MemberOf<T>, SecretOf<T>)> {
	let mut people = Vec::new();
	for (secret, public) in members.iter() {
		let person = pallet::Pallet::<T>::reserve_new_id();
		pallet::Pallet::<T>::recognize_personhood(person, Some(public.clone())).unwrap();
		people.push((person, public.clone(), secret.clone()));
	}

	people
}

pub trait BenchmarkHelper<Chunk> {
	fn valid_account_context() -> Context;
	fn initialize_chunks() -> Vec<Chunk>;
}

#[cfg(feature = "std")]
impl BenchmarkHelper<()> for () {
	fn valid_account_context() -> Context {
		[0u8; 32]
	}

	fn initialize_chunks() -> Vec<()> {
		vec![]
	}
}

#[cfg(feature = "std")]
impl BenchmarkHelper<verifiable::ring_vrf_impl::StaticChunk> for () {
	fn valid_account_context() -> Context {
		[0u8; 32]
	}

	fn initialize_chunks() -> Vec<verifiable::ring_vrf_impl::StaticChunk> {
		vec![]
	}
}

fn prepare_chunks<T: Config>() {
	let chunks = T::BenchmarkHelper::initialize_chunks();

	let page_size = <T as Config>::ChunkPageSize::get();

	let mut page_idx = 0;
	let mut chunk_idx = 0;
	while chunk_idx < chunks.len() {
		let chunk_idx_end = core::cmp::min(chunk_idx + page_size as usize, chunks.len());
		let chunk_page: ChunksOf<T> = chunks[chunk_idx..chunk_idx_end]
			.to_vec()
			.try_into()
			.expect("page size was checked against the array length; qed");
		Chunks::<T>::insert(page_idx, chunk_page);
		page_idx += 1;
		chunk_idx = chunk_idx_end;
	}
}

#[benchmarks(
	where T: Send + Sync,
		<T as frame_system::Config>::RuntimeCall:
			Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo> + IsSubType<Call<T>> + From<Call<T>> + GetDispatchInfo,
		<T as frame_system::Config>::RuntimeOrigin: AsTransactionAuthorizedOrigin,
)]
mod benches {
	use super::*;

	#[benchmark]
	fn under_alias() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let members = generate_members_for_ring::<T>(SEED);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		// Create account and alias
		let account: T::AccountId = whitelisted_caller();
		let context = T::BenchmarkHelper::valid_account_context();
		let alias_value: Alias = [0u8; 32];
		let ra = RevisedContextualAlias {
			revision: 0,
			ring: RI_ZERO,
			ca: ContextualAlias { context, alias: alias_value },
		};

		// Set up alias account association
		let block_number = frame_system::Pallet::<T>::block_number();
		assert_ok!(pallet::Pallet::<T>::set_alias_account(
			Origin::PersonalAlias(ra.clone()).into(),
			account.clone(),
			block_number
		));
		assert!(AccountToAlias::<T>::contains_key(&account));
		assert!(AliasToAccount::<T>::contains_key(&ra.ca));

		// A simple call to benchmark with
		let call = frame_system::Call::<T>::remark { remark: vec![] };
		let boxed_call = Box::new(call.into());

		#[extrinsic_call]
		_(RawOrigin::Signed(account), boxed_call);

		Ok(())
	}

	#[benchmark]
	fn set_alias_account() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let members = generate_members_for_ring::<T>(SEED);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		let block_number = frame_system::Pallet::<T>::block_number();

		let alias_value: Alias = [0u8; 32];
		let alias = RevisedContextualAlias {
			ca: ContextualAlias {
				context: T::BenchmarkHelper::valid_account_context(),
				alias: alias_value,
			},
			revision: 0,
			ring: 0,
		};

		// An account had already been assigned to this alias
		let old_account: T::AccountId = account("test_old", 0, SEED);
		assert_ok!(pallet::Pallet::<T>::set_alias_account(
			Origin::PersonalAlias(alias.clone()).into(),
			old_account.clone(),
			block_number
		));
		assert!(AccountToAlias::<T>::contains_key(&old_account));
		assert!(AliasToAccount::<T>::contains_key(&alias.ca));

		let account: T::AccountId = account("test", 0, SEED);

		#[extrinsic_call]
		_(Origin::PersonalAlias(alias.clone()), account.clone(), block_number);

		assert!(!AccountToAlias::<T>::contains_key(&old_account));
		assert!(AccountToAlias::<T>::contains_key(&account));
		assert!(AliasToAccount::<T>::contains_key(&alias.ca));
		assert_eq!(AliasToAccount::<T>::get(&alias.ca), Some(account));

		Ok(())
	}

	#[benchmark]
	fn unset_alias_account() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let members = generate_members_for_ring::<T>(SEED);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		let account: T::AccountId = account("test", 0, SEED);
		let block_number = frame_system::Pallet::<T>::block_number();

		let alias_value: Alias = [0u8; 32];
		let alias = RevisedContextualAlias {
			ca: ContextualAlias {
				context: T::BenchmarkHelper::valid_account_context(),
				alias: alias_value,
			},
			revision: 0,
			ring: 0,
		};

		assert_ok!(pallet::Pallet::<T>::set_alias_account(
			Origin::PersonalAlias(alias.clone()).into(),
			account.clone(),
			block_number
		));
		assert!(AccountToAlias::<T>::contains_key(&account));
		assert!(AliasToAccount::<T>::contains_key(&alias.ca));

		#[extrinsic_call]
		_(Origin::PersonalAlias(alias.clone()));

		assert!(!AccountToAlias::<T>::contains_key(&account));
		assert!(!AliasToAccount::<T>::contains_key(&alias.ca));

		Ok(())
	}

	#[benchmark]
	fn force_recognize_personhood() -> Result<(), BenchmarkError> {
		let members = generate_members_for_ring::<T>(SEED);

		#[extrinsic_call]
		_(SystemOrigin::Root, members.iter().map(|(_, m)| m.clone()).collect::<Vec<_>>());

		for person in members {
			assert!(pallet::Keys::<T>::get(person.1).is_some());
		}

		Ok(())
	}

	#[benchmark]
	fn set_personal_id_account() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let members = generate_members_for_ring::<T>(SEED);
		let people = recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		// Get one of the generated people's information
		let (personal_id, _, _): &(PersonalId, MemberOf<T>, SecretOf<T>) = &people[0];

		let account: T::AccountId = account("test", 0, SEED);
		let block_number = frame_system::Pallet::<T>::block_number();

		// An account had already been assigned to this personal id
		let old_account: T::AccountId = frame_benchmarking::account("test_old", 0, SEED);
		assert_ok!(pallet::Pallet::<T>::set_personal_id_account(
			Origin::PersonalIdentity(*personal_id).into(),
			old_account.clone(),
			block_number
		));

		#[extrinsic_call]
		_(Origin::PersonalIdentity(*personal_id), account.clone(), block_number);

		assert_eq!(AccountToPersonalId::<T>::get(&old_account), None);
		assert_eq!(AccountToPersonalId::<T>::get(&account), Some(*personal_id));
		assert!(People::<T>::get(personal_id).is_some());
		assert_eq!(People::<T>::get(personal_id).unwrap().account, Some(account));

		Ok(())
	}

	#[benchmark]
	fn unset_personal_id_account() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let members = generate_members_for_ring::<T>(SEED);
		let people = recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		// Get one of the generated people's information
		let (personal_id, _, _): &(PersonalId, MemberOf<T>, SecretOf<T>) = &people[0];

		let account: T::AccountId = account("test", 0, SEED);
		let block_number = frame_system::Pallet::<T>::block_number();

		// An account had already been assigned to this personal id
		let old_account: T::AccountId = frame_benchmarking::account("test_old", 0, SEED);
		assert_ok!(pallet::Pallet::<T>::set_personal_id_account(
			Origin::PersonalIdentity(*personal_id).into(),
			old_account.clone(),
			block_number
		));

		#[extrinsic_call]
		_(Origin::PersonalIdentity(*personal_id));

		assert_eq!(AccountToPersonalId::<T>::get(&old_account), None);
		assert_eq!(AccountToPersonalId::<T>::get(&account), None);
		assert!(People::<T>::get(personal_id).is_some());
		assert_eq!(People::<T>::get(personal_id).unwrap().account, None);

		Ok(())
	}

	#[benchmark]
	fn set_onboarding_size() -> Result<(), BenchmarkError> {
		#[extrinsic_call]
		_(SystemOrigin::Root, 1);

		assert_eq!(OnboardingSize::<T>::get(), 1);

		Ok(())
	}

	#[benchmark]
	fn merge_rings() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Two rings exist
		let ring_size: u32 = <T as Config>::MaxRingSize::get();
		let members = generate_members::<T>(SEED, 0, ring_size * 2);

		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		assert_eq!(RingKeysStatus::<T>::get(RI_ZERO).total, ring_size);

		assert_ok!(pallet::Pallet::<T>::onboard_people());
		assert_eq!(RingKeysStatus::<T>::get(1).total, ring_size);

		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, T::MaxRingSize::get()));
		assert_eq!(RingKeysStatus::<T>::get(RI_ZERO).included, ring_size);

		assert_ok!(pallet::Pallet::<T>::build_ring(1, T::MaxRingSize::get()));
		assert_eq!(RingKeysStatus::<T>::get(1).included, ring_size);

		// Suspend and remove more than half of the people in both rings
		assert_ok!(pallet::Pallet::<T>::start_people_set_mutation_session());
		let suspensions: Vec<PersonalId> = (1..ring_size / 2 + 3)
			.chain(ring_size + 1..ring_size * 3 / 2 + 3)
			.map(|i| i as PersonalId)
			.collect();
		assert_ok!(pallet::Pallet::<T>::suspend_personhood(&suspensions));
		assert_ok!(pallet::Pallet::<T>::end_people_set_mutation_session());

		assert!(PendingSuspensions::<T>::get(RI_ZERO).len() > (ring_size / 2) as usize);
		assert!(PendingSuspensions::<T>::get(1).len() > (ring_size / 2) as usize);

		let mut meter = WeightMeter::new();
		pallet::Pallet::<T>::migrate_keys(&mut meter);

		pallet::Pallet::<T>::remove_suspended_keys(RI_ZERO);
		pallet::Pallet::<T>::remove_suspended_keys(1);

		assert!(RingKeys::<T>::get(RI_ZERO).len() < (ring_size / 2) as usize);
		assert!(RingKeys::<T>::get(1).len() < (ring_size / 2) as usize);

		let keys_left_len = RingKeys::<T>::get(RI_ZERO).len() + RingKeys::<T>::get(1).len();

		// The current ring has to have a higher index than the ones being merged
		CurrentRingIndex::<T>::set(14);

		let account: T::AccountId = account("caller", 0, SEED);

		#[extrinsic_call]
		_(SystemOrigin::Signed(account), RI_ZERO, 1);

		assert_eq!(RingKeys::<T>::get(RI_ZERO).len(), keys_left_len);
		assert_eq!(RingKeysStatus::<T>::get(RI_ZERO).total, keys_left_len as u32);
		assert!(Root::<T>::get(RI_ZERO).is_some());
		assert!(Root::<T>::get(1).is_none());

		Ok(())
	}

	#[benchmark]
	fn migrate_included_key() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let members = generate_members_for_ring::<T>(SEED);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		let temp_key = new_member_from::<T>(u32::MAX, SEED).1;
		KeyMigrationQueue::<T>::insert(0, temp_key);

		let new_key = new_member_from::<T>(u32::MAX - 1, SEED).1;

		#[extrinsic_call]
		_(Origin::PersonalIdentity(0u64), new_key.clone());

		// Pending suspensions are reflected in the ring status.
		assert_eq!(KeyMigrationQueue::<T>::get(0), Some(new_key));

		Ok(())
	}

	#[benchmark]
	fn migrate_onboarding_key() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let members = generate_members_for_ring::<T>(SEED);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		let temp_key = new_member_from::<T>(u32::MAX, SEED).1;

		let new_person = pallet::Pallet::<T>::reserve_new_id();
		pallet::Pallet::<T>::recognize_personhood(new_person, Some(temp_key.clone())).unwrap();

		let new_key = new_member_from::<T>(u32::MAX - 1, SEED).1;

		#[extrinsic_call]
		_(Origin::PersonalIdentity(new_person), new_key.clone());

		// Pending suspensions are reflected in the ring status.
		assert!(KeyMigrationQueue::<T>::iter().next().is_none());
		assert_eq!(OnboardingQueue::<T>::get(0)[0], new_key);

		Ok(())
	}

	#[benchmark]
	fn should_build_ring(n: Linear<1, { T::MaxRingSize::get() }>) -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// One full queue page of people awaiting
		let queue_page_size: u32 = <T as Config>::OnboardingQueuePageSize::get();
		let ring_size: u32 = <T as Config>::MaxRingSize::get();
		let members = generate_members::<T>(SEED, 0, queue_page_size);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());

		// No ring built but people onboarded successfully
		assert!(Root::<T>::get(RI_ZERO).is_none());
		assert_eq!(RingKeys::<T>::get(RI_ZERO).len(), ring_size as usize);
		assert_eq!(RingKeysStatus::<T>::get(RI_ZERO), RingStatus { total: ring_size, included: 0 });

		#[block]
		{
			let _ = Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get());
		}

		Ok(())
	}

	#[benchmark]
	fn build_ring(n: Linear<1, { T::MaxRingSize::get() }>) -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// One full queue page of people awaiting
		let queue_page_size: u32 = <T as Config>::OnboardingQueuePageSize::get();
		let ring_size: u32 = <T as Config>::MaxRingSize::get();
		let members = generate_members::<T>(SEED, 0, queue_page_size);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());

		// No ring built but people onboarded successfully
		assert!(Root::<T>::get(RI_ZERO).is_none());
		assert_eq!(RingKeys::<T>::get(RI_ZERO).len(), ring_size as usize);
		assert_eq!(RingKeysStatus::<T>::get(RI_ZERO), RingStatus { total: ring_size, included: 0 });

		#[block]
		{
			assert_ok!(Pallet::<T>::build_ring(RI_ZERO, n));
		}

		// The ring becomes built
		assert!(Root::<T>::get(RI_ZERO).is_some());
		assert_eq!(RingKeys::<T>::get(RI_ZERO).len(), ring_size as usize);
		assert_eq!(RingKeysStatus::<T>::get(RI_ZERO), RingStatus { total: ring_size, included: n });

		Ok(())
	}

	#[benchmark]
	fn onboard_people() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// One full ring exists
		let ring_size: u32 = <T as Config>::MaxRingSize::get();
		let members = generate_members::<T>(SEED, 0, ring_size);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));
		assert_eq!(RingKeys::<T>::get(RI_ZERO).len(), ring_size as usize);
		assert_eq!(
			RingKeysStatus::<T>::get(RI_ZERO),
			RingStatus { total: ring_size, included: ring_size }
		);

		assert_eq!(QueuePageIndices::<T>::get(), (0, 0));
		assert!(OnboardingQueue::<T>::get(0).is_empty());

		// 1st onboarding page with fewer people than open slots
		let keys_len: u32 = Keys::<T>::iter().collect::<Vec<_>>().len().try_into().unwrap();
		let members = generate_members::<T>(SEED, keys_len, keys_len + ring_size / 2);
		recognize_people::<T>(&members);
		assert_eq!(OnboardingQueue::<T>::get(0).len(), (ring_size as u8 / 2) as usize);

		// To stop adding keys to the first page and start filling the next one
		QueuePageIndices::<T>::put((0, 1));
		assert!(OnboardingQueue::<T>::get(1).is_empty());

		// 2nd onboarding page full
		let keys_len: u32 = Keys::<T>::iter().collect::<Vec<_>>().len().try_into().unwrap();
		assert_eq!(keys_len, (ring_size + ring_size / 2));
		let queue_page_size: u32 = <T as Config>::OnboardingQueuePageSize::get();
		let members = generate_members::<T>(SEED, keys_len, keys_len + queue_page_size);
		recognize_people::<T>(&members);

		assert_eq!(QueuePageIndices::<T>::get(), (0, 1));
		assert_eq!(OnboardingQueue::<T>::get(0).len(), (ring_size / 2) as usize);
		assert!(OnboardingQueue::<T>::get(1).is_full());

		assert_eq!(RingKeys::<T>::get(1).len(), 0);

		#[block]
		{
			assert_ok!(Pallet::<T>::onboard_people());
		}

		assert_eq!(RingKeys::<T>::get(1).len(), ring_size as usize);
		assert_eq!(RingKeysStatus::<T>::get(1), RingStatus { total: ring_size, included: 0 });

		Ok(())
	}

	#[benchmark]
	fn pending_suspensions_iteration() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let members = generate_members_for_ring::<T>(SEED);
		let max_ring_size = T::MaxRingSize::get();
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include = pallet::Pallet::<T>::should_build_ring(RI_ZERO, max_ring_size).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		// Suspend all people in the ring
		assert_ok!(pallet::Pallet::<T>::start_people_set_mutation_session());
		let suspensions: Vec<PersonalId> = (0..max_ring_size as PersonalId).collect();
		assert_ok!(pallet::Pallet::<T>::suspend_personhood(&suspensions));
		assert_ok!(pallet::Pallet::<T>::end_people_set_mutation_session());
		let mut meter = WeightMeter::new();
		pallet::Pallet::<T>::migrate_keys(&mut meter);

		// To make sure they are indeed pending suspension
		assert_eq!(PendingSuspensions::<T>::get(RI_ZERO).len(), max_ring_size as usize);

		#[block]
		{
			assert!(PendingSuspensions::<T>::iter_keys().next().is_some());
		}

		Ok(())
	}

	#[benchmark]
	fn remove_suspended_keys(
		n: Linear<1, { T::MaxRingSize::get() }>,
	) -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let members = generate_members_for_ring::<T>(SEED);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		// For later verification
		let initial_root = Root::<T>::get(RI_ZERO).unwrap();

		// Suspend 'n' number of people in the ring
		assert_ok!(pallet::Pallet::<T>::start_people_set_mutation_session());
		let suspensions: Vec<PersonalId> = (0..n as PersonalId).collect();
		assert_ok!(pallet::Pallet::<T>::suspend_personhood(&suspensions));
		assert_ok!(pallet::Pallet::<T>::end_people_set_mutation_session());
		let mut meter = WeightMeter::new();
		pallet::Pallet::<T>::migrate_keys(&mut meter);

		// To make sure they are indeed pending suspension
		assert_eq!(PendingSuspensions::<T>::get(RI_ZERO).len(), n as usize);

		#[block]
		{
			pallet::Pallet::<T>::remove_suspended_keys(RI_ZERO);
		}

		// Pending suspensions are cleared for the ring
		assert!(PendingSuspensions::<T>::get(RI_ZERO).is_empty());

		// Ring data becomes modified
		let ring_size: u32 = <T as Config>::MaxRingSize::get();
		assert_eq!(
			RingKeysStatus::<T>::get(RI_ZERO),
			RingStatus { included: 0, total: ring_size - n as u32 }
		);
		assert_eq!(RingKeys::<T>::get(RI_ZERO).len(), (ring_size - n as u32) as usize);
		assert_ne!(Root::<T>::get(RI_ZERO).unwrap().intermediate, initial_root.intermediate);

		Ok(())
	}

	#[benchmark]
	fn migrate_keys_single_included_key() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		let max_members = T::MaxRingSize::get();
		// Generate people and build a ring
		let members = generate_members_for_ring::<T>(SEED);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		// Migrate 'n' number of people in the ring
		for (personal_id, key) in (0..max_members as PersonalId)
			.map(|i| new_member_from::<T>(u32::MAX - i as u32, SEED).1)
			.enumerate()
		{
			assert_ok!(pallet::Pallet::<T>::migrate_included_key(
				Origin::PersonalIdentity(personal_id as PersonalId).into(),
				key
			));
		}
		assert_ok!(pallet::Pallet::<T>::start_people_set_mutation_session());
		assert_ok!(pallet::Pallet::<T>::end_people_set_mutation_session());
		assert!(PendingSuspensions::<T>::get(RI_ZERO).is_empty());
		// All migrated keys are queued, but we only want one as this function benchmarks just one
		// iteration of `migrate_keys`.
		assert_eq!(KeyMigrationQueue::<T>::iter().count(), T::MaxRingSize::get() as usize);
		let (first_id, first_key) = KeyMigrationQueue::<T>::iter().next().unwrap();

		#[block]
		{
			assert_ok!(pallet::Pallet::<T>::migrate_keys_single_included_key(first_id, first_key));
		}

		// Pending suspensions are reflected in the ring status.
		assert_eq!(PendingSuspensions::<T>::get(RI_ZERO).len(), 1);

		Ok(())
	}

	#[benchmark]
	fn merge_queue_pages() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Two pages exists: first is full, the second contains one member
		let queue_page_size: u32 = <T as Config>::OnboardingQueuePageSize::get();
		let members = generate_members::<T>(SEED, 0, queue_page_size + 1);
		recognize_people::<T>(&members);

		assert_eq!(QueuePageIndices::<T>::get(), (0, 1));
		assert!(OnboardingQueue::<T>::get(0).is_full());
		assert_eq!(OnboardingQueue::<T>::get(1).len(), 1);

		// One key is removed from the first page
		OnboardingQueue::<T>::mutate(0, |keys| {
			keys.pop();
		});
		assert_eq!(OnboardingQueue::<T>::get(0).len(), queue_page_size as usize - 1);

		// Attempt to merge pages succeeds
		let QueueMergeAction::Merge { initial_head, new_head, first_key_page, second_key_page } =
			pallet::Pallet::<T>::should_merge_queue_pages()
		else {
			panic!("should be mergeable")
		};

		#[block]
		{
			pallet::Pallet::<T>::merge_queue_pages(
				initial_head,
				new_head,
				first_key_page,
				second_key_page,
			);
		}

		// The queue pages have changed
		assert_eq!(QueuePageIndices::<T>::get(), (1, 1));
		assert!(OnboardingQueue::<T>::get(0).is_empty());
		assert!(OnboardingQueue::<T>::get(1).is_full());

		Ok(())
	}

	#[benchmark]
	fn on_poll_base() -> Result<(), BenchmarkError> {
		// Two pages exists: first is full, the second contains one member
		let queue_page_size: u32 = <T as Config>::OnboardingQueuePageSize::get();
		let members = generate_members::<T>(SEED, 0, queue_page_size + 1);
		recognize_people::<T>(&members);

		assert_eq!(QueuePageIndices::<T>::get(), (0, 1));
		assert!(OnboardingQueue::<T>::get(0).is_full());
		assert_eq!(OnboardingQueue::<T>::get(1).len(), 1);
		assert!(RingsState::<T>::get().append_only());

		let mut meter = WeightMeter::new();

		#[block]
		{
			pallet::Pallet::<T>::on_poll(0u32.into(), &mut meter);
		}

		assert_eq!(meter.consumed(), T::WeightInfo::on_poll_base());
		Ok(())
	}

	#[benchmark]
	fn on_idle_base() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Two pages exists: first is full, the second contains one member
		let queue_page_size: u32 = <T as Config>::OnboardingQueuePageSize::get();
		let ring_size = T::MaxRingSize::get();
		let members = generate_members::<T>(SEED, 0, queue_page_size + 1);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());

		// No ring built but people onboarded successfully
		assert!(Root::<T>::get(RI_ZERO).is_none());
		assert_eq!(RingKeys::<T>::get(RI_ZERO).len(), ring_size as usize);
		assert_eq!(RingKeysStatus::<T>::get(RI_ZERO), RingStatus { total: ring_size, included: 0 });
		let to_include = Pallet::<T>::should_build_ring(RI_ZERO, ring_size).unwrap();
		assert_ok!(Pallet::<T>::build_ring(RI_ZERO, to_include));
		// The ring becomes built
		assert!(Root::<T>::get(RI_ZERO).is_some());
		assert_eq!(RingKeys::<T>::get(RI_ZERO).len(), ring_size as usize);
		assert_eq!(
			RingKeysStatus::<T>::get(RI_ZERO),
			RingStatus { total: ring_size, included: ring_size }
		);

		#[block]
		{
			pallet::Pallet::<T>::on_idle(0u32.into(), Weight::MAX);
		}

		Ok(())
	}

	#[benchmark]
	fn as_person_alias_with_account() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let members = generate_members_for_ring::<T>(SEED);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		// Create account and alias
		let account: T::AccountId = account("caller", 0, SEED);
		let context = T::BenchmarkHelper::valid_account_context();
		let alias_value: Alias = [0u8; 32];
		let ra = RevisedContextualAlias {
			revision: 0,
			ring: RI_ZERO,
			ca: ContextualAlias { context, alias: alias_value },
		};

		// Set up alias account association
		let block_number = frame_system::Pallet::<T>::block_number();
		assert_ok!(pallet::Pallet::<T>::set_alias_account(
			Origin::PersonalAlias(ra.clone()).into(),
			account.clone(),
			block_number
		));
		assert!(AccountToAlias::<T>::contains_key(&account));
		assert!(AliasToAccount::<T>::contains_key(&ra.ca));

		// A simple call to benchmark with
		let inner = frame_system::Call::<T>::remark { remark: vec![] };
		let call: <T as frame_system::Config>::RuntimeCall = inner.into();

		let ext =
			AsPerson::new(Some(AsPersonInfo::<T>::AsPersonalAliasWithAccount(T::Nonce::default())));
		let info = call.get_dispatch_info();
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(10, 0)),
			pays_fee: Pays::Yes,
		};
		let len = call.encoded_size();

		#[block]
		{
			ext.test_run(RawOrigin::Signed(account).into(), &call, &info, len, 0, |_| {
				Ok(post_info)
			})
			.unwrap()
			.unwrap();
		}

		Ok(())
	}

	#[benchmark]
	fn as_person_identity_with_account() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let members = generate_members_for_ring::<T>(SEED);
		let recognized_people = recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		// Select one of the generated people's information
		let (personal_id, _, _): &(PersonalId, MemberOf<T>, SecretOf<T>) = &recognized_people[0];

		// Set up personal ID account association
		let account: T::AccountId = account("caller", 0, SEED);
		let block_number = frame_system::Pallet::<T>::block_number();
		assert_ok!(pallet::Pallet::<T>::set_personal_id_account(
			Origin::PersonalIdentity(*personal_id).into(),
			account.clone(),
			block_number
		));
		assert!(AccountToPersonalId::<T>::contains_key(&account));

		// A simple call to benchmark with
		let inner = frame_system::Call::<T>::remark { remark: vec![] };
		let call: <T as frame_system::Config>::RuntimeCall = inner.into();

		let ext = AsPerson::new(Some(AsPersonInfo::<T>::AsPersonalIdentityWithAccount(
			T::Nonce::default(),
		)));
		let info = call.get_dispatch_info();
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(10, 0)),
			pays_fee: Pays::Yes,
		};
		let len = call.encoded_size();

		#[block]
		{
			ext.test_run(RawOrigin::Signed(account).into(), &call, &info, len, 0, |_| {
				Ok(post_info)
			})
			.unwrap()
			.unwrap();
		}

		Ok(())
	}

	#[benchmark]
	fn as_person_alias_with_proof() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let account: T::AccountId = account("caller", 0, SEED);
		let members = generate_members_for_ring::<T>(SEED);
		recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		// The call to set the alias, the only one valid for this extension code path.
		let block_number = frame_system::Pallet::<T>::block_number();
		let inner = Call::<T>::set_alias_account { account, call_valid_at: block_number };
		let call: <T as frame_system::Config>::RuntimeCall = inner.into();

		let context = T::BenchmarkHelper::valid_account_context();
		let ext_version: ExtensionVersion = 0;

		// Generate a valid proof
		let proof = (ext_version, &call).using_encoded(|msg| {
			let (secret, member) = &members[0];
			T::Crypto::create(
				T::Crypto::open(member, members.iter().map(|(_, m)| m.clone())).unwrap(),
				secret,
				&context[..],
				&sp_io::hashing::blake2_256(msg),
			)
			.map(|(p, _)| p)
			.expect("should create proof")
		});

		let ext = AsPerson::new(Some(AsPersonInfo::<T>::AsPersonalAliasWithProof(
			proof, RI_ZERO, context,
		)));
		let info = call.get_dispatch_info();
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(10, 0)),
			pays_fee: Pays::Yes,
		};
		let len = call.encoded_size();

		#[block]
		{
			ext.test_run(RawOrigin::None.into(), &call, &info, len, 0, |_| Ok(post_info))
				.unwrap()
				.unwrap();
		}

		Ok(())
	}

	#[benchmark]
	fn as_person_identity_with_proof() -> Result<(), BenchmarkError> {
		prepare_chunks::<T>();

		// Generate people and build a ring
		let account: T::AccountId = account("caller", 0, SEED);
		let members = generate_members_for_ring::<T>(SEED);
		let recognized_people = recognize_people::<T>(&members);
		assert_ok!(pallet::Pallet::<T>::onboard_people());
		let to_include =
			pallet::Pallet::<T>::should_build_ring(RI_ZERO, T::MaxRingSize::get()).unwrap();
		assert_ok!(pallet::Pallet::<T>::build_ring(RI_ZERO, to_include));

		// Select one of the generated people's information
		let (personal_id, _, secret): &(PersonalId, MemberOf<T>, SecretOf<T>) =
			&recognized_people[0];

		// The call to set the personal ID account, the only one valid for this extension code path.
		let block_number = frame_system::Pallet::<T>::block_number();
		let inner = Call::<T>::set_personal_id_account { account, call_valid_at: block_number };
		let call: <T as frame_system::Config>::RuntimeCall = inner.into();
		let ext_version: ExtensionVersion = 0;
		let signature = (ext_version, &call).using_encoded(|msg| {
			<T::Crypto as GenerateVerifiable>::sign(secret, &sp_io::hashing::blake2_256(msg))
				.expect("failed to create signature")
		});

		let ext = AsPerson::new(Some(AsPersonInfo::<T>::AsPersonalIdentityWithProof(
			signature,
			*personal_id,
		)));
		let info = call.get_dispatch_info();
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(10, 0)),
			pays_fee: Pays::Yes,
		};
		let len = call.encoded_size();

		#[block]
		{
			ext.test_run(RawOrigin::None.into(), &call, &info, len, 0, |_| Ok(post_info))
				.unwrap()
				.unwrap();
		}

		Ok(())
	}

	// Implements a test for each benchmark. Execute with:
	// `cargo test -p pallet-people --features runtime-benchmarks`.
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
