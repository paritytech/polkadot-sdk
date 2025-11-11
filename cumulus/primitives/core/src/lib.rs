// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Cumulus related core primitive types and traits.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Compact, Decode, DecodeAll, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::time::Duration;
use polkadot_parachain_primitives::primitives::HeadData;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

/// The ref time per core in seconds.
///
/// This is the execution time each PoV gets on a core on the relay chain.
pub const REF_TIME_PER_CORE_IN_SECS: u64 = 2;

pub mod parachain_block_data;

pub use parachain_block_data::ParachainBlockData;
pub use polkadot_core_primitives::InboundDownwardMessage;
pub use polkadot_parachain_primitives::primitives::{
	DmpMessageHandler, Id as ParaId, IsSystem, UpwardMessage, ValidationParams, XcmpMessageFormat,
	XcmpMessageHandler,
};
pub use polkadot_primitives::{
	AbridgedHostConfiguration, AbridgedHrmpChannel, ClaimQueueOffset, CoreSelector,
	PersistedValidationData,
};
pub use sp_runtime::{
	generic::{Digest, DigestItem},
	traits::Block as BlockT,
	ConsensusEngineId,
};
pub use xcm::latest::prelude::*;

/// A module that re-exports relevant relay chain definitions.
pub mod relay_chain {
	pub use polkadot_core_primitives::*;
	pub use polkadot_primitives::*;
}

/// An inbound HRMP message.
pub type InboundHrmpMessage = polkadot_primitives::InboundHrmpMessage<relay_chain::BlockNumber>;

/// And outbound HRMP message
pub type OutboundHrmpMessage = polkadot_primitives::OutboundHrmpMessage<ParaId>;

/// Error description of a message send failure.
#[derive(Eq, PartialEq, Copy, Clone, RuntimeDebug, Encode, Decode)]
pub enum MessageSendError {
	/// The dispatch queue is full.
	QueueFull,
	/// There does not exist a channel for sending the message.
	NoChannel,
	/// The message is too big to ever fit in a channel.
	TooBig,
	/// Some other error.
	Other,
	/// There are too many channels open at once.
	TooManyChannels,
}

impl From<MessageSendError> for &'static str {
	fn from(e: MessageSendError) -> Self {
		use MessageSendError::*;
		match e {
			QueueFull => "QueueFull",
			NoChannel => "NoChannel",
			TooBig => "TooBig",
			Other => "Other",
			TooManyChannels => "TooManyChannels",
		}
	}
}

/// The origin of an inbound message.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, Clone, Eq, PartialEq, TypeInfo, Debug,
)]
pub enum AggregateMessageOrigin {
	/// The message came from the para-chain itself.
	Here,
	/// The message came from the relay-chain.
	///
	/// This is used by the DMP queue.
	Parent,
	/// The message came from a sibling para-chain.
	///
	/// This is used by the HRMP queue.
	Sibling(ParaId),
}

