// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! Types that allow runtime to act as a source/target endpoint of message lanes.
//!
//! Messages are assumed to be encoded `Call`s of the target chain. Call-dispatch
//! pallet is used to dispatch incoming messages. Message identified by a tuple
//! of to elements - message lane id and message nonce.

use bp_message_dispatch::MessageDispatch as _;
use bp_message_lane::{
	source_chain::LaneMessageVerifier,
	target_chain::{DispatchMessage, MessageDispatch, ProvedLaneMessages, ProvedMessages},
	InboundLaneData, LaneId, Message, MessageData, MessageKey, MessageNonce, OutboundLaneData,
};
use bp_runtime::InstanceId;
use codec::{Compact, Decode, Input};
use frame_support::{traits::Instance, RuntimeDebug};
use sp_runtime::traits::{CheckedAdd, CheckedDiv, CheckedMul};
use sp_std::{cmp::PartialOrd, marker::PhantomData, vec::Vec};
use sp_trie::StorageProof;

/// Bidirectional message bridge.
pub trait MessageBridge {
	/// Instance id of this bridge.
	const INSTANCE: InstanceId;

	/// Relayer interest (in percents).
	const RELAYER_FEE_PERCENT: u32;

	/// This chain in context of message bridge.
	type ThisChain: ChainWithMessageLanes;
	/// Bridged chain in context of message bridge.
	type BridgedChain: ChainWithMessageLanes;

	/// Maximal (dispatch) weight of the message that we are able to send to Bridged chain.
	fn maximal_dispatch_weight_of_message_on_bridged_chain() -> WeightOf<BridgedChain<Self>>;

	/// Maximal weight of single message delivery transaction on Bridged chain.
	fn weight_of_delivery_transaction() -> WeightOf<BridgedChain<Self>>;

	/// Maximal weight of single message delivery confirmation transaction on This chain.
	fn weight_of_delivery_confirmation_transaction_on_this_chain() -> WeightOf<ThisChain<Self>>;

	/// Weight of single message reward confirmation on the Bridged chain. This confirmation
	/// is a part of delivery transaction, so this weight is added to the delivery
	/// transaction weight.
	fn weight_of_reward_confirmation_transaction_on_target_chain() -> WeightOf<BridgedChain<Self>>;

	/// Convert weight of This chain to the fee (paid in Balance) of This chain.
	fn this_weight_to_this_balance(weight: WeightOf<ThisChain<Self>>) -> BalanceOf<ThisChain<Self>>;

	/// Convert weight of the Bridged chain to the fee (paid in Balance) of the Bridged chain.
	fn bridged_weight_to_bridged_balance(weight: WeightOf<BridgedChain<Self>>) -> BalanceOf<BridgedChain<Self>>;

	/// Convert This chain Balance into Bridged chain Balance.
	fn this_balance_to_bridged_balance(this_balance: BalanceOf<ThisChain<Self>>) -> BalanceOf<BridgedChain<Self>>;
}

/// Chain that has `message-lane` and `call-dispatch` modules.
pub trait ChainWithMessageLanes {
	/// Hash used in the chain.
	type Hash: Decode;
	/// Accound id on the chain.
	type AccountId: Decode;
	/// Public key of the chain account that may be used to verify signatures.
	type Signer: Decode;
	/// Signature type used on the chain.
	type Signature: Decode;
	/// Call type on the chain.
	type Call: Decode;
	/// Type of weight that is used on the chain. This would almost always be a regular
	/// `frame_support::weight::Weight`. But since the meaning of weight on different chains
	/// may be different, the `WeightOf<>` construct is used to avoid confusion between
	/// different weights.
	type Weight: From<frame_support::weights::Weight>;
	/// Type of balances that is used on the chain.
	type Balance: Decode + CheckedAdd + CheckedDiv + CheckedMul + PartialOrd + From<u32> + Copy;

	/// Instance of the message-lane pallet.
	type MessageLaneInstance: Instance;
}

