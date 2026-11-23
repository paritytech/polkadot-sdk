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

//! The relay backing: `ParachainSystem` validates the chain against a relay chain, and `Aura`
//! with `AuraExt` authors it.

use alloc::vec::Vec;
use sp_core::{ConstBool, ConstU32, ConstU64};

use crate::{
	features::*, Aura, AuraId, NoVerification, Offset, Period, Runtime, RuntimeEvent, TestPallet,
};

const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32 = 6000;

/// Checks the block's Aura slot against the relay slot and bounds the unincluded segment, with the
/// flavor's velocity and capacity.
pub type ConsensusHook = cumulus_pallet_aura_ext::FixedVelocityConsensusHook<
	Runtime,
	RELAY_CHAIN_SLOT_DURATION_MILLIS,
	{ block_processing_velocity() },
	{ unincluded_segment_capacity() },
>;

/// `set_code` hands the code to `ParachainSystem`, which schedules the upgrade.
pub type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Runtime>;

impl cumulus_pallet_parachain_system::Config for Runtime {
	type WeightInfo = ();
	type SelfParaId = parachain_info::Pallet<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type OnSystemEvent = ();
	type OutboundXcmpMessageSource = TestPallet;
	// Ignore all DMP messages by enqueueing them into `()`:
	type DmpQueue = frame_support::traits::EnqueueWithOrigin<(), sp_core::ConstU8<0>>;
	type ReservedDmpWeight = ();
	type XcmpMessageHandler = ();
	type ReservedXcmpWeight = ();
	type CheckAssociatedRelayNumber =
		cumulus_pallet_parachain_system::RelayNumberMonotonicallyIncreases;
	type ConsensusHook = ConsensusHook;
	type RelayParentOffset = ConstU32<{ relay_parent_offset() }>;
	type SchedulingSignatureVerifier = NoVerification;
}

impl cumulus_pallet_aura_ext::Config for Runtime {}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<32>;
	type AllowMultipleBlocksPerSlot = ConstBool<{ !cfg!(feature = "sync-backing") }>;
	type SlotDuration = ConstU64<{ slot_duration() }>;
}

/// `Aura` checks that the timestamp is in the block's slot.
pub type OnTimestampSet = Aura;

/// A session every `Period` blocks (only the `with-authority-discovery` flavor composes
/// `pallet_session`).
pub type SessionRotation = pallet_session::PeriodicSessions<Period, Offset>;

/// The Aura authorities.
pub fn authorities() -> Vec<AuraId> {
	pallet_aura::Authorities::<Runtime>::get().into_inner()
}