impl From<AggregateMessageOrigin> for Location {
	fn from(origin: AggregateMessageOrigin) -> Self {
		match origin {
			AggregateMessageOrigin::Here => Location::here(),
			AggregateMessageOrigin::Parent => Location::parent(),
			AggregateMessageOrigin::Sibling(id) => Location::new(1, Junction::Parachain(id.into())),
		}
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl From<u32> for AggregateMessageOrigin {
	fn from(x: u32) -> Self {
		match x {
			0 => Self::Here,
			1 => Self::Parent,
			p => Self::Sibling(ParaId::from(p)),
		}
	}
}

/// Information about an XCMP channel.
pub struct ChannelInfo {
	/// The maximum number of messages that can be pending in the channel at once.
	pub max_capacity: u32,
	/// The maximum total size of the messages that can be pending in the channel at once.
	pub max_total_size: u32,
	/// The maximum message size that could be put into the channel.
	pub max_message_size: u32,
	/// The current number of messages pending in the channel.
	/// Invariant: should be less or equal to `max_capacity`.s`.
	pub msg_count: u32,
	/// The total size in bytes of all message payloads in the channel.
	/// Invariant: should be less or equal to `max_total_size`.
	pub total_size: u32,
}

pub trait GetChannelInfo {
	fn get_channel_status(id: ParaId) -> ChannelStatus;
	fn get_channel_info(id: ParaId) -> Option<ChannelInfo>;
}

/// List all open outgoing channels.
pub trait ListChannelInfos {
	fn outgoing_channels() -> Vec<ParaId>;
}

/// Something that should be called when sending an upward message.
pub trait UpwardMessageSender {
	/// Send the given UMP message; return the expected number of blocks before the message will
	/// be dispatched or an error if the message cannot be sent.
	/// return the hash of the message sent
	fn send_upward_message(message: UpwardMessage) -> Result<(u32, XcmHash), MessageSendError>;

	/// Pre-check the given UMP message.
	fn can_send_upward_message(message: &UpwardMessage) -> Result<(), MessageSendError>;

	/// Ensure `[Self::send_upward_message]` is successful when called in benchmarks/tests.
	#[cfg(any(feature = "std", feature = "runtime-benchmarks", test))]
	fn ensure_successful_delivery() {}
}

impl UpwardMessageSender for () {
	fn send_upward_message(_message: UpwardMessage) -> Result<(u32, XcmHash), MessageSendError> {
		Err(MessageSendError::NoChannel)
	}

	fn can_send_upward_message(_message: &UpwardMessage) -> Result<(), MessageSendError> {
		Err(MessageSendError::Other)
	}
}

/// The status of a channel.
pub enum ChannelStatus {
	/// Channel doesn't exist/has been closed.
	Closed,
	/// Channel is completely full right now.
	Full,
	/// Channel is ready for sending; the two parameters are the maximum size a valid message may
	/// have right now, and the maximum size a message may ever have (this will generally have been
	/// available during message construction, but it's possible the channel parameters changed in
	/// the meantime).
	Ready(usize, usize),
}

/// A means of figuring out what outbound XCMP messages should be being sent.
pub trait XcmpMessageSource {
	/// Take a single XCMP message from the queue for the given `dest`, if one exists.
	fn take_outbound_messages(maximum_channels: usize) -> Vec<(ParaId, Vec<u8>)>;
}

impl XcmpMessageSource for () {
	fn take_outbound_messages(_maximum_channels: usize) -> Vec<(ParaId, Vec<u8>)> {
		Vec::new()
	}
}

/// The "quality of service" considerations for message sending.
#[derive(Eq, PartialEq, Clone, Copy, Encode, Decode, RuntimeDebug)]
pub enum ServiceQuality {
	/// Ensure that this message is dispatched in the same relative order as any other messages
	/// that were also sent with `Ordered`. This only guarantees message ordering on the dispatch
	/// side, and not necessarily on the execution side.
	Ordered,
	/// Ensure that the message is dispatched as soon as possible, which could result in it being
	/// dispatched before other messages which are larger and/or rely on relative ordering.
	Fast,
}

/// A consensus engine ID indicating that this is a Cumulus Parachain.
pub const CUMULUS_CONSENSUS_ID: ConsensusEngineId = *b"CMLS";

/// Information about the core on the relay chain this block will be validated on.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct CoreInfo {
	/// The selector that determines the actual core at `claim_queue_offset`.
	pub selector: CoreSelector,
	/// The claim queue offset that determines how far "into the future" the core is selected.
	pub claim_queue_offset: ClaimQueueOffset,
	/// The number of cores assigned to the parachain at `claim_queue_offset`.
	pub number_of_cores: Compact<u16>,
}

/// Return value of [`CumulusDigestItem::core_info_exists_at_max_once`]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreInfoExistsAtMaxOnce {
	/// Exists exactly once.
	Once(CoreInfo),
	/// Not found.
	NotFound,
	/// Found more than once.
	MoreThanOnce,
}

/// Identifier for a relay chain block used by [`CumulusDigestItem`].
#[derive(Clone, Debug, PartialEq, Hash, Eq)]
pub enum RelayBlockIdentifier {
	/// The block is identified using its block hash.
	ByHash(relay_chain::Hash),
	/// The block is identified using its storage root and block number.
	ByStorageRoot { storage_root: relay_chain::Hash, block_number: relay_chain::BlockNumber },
}