pub(crate) type ThisChain<B> = <B as MessageBridge>::ThisChain;
pub(crate) type BridgedChain<B> = <B as MessageBridge>::BridgedChain;
pub(crate) type HashOf<C> = <C as ChainWithMessageLanes>::Hash;
pub(crate) type AccountIdOf<C> = <C as ChainWithMessageLanes>::AccountId;
pub(crate) type SignerOf<C> = <C as ChainWithMessageLanes>::Signer;
pub(crate) type SignatureOf<C> = <C as ChainWithMessageLanes>::Signature;
pub(crate) type WeightOf<C> = <C as ChainWithMessageLanes>::Weight;
pub(crate) type BalanceOf<C> = <C as ChainWithMessageLanes>::Balance;
pub(crate) type CallOf<C> = <C as ChainWithMessageLanes>::Call;
pub(crate) type MessageLaneInstanceOf<C> = <C as ChainWithMessageLanes>::MessageLaneInstance;

/// Sub-module that is declaring types required for processing This -> Bridged chain messages.
pub mod source {
	use super::*;

	/// Encoded Call of the Bridged chain. We never try to decode it on This chain.
	pub type BridgedChainOpaqueCall = Vec<u8>;

	/// Message payload for This -> Bridged chain messages.
	pub type FromThisChainMessagePayload<B> = pallet_bridge_call_dispatch::MessagePayload<
		AccountIdOf<ThisChain<B>>,
		SignerOf<BridgedChain<B>>,
		SignatureOf<BridgedChain<B>>,
		BridgedChainOpaqueCall,
	>;

	/// Messages delivery proof from bridged chain:
	///
	/// - hash of finalized header;
	/// - storage proof of inbound lane state;
	/// - lane id.
	pub type FromBridgedChainMessagesDeliveryProof<B> = (HashOf<BridgedChain<B>>, StorageProof, LaneId);

	/// 'Parsed' message delivery proof - inbound lane id and its state.
	pub type ParsedMessagesDeliveryProofFromBridgedChain<B> = (LaneId, InboundLaneData<AccountIdOf<ThisChain<B>>>);

	/// Message verifier that requires submitter to pay minimal delivery and dispatch fee.
	#[derive(RuntimeDebug)]
	pub struct FromThisChainMessageVerifier<B>(PhantomData<B>);

	impl<B: MessageBridge>
		LaneMessageVerifier<AccountIdOf<ThisChain<B>>, FromThisChainMessagePayload<B>, BalanceOf<ThisChain<B>>>
		for FromThisChainMessageVerifier<B>
	{
		type Error = &'static str;

		fn verify_message(
			_submitter: &AccountIdOf<ThisChain<B>>,
			delivery_and_dispatch_fee: &BalanceOf<ThisChain<B>>,
			_lane: &LaneId,
			payload: &FromThisChainMessagePayload<B>,
		) -> Result<(), Self::Error> {
			let minimal_fee_in_bridged_tokens =
				estimate_message_dispatch_and_delivery_fee::<B>(payload, B::RELAYER_FEE_PERCENT)?;

			// compare with actual fee paid
			let actual_fee_in_bridged_tokens = B::this_balance_to_bridged_balance(*delivery_and_dispatch_fee);
			if actual_fee_in_bridged_tokens < minimal_fee_in_bridged_tokens {
				return Err("Too low fee paid");
			}

			Ok(())
		}
	}

	/// Estimate delivery and dispatch fee that must be paid for delivering a message to the Bridged chain.
	///
	/// The fee is paid in This chain Balance, but we use Bridged chain balance to avoid additional conversions.
	/// Returns `None` if overflow has happened.
	pub fn estimate_message_dispatch_and_delivery_fee<B: MessageBridge>(
		payload: &FromThisChainMessagePayload<B>,
		relayer_fee_percent: u32,
	) -> Result<BalanceOf<BridgedChain<B>>, &'static str> {
		// the fee (in Bridged tokens) of all transactions that are made on the Bridged chain
		let delivery_fee = B::bridged_weight_to_bridged_balance(B::weight_of_delivery_transaction());
		let dispatch_fee = B::bridged_weight_to_bridged_balance(payload.weight.into());
		let reward_confirmation_fee =
			B::bridged_weight_to_bridged_balance(B::weight_of_reward_confirmation_transaction_on_target_chain());

