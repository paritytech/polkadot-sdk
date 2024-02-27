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

//! Test utilities for Sassafras pallet.

use crate::{self as pallet_sassafras, EpochChangeInternalTrigger, *};

use frame_support::{
	derive_impl,
	traits::{ConstU32, OnFinalize, OnInitialize},
};
use sp_consensus_sassafras::{
	digests::SlotClaim,
	vrf::{RingProver, VrfSignature},
	AuthorityIndex, AuthorityPair, EpochConfiguration, Slot, TicketBody, TicketEnvelope, TicketId,
};
use sp_core::{
	crypto::{ByteArray, Pair, UncheckedFrom, VrfSecret, Wraps},
	ed25519::Public as EphemeralPublic,
	H256, U256,
};
use sp_runtime::{
	testing::{Digest, DigestItem, Header, TestXt},
	BuildStorage,
};

const LOG_TARGET: &str = "sassafras::tests";

const EPOCH_LENGTH: u32 = 10;
const MAX_AUTHORITIES: u32 = 100;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlock<Test>;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Test
where
	RuntimeCall: From<C>,
{
	type OverarchingCall = RuntimeCall;
	type Extrinsic = TestXt<RuntimeCall, ()>;
}

impl pallet_sassafras::Config for Test {
	type EpochLength = ConstU32<EPOCH_LENGTH>;
	type MaxAuthorities = ConstU32<MAX_AUTHORITIES>;
	type EpochChangeTrigger = EpochChangeInternalTrigger;
	type WeightInfo = ();
}

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Sassafras: pallet_sassafras,
	}
);

// Default used for most of the tests.
//
// The redundancy factor has been set to max value to accept all submitted
// tickets without worrying about the threshold.
pub const TEST_EPOCH_CONFIGURATION: EpochConfiguration =
	EpochConfiguration { redundancy_factor: u32::MAX, attempts_number: 5 };

/// Build and returns test storage externalities
pub fn new_test_ext(authorities_len: usize) -> sp_io::TestExternalities {
	new_test_ext_with_pairs(authorities_len, false).1
}

