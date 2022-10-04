// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! Everything required to serve Millau <-> RialtoParachain messages.

// TODO: this is almost exact copy of `millau_messages.rs` from Rialto runtime.
// Should be extracted to a separate crate and reused here.

use crate::{OriginCaller, Runtime};

use bp_messages::{
	source_chain::{SenderOrigin, TargetHeaderChain},
	target_chain::{ProvedMessages, SourceHeaderChain},
	InboundLaneData, LaneId, Message, MessageNonce, Parameter as MessagesParameter,
};
use bp_runtime::{Chain, ChainId, MILLAU_CHAIN_ID, RIALTO_PARACHAIN_CHAIN_ID};
use bridge_runtime_common::messages::{
	self, BasicConfirmationTransactionEstimation, MessageBridge, MessageTransaction,
};
use codec::{Decode, Encode};
use frame_support::{
	parameter_types,
	weights::{DispatchClass, Weight},
	RuntimeDebug,
};
use scale_info::TypeInfo;
use sp_runtime::{traits::Saturating, FixedPointNumber, FixedU128};
use sp_std::convert::TryFrom;

/// Default lane that is used to send messages to Millau.
pub const DEFAULT_XCM_LANE_TO_MILLAU: LaneId = [0, 0, 0, 0];
/// Initial value of `MillauToRialtoParachainConversionRate` parameter.
pub const INITIAL_MILLAU_TO_RIALTO_PARACHAIN_CONVERSION_RATE: FixedU128 =
	FixedU128::from_inner(FixedU128::DIV);
/// Initial value of `MillauFeeMultiplier` parameter.
pub const INITIAL_MILLAU_FEE_MULTIPLIER: FixedU128 = FixedU128::from_inner(FixedU128::DIV);
/// Weight of 2 XCM instructions is for simple `Trap(42)` program, coming through bridge
/// (it is prepended with `UniversalOrigin` instruction). It is used just for simplest manual
/// tests, confirming that we don't break encoding somewhere between.
pub const BASE_XCM_WEIGHT_TWICE: Weight = 2 * crate::BASE_XCM_WEIGHT;

parameter_types! {
	/// Millau to RialtoParachain conversion rate. Initially we treat both tokens as equal.
	pub storage MillauToRialtoParachainConversionRate: FixedU128 = INITIAL_MILLAU_TO_RIALTO_PARACHAIN_CONVERSION_RATE;
	/// Fee multiplier value at Millau chain.
	pub storage MillauFeeMultiplier: FixedU128 = INITIAL_MILLAU_FEE_MULTIPLIER;
}

/// Message payload for RialtoParachain -> Millau messages.
pub type ToMillauMessagePayload = messages::source::FromThisChainMessagePayload;

/// Message verifier for RialtoParachain -> Millau messages.
pub type ToMillauMessageVerifier =
	messages::source::FromThisChainMessageVerifier<WithMillauMessageBridge>;

/// Message payload for Millau -> RialtoParachain messages.
pub type FromMillauMessagePayload = messages::target::FromBridgedChainMessagePayload<crate::Call>;

/// Call-dispatch based message dispatch for Millau -> RialtoParachain messages.
pub type FromMillauMessageDispatch = messages::target::FromBridgedChainMessageDispatch<
	WithMillauMessageBridge,
	xcm_executor::XcmExecutor<crate::XcmConfig>,
	crate::XcmWeigher,
	// 2 XCM instructions is for simple `Trap(42)` program, coming through bridge
	// (it is prepended with `UniversalOrigin` instruction)
	frame_support::traits::ConstU64<BASE_XCM_WEIGHT_TWICE>,
>;

/// Messages proof for Millau -> RialtoParachain messages.
pub type FromMillauMessagesProof = messages::target::FromBridgedChainMessagesProof<bp_millau::Hash>;

/// Messages delivery proof for RialtoParachain -> Millau messages.
pub type ToMillauMessagesDeliveryProof =
	messages::source::FromBridgedChainMessagesDeliveryProof<bp_millau::Hash>;

/// Maximal outbound payload size of Rialto -> Millau messages.
pub type ToMillauMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithMillauMessageBridge>;

/// Millau <-> RialtoParachain message bridge.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct WithMillauMessageBridge;

