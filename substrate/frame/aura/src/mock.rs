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

//! Test utilities

#![cfg(test)]

use crate::{self as pallet_aura, CurrentSlot};
use frame_support::{
	parameter_types,
	traits::{ConstU32, ConstU64, DisabledValidators, KeyOwnerProofSystem},
};
use sp_consensus_aura::{
	digests::CompatibleDigestItem,
	ed25519::{AuthorityId, AuthorityPair, AuthoritySignature},
	AuthorityIndex, EquivocationProof, Slot,
};
use sp_core::{crypto::Pair, H256, U256};
use sp_runtime::{
	impl_opaque_keys,
	testing::{Digest, DigestItem, Header, TestXt},
	traits::{Convert, Header as _, IdentityLookup, OpaqueKeys},
	BuildStorage,
};
use sp_staking::offence::{OffenceError, ReportOffence};

type Block = frame_system::mocking::MockBlock<Test>;

const SLOT_DURATION: u64 = 2;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Authorship: pallet_authorship,
		Session: pallet_session,
		Timestamp: pallet_timestamp,
		Aura: pallet_aura,
	}
);

impl<C> frame_system::offchain::SendTransactionTypes<C> for Test
where
	RuntimeCall: From<C>,
{
	type OverarchingCall = RuntimeCall;
	type Extrinsic = TestXt<RuntimeCall, ()>;
}

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type Nonce = u64;
	type RuntimeCall = RuntimeCall;
	type Hash = H256;
	type Hashing = ::sp_runtime::traits::BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_authorship::Config for Test {
	type FindAuthor = ();
	type EventHandler = ();
}

parameter_types! {
	pub const Period: u32 = 10;
	pub const Offset: u32 = 0;
}

impl_opaque_keys! {
	pub struct MockSessionKeys {
		pub aura_authority: super::Pallet<Test>,
	}
}

pub struct ValidatorIdOf<T>(sp_std::marker::PhantomData<T>);

impl<T: frame_system::Config> Convert<T::AccountId, Option<T::AccountId>> for ValidatorIdOf<T> {
	fn convert(controller: T::AccountId) -> Option<T::AccountId> {
		Some(controller)
	}
}

impl pallet_session::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = ValidatorIdOf<Self>;
	type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
	type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
	type SessionManager = ();
	type SessionHandler = <MockSessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type Keys = MockSessionKeys;
	type WeightInfo = ();
}

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
}

parameter_types! {
	static DisabledValidatorTestValue: Vec<AuthorityIndex> = Default::default();
	pub static AllowMultipleBlocksPerSlot: bool = false;
}

pub struct MockDisabledValidators;

impl MockDisabledValidators {
	pub fn disable_validator(index: AuthorityIndex) {
		DisabledValidatorTestValue::mutate(|v| {
			if let Err(i) = v.binary_search(&index) {
				v.insert(i, index);
			}
		})
	}
}

impl DisabledValidators for MockDisabledValidators {
	fn is_disabled(index: AuthorityIndex) -> bool {
		DisabledValidatorTestValue::get().binary_search(&index).is_ok()
	}
}

/// A mock offence report handler.
type IdentificationTuple = (sp_core::crypto::KeyTypeId, AuthorityId);

type EquivocationOffence = crate::equivocation::EquivocationOffence<IdentificationTuple>;

type MembershipProof = sp_session::MembershipProof;

pub struct TestOffenceHandler;

parameter_types! {
	pub static Offences: Vec<(Vec<u64>, EquivocationOffence)> = vec![];
}

impl ReportOffence<u64, IdentificationTuple, EquivocationOffence> for TestOffenceHandler {
	fn report_offence(
		reporters: Vec<u64>,
		offence: EquivocationOffence,
	) -> Result<(), OffenceError> {
		Offences::mutate(|l| l.push((reporters, offence)));
		Ok(())
	}

	fn is_known_offence(_offenders: &[IdentificationTuple], _time_slot: &Slot) -> bool {
		false
	}
}

pub struct TestKeyOwnerProofSystem;

impl KeyOwnerProofSystem<IdentificationTuple> for TestKeyOwnerProofSystem {
	type Proof = MembershipProof;
	type IdentificationTuple = IdentificationTuple;

	fn prove(_key: IdentificationTuple) -> Option<Self::Proof> {
		None
	}

