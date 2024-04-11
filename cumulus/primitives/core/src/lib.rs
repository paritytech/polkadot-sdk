// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Cumulus related core primitive types and traits.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use polkadot_parachain_primitives::primitives::HeadData;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

pub use polkadot_core_primitives::InboundDownwardMessage;
pub use polkadot_parachain_primitives::primitives::{
	DmpMessageHandler, Id as ParaId, IsSystem, UpwardMessage, ValidationParams, XcmpMessageFormat,
	XcmpMessageHandler,
};
pub use polkadot_primitives::{
	AbridgedHostConfiguration, AbridgedHrmpChannel, PersistedValidationData,
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
}

impl From<MessageSendError> for &'static str {
	fn from(e: MessageSendError) -> Self {
		use MessageSendError::*;
		match e {
			QueueFull => "QueueFull",
			NoChannel => "NoChannel",
			TooBig => "TooBig",
			Other => "Other",
		}
	}
}

/// The origin of an inbound message.
#[derive(Encode, Decode, MaxEncodedLen, Clone, Eq, PartialEq, TypeInfo, Debug)]
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

/// Something that should be called when sending an upward message.
pub trait UpwardMessageSender {
	/// Send the given UMP message; return the expected number of blocks before the message will
	/// be dispatched or an error if the message cannot be sent.
	/// return the hash of the message sent
	fn send_upward_message(msg: UpwardMessage) -> Result<(u32, XcmHash), MessageSendError>;
}
impl UpwardMessageSender for () {
	fn send_upward_message(_msg: UpwardMessage) -> Result<(u32, XcmHash), MessageSendError> {
		Err(MessageSendError::NoChannel)
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

/// The parachain block that is created by a collator.
///
/// This is send as PoV (proof of validity block) to the relay-chain validators. There it will be
/// passed to the parachain validation Wasm blob to be validated.
#[derive(codec::Encode, codec::Decode, Clone)]
pub struct ParachainBlockData<B: BlockT> {
	/// The header of the parachain block.
	header: B::Header,
	/// The extrinsics of the parachain block.
	extrinsics: sp_std::vec::Vec<B::Extrinsic>,
	/// The data that is required to emulate the storage accesses executed by all extrinsics.
	storage_proof: sp_trie::CompactProof,
}

impl<B: BlockT> ParachainBlockData<B> {
	/// Creates a new instance of `Self`.
	pub fn new(
		header: <B as BlockT>::Header,
		extrinsics: sp_std::vec::Vec<<B as BlockT>::Extrinsic>,
		storage_proof: sp_trie::CompactProof,
	) -> Self {
		Self { header, extrinsics, storage_proof }
	}

	/// Convert `self` into the stored block.
	pub fn into_block(self) -> B {
		B::new(self.header, self.extrinsics)
	}

	/// Convert `self` into the stored header.
	pub fn into_header(self) -> B::Header {
		self.header
	}

	/// Returns the header.
	pub fn header(&self) -> &B::Header {
		&self.header
	}

	/// Returns the extrinsics.
	pub fn extrinsics(&self) -> &[B::Extrinsic] {
		&self.extrinsics
	}

	/// Returns the [`CompactProof`](sp_trie::CompactProof).
	pub fn storage_proof(&self) -> &sp_trie::CompactProof {
		&self.storage_proof
	}

	/// Deconstruct into the inner parts.
	pub fn deconstruct(self) -> (B::Header, sp_std::vec::Vec<B::Extrinsic>, sp_trie::CompactProof) {
		(self.header, self.extrinsics, self.storage_proof)
	}
}

/// A consensus engine ID indicating that this is a Cumulus Parachain.
pub const CUMULUS_CONSENSUS_ID: ConsensusEngineId = *b"CMLS";

/// Consensus header digests for Cumulus parachains.
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq)]
pub enum CumulusDigestItem {
	/// A digest item indicating the relay-parent a parachain block was built against.
	#[codec(index = 0)]
	RelayParent(relay_chain::Hash),
}

impl CumulusDigestItem {
	/// Encode this as a Substrate [`DigestItem`].
	pub fn to_digest_item(&self) -> DigestItem {
		DigestItem::Consensus(CUMULUS_CONSENSUS_ID, self.encode())
	}
}

/// Extract the relay-parent from the provided header digest. Returns `None` if none were found.
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
	use super::{relay_chain, ConsensusEngineId, Decode, Digest, DigestItem, Encode};
	use codec::Compact;

	/// A consensus engine ID for relay-parent storage root digests.
	pub const RPSR_CONSENSUS_ID: ConsensusEngineId = *b"RPSR";

	/// Construct a digest item for relay-parent storage roots.
	pub fn relay_parent_storage_root_item(
		storage_root: relay_chain::Hash,
		number: impl Into<Compact<relay_chain::BlockNumber>>,
	) -> DigestItem {
		DigestItem::Consensus(RPSR_CONSENSUS_ID, (storage_root, number.into()).encode())
	}

	/// Extract the relay-parent storage root and number from the provided header digest. Returns
	/// `None` if none were found.
	pub fn extract_relay_parent_storage_root(
		digest: &Digest,
	) -> Option<(relay_chain::Hash, relay_chain::BlockNumber)> {
		digest.convert_first(|d| match d {
			DigestItem::Consensus(id, val) if id == &RPSR_CONSENSUS_ID => {
				let (h, n): (relay_chain::Hash, Compact<relay_chain::BlockNumber>) =
					Decode::decode(&mut &val[..]).ok()?;

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

sp_api::decl_runtime_apis! {
	/// Runtime api to collect information about a collation.
	#[api_version(2)]
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
}
