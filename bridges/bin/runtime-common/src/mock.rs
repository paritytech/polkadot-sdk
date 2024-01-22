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

//! A mock runtime for testing different stuff in the crate.

#![cfg(test)]

use crate::messages::{
	source::{
		FromThisChainMaximalOutboundPayloadSize, FromThisChainMessagePayload,
		TargetHeaderChainAdapter,
	},
	target::{FromBridgedChainMessagePayload, SourceHeaderChainAdapter},
	BridgedChainWithMessages, HashOf, MessageBridge, ThisChainWithMessages,
};

use bp_header_chain::{ChainWithGrandpa, HeaderChain};
use bp_messages::{
	target_chain::{DispatchMessage, MessageDispatch},
	LaneId, MessageNonce,
};
use bp_parachains::SingleParaStoredHeaderDataBuilder;
use bp_relayers::PayRewardFromAccount;
use bp_runtime::{
	messages::MessageDispatchResult, Chain, ChainId, Parachain, UnderlyingChainProvider,
};
use codec::{Decode, Encode};
use frame_support::{
	derive_impl, parameter_types,
	weights::{ConstantMultiplier, IdentityFee, RuntimeDbWeight, Weight},
};
use pallet_transaction_payment::Multiplier;
use sp_runtime::{
	testing::H256,
	traits::{BlakeTwo256, ConstU32, ConstU64, ConstU8},
	FixedPointNumber, Perquintill,
};

/// Account identifier at `ThisChain`.
pub type ThisChainAccountId = u64;
/// Balance at `ThisChain`.
pub type ThisChainBalance = u64;
/// Block number at `ThisChain`.
pub type ThisChainBlockNumber = u32;
/// Hash at `ThisChain`.
pub type ThisChainHash = H256;
/// Hasher at `ThisChain`.
pub type ThisChainHasher = BlakeTwo256;
/// Runtime call at `ThisChain`.
pub type ThisChainRuntimeCall = RuntimeCall;
/// Runtime call origin at `ThisChain`.
pub type ThisChainCallOrigin = RuntimeOrigin;
/// Header of `ThisChain`.
pub type ThisChainHeader = sp_runtime::generic::Header<ThisChainBlockNumber, ThisChainHasher>;
/// Block of `ThisChain`.
pub type ThisChainBlock = frame_system::mocking::MockBlockU32<TestRuntime>;

/// Account identifier at the `BridgedChain`.
pub type BridgedChainAccountId = u128;
/// Balance at the `BridgedChain`.
pub type BridgedChainBalance = u128;
/// Block number at the `BridgedChain`.
pub type BridgedChainBlockNumber = u32;
/// Hash at the `BridgedChain`.
pub type BridgedChainHash = H256;
/// Hasher at the `BridgedChain`.
pub type BridgedChainHasher = BlakeTwo256;
/// Header of the `BridgedChain`.
pub type BridgedChainHeader =
	sp_runtime::generic::Header<BridgedChainBlockNumber, BridgedChainHasher>;

/// Rewards payment procedure.
pub type TestPaymentProcedure = PayRewardFromAccount<Balances, ThisChainAccountId>;
/// Stake that we are using in tests.
pub type TestStake = ConstU64<5_000>;
/// Stake and slash mechanism to use in tests.
pub type TestStakeAndSlash = pallet_bridge_relayers::StakeAndSlashNamed<
	ThisChainAccountId,
	ThisChainBlockNumber,
	Balances,
	ReserveId,
	TestStake,
	ConstU32<8>,
>;

/// Message lane used in tests.
pub const TEST_LANE_ID: LaneId = LaneId([0, 0, 0, 0]);
/// Bridged chain id used in tests.
pub const TEST_BRIDGED_CHAIN_ID: ChainId = *b"brdg";
/// Maximal extrinsic weight at the `BridgedChain`.
pub const BRIDGED_CHAIN_MAX_EXTRINSIC_WEIGHT: usize = 2048;
/// Maximal extrinsic size at the `BridgedChain`.
pub const BRIDGED_CHAIN_MAX_EXTRINSIC_SIZE: u32 = 1024;