/// Consensus header digests for Cumulus parachains.
#[derive(Clone, Debug, Decode, Encode, PartialEq)]
pub enum CumulusDigestItem {
	/// A digest item indicating the relay-parent a parachain block was built against.
	#[codec(index = 0)]
	RelayParent(relay_chain::Hash),
	/// A digest item providing information about the core selected on the relay chain for this
	/// block.
	#[codec(index = 1)]
	CoreInfo(CoreInfo),
}

impl CumulusDigestItem {
	/// Encode this as a Substrate [`DigestItem`].
	pub fn to_digest_item(&self) -> DigestItem {
		match self {
			Self::RelayParent(_) => DigestItem::Consensus(CUMULUS_CONSENSUS_ID, self.encode()),
			Self::CoreInfo(_) => DigestItem::PreRuntime(CUMULUS_CONSENSUS_ID, self.encode()),
		}
	}

	/// Find [`CumulusDigestItem::CoreInfo`] in the given `digest`.
	///
	/// If there are multiple valid digests, this returns the value of the first one.
	pub fn find_core_info(digest: &Digest) -> Option<CoreInfo> {
		digest.convert_first(|d| match d {
			DigestItem::PreRuntime(id, val) if id == &CUMULUS_CONSENSUS_ID => {
				let Ok(CumulusDigestItem::CoreInfo(core_info)) =
					CumulusDigestItem::decode_all(&mut &val[..])
				else {
					return None
				};

				Some(core_info)
			},
			_ => None,
		})
	}

	/// Returns the found [`CoreInfo`] and iff [`Self::CoreInfo`] exists at max once in the given
	/// `digest`.
	pub fn core_info_exists_at_max_once(digest: &Digest) -> CoreInfoExistsAtMaxOnce {
		let mut core_info = None;
		if digest
			.logs()
			.iter()
			.filter(|l| match l {
				DigestItem::PreRuntime(CUMULUS_CONSENSUS_ID, d) => {
					if let Ok(Self::CoreInfo(ci)) = Self::decode_all(&mut &d[..]) {
						core_info = Some(ci);
						true
					} else {
						false
					}
				},
				_ => false,
			})
			.count() <= 1
		{
			core_info
				.map(CoreInfoExistsAtMaxOnce::Once)
				.unwrap_or(CoreInfoExistsAtMaxOnce::NotFound)
		} else {
			CoreInfoExistsAtMaxOnce::MoreThanOnce
		}
	}

	/// Returns the [`RelayBlockIdentifier`] from the given `digest`.
	///
	/// The identifier corresponds to the relay parent used to build the parachain block.
	pub fn find_relay_block_identifier(digest: &Digest) -> Option<RelayBlockIdentifier> {
		digest.convert_first(|d| match d {
			DigestItem::Consensus(id, val) if id == &CUMULUS_CONSENSUS_ID => {
				let Ok(CumulusDigestItem::RelayParent(hash)) =
					CumulusDigestItem::decode_all(&mut &val[..])
				else {
					return None
				};

				Some(RelayBlockIdentifier::ByHash(hash))
			},
			DigestItem::Consensus(id, val) if id == &rpsr_digest::RPSR_CONSENSUS_ID => {
				let Ok((storage_root, block_number)) =
					rpsr_digest::RpsrType::decode_all(&mut &val[..])
				else {
					return None
				};

				Some(RelayBlockIdentifier::ByStorageRoot {
					storage_root,
					block_number: block_number.into(),
				})
			},
			_ => None,
		})
	}
}

///
/// If there are multiple valid digests, this returns the value of the first one, although
/// well-behaving runtimes should not produce headers with more than one.
pub fn extract_relay_parent(digest: &Digest) -> Option<relay_chain::Hash> {
	digest.convert_first(|d| match d {
		DigestItem::Consensus(id, val) if id == &CUMULUS_CONSENSUS_ID =>
			match CumulusDigestItem::decode(&mut &val[..]) {
				Ok(CumulusDigestItem::RelayParent(hash)) => Some(hash),
				_ => None,
			},
		_ => None,
	})
}