		// the fee (in Bridged tokens) of all transactions that are made on This chain
		let delivery_confirmation_fee = B::this_balance_to_bridged_balance(B::this_weight_to_this_balance(
			B::weight_of_delivery_confirmation_transaction_on_this_chain(),
		));

		// minimal fee (in Bridged tokens) is a sum of all required fees
		let minimal_fee = delivery_fee
			.checked_add(&dispatch_fee)
			.and_then(|fee| fee.checked_add(&reward_confirmation_fee))
			.and_then(|fee| fee.checked_add(&delivery_confirmation_fee));

		// before returning, add extra fee that is paid to the relayer (relayer interest)
		minimal_fee
			.and_then(|fee|
			// having message with fee that is near the `Balance::MAX_VALUE` of the chain is
			// unlikely and should be treated as an error
			// => let's do multiplication first
			fee
				.checked_mul(&relayer_fee_percent.into())
				.and_then(|interest| interest.checked_div(&100u32.into()))
				.and_then(|interest| fee.checked_add(&interest)))
			.ok_or("Overflow when computing minimal required message delivery and dispatch fee")
	}

	/// Verify proof of This -> Bridged chain messages delivery.
	pub fn verify_messages_delivery_proof<B: MessageBridge, ThisRuntime>(
		proof: FromBridgedChainMessagesDeliveryProof<B>,
	) -> Result<ParsedMessagesDeliveryProofFromBridgedChain<B>, &'static str>
	where
		ThisRuntime: pallet_substrate_bridge::Trait,
		ThisRuntime: pallet_message_lane::Trait<MessageLaneInstanceOf<BridgedChain<B>>>,
		HashOf<BridgedChain<B>>:
			Into<bp_runtime::HashOf<<ThisRuntime as pallet_substrate_bridge::Trait>::BridgedChain>>,
	{
		let (bridged_header_hash, bridged_storage_proof, lane) = proof;
		pallet_substrate_bridge::Module::<ThisRuntime>::parse_finalized_storage_proof(
			bridged_header_hash.into(),
			bridged_storage_proof,
			|storage| {
				// Messages delivery proof is just proof of single storage key read => any error
				// is fatal.
				let storage_inbound_lane_data_key = pallet_message_lane::storage_keys::inbound_lane_data_key::<
					ThisRuntime,
					MessageLaneInstanceOf<BridgedChain<B>>,
				>(&lane);
				let raw_inbound_lane_data = storage
					.read_value(storage_inbound_lane_data_key.0.as_ref())
					.map_err(|_| "Failed to read inbound lane state from storage proof")?
					.ok_or("Inbound lane state is missing from the messages proof")?;
				let inbound_lane_data = InboundLaneData::decode(&mut &raw_inbound_lane_data[..])
					.map_err(|_| "Failed to decode inbound lane state from the proof")?;

				Ok((lane, inbound_lane_data))
			},
		)
		.map_err(<&'static str>::from)?
	}
}

/// Sub-module that is declaring types required for processing Bridged -> This chain messages.
pub mod target {
	use super::*;

	/// Call origin for Bridged -> This chain messages.
	pub type FromBridgedChainMessageCallOrigin<B> = pallet_bridge_call_dispatch::CallOrigin<
		AccountIdOf<BridgedChain<B>>,
		SignerOf<ThisChain<B>>,
		SignatureOf<ThisChain<B>>,
	>;

	/// Decoded Bridged -> This message payload.
	pub type FromBridgedChainDecodedMessagePayload<B> = pallet_bridge_call_dispatch::MessagePayload<
		AccountIdOf<BridgedChain<B>>,
		SignerOf<ThisChain<B>>,
		SignatureOf<ThisChain<B>>,
		CallOf<ThisChain<B>>,
	>;

