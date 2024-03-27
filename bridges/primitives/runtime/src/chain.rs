// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

use crate::{ChainId, HeaderIdProvider};

use codec::{Codec, Decode, Encode, MaxEncodedLen};
use frame_support::{weights::Weight, Parameter};
use num_traits::{AsPrimitive, Bounded, CheckedSub, Saturating, SaturatingAdd, Zero};
use sp_runtime::{
	traits::{
		AtLeast32Bit, AtLeast32BitUnsigned, Hash as HashT, Header as HeaderT, MaybeDisplay,
		MaybeSerialize, MaybeSerializeDeserialize, Member, SimpleBitOps, Verify,
	},
	FixedPointOperand,
};
use sp_std::{convert::TryFrom, fmt::Debug, hash::Hash, str::FromStr, vec, vec::Vec};

/// Chain call, that is either SCALE-encoded, or decoded.
#[derive(Debug, Clone, PartialEq)]
pub enum EncodedOrDecodedCall<ChainCall> {
	/// The call that is SCALE-encoded.
	///
	/// This variant is used when we the chain runtime is not bundled with the relay, but
	/// we still need the represent call in some RPC calls or transactions.
	Encoded(Vec<u8>),
	/// The decoded call.
	Decoded(ChainCall),
}

impl<ChainCall: Clone + Codec> EncodedOrDecodedCall<ChainCall> {
	/// Returns decoded call.
	pub fn to_decoded(&self) -> Result<ChainCall, codec::Error> {
		match self {
			Self::Encoded(ref encoded_call) =>
				ChainCall::decode(&mut &encoded_call[..]).map_err(Into::into),
			Self::Decoded(ref decoded_call) => Ok(decoded_call.clone()),
		}
	}

	/// Converts self to decoded call.
	pub fn into_decoded(self) -> Result<ChainCall, codec::Error> {
		match self {
			Self::Encoded(encoded_call) =>
				ChainCall::decode(&mut &encoded_call[..]).map_err(Into::into),
			Self::Decoded(decoded_call) => Ok(decoded_call),
		}
	}

	/// Converts self to encoded call.
	pub fn into_encoded(self) -> Vec<u8> {
		match self {
			Self::Encoded(encoded_call) => encoded_call,
			Self::Decoded(decoded_call) => decoded_call.encode(),
		}
	}
}

impl<ChainCall> From<ChainCall> for EncodedOrDecodedCall<ChainCall> {
	fn from(call: ChainCall) -> EncodedOrDecodedCall<ChainCall> {
		EncodedOrDecodedCall::Decoded(call)
	}
}

impl<ChainCall: Decode> Decode for EncodedOrDecodedCall<ChainCall> {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		// having encoded version is better than decoded, because decoding isn't required
		// everywhere and for mocked calls it may lead to **unneeded** errors
		match input.remaining_len()? {
			Some(remaining_len) => {
				let mut encoded_call = vec![0u8; remaining_len];
				input.read(&mut encoded_call)?;
				Ok(EncodedOrDecodedCall::Encoded(encoded_call))
			},
			None => Ok(EncodedOrDecodedCall::Decoded(ChainCall::decode(input)?)),
		}
	}
}

impl<ChainCall: Encode> Encode for EncodedOrDecodedCall<ChainCall> {
	fn encode(&self) -> Vec<u8> {
		match *self {
			Self::Encoded(ref encoded_call) => encoded_call.clone(),
			Self::Decoded(ref decoded_call) => decoded_call.encode(),
		}
	}
}