impl MessageBridge for WithMillauMessageBridge {
	const RELAYER_FEE_PERCENT: u32 = 10;
	const THIS_CHAIN_ID: ChainId = RIALTO_PARACHAIN_CHAIN_ID;
	const BRIDGED_CHAIN_ID: ChainId = MILLAU_CHAIN_ID;
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str =
		bp_rialto_parachain::WITH_RIALTO_PARACHAIN_MESSAGES_PALLET_NAME;

	type ThisChain = RialtoParachain;
	type BridgedChain = Millau;

	fn bridged_balance_to_this_balance(
		bridged_balance: bp_millau::Balance,
		bridged_to_this_conversion_rate_override: Option<FixedU128>,
	) -> bp_rialto_parachain::Balance {
		let conversion_rate = bridged_to_this_conversion_rate_override
			.unwrap_or_else(MillauToRialtoParachainConversionRate::get);
		bp_rialto_parachain::Balance::try_from(conversion_rate.saturating_mul_int(bridged_balance))
			.unwrap_or(bp_rialto_parachain::Balance::MAX)
	}
}

/// RialtoParachain chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct RialtoParachain;

impl messages::ChainWithMessages for RialtoParachain {
	type Hash = bp_rialto_parachain::Hash;
	type AccountId = bp_rialto_parachain::AccountId;
	type Signer = bp_rialto_parachain::AccountSigner;
	type Signature = bp_rialto_parachain::Signature;
	type Weight = Weight;
	type Balance = bp_rialto_parachain::Balance;
}

impl messages::ThisChainWithMessages for RialtoParachain {
	type Call = crate::Call;
	type Origin = crate::Origin;
	type ConfirmationTransactionEstimation = BasicConfirmationTransactionEstimation<
		Self::AccountId,
		{ bp_rialto_parachain::MAX_SINGLE_MESSAGE_DELIVERY_CONFIRMATION_TX_WEIGHT },
		{ bp_millau::EXTRA_STORAGE_PROOF_SIZE },
		{ bp_rialto_parachain::TX_EXTRA_BYTES },
	>;

	fn is_message_accepted(send_origin: &Self::Origin, lane: &LaneId) -> bool {
		let here_location = xcm::v3::MultiLocation::from(crate::UniversalLocation::get());
		match send_origin.caller {
			OriginCaller::PolkadotXcm(pallet_xcm::Origin::Xcm(ref location))
				if *location == here_location =>
			{
				log::trace!(target: "runtime::bridge", "Verifying message sent using XCM pallet to Millau");
			},
			_ => {
				// keep in mind that in this case all messages are free (in term of fees)
				// => it's just to keep testing bridge on our test deployments until we'll have a
				// better option
				log::trace!(target: "runtime::bridge", "Verifying message sent using messages pallet to Millau");
			},
		}

		*lane == [0, 0, 0, 0] || *lane == [0, 0, 0, 1]
	}

	fn maximal_pending_messages_at_outbound_lane() -> MessageNonce {
		MessageNonce::MAX
	}

	fn transaction_payment(
		transaction: MessageTransaction<Weight>,
	) -> bp_rialto_parachain::Balance {
		// `transaction` may represent transaction from the future, when multiplier value will
		// be larger, so let's use slightly increased value
		let multiplier = FixedU128::saturating_from_rational(110, 100)
			.saturating_mul(pallet_transaction_payment::Pallet::<Runtime>::next_fee_multiplier());
		// in our testnets, both per-byte fee and weight-to-fee are 1:1
		messages::transaction_payment(
			bp_rialto_parachain::BlockWeights::get()
				.get(DispatchClass::Normal)
				.base_extrinsic,
			1,
			multiplier,
			|weight| weight as _,
			transaction,
		)
	}
}

/// Millau chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct Millau;

impl messages::ChainWithMessages for Millau {
	type Hash = bp_millau::Hash;
	type AccountId = bp_millau::AccountId;
	type Signer = bp_millau::AccountSigner;
	type Signature = bp_millau::Signature;
	type Weight = Weight;
	type Balance = bp_millau::Balance;
}

impl messages::BridgedChainWithMessages for Millau {
	fn maximal_extrinsic_size() -> u32 {
		bp_millau::Millau::max_extrinsic_size()
	}

	fn verify_dispatch_weight(_message_payload: &[u8]) -> bool {
		true
	}

