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

use crate::Runtime;

use bp_messages::{
	source_chain::TargetHeaderChain,
	target_chain::{ProvedMessages, SourceHeaderChain},
	InboundLaneData, LaneId, Message, MessageNonce, Parameter as MessagesParameter,
};
use bp_polkadot_core::parachains::ParaId;
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

/// Default lane that is used to send messages to Rialto parachain.
pub const DEFAULT_XCM_LANE_TO_RIALTO_PARACHAIN: LaneId = [0, 0, 0, 0];
/// Weight of 2 XCM instructions is for simple `Trap(42)` program, coming through bridge
/// (it is prepended with `UniversalOrigin` instruction). It is used just for simplest manual
/// tests, confirming that we don't break encoding somewhere between.
pub const BASE_XCM_WEIGHT_TWICE: Weight = 2 * crate::xcm_config::BASE_XCM_WEIGHT;

/// Initial value of `RialtoParachainToMillauConversionRate` parameter.
pub const INITIAL_RIALTO_PARACHAIN_TO_MILLAU_CONVERSION_RATE: FixedU128 =
	FixedU128::from_inner(FixedU128::DIV);
/// Initial value of `RialtoParachainFeeMultiplier` parameter.
pub const INITIAL_RIALTO_PARACHAIN_FEE_MULTIPLIER: FixedU128 =
	FixedU128::from_inner(FixedU128::DIV);

parameter_types! {
	/// RialtoParachain to Millau conversion rate. Initially we treat both tokens as equal.
	pub storage RialtoParachainToMillauConversionRate: FixedU128 = INITIAL_RIALTO_PARACHAIN_TO_MILLAU_CONVERSION_RATE;
	/// Fee multiplier value at RialtoParachain chain.
	pub storage RialtoParachainFeeMultiplier: FixedU128 = INITIAL_RIALTO_PARACHAIN_FEE_MULTIPLIER;
}

/// Message payload for Millau -> RialtoParachain messages.
pub type ToRialtoParachainMessagePayload = messages::source::FromThisChainMessagePayload;

/// Message verifier for Millau -> RialtoParachain messages.
pub type ToRialtoParachainMessageVerifier =
	messages::source::FromThisChainMessageVerifier<WithRialtoParachainMessageBridge>;

/// Message payload for RialtoParachain -> Millau messages.
pub type FromRialtoParachainMessagePayload =
	messages::target::FromBridgedChainMessagePayload<crate::Call>;

/// Messages proof for RialtoParachain -> Millau messages.
type FromRialtoParachainMessagesProof =
	messages::target::FromBridgedChainMessagesProof<bp_rialto_parachain::Hash>;

/// Messages delivery proof for Millau -> RialtoParachain messages.
type ToRialtoParachainMessagesDeliveryProof =
	messages::source::FromBridgedChainMessagesDeliveryProof<bp_rialto_parachain::Hash>;

/// Call-dispatch based message dispatch for RialtoParachain -> Millau messages.
pub type FromRialtoParachainMessageDispatch = messages::target::FromBridgedChainMessageDispatch<
	WithRialtoParachainMessageBridge,
	xcm_executor::XcmExecutor<crate::xcm_config::XcmConfig>,
	crate::xcm_config::XcmWeigher,
	// 2 XCM instructions is for simple `Trap(42)` program, coming through bridge
	// (it is prepended with `UniversalOrigin` instruction)
	frame_support::traits::ConstU64<BASE_XCM_WEIGHT_TWICE>,
>;

/// Maximal outbound payload size of Millau -> RialtoParachain messages.
pub type ToRialtoParachainMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithRialtoParachainMessageBridge>;

/// Millau <-> RialtoParachain message bridge.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct WithRialtoParachainMessageBridge;

impl MessageBridge for WithRialtoParachainMessageBridge {
	const RELAYER_FEE_PERCENT: u32 = 10;
	const THIS_CHAIN_ID: ChainId = MILLAU_CHAIN_ID;
	const BRIDGED_CHAIN_ID: ChainId = RIALTO_PARACHAIN_CHAIN_ID;
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str = bp_millau::WITH_MILLAU_MESSAGES_PALLET_NAME;

	type ThisChain = Millau;
	type BridgedChain = RialtoParachain;