	/// Messages proof from bridged chain:
	///
	/// - hash of finalized header;
	/// - storage proof of messages and (optionally) outbound lane state;
	/// - lane id;
	/// - nonces (inclusive range) of messages which are included in this proof.
	pub type FromBridgedChainMessagesProof<B> = (
		HashOf<BridgedChain<B>>,
		StorageProof,
		LaneId,
		MessageNonce,
		MessageNonce,
	);

	/// Message payload for Bridged -> This messages.
	pub struct FromBridgedChainMessagePayload<B: MessageBridge>(pub(crate) FromBridgedChainDecodedMessagePayload<B>);

	impl<B: MessageBridge> Decode for FromBridgedChainMessagePayload<B> {
		fn decode<I: Input>(input: &mut I) -> Result<Self, codec::Error> {
			// for bridged chain our Calls are opaque - they're encoded to Vec<u8> by submitter
			// => skip encoded vec length here before decoding Call
			let spec_version = pallet_bridge_call_dispatch::SpecVersion::decode(input)?;
			let weight = frame_support::weights::Weight::decode(input)?;
			let origin = FromBridgedChainMessageCallOrigin::<B>::decode(input)?;
			let _skipped_length = Compact::<u32>::decode(input)?;
			let call = CallOf::<ThisChain<B>>::decode(input)?;

			Ok(FromBridgedChainMessagePayload(
				pallet_bridge_call_dispatch::MessagePayload {
					spec_version,
					weight,
					origin,
					call,
				},
			))
		}
	}

	/// Dispatching Bridged -> This chain messages.
	#[derive(RuntimeDebug, Clone, Copy)]
	pub struct FromBridgedChainMessageDispatch<B, ThisRuntime, ThisCallDispatchInstance> {
		_marker: PhantomData<(B, ThisRuntime, ThisCallDispatchInstance)>,
	}

	impl<B: MessageBridge, ThisRuntime, ThisCallDispatchInstance>
		MessageDispatch<<BridgedChain<B> as ChainWithMessageLanes>::Balance>
		for FromBridgedChainMessageDispatch<B, ThisRuntime, ThisCallDispatchInstance>
	where
		ThisCallDispatchInstance: frame_support::traits::Instance,
		ThisRuntime: pallet_bridge_call_dispatch::Trait<ThisCallDispatchInstance>,
		pallet_bridge_call_dispatch::Module<ThisRuntime, ThisCallDispatchInstance>:
			bp_message_dispatch::MessageDispatch<
				(LaneId, MessageNonce),
				Message = FromBridgedChainDecodedMessagePayload<B>,
			>,
	{
		type DispatchPayload = FromBridgedChainMessagePayload<B>;

		fn dispatch_weight(
			message: &DispatchMessage<Self::DispatchPayload, BalanceOf<BridgedChain<B>>>,
		) -> frame_support::weights::Weight {
			message
				.data
				.payload
				.as_ref()
				.map(|payload| payload.0.weight)
				.unwrap_or(0)
		}

		fn dispatch(message: DispatchMessage<Self::DispatchPayload, BalanceOf<BridgedChain<B>>>) {
			if let Ok(payload) = message.data.payload {
				pallet_bridge_call_dispatch::Module::<ThisRuntime, ThisCallDispatchInstance>::dispatch(
					B::INSTANCE,
					(message.key.lane_id, message.key.nonce),
					payload.0,
				);
			}
		}
	}