/// Utilities for handling the relay-parent storage root as a digest item.
///
/// This is not intended to be part of the public API, as it is a workaround for
/// <https://github.com/paritytech/cumulus/issues/303> via
/// <https://github.com/paritytech/polkadot/issues/7191>.
///
/// Runtimes using the parachain-system pallet are expected to produce this digest item,
/// but will stop as soon as they are able to provide the relay-parent hash directly.
///
/// The relay-chain storage root is, in practice, a unique identifier of a block
/// in the absence of equivocations (which are slashable). This assumes that the relay chain
/// uses BABE or SASSAFRAS, because the slot and the author's VRF randomness are both included
/// in the relay-chain storage root in both cases.
///
/// Therefore, the relay-parent storage root is a suitable identifier of unique relay chain
/// blocks in low-value scenarios such as performance optimizations.
#[doc(hidden)]
pub mod rpsr_digest {
	use super::{relay_chain, ConsensusEngineId, DecodeAll, Digest, DigestItem, Encode};
	use codec::Compact;

	/// The type used to store the relay-parent storage root and number.
	pub type RpsrType = (relay_chain::Hash, Compact<relay_chain::BlockNumber>);

	/// A consensus engine ID for relay-parent storage root digests.
	pub const RPSR_CONSENSUS_ID: ConsensusEngineId = *b"RPSR";

	/// Construct a digest item for relay-parent storage roots.
	pub fn relay_parent_storage_root_item(
		storage_root: relay_chain::Hash,
		number: impl Into<Compact<relay_chain::BlockNumber>>,
	) -> DigestItem {
		DigestItem::Consensus(
			RPSR_CONSENSUS_ID,
			RpsrType::from((storage_root, number.into())).encode(),
		)
	}

	/// Extract the relay-parent storage root and number from the provided header digest. Returns
	/// `None` if none were found.
	pub fn extract_relay_parent_storage_root(
		digest: &Digest,
	) -> Option<(relay_chain::Hash, relay_chain::BlockNumber)> {
		digest.convert_first(|d| match d {
			DigestItem::Consensus(id, val) if id == &RPSR_CONSENSUS_ID => {
				let (h, n) = RpsrType::decode_all(&mut &val[..]).ok()?;

				Some((h, n.0))
			},
			_ => None,
		})
	}
}

/// Information about a collation.
///
/// This was used in version 1 of the [`CollectCollationInfo`] runtime api.
#[derive(Clone, Debug, codec::Decode, codec::Encode, PartialEq)]
pub struct CollationInfoV1 {
	/// Messages destined to be interpreted by the Relay chain itself.
	pub upward_messages: Vec<UpwardMessage>,
	/// The horizontal messages sent by the parachain.
	pub horizontal_messages: Vec<OutboundHrmpMessage>,
	/// New validation code.
	pub new_validation_code: Option<relay_chain::ValidationCode>,
	/// The number of messages processed from the DMQ.
	pub processed_downward_messages: u32,
	/// The mark which specifies the block number up to which all inbound HRMP messages are
	/// processed.
	pub hrmp_watermark: relay_chain::BlockNumber,
}

impl CollationInfoV1 {
	/// Convert into the latest version of the [`CollationInfo`] struct.
	pub fn into_latest(self, head_data: HeadData) -> CollationInfo {
		CollationInfo {
			upward_messages: self.upward_messages,
			horizontal_messages: self.horizontal_messages,
			new_validation_code: self.new_validation_code,
			processed_downward_messages: self.processed_downward_messages,
			hrmp_watermark: self.hrmp_watermark,
			head_data,
		}
	}
}