frame_support::construct_runtime! {
	pub enum TestRuntime
	{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Utility: pallet_utility,
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>},
		BridgeRelayers: pallet_bridge_relayers::{Pallet, Call, Storage, Event<T>},
		BridgeGrandpa: pallet_bridge_grandpa::{Pallet, Call, Storage, Event<T>},
		BridgeParachains: pallet_bridge_parachains::{Pallet, Call, Storage, Event<T>},
		BridgeMessages: pallet_bridge_messages::{Pallet, Call, Storage, Event<T>, Config<T>},
	}
}

crate::generate_bridge_reject_obsolete_headers_and_messages! {
	ThisChainRuntimeCall, ThisChainAccountId,
	BridgeGrandpa, BridgeParachains, BridgeMessages
}

parameter_types! {
	pub const ActiveOutboundLanes: &'static [LaneId] = &[TEST_LANE_ID];
	pub const BridgedChainId: ChainId = TEST_BRIDGED_CHAIN_ID;
	pub const BridgedParasPalletName: &'static str = "Paras";
	pub const ExistentialDeposit: ThisChainBalance = 500;
	pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight { read: 1, write: 2 };
	pub const TargetBlockFullness: Perquintill = Perquintill::from_percent(25);
	pub const TransactionBaseFee: ThisChainBalance = 0;
	pub const TransactionByteFee: ThisChainBalance = 1;
	pub AdjustmentVariable: Multiplier = Multiplier::saturating_from_rational(3, 100_000);
	pub MinimumMultiplier: Multiplier = Multiplier::saturating_from_rational(1, 1_000_000u128);
	pub MaximumMultiplier: Multiplier = sp_runtime::traits::Bounded::max_value();
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: MessageNonce = 16;
	pub const MaxUnconfirmedMessagesAtInboundLane: MessageNonce = 1_000;
	pub const ReserveId: [u8; 8] = *b"brdgrlrs";
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for TestRuntime {
	type Hash = ThisChainHash;
	type Hashing = ThisChainHasher;
	type AccountId = ThisChainAccountId;
	type Block = ThisChainBlock;
	type AccountData = pallet_balances::AccountData<ThisChainBalance>;
	type BlockHashCount = ConstU32<250>;
}

impl pallet_utility::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = ();
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::DefaultConfig)]
impl pallet_balances::Config for TestRuntime {
	type ReserveIdentifier = [u8; 8];
	type AccountStore = System;
}

impl pallet_transaction_payment::Config for TestRuntime {
	type OnChargeTransaction = pallet_transaction_payment::CurrencyAdapter<Balances, ()>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = IdentityFee<ThisChainBalance>;
	type LengthToFee = ConstantMultiplier<ThisChainBalance, TransactionByteFee>;
	type FeeMultiplierUpdate = pallet_transaction_payment::TargetedFeeAdjustment<
		TestRuntime,
		TargetBlockFullness,
		AdjustmentVariable,
		MinimumMultiplier,
		MaximumMultiplier,
	>;
	type RuntimeEvent = RuntimeEvent;
}

impl pallet_bridge_grandpa::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = BridgedUnderlyingChain;
	type MaxFreeMandatoryHeadersPerBlock = ConstU32<4>;
	type HeadersToKeep = ConstU32<8>;
	type WeightInfo = pallet_bridge_grandpa::weights::BridgeWeight<TestRuntime>;
}

impl pallet_bridge_parachains::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type BridgesGrandpaPalletInstance = ();
	type ParasPalletName = BridgedParasPalletName;
	type ParaStoredHeaderDataBuilder =
		SingleParaStoredHeaderDataBuilder<BridgedUnderlyingParachain>;
	type HeadsToKeep = ConstU32<8>;
	type MaxParaHeadDataSize = ConstU32<1024>;
	type WeightInfo = pallet_bridge_parachains::weights::BridgeWeight<TestRuntime>;
}