	fn bridged_balance_to_this_balance(
		bridged_balance: bp_rialto_parachain::Balance,
		bridged_to_this_conversion_rate_override: Option<FixedU128>,
	) -> bp_millau::Balance {
		let conversion_rate = bridged_to_this_conversion_rate_override
			.unwrap_or_else(RialtoParachainToMillauConversionRate::get);
		bp_millau::Balance::try_from(conversion_rate.saturating_mul_int(bridged_balance))
			.unwrap_or(bp_millau::Balance::MAX)
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

impl messages::ThisChainWithMessages for Millau {
	type Call = crate::Call;
	type Origin = crate::Origin;
	type ConfirmationTransactionEstimation = BasicConfirmationTransactionEstimation<
		Self::AccountId,
		{ bp_millau::MAX_SINGLE_MESSAGE_DELIVERY_CONFIRMATION_TX_WEIGHT },
		{ bp_rialto_parachain::EXTRA_STORAGE_PROOF_SIZE },
		{ bp_millau::TX_EXTRA_BYTES },
	>;

	fn is_message_accepted(_send_origin: &Self::Origin, lane: &LaneId) -> bool {
		*lane == DEFAULT_XCM_LANE_TO_RIALTO_PARACHAIN || *lane == [0, 0, 0, 1]
	}

	fn maximal_pending_messages_at_outbound_lane() -> MessageNonce {
		MessageNonce::MAX
	}

	fn transaction_payment(transaction: MessageTransaction<Weight>) -> bp_millau::Balance {
		// `transaction` may represent transaction from the future, when multiplier value will
		// be larger, so let's use slightly increased value
		let multiplier = FixedU128::saturating_from_rational(110, 100)
			.saturating_mul(pallet_transaction_payment::Pallet::<Runtime>::next_fee_multiplier());
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

impl messages::BridgedChainWithMessages for RialtoParachain {
	fn maximal_extrinsic_size() -> u32 {
		bp_rialto_parachain::RialtoParachain::max_extrinsic_size()
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
				.saturating_mul(bp_rialto_parachain::ADDITIONAL_MESSAGE_BYTE_DELIVERY_WEIGHT)
				.saturating_add(bp_rialto_parachain::DEFAULT_MESSAGE_DELIVERY_TX_WEIGHT)
				.saturating_sub(if include_pay_dispatch_fee_cost {
					0
				} else {
					bp_rialto_parachain::PAY_INBOUND_DISPATCH_FEE_WEIGHT
				})
				.saturating_add(message_dispatch_weight),
			size: message_payload_len
				.saturating_add(bp_millau::EXTRA_STORAGE_PROOF_SIZE)
				.saturating_add(bp_rialto_parachain::TX_EXTRA_BYTES),
		}
	}

	fn transaction_payment(
		transaction: MessageTransaction<Weight>,
	) -> bp_rialto_parachain::Balance {
		// we don't have a direct access to the value of multiplier at RialtoParachain chain
		// => it is a messages module parameter
		let multiplier = RialtoParachainFeeMultiplier::get();
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

impl TargetHeaderChain<ToRialtoParachainMessagePayload, bp_millau::AccountId> for RialtoParachain {
	type Error = &'static str;
	// The proof is:
	// - hash of the header this proof has been created with;
	// - the storage proof or one or several keys;
	// - id of the lane we prove state of.
	type MessagesDeliveryProof = ToRialtoParachainMessagesDeliveryProof;

	fn verify_message(payload: &ToRialtoParachainMessagePayload) -> Result<(), Self::Error> {
		messages::source::verify_chain_message::<WithRialtoParachainMessageBridge>(payload)
	}

	fn verify_messages_delivery_proof(
		proof: Self::MessagesDeliveryProof,
	) -> Result<(LaneId, InboundLaneData<bp_millau::AccountId>), Self::Error> {
		messages::source::verify_messages_delivery_proof_from_parachain::<
			WithRialtoParachainMessageBridge,
			bp_rialto_parachain::Header,
			Runtime,
			crate::WithRialtoParachainsInstance,
		>(ParaId(bp_rialto_parachain::RIALTO_PARACHAIN_ID), proof)
	}
}

impl SourceHeaderChain<bp_rialto_parachain::Balance> for RialtoParachain {
	type Error = &'static str;
	// The proof is:
	// - hash of the header this proof has been created with;
	// - the storage proof or one or several keys;
	// - id of the lane we prove messages for;
	// - inclusive range of messages nonces that are proved.
	type MessagesProof = FromRialtoParachainMessagesProof;

	fn verify_messages_proof(
		proof: Self::MessagesProof,
		messages_count: u32,
	) -> Result<ProvedMessages<Message<bp_rialto_parachain::Balance>>, Self::Error> {
		messages::target::verify_messages_proof_from_parachain::<
			WithRialtoParachainMessageBridge,
			bp_rialto_parachain::Header,
			Runtime,
			crate::WithRialtoParachainsInstance,
		>(ParaId(bp_rialto_parachain::RIALTO_PARACHAIN_ID), proof, messages_count)
	}
}

/// Millau -> RialtoParachain message lane pallet parameters.
#[derive(RuntimeDebug, Clone, Encode, Decode, PartialEq, Eq, TypeInfo)]
pub enum MillauToRialtoParachainMessagesParameter {
	/// The conversion formula we use is: `MillauTokens = RialtoParachainTokens * conversion_rate`.
	RialtoParachainToMillauConversionRate(FixedU128),
}

impl MessagesParameter for MillauToRialtoParachainMessagesParameter {
	fn save(&self) {
		match *self {
			MillauToRialtoParachainMessagesParameter::RialtoParachainToMillauConversionRate(
				ref conversion_rate,
			) => RialtoParachainToMillauConversionRate::set(conversion_rate),
		}
	}
}