/// Information about a collation.
#[derive(Clone, Debug, codec::Decode, codec::Encode, PartialEq, TypeInfo)]
pub struct CollationInfo {
	/// Messages destined to be interpreted by the Relay chain itself.
	pub upward_messages: Vec<UpwardMessage>,
	/// The horizontal messages sent by the parachain.
	pub horizontal_messages: Vec<OutboundHrmpMessage>,
	/// New validation code.
	pub new_validation_code: Option<relay_chain::ValidationCode>,
	/// The number of messages processed from the DMQ.
	pub processed_downward_messages: u32,
	/// The mark which specifies the block number up to which all inbound HRMP messages are
	/// processed.
	pub hrmp_watermark: relay_chain::BlockNumber,
	/// The head data, aka encoded header, of the block that corresponds to the collation.
	pub head_data: HeadData,
}

/// The schedule for the next relay chain slot.
///
/// Returns the maximum number of parachain blocks to produce and the block time per block to use.
#[derive(Clone, Debug, codec::Decode, codec::Encode, PartialEq, TypeInfo)]
pub struct NextSlotSchedule {
	/// The maximum number of blocks to produce in the relay chain slot.
	///
	/// The node is free to produce less blocks.
	pub number_of_blocks: u32,
	/// The target block time in wall clock time for each block.
	///
	/// The maximum should be [`REF_TIME_PER_CORE_IN_SECS`] or otherwise blocks may fail to
	/// validate on the relay chain.
	pub block_time: Duration,
}

impl NextSlotSchedule {
	/// Creates a schedule that produces one block, occupying an entire core.
	pub fn one_block_using_one_core() -> Self {
		Self { number_of_blocks: 1, block_time: Duration::from_secs(REF_TIME_PER_CORE_IN_SECS) }
	}

	/// A schedule that maps `x` blocks onto `y` cores.
	pub fn x_blocks_using_y_cores(blocks: u32, cores: u32) -> Self {
		let ref_time_per_core = Duration::from_secs(REF_TIME_PER_CORE_IN_SECS);

		if blocks == 0 || cores == 0 {
			return Self { number_of_blocks: 0, block_time: Duration::from_secs(0) }
		}

		// In wall clock time we can not go above `6s` (relay chain slot duration), so we need to
		// cap there.
		let block_time = (ref_time_per_core * cores).min(Duration::from_secs(6)) / blocks;
		// One block can at max occupy one core.
		let block_time = block_time.min(ref_time_per_core);

		Self { block_time, number_of_blocks: blocks }
	}
}

