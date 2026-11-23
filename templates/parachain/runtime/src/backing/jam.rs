// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

//! The JAM backing: the Parachain Service on JAM validates the chain instead of a relay chain.
//!
//! `JamSystem` takes the place of `ParachainSystem`, and `AuraJam` the one of `Aura` and `AuraExt`.
//! A JAM block carries no parachain inherent: its context is its `JamParent` digest, the JAM state
//! proof carried with it, and the timestamp inherent. Its Aura authority set is the one JAM
//! authorizes at its anchor slot. There is no XCM transport on JAM: an XCM to the relay chain
//! finds no route and an XCMP message to a sibling no channel.

use alloc::vec::Vec;

use polkadot_sdk::{staging_parachain_info as parachain_info, *};

use cumulus_pallet_parachain_system::block_weight::BlockWeightMode;
use frame_support::traits::{ConstU32, ConstU64};
use sp_runtime::impl_opaque_keys;

use crate::{configs::Period, AuraId, AuraJam, JamSystem, Runtime, SLOT_DURATION};

impl_opaque_keys! {
	pub struct SessionKeys {
		pub aura: AuraJam,
	}
}

/// Maximum encoded block length, in bytes.
///
/// A block travels in a work package, whose input JAM bounds by `max_input` (13,791,360 bytes);
/// the rest of the bound covers the work-package encoding and the proof.
pub(crate) const MAX_BLOCK_LENGTH: u32 = 12 * 1024 * 1024;

/// How many parachain blocks may build on one JAM anchor slot.
///
/// Two, because a JAM collator turn places one work package on each core that holds the para's
/// authorizer, and the JAM tiny preset has two cores: the runtime has to admit two blocks per slot
/// for a turn to reach both.
pub(crate) const BLOCK_PROCESSING_VELOCITY: u32 = 2;

/// Maximum number of blocks not yet included by the Parachain Service: three slots' worth of
/// velocity.
pub(crate) const UNINCLUDED_SEGMENT_CAPACITY: u32 = 3 * BLOCK_PROCESSING_VELOCITY;

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
	type MaxAuthorities = ConstU32<100_000>;
	type SlotDuration = ConstU64<SLOT_DURATION>;
	/// One session per `Period` JAM slots; a JAM slot is 6 s, the same as this template's block.
	type RotationPeriod = Period;
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

/// The set JAM authorizes now.
pub(crate) fn authorities() -> Vec<AuraId> {
	pallet_aura_jam::Authorities::<Runtime>::get().into_inner()
}

/// The set JAM authorizes for a package anchored at `anchor_slot`.
pub(crate) fn authorities_at(anchor_slot: u32) -> Vec<AuraId> {
	AuraJam::authorities_at(anchor_slot)
}

/// The scheduled sets with the JAM slot each takes over at.
pub(crate) fn scheduled_changes() -> Vec<(u32, Vec<AuraId>)> {
	AuraJam::scheduled_changes()
}
