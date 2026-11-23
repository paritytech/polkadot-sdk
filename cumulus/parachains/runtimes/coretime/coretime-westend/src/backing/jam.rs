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

//! The JAM backing: the Parachain Service on JAM validates the chain instead of a relay chain, and
//! its cores are the ones the broker sells.
//!
//! `JamSystem` takes the place of `ParachainSystem`, and `AuraJam` the one of `Aura` and `AuraExt`.
//! A JAM block carries no parachain inherent: its context is its `JamParent` digest, the JAM state
//! proof carried with it, and the timestamp inherent. Its Aura authority set is the one JAM
//! authorizes at its anchor slot. There is no XCM transport on JAM: an XCM to the relay chain
//! finds no route and an XCMP message to a sibling no channel.

use alloc::vec::Vec;
use frame_support::traits::{ConstU32, ConstU64, Get};
use pallet_broker::CoreIndex;
use parachains_common::AuraId;
use sp_runtime::impl_opaque_keys;
use testnet_parachains_constants::westend::consensus::{
	BLOCK_PROCESSING_VELOCITY, SLOT_DURATION, UNINCLUDED_SEGMENT_CAPACITY,
};

use crate::{AuraJam, JamSystem, Runtime, PERIOD};

impl_opaque_keys! {
	pub struct SessionKeys {
		pub aura: AuraJam,
	}
}

/// Checks that the block's Aura slot is the one of its JAM anchor slot and bounds the unincluded
/// segment.
pub(crate) type ConsensusHook = pallet_aura_jam::JamFixedVelocityConsensusHook<
	Runtime,
	BLOCK_PROCESSING_VELOCITY,
	UNINCLUDED_SEGMENT_CAPACITY,
>;

/// The runtime upgrades by hash, through `JamSystem::schedule_code_upgrade_hash`; `set_code` with
/// code bytes fails.
pub(crate) type OnSetCode = cumulus_pallet_jam_system::JamSetCode<Runtime>;

impl cumulus_pallet_jam_system::Config for Runtime {
	type SelfParaId = parachain_info::Pallet<Runtime>;
	type ConsensusHook = ConsensusHook;
	type OnCodeApplied = ();
}

/// The clock the runtime reads as "the relay block number": there is no relay chain, so the JAM
/// lookup-anchor slot of the block's own `JamParent` digest serves instead.
pub type SlotNumber = cumulus_pallet_jam_system::JamSlotNumber<Runtime>;

impl pallet_aura_jam::Config for Runtime {
	type AuthorityId = AuraId;
	type MaxAuthorities = ConstU32<100_000>;
	type SlotDuration = ConstU64<SLOT_DURATION>;
	/// One session per `PERIOD` JAM slots: a JAM slot is 6 s, this chain's block time, so a
	/// session lasts as long as on the relay backing.
	type RotationPeriod = ConstU32<PERIOD>;
}

/// A session ends once the block's anchor slot reaches the next authorizer switch.
pub(crate) type SessionRotation = pallet_aura_jam::JamSlotSessions<Runtime>;

/// `AuraJam` checks that the timestamp is in the block's slot.
pub(crate) type OnTimestampSet = AuraJam;

/// The block author is the authority of the block's slot in the set JAM authorizes at its anchor
/// slot.
pub(crate) type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Runtime, AuraJam>;

/// No route to a relay chain.
pub(crate) type ParentXcmRouter = ();

/// No XCMP channels: every message to a sibling fails with `NoChannel`.
pub(crate) type XcmpChannelInfo = ();

/// The pallets this backing composes have no migrations.
pub(crate) type Migrations = ();

/// The cores are the Parachain Service's: the broker's core assignments go to it as `AssignCore`
/// upward messages.
pub(crate) type CoretimeAllocator = pallet_jam_coretime::JamCoretime<Runtime, SlotNumber>;

impl pallet_jam_coretime::Config for Runtime {
	type RejectedCores = JamRejectedCores;
}

/// The cores whose assignment the Parachain Service rejected, from this chain's `parachain_log`.
pub struct JamRejectedCores;
impl Get<Vec<CoreIndex>> for JamRejectedCores {
	fn get() -> Vec<CoreIndex> {
		JamSystem::rejected_cores()
	}
}

/// The set JAM authorizes now.
pub(crate) fn authorities() -> Vec<AuraId> {
	pallet_aura_jam::Authorities::<Runtime>::get().into_inner()
}