sp_api::decl_runtime_apis! {
	/// Runtime api to collect information about a collation.
	///
	/// Version history:
	/// - Version 2: Changed [`Self::collect_collation_info`] signature
	/// - Version 3: Signals to the node to use version 1 of [`ParachainBlockData`].
	#[api_version(3)]
	pub trait CollectCollationInfo {
		/// Collect information about a collation.
		#[changed_in(2)]
		fn collect_collation_info() -> CollationInfoV1;
		/// Collect information about a collation.
		///
		/// The given `header` is the header of the built block for that
		/// we are collecting the collation info for.
		fn collect_collation_info(header: &Block::Header) -> CollationInfo;
	}

	/// Runtime api used to access general info about a parachain runtime.
	pub trait GetParachainInfo {
		/// Retrieve the parachain id used for runtime.
		fn parachain_id() -> ParaId;
  }

	/// API to tell the node side how the relay parent should be chosen.
	///
	/// A larger offset indicates that the relay parent should not be the tip of the relay chain,
	/// but `N` blocks behind the tip. This offset is then enforced by the runtime.
	pub trait RelayParentOffsetApi {
		/// Fetch the slot offset that is expected from the relay chain.
		fn relay_parent_offset() -> u32;
	}

	/// API for parachain slot scheduling.
	///
	/// This runtime API allows the parachain runtime to communicate the block interval
	/// to the node side. The node will call this API every relay chain slot (~6 seconds)
	/// to get the scheduled parachain block interval.
	pub trait SlotSchedule {
		/// Get the block production schedule for the next relay chain slot.
		///
		/// - `num_cores`: The number of cores assigned to this parachain
		///
		/// Returns a [`NextSlotSchedule`].
		fn next_slot_schedule(num_cores: u32) -> NextSlotSchedule;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn one_block_using_one_core_works() {
		let schedule = NextSlotSchedule::one_block_using_one_core();
		assert_eq!(schedule.number_of_blocks, 1);
		assert_eq!(schedule.block_time, Duration::from_secs(REF_TIME_PER_CORE_IN_SECS));
	}

	#[test]
	fn x_blocks_using_y_cores_basic_functionality() {
		// 2 blocks using 1 core: each block gets 1 second
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(2, 1);
		assert_eq!(schedule.number_of_blocks, 2);
		assert_eq!(schedule.block_time, Duration::from_secs(1));

		// 4 blocks using 2 cores: each block gets 1 second
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(4, 2);
		assert_eq!(schedule.number_of_blocks, 4);
		assert_eq!(schedule.block_time, Duration::from_secs(1));

		// 2 blocks using 2 cores: each block gets 2 seconds (max)
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(2, 2);
		assert_eq!(schedule.number_of_blocks, 2);
		assert_eq!(schedule.block_time, Duration::from_secs(2));
	}

	#[test]
	fn x_blocks_using_y_cores_caps_block_time_at_ref_time() {
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(2, 10);
		assert_eq!(schedule.number_of_blocks, 2);
		assert_eq!(schedule.block_time, Duration::from_secs(REF_TIME_PER_CORE_IN_SECS));

		let schedule = NextSlotSchedule::x_blocks_using_y_cores(1, 5);
		assert_eq!(schedule.number_of_blocks, 1);
		assert_eq!(schedule.block_time, Duration::from_secs(REF_TIME_PER_CORE_IN_SECS));
	}

	#[test]
	fn x_blocks_using_y_cores_edge_cases() {
		// Zero blocks
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(0, 1);
		assert_eq!(schedule.number_of_blocks, 0);
		assert_eq!(schedule.block_time, Duration::from_secs(0));

		// Zero cores (should not panic, though not realistic)
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(2, 0);
		assert_eq!(schedule.number_of_blocks, 0);
		assert_eq!(schedule.block_time, Duration::from_secs(0));

		// Large numbers
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(100, 50);
		assert_eq!(schedule.number_of_blocks, 100);
		assert_eq!(schedule.block_time, Duration::from_millis(60));
	}

	#[test]
	fn x_blocks_using_y_cores_various_ratios() {
		// 6 blocks, 3 cores: each block gets 1 second
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(6, 3);
		assert_eq!(schedule.number_of_blocks, 6);
		assert_eq!(schedule.block_time, Duration::from_secs(1));

		// 8 blocks, 4 cores: each block gets 1 second
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(8, 4);
		assert_eq!(schedule.number_of_blocks, 8);
		assert_eq!(schedule.block_time, Duration::from_millis(750));

		// 4 blocks, 8 cores: each block gets 2 seconds (capped)
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(4, 8);
		assert_eq!(schedule.number_of_blocks, 4);
		assert_eq!(schedule.block_time, Duration::from_millis(1500));

		// 10 blocks, 2 cores: each block gets `400ms`
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(10, 2);
		assert_eq!(schedule.number_of_blocks, 10);
		assert_eq!(schedule.block_time, Duration::from_millis(400));
	}

	#[test]
	fn x_blocks_using_y_cores_fractional_seconds() {
		// 6 blocks, 1 core: each block gets `333.333... ms (2000ms / 6)`
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(6, 1);
		assert_eq!(schedule.number_of_blocks, 6);
		assert_eq!(schedule.block_time, Duration::from_nanos(333_333_333));

		// 8 blocks, 1 core: each block gets `250ms`
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(8, 1);
		assert_eq!(schedule.number_of_blocks, 8);
		assert_eq!(schedule.block_time, Duration::from_millis(250));

		// 12 blocks, 1 core: each block gets `~166.666ms`
		let schedule = NextSlotSchedule::x_blocks_using_y_cores(12, 1);
		assert_eq!(schedule.number_of_blocks, 12);
		assert_eq!(schedule.block_time, Duration::from_nanos(166_666_666));
	}
}