/// Build and returns test storage externalities and authority set pairs used
/// by Sassafras genesis configuration.
pub fn new_test_ext_with_pairs(
	authorities_len: usize,
	with_ring_context: bool,
) -> (Vec<AuthorityPair>, sp_io::TestExternalities) {
	let pairs = (0..authorities_len)
		.map(|i| AuthorityPair::from_seed(&U256::from(i).into()))
		.collect::<Vec<_>>();

	let authorities: Vec<_> = pairs.iter().map(|p| p.public()).collect();

	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	pallet_sassafras::GenesisConfig::<Test> {
		authorities: authorities.clone(),
		epoch_config: TEST_EPOCH_CONFIGURATION,
		_phantom: core::marker::PhantomData,
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	let mut ext: sp_io::TestExternalities = storage.into();

	if with_ring_context {
		ext.execute_with(|| {
			log::debug!(target: LOG_TARGET, "Building testing ring context");
			let ring_ctx = vrf::RingContext::new_testing();
			RingContext::<Test>::set(Some(ring_ctx.clone()));
			Sassafras::update_ring_verifier(&authorities);
		});
	}

	(pairs, ext)
}

fn make_ticket_with_prover(
	attempt: u32,
	pair: &AuthorityPair,
	prover: &RingProver,
) -> TicketEnvelope {
	log::debug!("attempt: {}", attempt);

	// Values are referring to the next epoch
	let epoch = Sassafras::epoch_index() + 1;
	let randomness = Sassafras::next_randomness();

	// Make a dummy ephemeral public that hopefully is unique within one test instance.
	// In the tests, the values within the erased public are just used to compare
	// ticket bodies, so it is not important to be a valid key.
	let mut raw: [u8; 32] = [0; 32];
	raw.copy_from_slice(&pair.public().as_slice()[0..32]);
	let erased_public = EphemeralPublic::unchecked_from(raw);
	let revealed_public = erased_public;

	let ticket_id_input = vrf::ticket_id_input(&randomness, attempt, epoch);

	let body = TicketBody { attempt_idx: attempt, erased_public, revealed_public };
	let sign_data = vrf::ticket_body_sign_data(&body, ticket_id_input);

	let signature = pair.as_ref().ring_vrf_sign(&sign_data, &prover);

	// Ticket-id can be generated via vrf-preout.
	// We don't care that much about its value here.
	TicketEnvelope { body, signature }
}

pub fn make_prover(pair: &AuthorityPair) -> RingProver {
	let public = pair.public();
	let mut prover_idx = None;

	let ring_ctx = Sassafras::ring_context().unwrap();

	let pks: Vec<sp_core::bandersnatch::Public> = Sassafras::authorities()
		.iter()
		.enumerate()
		.map(|(idx, auth)| {
			if public == *auth {
				prover_idx = Some(idx);
			}
			*auth.as_ref()
		})
		.collect();

	log::debug!("Building prover. Ring size: {}", pks.len());
	let prover = ring_ctx.prover(&pks, prover_idx.unwrap()).unwrap();
	log::debug!("Done");

	prover
}

/// Construct `attempts` tickets envelopes for the next epoch.
///
/// E.g. by passing an optional threshold
pub fn make_tickets(attempts: u32, pair: &AuthorityPair) -> Vec<TicketEnvelope> {
	let prover = make_prover(pair);
	(0..attempts)
		.into_iter()
		.map(|attempt| make_ticket_with_prover(attempt, pair, &prover))
		.collect()
}

pub fn make_ticket_body(attempt_idx: u32, pair: &AuthorityPair) -> (TicketId, TicketBody) {
	// Values are referring to the next epoch
	let epoch = Sassafras::epoch_index() + 1;
	let randomness = Sassafras::next_randomness();

	let ticket_id_input = vrf::ticket_id_input(&randomness, attempt_idx, epoch);
	let ticket_id_pre_output = pair.as_inner_ref().vrf_pre_output(&ticket_id_input);

	let id = vrf::make_ticket_id(&ticket_id_input, &ticket_id_pre_output);

	// Make a dummy ephemeral public that hopefully is unique within one test instance.
	// In the tests, the values within the erased public are just used to compare
	// ticket bodies, so it is not important to be a valid key.
	let mut raw: [u8; 32] = [0; 32];
	raw[..16].copy_from_slice(&pair.public().as_slice()[0..16]);
	raw[16..].copy_from_slice(&id.to_le_bytes());
	let erased_public = EphemeralPublic::unchecked_from(raw);
	let revealed_public = erased_public;

	let body = TicketBody { attempt_idx, erased_public, revealed_public };

	(id, body)
}

pub fn make_dummy_ticket_body(attempt_idx: u32) -> (TicketId, TicketBody) {
	let hash = sp_crypto_hashing::blake2_256(&attempt_idx.to_le_bytes());

	let erased_public = EphemeralPublic::unchecked_from(hash);
	let revealed_public = erased_public;

	let body = TicketBody { attempt_idx, erased_public, revealed_public };

	let mut bytes = [0u8; 16];
	bytes.copy_from_slice(&hash[..16]);
	let id = TicketId::from_le_bytes(bytes);

	(id, body)
}

pub fn make_ticket_bodies(
	number: u32,
	pair: Option<&AuthorityPair>,
) -> Vec<(TicketId, TicketBody)> {
	(0..number)
		.into_iter()
		.map(|i| match pair {
			Some(pair) => make_ticket_body(i, pair),
			None => make_dummy_ticket_body(i),
		})
		.collect()
}

/// Persist the given tickets in the unsorted segments buffer.
///
/// This function skips all the checks performed by the `submit_tickets` extrinsic and
/// directly appends the tickets to the `UnsortedSegments` structure.
pub fn persist_next_epoch_tickets_as_segments(tickets: &[(TicketId, TicketBody)]) {
	let mut ids = Vec::with_capacity(tickets.len());
	tickets.iter().for_each(|(id, body)| {
		TicketsData::<Test>::set(id, Some(body.clone()));
		ids.push(*id);
	});
	let max_chunk_size = Sassafras::epoch_length() as usize;
	ids.chunks(max_chunk_size).for_each(|chunk| {
		Sassafras::append_tickets(BoundedVec::truncate_from(chunk.to_vec()));
	})
}

/// Calls the [`persist_next_epoch_tickets_as_segments`] and then proceeds to the
/// sorting of the candidates.
///
/// Only "winning" tickets are left.
pub fn persist_next_epoch_tickets(tickets: &[(TicketId, TicketBody)]) {
	persist_next_epoch_tickets_as_segments(tickets);
	// Force sorting of next epoch tickets (enactment) by explicitly querying the first of them.
	let next_epoch = Sassafras::next_epoch();
	assert_eq!(TicketsMeta::<Test>::get().unsorted_tickets_count, tickets.len() as u32);
	Sassafras::slot_ticket(next_epoch.start).unwrap();
	assert_eq!(TicketsMeta::<Test>::get().unsorted_tickets_count, 0);
}

fn slot_claim_vrf_signature(slot: Slot, pair: &AuthorityPair) -> VrfSignature {
	let mut epoch = Sassafras::epoch_index();
	let mut randomness = Sassafras::randomness();

	// Check if epoch is going to change on initialization.
	let epoch_start = Sassafras::current_epoch_start();
	let epoch_length = EPOCH_LENGTH.into();
	if epoch_start != 0_u64 && slot >= epoch_start + epoch_length {
		epoch += slot.saturating_sub(epoch_start).saturating_div(epoch_length);
		randomness = crate::NextRandomness::<Test>::get();
	}

	let data = vrf::slot_claim_sign_data(&randomness, slot, epoch);
	pair.as_ref().vrf_sign(&data)
}

/// Construct a `PreDigest` instance for the given parameters.
pub fn make_slot_claim(
	authority_idx: AuthorityIndex,
	slot: Slot,
	pair: &AuthorityPair,
) -> SlotClaim {
	let vrf_signature = slot_claim_vrf_signature(slot, pair);
	SlotClaim { authority_idx, slot, vrf_signature, ticket_claim: None }
}

/// Construct a `Digest` with a `SlotClaim` item.
pub fn make_digest(authority_idx: AuthorityIndex, slot: Slot, pair: &AuthorityPair) -> Digest {
	let claim = make_slot_claim(authority_idx, slot, pair);
	Digest { logs: vec![DigestItem::from(&claim)] }
}

pub fn initialize_block(
	number: u64,
	slot: Slot,
	parent_hash: H256,
	pair: &AuthorityPair,
) -> Digest {
	let digest = make_digest(0, slot, pair);
	System::reset_events();
	System::initialize(&number, &parent_hash, &digest);
	Sassafras::on_initialize(number);
	digest
}

pub fn finalize_block(number: u64) -> Header {
	Sassafras::on_finalize(number);
	System::finalize()
}

/// Progress the pallet state up to the given block `number` and `slot`.
pub fn go_to_block(number: u64, slot: Slot, pair: &AuthorityPair) -> Digest {
	Sassafras::on_finalize(System::block_number());
	let parent_hash = System::finalize().hash();

	let digest = make_digest(0, slot, pair);

	System::reset_events();
	System::initialize(&number, &parent_hash, &digest);
	Sassafras::on_initialize(number);

	digest
}

/// Progress the pallet state up to the given block `number`.
/// Slots will grow linearly accordingly to blocks.
pub fn progress_to_block(number: u64, pair: &AuthorityPair) -> Option<Digest> {
	let mut slot = Sassafras::current_slot() + 1;
	let mut digest = None;
	for i in System::block_number() + 1..=number {
		let dig = go_to_block(i, slot, pair);
		digest = Some(dig);
		slot = slot + 1;
	}
	digest
}