	/// Verify proof of Bridged -> This chain messages.
	pub fn verify_messages_proof<B: MessageBridge, ThisRuntime>(
		proof: FromBridgedChainMessagesProof<B>,
	) -> Result<ProvedMessages<Message<BalanceOf<BridgedChain<B>>>>, &'static str>
	where
		ThisRuntime: pallet_substrate_bridge::Trait,
		ThisRuntime: pallet_message_lane::Trait<MessageLaneInstanceOf<BridgedChain<B>>>,
		HashOf<BridgedChain<B>>:
			Into<bp_runtime::HashOf<<ThisRuntime as pallet_substrate_bridge::Trait>::BridgedChain>>,
	{
		let (bridged_header_hash, bridged_storage_proof, lane_id, begin, end) = proof;
		pallet_substrate_bridge::Module::<ThisRuntime>::parse_finalized_storage_proof(
			bridged_header_hash.into(),
			bridged_storage_proof,
			|storage| {
				// Read messages first. All messages that are claimed to be in the proof must
				// be in the proof. So any error in `read_value`, or even missing value is fatal.
				//
				// Mind that we allow proofs with no messages if outbound lane state is proved.
				let mut messages = Vec::with_capacity(end.saturating_sub(begin) as _);
				for nonce in begin..=end {
					let message_key = MessageKey { lane_id, nonce };
					let storage_message_key = pallet_message_lane::storage_keys::message_key::<
						ThisRuntime,
						MessageLaneInstanceOf<BridgedChain<B>>,
					>(&lane_id, nonce);
					let raw_message_data = storage
						.read_value(storage_message_key.0.as_ref())
						.map_err(|_| "Failed to read message from storage proof")?
						.ok_or("Message is missing from the messages proof")?;
					let message_data = MessageData::<BalanceOf<BridgedChain<B>>>::decode(&mut &raw_message_data[..])
						.map_err(|_| "Failed to decode message from the proof")?;
					messages.push(Message {
						key: message_key,
						data: message_data,
					});
				}

				// Now let's check if proof contains outbound lane state proof. It is optional, so we
				// simply ignore `read_value` errors and missing value.
				let mut proved_lane_messages = ProvedLaneMessages {
					lane_state: None,
					messages,
				};
				let storage_outbound_lane_data_key = pallet_message_lane::storage_keys::outbound_lane_data_key::<
					MessageLaneInstanceOf<BridgedChain<B>>,
				>(&lane_id);
				let raw_outbound_lane_data = storage.read_value(storage_outbound_lane_data_key.0.as_ref());
				if let Ok(Some(raw_outbound_lane_data)) = raw_outbound_lane_data {
					proved_lane_messages.lane_state = Some(
						OutboundLaneData::decode(&mut &raw_outbound_lane_data[..])
							.map_err(|_| "Failed to decode outbound lane data from the proof")?,
					);
				}

				// Now we may actually check if the proof is empty or not.
				if proved_lane_messages.lane_state.is_none() && proved_lane_messages.messages.is_empty() {
					return Err("Messages proof is empty");
				}

				// We only support single lane messages in this schema
				let mut proved_messages = ProvedMessages::new();
				proved_messages.insert(lane_id, proved_lane_messages);

				Ok(proved_messages)
			},
		)
		.map_err(<&'static str>::from)?
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Decode, Encode};
	use frame_support::weights::Weight;

	const DELIVERY_TRANSACTION_WEIGHT: Weight = 100;
	const DELIVERY_CONFIRMATION_TRANSACTION_WEIGHT: Weight = 100;
	const REWARD_CONFIRMATION_TRANSACTION_WEIGHT: Weight = 100;
	const THIS_CHAIN_WEIGHT_TO_BALANCE_RATE: Weight = 2;
	const BRIDGED_CHAIN_WEIGHT_TO_BALANCE_RATE: Weight = 4;
	const THIS_CHAIN_TO_BRIDGED_CHAIN_BALANCE_RATE: u32 = 6;

	/// Bridge that is deployed on ThisChain and allows sending/receiving messages to/from BridgedChain;
	struct OnThisChainBridge;

	impl MessageBridge for OnThisChainBridge {
		const INSTANCE: InstanceId = *b"this";
		const RELAYER_FEE_PERCENT: u32 = 10;

		type ThisChain = ThisChain;
		type BridgedChain = BridgedChain;

		fn maximal_dispatch_weight_of_message_on_bridged_chain() -> Weight {
			unreachable!()
		}

		fn weight_of_delivery_transaction() -> Weight {
			DELIVERY_TRANSACTION_WEIGHT
		}

		fn weight_of_delivery_confirmation_transaction_on_this_chain() -> Weight {
			DELIVERY_CONFIRMATION_TRANSACTION_WEIGHT
		}

		fn weight_of_reward_confirmation_transaction_on_target_chain() -> Weight {
			REWARD_CONFIRMATION_TRANSACTION_WEIGHT
		}

		fn this_weight_to_this_balance(weight: Weight) -> ThisChainBalance {
			ThisChainBalance(weight as u32 * THIS_CHAIN_WEIGHT_TO_BALANCE_RATE as u32)
		}

		fn bridged_weight_to_bridged_balance(weight: Weight) -> BridgedChainBalance {
			BridgedChainBalance(weight as u32 * BRIDGED_CHAIN_WEIGHT_TO_BALANCE_RATE as u32)
		}

		fn this_balance_to_bridged_balance(this_balance: ThisChainBalance) -> BridgedChainBalance {
			BridgedChainBalance(this_balance.0 * THIS_CHAIN_TO_BRIDGED_CHAIN_BALANCE_RATE as u32)
		}
	}

	/// Bridge that is deployed on BridgedChain and allows sending/receiving messages to/from ThisChain;
	struct OnBridgedChainBridge;

	impl MessageBridge for OnBridgedChainBridge {
		const INSTANCE: InstanceId = *b"brdg";
		const RELAYER_FEE_PERCENT: u32 = 20;

		type ThisChain = BridgedChain;
		type BridgedChain = ThisChain;

		fn maximal_dispatch_weight_of_message_on_bridged_chain() -> Weight {
			unreachable!()
		}

		fn weight_of_delivery_transaction() -> Weight {
			unreachable!()
		}

		fn weight_of_delivery_confirmation_transaction_on_this_chain() -> Weight {
			unreachable!()
		}

		fn weight_of_reward_confirmation_transaction_on_target_chain() -> Weight {
			unreachable!()
		}

		fn this_weight_to_this_balance(_weight: Weight) -> BridgedChainBalance {
			unreachable!()
		}

		fn bridged_weight_to_bridged_balance(_weight: Weight) -> ThisChainBalance {
			unreachable!()
		}

		fn this_balance_to_bridged_balance(_this_balance: BridgedChainBalance) -> ThisChainBalance {
			unreachable!()
		}
	}

	#[derive(Debug, PartialEq, Decode, Encode)]
	struct ThisChainAccountId(u32);
	#[derive(Debug, PartialEq, Decode, Encode)]
	struct ThisChainSigner(u32);
	#[derive(Debug, PartialEq, Decode, Encode)]
	struct ThisChainSignature(u32);
	#[derive(Debug, PartialEq, Decode, Encode)]
	enum ThisChainCall {
		#[codec(index = 42)]
		Transfer,
		#[codec(index = 84)]
		Mint,
	}

	#[derive(Debug, PartialEq, Decode, Encode)]
	struct BridgedChainAccountId(u32);
	#[derive(Debug, PartialEq, Decode, Encode)]
	struct BridgedChainSigner(u32);
	#[derive(Debug, PartialEq, Decode, Encode)]
	struct BridgedChainSignature(u32);
	#[derive(Debug, PartialEq, Decode, Encode)]
	enum BridgedChainCall {}

	macro_rules! impl_wrapped_balance {
		($name:ident) => {
			#[derive(Debug, PartialEq, Decode, Encode, Clone, Copy)]
			struct $name(u32);

			impl From<u32> for $name {
				fn from(balance: u32) -> Self {
					Self(balance)
				}
			}

			impl sp_std::ops::Add for $name {
				type Output = $name;

				fn add(self, other: Self) -> Self {
					Self(self.0 + other.0)
				}
			}

			impl sp_std::ops::Div for $name {
				type Output = $name;

				fn div(self, other: Self) -> Self {
					Self(self.0 / other.0)
				}
			}

			impl sp_std::ops::Mul for $name {
				type Output = $name;

				fn mul(self, other: Self) -> Self {
					Self(self.0 * other.0)
				}
			}

			impl sp_std::cmp::PartialOrd for $name {
				fn partial_cmp(&self, other: &Self) -> Option<sp_std::cmp::Ordering> {
					self.0.partial_cmp(&other.0)
				}
			}

			impl CheckedAdd for $name {
				fn checked_add(&self, other: &Self) -> Option<Self> {
					self.0.checked_add(other.0).map(Self)
				}
			}

			impl CheckedDiv for $name {
				fn checked_div(&self, other: &Self) -> Option<Self> {
					self.0.checked_div(other.0).map(Self)
				}
			}

			impl CheckedMul for $name {
				fn checked_mul(&self, other: &Self) -> Option<Self> {
					self.0.checked_mul(other.0).map(Self)
				}
			}
		};
	}

	impl_wrapped_balance!(ThisChainBalance);
	impl_wrapped_balance!(BridgedChainBalance);

	struct ThisChain;

	impl ChainWithMessageLanes for ThisChain {
		type Hash = ();
		type AccountId = ThisChainAccountId;
		type Signer = ThisChainSigner;
		type Signature = ThisChainSignature;
		type Call = ThisChainCall;
		type Weight = frame_support::weights::Weight;
		type Balance = ThisChainBalance;

		type MessageLaneInstance = pallet_message_lane::DefaultInstance;
	}

	struct BridgedChain;

	impl ChainWithMessageLanes for BridgedChain {
		type Hash = ();
		type AccountId = BridgedChainAccountId;
		type Signer = BridgedChainSigner;
		type Signature = BridgedChainSignature;
		type Call = BridgedChainCall;
		type Weight = frame_support::weights::Weight;
		type Balance = BridgedChainBalance;

		type MessageLaneInstance = pallet_message_lane::DefaultInstance;
	}

	#[test]
	fn message_from_bridged_chain_is_decoded() {
		// the message is encoded on the bridged chain
		let message_on_bridged_chain = source::FromThisChainMessagePayload::<OnBridgedChainBridge> {
			spec_version: 1,
			weight: 100,
			origin: pallet_bridge_call_dispatch::CallOrigin::SourceRoot,
			call: ThisChainCall::Transfer.encode(),
		}
		.encode();

		// and sent to this chain where it is decoded
		let message_on_this_chain =
			target::FromBridgedChainMessagePayload::<OnThisChainBridge>::decode(&mut &message_on_bridged_chain[..])
				.unwrap();
		assert_eq!(
			message_on_this_chain.0,
			target::FromBridgedChainDecodedMessagePayload::<OnThisChainBridge> {
				spec_version: 1,
				weight: 100,
				origin: pallet_bridge_call_dispatch::CallOrigin::SourceRoot,
				call: ThisChainCall::Transfer,
			}
		);
	}

	#[test]
	fn message_fee_is_checked_by_verifier() {
		const EXPECTED_MINIMAL_FEE: u32 = 2640;

		// payload of the This -> Bridged chain message
		let payload = source::FromThisChainMessagePayload::<OnThisChainBridge> {
			spec_version: 1,
			weight: 100,
			origin: pallet_bridge_call_dispatch::CallOrigin::SourceRoot,
			call: vec![42],
		};

		// let's check if estimation matching hardcoded value
		assert_eq!(
			source::estimate_message_dispatch_and_delivery_fee::<OnThisChainBridge>(
				&payload,
				OnThisChainBridge::RELAYER_FEE_PERCENT,
			),
			Ok(BridgedChainBalance(EXPECTED_MINIMAL_FEE)),
		);

		// and now check that the verifier checks the fee
		assert!(
			source::FromThisChainMessageVerifier::<OnThisChainBridge>::verify_message(
				&ThisChainAccountId(0),
				&ThisChainBalance(1),
				&*b"test",
				&payload,
			)
			.is_err(),
		);
		assert!(
			source::FromThisChainMessageVerifier::<OnThisChainBridge>::verify_message(
				&ThisChainAccountId(0),
				&ThisChainBalance(1_000_000),
				&*b"test",
				&payload,
			)
			.is_ok(),
		);
	}
}