impl pallet_bridge_messages::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_bridge_messages::weights::BridgeWeight<TestRuntime>;
	type ActiveOutboundLanes = ActiveOutboundLanes;
	type MaxUnrewardedRelayerEntriesAtInboundLane = MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaxUnconfirmedMessagesAtInboundLane = MaxUnconfirmedMessagesAtInboundLane;

	type MaximalOutboundPayloadSize = FromThisChainMaximalOutboundPayloadSize<OnThisChainBridge>;
	type OutboundPayload = FromThisChainMessagePayload;

	type InboundPayload = FromBridgedChainMessagePayload;
	type InboundRelayer = BridgedChainAccountId;
	type DeliveryPayments = ();

	type TargetHeaderChain = TargetHeaderChainAdapter<OnThisChainBridge>;
	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		TestRuntime,
		(),
		ConstU64<100_000>,
	>;
	type OnMessagesDelivered = ();

	type SourceHeaderChain = SourceHeaderChainAdapter<OnThisChainBridge>;
	type MessageDispatch = DummyMessageDispatch;
	type BridgedChainId = BridgedChainId;
}

impl pallet_bridge_relayers::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type Reward = ThisChainBalance;
	type PaymentProcedure = TestPaymentProcedure;
	type StakeAndSlash = TestStakeAndSlash;
	type WeightInfo = ();
}

/// Dummy message dispatcher.
pub struct DummyMessageDispatch;

impl DummyMessageDispatch {
	pub fn deactivate() {
		frame_support::storage::unhashed::put(&b"inactive"[..], &false);
	}
}

impl MessageDispatch for DummyMessageDispatch {
	type DispatchPayload = Vec<u8>;
	type DispatchLevelResult = ();

	fn is_active() -> bool {
		frame_support::storage::unhashed::take::<bool>(&b"inactive"[..]) != Some(false)
	}

	fn dispatch_weight(_message: &mut DispatchMessage<Self::DispatchPayload>) -> Weight {
		Weight::zero()
	}

	fn dispatch(
		_: DispatchMessage<Self::DispatchPayload>,
	) -> MessageDispatchResult<Self::DispatchLevelResult> {
		MessageDispatchResult { unspent_weight: Weight::zero(), dispatch_level_result: () }
	}
}

/// Bridge that is deployed on `ThisChain` and allows sending/receiving messages to/from
/// `BridgedChain`.
#[derive(Debug, PartialEq, Eq)]
pub struct OnThisChainBridge;

impl MessageBridge for OnThisChainBridge {
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str = "";

	type ThisChain = ThisChain;
	type BridgedChain = BridgedChain;
	type BridgedHeaderChain = pallet_bridge_grandpa::GrandpaChainHeaders<TestRuntime, ()>;
}

/// Bridge that is deployed on `BridgedChain` and allows sending/receiving messages to/from
/// `ThisChain`.
#[derive(Debug, PartialEq, Eq)]
pub struct OnBridgedChainBridge;

impl MessageBridge for OnBridgedChainBridge {
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str = "";

	type ThisChain = BridgedChain;
	type BridgedChain = ThisChain;
	type BridgedHeaderChain = ThisHeaderChain;
}

/// Dummy implementation of `HeaderChain` for `ThisChain` at the `BridgedChain`.
pub struct ThisHeaderChain;

impl HeaderChain<ThisUnderlyingChain> for ThisHeaderChain {
	fn finalized_header_state_root(_hash: HashOf<ThisChain>) -> Option<HashOf<ThisChain>> {
		unreachable!()
	}
}

/// Call origin at `BridgedChain`.
#[derive(Clone, Debug)]
pub struct BridgedChainOrigin;