	fn estimate_delivery_transaction(
		message_payload: &[u8],
		include_pay_dispatch_fee_cost: bool,
		message_dispatch_weight: Weight,
	) -> MessageTransaction<Weight> {
		let message_payload_len = u32::try_from(message_payload.len()).unwrap_or(u32::MAX);
		let extra_bytes_in_payload = Weight::from(message_payload_len)
			.saturating_sub(pallet_bridge_messages::EXPECTED_DEFAULT_MESSAGE_LENGTH.into());

		MessageTransaction {
			dispatch_weight: extra_bytes_in_payload
				.saturating_mul(bp_millau::ADDITIONAL_MESSAGE_BYTE_DELIVERY_WEIGHT)
				.saturating_add(bp_millau::DEFAULT_MESSAGE_DELIVERY_TX_WEIGHT)
				.saturating_sub(if include_pay_dispatch_fee_cost {
					0
				} else {
					bp_millau::PAY_INBOUND_DISPATCH_FEE_WEIGHT
				})
				.saturating_add(message_dispatch_weight),
			size: message_payload_len
				.saturating_add(bp_rialto_parachain::EXTRA_STORAGE_PROOF_SIZE)
				.saturating_add(bp_millau::TX_EXTRA_BYTES),
		}
	}

	fn transaction_payment(transaction: MessageTransaction<Weight>) -> bp_millau::Balance {
		// we don't have a direct access to the value of multiplier at Millau chain
		// => it is a messages module parameter
		let multiplier = MillauFeeMultiplier::get();
		// in our testnets, both per-byte fee and weight-to-fee are 1:1
		messages::transaction_payment(
			bp_millau::BlockWeights::get().get(DispatchClass::Normal).base_extrinsic,
			1,
			multiplier,
			|weight| weight as _,
			transaction,
		)
	}
}

impl TargetHeaderChain<ToMillauMessagePayload, bp_rialto_parachain::AccountId> for Millau {
	type Error = &'static str;
	// The proof is:
	// - hash of the header this proof has been created with;
	// - the storage proof of one or several keys;
	// - id of the lane we prove state of.
	type MessagesDeliveryProof = ToMillauMessagesDeliveryProof;

	fn verify_message(payload: &ToMillauMessagePayload) -> Result<(), Self::Error> {
		messages::source::verify_chain_message::<WithMillauMessageBridge>(payload)
	}

	fn verify_messages_delivery_proof(
		proof: Self::MessagesDeliveryProof,
	) -> Result<(LaneId, InboundLaneData<bp_rialto_parachain::AccountId>), Self::Error> {
		messages::source::verify_messages_delivery_proof::<
			WithMillauMessageBridge,
			Runtime,
			crate::MillauGrandpaInstance,
		>(proof)
	}
}

impl SourceHeaderChain<bp_millau::Balance> for Millau {
	type Error = &'static str;
	// The proof is:
	// - hash of the header this proof has been created with;
	// - the storage proof of one or several keys;
	// - id of the lane we prove messages for;
	// - inclusive range of messages nonces that are proved.
	type MessagesProof = FromMillauMessagesProof;

	fn verify_messages_proof(
		proof: Self::MessagesProof,
		messages_count: u32,
	) -> Result<ProvedMessages<Message<bp_millau::Balance>>, Self::Error> {
		messages::target::verify_messages_proof::<
			WithMillauMessageBridge,
			Runtime,
			crate::MillauGrandpaInstance,
		>(proof, messages_count)
	}
}

impl SenderOrigin<crate::AccountId> for crate::Origin {
	fn linked_account(&self) -> Option<crate::AccountId> {
		match self.caller {
			crate::OriginCaller::system(frame_system::RawOrigin::Signed(ref submitter)) =>
				Some(submitter.clone()),
			_ => None,
		}
	}
}

/// RialtoParachain -> Millau message lane pallet parameters.
#[derive(RuntimeDebug, Clone, Encode, Decode, PartialEq, Eq, TypeInfo)]
pub enum RialtoParachainToMillauMessagesParameter {
	/// The conversion formula we use is: `RialtoParachainTokens = MillauTokens * conversion_rate`.
	MillauToRialtoParachainConversionRate(FixedU128),
}

impl MessagesParameter for RialtoParachainToMillauMessagesParameter {
	fn save(&self) {
		match *self {
			RialtoParachainToMillauMessagesParameter::MillauToRialtoParachainConversionRate(
				ref conversion_rate,
			) => MillauToRialtoParachainConversionRate::set(conversion_rate),
		}
	}
}