/// Minimal Substrate-based chain representation that may be used from no_std environment.
pub trait Chain: Send + Sync + 'static {
	/// Chain id.
	const ID: ChainId;

	/// A type that fulfills the abstract idea of what a Substrate block number is.
	// Constraints come from the associated Number type of `sp_runtime::traits::Header`
	// See here for more info:
	// https://crates.parity.io/sp_runtime/traits/trait.Header.html#associatedtype.Number
	//
	// Note that the `AsPrimitive<usize>` trait is required by the GRANDPA justification
	// verifier, and is not usually part of a Substrate Header's Number type.
	type BlockNumber: Parameter
		+ Member
		+ MaybeSerializeDeserialize
		+ Hash
		+ Copy
		+ Default
		+ MaybeDisplay
		+ AtLeast32BitUnsigned
		+ FromStr
		+ AsPrimitive<usize>
		+ Default
		+ Saturating
		+ MaxEncodedLen;

	/// A type that fulfills the abstract idea of what a Substrate hash is.
	// Constraints come from the associated Hash type of `sp_runtime::traits::Header`
	// See here for more info:
	// https://crates.parity.io/sp_runtime/traits/trait.Header.html#associatedtype.Hash
	type Hash: Parameter
		+ Member
		+ MaybeSerializeDeserialize
		+ Hash
		+ Ord
		+ Copy
		+ MaybeDisplay
		+ Default
		+ SimpleBitOps
		+ AsRef<[u8]>
		+ AsMut<[u8]>
		+ MaxEncodedLen;

	/// A type that fulfills the abstract idea of what a Substrate hasher (a type
	/// that produces hashes) is.
	// Constraints come from the associated Hashing type of `sp_runtime::traits::Header`
	// See here for more info:
	// https://crates.parity.io/sp_runtime/traits/trait.Header.html#associatedtype.Hashing
	type Hasher: HashT<Output = Self::Hash>;

	/// A type that fulfills the abstract idea of what a Substrate header is.
	// See here for more info:
	// https://crates.parity.io/sp_runtime/traits/trait.Header.html
	type Header: Parameter
		+ HeaderT<Number = Self::BlockNumber, Hash = Self::Hash>
		+ HeaderIdProvider<Self::Header>
		+ MaybeSerializeDeserialize;

	/// The user account identifier type for the runtime.
	type AccountId: Parameter
		+ Member
		+ MaybeSerializeDeserialize
		+ Debug
		+ MaybeDisplay
		+ Ord
		+ MaxEncodedLen;
	/// Balance of an account in native tokens.
	///
	/// The chain may support multiple tokens, but this particular type is for token that is used
	/// to pay for transaction dispatch, to reward different relayers (headers, messages), etc.
	type Balance: AtLeast32BitUnsigned
		+ FixedPointOperand
		+ Parameter
		+ Member
		+ MaybeSerializeDeserialize
		+ Clone
		+ Copy
		+ Bounded
		+ CheckedSub
		+ PartialOrd
		+ SaturatingAdd
		+ Zero
		+ TryFrom<sp_core::U256>
		+ MaxEncodedLen;
	/// Nonce of a transaction used by the chain.
	type Nonce: Parameter
		+ Member
		+ MaybeSerialize
		+ Debug
		+ Default
		+ MaybeDisplay
		+ MaybeSerializeDeserialize
		+ AtLeast32Bit
		+ Copy
		+ MaxEncodedLen;
	/// Signature type, used on this chain.
	type Signature: Parameter + Verify;

	/// Get the maximum size (in bytes) of a Normal extrinsic at this chain.
	fn max_extrinsic_size() -> u32;
	/// Get the maximum weight (compute time) that a Normal extrinsic at this chain can use.
	fn max_extrinsic_weight() -> Weight;
}

/// A trait that provides the type of the underlying chain.
pub trait UnderlyingChainProvider: Send + Sync + 'static {
	/// Underlying chain type.
	type Chain: Chain;
}

impl<T> Chain for T
where
	T: Send + Sync + 'static + UnderlyingChainProvider,
{
	const ID: ChainId = <T::Chain as Chain>::ID;

	type BlockNumber = <T::Chain as Chain>::BlockNumber;
	type Hash = <T::Chain as Chain>::Hash;
	type Hasher = <T::Chain as Chain>::Hasher;
	type Header = <T::Chain as Chain>::Header;
	type AccountId = <T::Chain as Chain>::AccountId;
	type Balance = <T::Chain as Chain>::Balance;
	type Nonce = <T::Chain as Chain>::Nonce;
	type Signature = <T::Chain as Chain>::Signature;

	fn max_extrinsic_size() -> u32 {
		<T::Chain as Chain>::max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		<T::Chain as Chain>::max_extrinsic_weight()
	}
}