	fn check_proof(
		key: IdentificationTuple,
		_proof: Self::Proof,
	) -> Option<Self::IdentificationTuple> {
		Some(key)
	}
}

impl pallet_aura::Config for Test {
	type AuthorityId = AuthorityId;
	type DisabledValidators = MockDisabledValidators;
	type MaxAuthorities = ConstU32<10>;
	type AllowMultipleBlocksPerSlot = AllowMultipleBlocksPerSlot;
	type KeyOwnerProof = MembershipProof;
	type EquivocationReportSystem = pallet_aura::equivocation::EquivocationReportSystem<
		Self,
		TestOffenceHandler,
		TestKeyOwnerProofSystem,
		sp_core::ConstU64<{ u64::MAX }>,
	>;
	#[cfg(feature = "experimental")]
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

pub fn new_test_ext_and_execute(
	authorities_len: usize,
	test: impl FnOnce(Vec<AuthorityPair>) -> (),
) {
	let (pairs, mut ext) = new_test_ext_with_pairs(authorities_len);
	ext.execute_with(|| {
		test(pairs);
		Aura::do_try_state().expect("Storage invariants should hold")
	});
}

pub fn new_test_ext_with_pairs(
	authorities_len: usize,
) -> (Vec<AuthorityPair>, sp_io::TestExternalities) {
	let pairs = (0..authorities_len)
		.map(|i| AuthorityPair::from_seed(&U256::from(i).into()))
		.collect::<Vec<_>>();

	let public = pairs.iter().map(|p| p.public()).collect();

	(pairs, new_test_ext_raw_authorities(public))
}

pub fn new_test_ext_raw_authorities(authorities: Vec<AuthorityId>) -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	pallet_aura::GenesisConfig::<Test> { authorities }
		.assimilate_storage(&mut storage)
		.unwrap();

	storage.into()
}

pub fn go_to_block(n: u64, s: u64) {
	use frame_support::traits::{OnFinalize, OnInitialize};

	Aura::on_finalize(System::block_number());

	let parent_hash = if System::block_number() > 1 {
		let hdr = System::finalize();
		hdr.hash()
	} else {
		System::parent_hash()
	};

	let digest = make_digest(s.into(), None);

	System::reset_events();
	System::initialize(&n, &parent_hash, &digest);

	Aura::on_initialize(n);
}

/// Slots will grow accordingly to blocks
pub fn progress_to_block(n: u64) {
	let mut slot = u64::from(Aura::current_slot()) + 1;
	for i in System::block_number() + 1..=n {
		go_to_block(i, slot);
		slot += 1;
	}
}

fn make_digest(slot: Slot, other: Option<Vec<u8>>) -> Digest {
	let item = <DigestItem as CompatibleDigestItem<AuthoritySignature>>::aura_pre_digest(slot);
	let mut logs = vec![item];
	if let Some(other) = other {
		logs.push(DigestItem::Other(other));
	}
	Digest { logs }
}

/// Creates an equivocation proof at the current block for current slot and block number
pub fn make_equivocation_proof(
	offender_authority_pair: &AuthorityPair,
) -> (EquivocationProof<Header, AuthorityId>, MembershipProof) {
	let current_block = System::block_number();
	let current_slot = CurrentSlot::<Test>::get();
	let validator_count = Aura::authorities().len() as u32;

	let make_header = |additional_data| {
		let mut header = Header::new(
			current_block,
			Default::default(),
			Default::default(),
			Default::default(),
			make_digest(current_slot, additional_data),
		);

		let seal = <DigestItem as CompatibleDigestItem<AuthoritySignature>>::aura_seal(
			offender_authority_pair.sign(header.hash().as_ref()),
		);
		header.digest_mut().push(seal);

		header
	};

	// Generate two different headers for `slot`.
	let first_header = make_header(None);
	let second_header = make_header(Some(vec![0xFF]));

	let equivocation_proof = EquivocationProof {
		slot: current_slot,
		offender: offender_authority_pair.public(),
		first_header,
		second_header,
	};

	let session = current_block as u32 / Period::get();

	// Dummy key owner proof
	let membership_proof = MembershipProof { session, trie_nodes: vec![], validator_count };

	(equivocation_proof, membership_proof)
}