impl From<BridgedChainOrigin>
	for Result<frame_system::RawOrigin<BridgedChainAccountId>, BridgedChainOrigin>
{
	fn from(
		_origin: BridgedChainOrigin,
	) -> Result<frame_system::RawOrigin<BridgedChainAccountId>, BridgedChainOrigin> {
		unreachable!()
	}
}

/// Underlying chain of `ThisChain`.
pub struct ThisUnderlyingChain;

impl Chain for ThisUnderlyingChain {
	const ID: ChainId = *b"tuch";

	type BlockNumber = ThisChainBlockNumber;
	type Hash = ThisChainHash;
	type Hasher = ThisChainHasher;
	type Header = ThisChainHeader;
	type AccountId = ThisChainAccountId;
	type Balance = ThisChainBalance;
	type Nonce = u32;
	type Signature = sp_runtime::MultiSignature;

	fn max_extrinsic_size() -> u32 {
		BRIDGED_CHAIN_MAX_EXTRINSIC_SIZE
	}

	fn max_extrinsic_weight() -> Weight {
		Weight::zero()
	}
}

/// The chain where we are in tests.
pub struct ThisChain;

impl UnderlyingChainProvider for ThisChain {
	type Chain = ThisUnderlyingChain;
}

impl ThisChainWithMessages for ThisChain {
	type RuntimeOrigin = ThisChainCallOrigin;
}

impl BridgedChainWithMessages for ThisChain {}

/// Underlying chain of `BridgedChain`.
pub struct BridgedUnderlyingChain;
/// Some parachain under `BridgedChain` consensus.
pub struct BridgedUnderlyingParachain;
/// Runtime call of the `BridgedChain`.
#[derive(Decode, Encode)]
pub struct BridgedChainCall;

impl Chain for BridgedUnderlyingChain {
	const ID: ChainId = *b"buch";

	type BlockNumber = BridgedChainBlockNumber;
	type Hash = BridgedChainHash;
	type Hasher = BridgedChainHasher;
	type Header = BridgedChainHeader;
	type AccountId = BridgedChainAccountId;
	type Balance = BridgedChainBalance;
	type Nonce = u32;
	type Signature = sp_runtime::MultiSignature;

	fn max_extrinsic_size() -> u32 {
		BRIDGED_CHAIN_MAX_EXTRINSIC_SIZE
	}
	fn max_extrinsic_weight() -> Weight {
		Weight::zero()
	}
}

impl ChainWithGrandpa for BridgedUnderlyingChain {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = "";
	const MAX_AUTHORITIES_COUNT: u32 = 16;
	const REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY: u32 = 8;
	const MAX_MANDATORY_HEADER_SIZE: u32 = 256;
	const AVERAGE_HEADER_SIZE: u32 = 64;
}

impl Chain for BridgedUnderlyingParachain {
	const ID: ChainId = *b"bupc";

	type BlockNumber = BridgedChainBlockNumber;
	type Hash = BridgedChainHash;
	type Hasher = BridgedChainHasher;
	type Header = BridgedChainHeader;
	type AccountId = BridgedChainAccountId;
	type Balance = BridgedChainBalance;
	type Nonce = u32;
	type Signature = sp_runtime::MultiSignature;

	fn max_extrinsic_size() -> u32 {
		BRIDGED_CHAIN_MAX_EXTRINSIC_SIZE
	}
	fn max_extrinsic_weight() -> Weight {
		Weight::zero()
	}
}

impl Parachain for BridgedUnderlyingParachain {
	const PARACHAIN_ID: u32 = 42;
}

/// The other, bridged chain, used in tests.
pub struct BridgedChain;

impl UnderlyingChainProvider for BridgedChain {
	type Chain = BridgedUnderlyingChain;
}

impl ThisChainWithMessages for BridgedChain {
	type RuntimeOrigin = BridgedChainOrigin;
}

impl BridgedChainWithMessages for BridgedChain {}

/// Run test within test externalities.
pub fn run_test(test: impl FnOnce()) {
	sp_io::TestExternalities::new(Default::default()).execute_with(test)
}
