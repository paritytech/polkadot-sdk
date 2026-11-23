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

//! The relay-chain backing: `ParachainSystem`, with `Aura` and `AuraExt` for consensus.

use alloc::vec::Vec;

use polkadot_sdk::{staging_parachain_info as parachain_info, *};

use cumulus_pallet_parachain_system::RelayNumberMonotonicallyIncreases;
use cumulus_primitives_core::AggregateMessageOrigin;
use frame_support::{
	parameter_types,
	traits::{ConstBool, ConstU32, ConstU64},
	weights::Weight,
};
use polkadot_runtime_common::xcm_sender::ExponentialPrice;
use sp_runtime::impl_opaque_keys;

use crate::{
	configs::{FeeAssetId, Offset, Period, ToParentBaseDeliveryFee, TransactionByteFee},
	Aura, AuraId, MessageQueue, ParachainSystem, PolkadotXcm, Runtime, RuntimeEvent, XcmpQueue,
	MAXIMUM_BLOCK_WEIGHT, SLOT_DURATION,
};

impl_opaque_keys! {
	pub struct SessionKeys {
		pub aura: Aura,
	}
}

/// Maximum encoded block length, in bytes.
pub(crate) const MAX_BLOCK_LENGTH: u32 = 5 * 1024 * 1024;

#[docify::export]
mod async_backing_params {
	/// Maximum number of blocks simultaneously accepted by the Runtime, not yet included
	/// into the relay chain.
	pub(crate) const UNINCLUDED_SEGMENT_CAPACITY: u32 = 3;
	/// How many parachain blocks are processed by the relay chain per parent. Limits the
	/// number of blocks authored per slot.
	pub(crate) const BLOCK_PROCESSING_VELOCITY: u32 = 1;
	/// Relay chain slot duration, in milliseconds.
	pub(crate) const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32 = 6000;
}
pub(crate) use async_backing_params::*;

#[docify::export]
/// Aura consensus hook
pub(crate) type ConsensusHook = cumulus_pallet_aura_ext::FixedVelocityConsensusHook<
	Runtime,
	RELAY_CHAIN_SLOT_DURATION_MILLIS,
	BLOCK_PROCESSING_VELOCITY,
	UNINCLUDED_SEGMENT_CAPACITY,
>;

/// The action to take on a runtime upgrade.
pub(crate) type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Runtime>;

parameter_types! {
	pub const ReservedXcmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
	pub const ReservedDmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
	pub const RelayOrigin: AggregateMessageOrigin = AggregateMessageOrigin::Parent;
}

impl cumulus_pallet_parachain_system::Config for Runtime {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type OnSystemEvent = ();
	type SelfParaId = parachain_info::Pallet<Runtime>;
	type OutboundXcmpMessageSource = XcmpQueue;
	type DmpQueue = frame_support::traits::EnqueueWithOrigin<MessageQueue, RelayOrigin>;
	type ReservedDmpWeight = ReservedDmpWeight;
	type XcmpMessageHandler = XcmpQueue;
	type ReservedXcmpWeight = ReservedXcmpWeight;
	type CheckAssociatedRelayNumber = RelayNumberMonotonicallyIncreases;
	type ConsensusHook = ConsensusHook;
	type RelayParentOffset = ConstU32<0>;
	type SchedulingSignatureVerifier = ();
}

impl cumulus_pallet_aura_ext::Config for Runtime {}

#[docify::export(aura_config)]
impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<100_000>;
	type AllowMultipleBlocksPerSlot = ConstBool<true>;
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

/// A session lasts `Period` blocks.
pub(crate) type SessionRotation = pallet_session::PeriodicSessions<Period, Offset>;

/// Aura checks that the timestamp is in the block's slot.
pub(crate) type OnTimestampSet = Aura;

/// The block author is the Aura authority of the block's slot.
pub(crate) type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Runtime, Aura>;

/// The price for delivering XCM messages to relay chain.
pub type PriceForParentDelivery =
	ExponentialPrice<FeeAssetId, ToParentBaseDeliveryFee, TransactionByteFee, ParachainSystem>;

/// Sends XCM to the relay chain over UMP.
pub(crate) type ParentXcmRouter =
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, PriceForParentDelivery>;

/// The XCMP channels to sibling parachains are the ones the relay chain opened.
pub(crate) type XcmpChannelInfo = ParachainSystem;

/// The Aura authorities.
pub(crate) fn authorities() -> Vec<AuraId> {
	pallet_aura::Authorities::<Runtime>::get().into_inner()
}

/// The Aura authorities: off JAM one set serves every slot.
pub(crate) fn authorities_at(_anchor_slot: u32) -> Vec<AuraId> {
	authorities()
}

/// No set is scheduled for a slot: off JAM a set takes over at the session change.
pub(crate) fn scheduled_changes() -> Vec<(u32, Vec<AuraId>)> {
	Vec::new()
}
