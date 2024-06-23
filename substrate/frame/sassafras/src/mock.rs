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
	traits::{ConstU32, ConstU8, OnFinalize, OnInitialize},
};
use sp_consensus_sassafras::{
	digests::SlotClaim,
	vrf::{RingProver, VrfSignature},
	AuthorityIndex, AuthorityPair, Slot, TicketBody, TicketEnvelope, TicketId,
};
use sp_core::{
	crypto::{ByteArray, Pair, VrfSecret, Wraps},
	H256, U256,
};
use sp_runtime::{
	testing::{Digest, DigestItem, Header, TestXt},
	BuildStorage,
};

const LOG_TARGET: &str = "sassafras::tests";

const EPOCH_LENGTH: u32 = 10;
const MAX_AUTHORITIES: u32 = 100;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
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
	type RedundancyFactor = ConstU8<32>;
	type AttemptsNumber = ConstU8<2>;
	type EpochChangeTrigger = EpochChangeInternalTrigger;
	type WeightInfo = ();
}

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Sassafras: pallet_sassafras,
	}
);

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

fn slot_claim_vrf_signature(slot: Slot, pair: &AuthorityPair) -> VrfSignature {
	let randomness = Sassafras::randomness_accumulator();
	let data = vrf::block_randomness_sign_data(&randomness, slot);
	pair.as_ref().vrf_sign(&data)
}

/// Construct a `PreDigest` instance for the given parameters.
pub fn make_slot_claim(
	authority_idx: AuthorityIndex,
	slot: Slot,
	pair: &AuthorityPair,
) -> SlotClaim {
	let vrf_signature = slot_claim_vrf_signature(slot, pair);
	SlotClaim { authority_idx, slot, vrf_signature }
}

/// Construct a `Digest` with a `SlotClaim` item.
pub fn make_digest(authority_idx: AuthorityIndex, slot: Slot, pair: &AuthorityPair) -> Digest {
	let claim = make_slot_claim(authority_idx, slot, pair);
	Digest { logs: vec![DigestItem::from(&claim)] }
}

/// Make a ticket which is claimable during the next epoch.
pub fn make_ticket_body(attempt: u8, pair: &AuthorityPair) -> TicketBody {
	let randomness = Sassafras::next_randomness();

	let ticket_id_input = vrf::ticket_id_input(&randomness, attempt);
	let ticket_id_pre_output = pair.as_inner_ref().vrf_pre_output(&ticket_id_input);

	let id = vrf::make_ticket_id(&ticket_id_input, &ticket_id_pre_output);

	// Make dummy extra data.
	let mut extra = [pair.public().as_slice(), &id.0[..]].concat();
	let extra = BoundedVec::truncate_from(extra);

	TicketBody { id, attempt, extra }
}

pub fn make_dummy_ticket_body(attempt: u8) -> TicketBody {
	let hash = sp_crypto_hashing::blake2_256(&[attempt]);
	let id = TicketId(hash);
	let hash = sp_crypto_hashing::blake2_256(&hash);
	let extra = BoundedVec::truncate_from(hash.to_vec());
	TicketBody { id, attempt, extra }
}

pub fn make_ticket_bodies(attempts: u8, pair: Option<&AuthorityPair>) -> Vec<TicketBody> {
	(0..attempts)
		.into_iter()
		.map(|i| match pair {
			Some(pair) => make_ticket_body(i, pair),
			None => make_dummy_ticket_body(i),
		})
		.collect()
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

fn make_ticket_with_prover(
	attempt: u8,
	pair: &AuthorityPair,
	prover: &RingProver,
) -> (TicketId, TicketEnvelope) {
	log::debug!("attempt: {}", attempt);

	// Values are referring to the next epoch
	let randomness = Sassafras::next_randomness();

	let ticket_id_input = vrf::ticket_id_input(&randomness, attempt);
	let sign_data = vrf::ticket_id_sign_data(ticket_id_input.clone(), &[]);
	let signature = pair.as_ref().ring_vrf_sign(&sign_data, &prover);
	let pre_output = &signature.pre_outputs[0];

	let ticket_id = vrf::make_ticket_id(&ticket_id_input, pre_output);
	let envelope = TicketEnvelope { attempt, extra: Default::default(), signature };

	(ticket_id, envelope)
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
pub fn make_tickets(attempts: u8, pair: &AuthorityPair) -> Vec<(TicketId, TicketEnvelope)> {
	let prover = make_prover(pair);
	(0..attempts)
		.into_iter()
		.map(|attempt| make_ticket_with_prover(attempt, pair, &prover))
		.collect()
}