/// Minimal parachain representation that may be used from no_std environment.
pub trait Parachain: Chain {
	/// Parachain identifier.
	const PARACHAIN_ID: u32;
}

impl<T> Parachain for T
where
	T: Chain + UnderlyingChainProvider,
	<T as UnderlyingChainProvider>::Chain: Parachain,
{
	const PARACHAIN_ID: u32 = <<T as UnderlyingChainProvider>::Chain as Parachain>::PARACHAIN_ID;
}

/// Adapter for `Get<u32>` to access `PARACHAIN_ID` from `trait Parachain`
pub struct ParachainIdOf<Para>(sp_std::marker::PhantomData<Para>);
impl<Para: Parachain> frame_support::traits::Get<u32> for ParachainIdOf<Para> {
	fn get() -> u32 {
		Para::PARACHAIN_ID
	}
}

/// Underlying chain type.
pub type UnderlyingChainOf<C> = <C as UnderlyingChainProvider>::Chain;

/// Block number used by the chain.
pub type BlockNumberOf<C> = <C as Chain>::BlockNumber;

/// Hash type used by the chain.
pub type HashOf<C> = <C as Chain>::Hash;

/// Hasher type used by the chain.
pub type HasherOf<C> = <C as Chain>::Hasher;

/// Header type used by the chain.
pub type HeaderOf<C> = <C as Chain>::Header;

/// Account id type used by the chain.
pub type AccountIdOf<C> = <C as Chain>::AccountId;

/// Balance type used by the chain.
pub type BalanceOf<C> = <C as Chain>::Balance;

/// Transaction nonce type used by the chain.
pub type NonceOf<C> = <C as Chain>::Nonce;

/// Signature type used by the chain.
pub type SignatureOf<C> = <C as Chain>::Signature;

/// Account public type used by the chain.
pub type AccountPublicOf<C> = <SignatureOf<C> as Verify>::Signer;

/// Transaction era used by the chain.
pub type TransactionEraOf<C> = crate::TransactionEra<BlockNumberOf<C>, HashOf<C>>;

/// Convenience macro that declares bridge finality runtime apis and related constants for a chain.
/// This includes:
/// - chain-specific bridge runtime APIs:
///     - `<ThisChain>FinalityApi`
/// - constants that are stringified names of runtime API methods:
///     - `BEST_FINALIZED_<THIS_CHAIN>_HEADER_METHOD`
///     - `<THIS_CHAIN>_ACCEPTED_<CONSENSUS>_FINALITY_PROOFS_METHOD`
/// The name of the chain has to be specified in snake case (e.g. `bridge_hub_polkadot`).
#[macro_export]
macro_rules! decl_bridge_finality_runtime_apis {
	($chain: ident $(, $consensus: ident => $justification_type: ty)?) => {
		bp_runtime::paste::item! {
			mod [<$chain _finality_api>] {
				use super::*;

				/// Name of the `<ThisChain>FinalityApi::best_finalized` runtime method.
				pub const [<BEST_FINALIZED_ $chain:upper _HEADER_METHOD>]: &str =
					stringify!([<$chain:camel FinalityApi_best_finalized>]);

				$(
					/// Name of the `<ThisChain>FinalityApi::accepted_<consensus>_finality_proofs`
					/// runtime method.
					pub const [<$chain:upper _SYNCED_HEADERS_ $consensus:upper _INFO_METHOD>]: &str =
						stringify!([<$chain:camel FinalityApi_synced_headers_ $consensus:lower _info>]);
				)?

				sp_api::decl_runtime_apis! {
					/// API for querying information about the finalized chain headers.
					///
					/// This API is implemented by runtimes that are receiving messages from this chain, not by this
					/// chain's runtime itself.
					pub trait [<$chain:camel FinalityApi>] {
						/// Returns number and hash of the best finalized header known to the bridge module.
						fn best_finalized() -> Option<bp_runtime::HeaderId<Hash, BlockNumber>>;

						$(
							/// Returns the justifications accepted in the current block.
							fn [<synced_headers_ $consensus:lower _info>](
							) -> sp_std::vec::Vec<$justification_type>;
						)?
					}
				}
			}

			pub use [<$chain _finality_api>]::*;
		}
	};
	($chain: ident, grandpa) => {
		decl_bridge_finality_runtime_apis!($chain, grandpa => bp_header_chain::StoredHeaderGrandpaInfo<Header>);
	};
}

