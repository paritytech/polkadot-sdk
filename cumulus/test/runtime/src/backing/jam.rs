// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! The JAM backing: the Parachain Service on JAM validates the chain instead of a relay chain.
//!
//! `JamSystem` takes the place of `ParachainSystem`, and `AuraJam` the one of `Aura` and `AuraExt`.
//! A JAM block carries no parachain inherent: its context is its `JamParent` digest, the JAM state
//! proof carried with it, and the timestamp inherent. Its Aura authority set is the one JAM
//! authorizes at its anchor slot. The dynamic block weight logic of
//! `cumulus-pallet-parachain-system` runs on `JamSystem`'s storage.

use alloc::vec::Vec;
use cumulus_pallet_parachain_system::block_weight::BlockWeightMode;
use sp_core::{ConstU32, ConstU64};

use crate::{features::*, AuraId, AuraJam, JamSystem, Period, Runtime};

/// Checks that the block's Aura slot is the one of its JAM anchor slot and bounds the unincluded
/// segment, with the flavor's velocity and capacity.
pub type ConsensusHook = pallet_aura_jam::JamFixedVelocityConsensusHook<
	Runtime,
	{ block_processing_velocity() },
	{ unincluded_segment_capacity() },
>;

/// The runtime upgrades by hash, through `JamSystem::schedule_code_upgrade_hash`; `set_code` with
/// code bytes fails.
pub type OnSetCode = cumulus_pallet_jam_system::JamSetCode<Runtime>;

impl cumulus_pallet_jam_system::Config for Runtime {
	type SelfParaId = parachain_info::Pallet<Runtime>;
	type ConsensusHook = ConsensusHook;
	type OnCodeApplied = ();
}

/// The dynamic block weight logic of `cumulus-pallet-parachain-system` on `JamSystem`'s storage,
/// where the relay backing gets it from `ParachainSystem`.
impl cumulus_pallet_parachain_system::block_weight::Config for Runtime {
	type WeightInfo = ();

	fn mode() -> Option<BlockWeightMode<Self>> {
		JamSystem::block_weight_mode()
	}

	fn set_mode(mode: Option<BlockWeightMode<Self>>) {
		JamSystem::set_block_weight_mode(mode)
	}

	fn previous_core_count() -> Option<u16> {
		JamSystem::previous_core_count()
	}
}

impl pallet_aura_jam::Config for Runtime {
	type AuthorityId = AuraId;
	type MaxAuthorities = ConstU32<32>;
	type SlotDuration = ConstU64<{ slot_duration() }>;
	/// One session per `Period` JAM slots.
	type RotationPeriod = Period;
}

/// `AuraJam` checks that the timestamp is in the block's slot.
pub type OnTimestampSet = AuraJam;

/// A session ends once the block's anchor slot reaches the next authorizer switch (only the
/// `with-authority-discovery` flavor composes `pallet_session`).
pub type SessionRotation = pallet_aura_jam::JamSlotSessions<Runtime>;

/// The set JAM authorizes now.
pub fn authorities() -> Vec<AuraId> {
	pallet_aura_jam::Authorities::<Runtime>::get().into_inner()
}