/// Convenience macro that declares bridge messages runtime apis and related constants for a chain.
/// This includes:
/// - chain-specific bridge runtime APIs:
///     - `To<ThisChain>OutboundLaneApi`
///     - `From<ThisChain>InboundLaneApi`
/// - constants that are stringified names of runtime API methods:
///     - `FROM_<THIS_CHAIN>_MESSAGE_DETAILS_METHOD`,
/// The name of the chain has to be specified in snake case (e.g. `bridge_hub_polkadot`).
#[macro_export]
macro_rules! decl_bridge_messages_runtime_apis {
	($chain: ident) => {
		bp_runtime::paste::item! {
			mod [<$chain _messages_api>] {
				use super::*;

				/// Name of the `To<ThisChain>OutboundLaneApi::message_details` runtime method.
				pub const [<TO_ $chain:upper _MESSAGE_DETAILS_METHOD>]: &str =
					stringify!([<To $chain:camel OutboundLaneApi_message_details>]);

				/// Name of the `From<ThisChain>InboundLaneApi::message_details` runtime method.
				pub const [<FROM_ $chain:upper _MESSAGE_DETAILS_METHOD>]: &str =
					stringify!([<From $chain:camel InboundLaneApi_message_details>]);

				sp_api::decl_runtime_apis! {
					/// Outbound message lane API for messages that are sent to this chain.
					///
					/// This API is implemented by runtimes that are receiving messages from this chain, not by this
					/// chain's runtime itself.
					pub trait [<To $chain:camel OutboundLaneApi>] {
						/// Returns dispatch weight, encoded payload size and delivery+dispatch fee of all
						/// messages in given inclusive range.
						///
						/// If some (or all) messages are missing from the storage, they'll also will
						/// be missing from the resulting vector. The vector is ordered by the nonce.
						fn message_details(
							lane: bp_messages::LaneId,
							begin: bp_messages::MessageNonce,
							end: bp_messages::MessageNonce,
						) -> sp_std::vec::Vec<bp_messages::OutboundMessageDetails>;
					}

					/// Inbound message lane API for messages sent by this chain.
					///
					/// This API is implemented by runtimes that are receiving messages from this chain, not by this
					/// chain's runtime itself.
					///
					/// Entries of the resulting vector are matching entries of the `messages` vector. Entries of the
					/// `messages` vector may (and need to) be read using `To<ThisChain>OutboundLaneApi::message_details`.
					pub trait [<From $chain:camel InboundLaneApi>] {
						/// Return details of given inbound messages.
						fn message_details(
							lane: bp_messages::LaneId,
							messages: sp_std::vec::Vec<(bp_messages::MessagePayload, bp_messages::OutboundMessageDetails)>,
						) -> sp_std::vec::Vec<bp_messages::InboundMessageDetails>;
					}
				}
			}

			pub use [<$chain _messages_api>]::*;
		}
	};
}

/// Convenience macro that declares bridge finality runtime apis, bridge messages runtime apis
/// and related constants for a chain.
/// The name of the chain has to be specified in snake case (e.g. `bridge_hub_polkadot`).
#[macro_export]
macro_rules! decl_bridge_runtime_apis {
	($chain: ident $(, $consensus: ident)?) => {
		bp_runtime::decl_bridge_finality_runtime_apis!($chain $(, $consensus)?);
		bp_runtime::decl_bridge_messages_runtime_apis!($chain);
	};
}
