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

#![cfg(test)]

use crate as pallet_xcm_bridge;
use bp_messages::{
	target_chain::{DispatchMessage, MessageDispatch},
	ChainWithMessages, HashedLaneId, MessageNonce,
};
use bp_runtime::{messages::MessageDispatchResult, Chain, ChainId, HashOf};
use bp_xcm_bridge::{BridgeId, BridgeLocations, LocalXcmChannelManager};
use codec::Encode;
use frame_support::{
	assert_ok, derive_impl, parameter_types,
	traits::{fungible::Mutate, EitherOf, EnsureOrigin, Equals, Everything, Get, OriginTrait},
	weights::RuntimeDbWeight,
};
use frame_system::{EnsureNever, EnsureRoot, EnsureRootWithSuccess};
use pallet_xcm_bridge::congestion::{
	BlobDispatcherWithChannelStatus, CongestionLimits, HereOrLocalConsensusXcmChannelManager,
	UpdateBridgeStatusXcmChannelManager,
};
use polkadot_parachain_primitives::primitives::Sibling;
use polkadot_runtime_common::xcm_sender::NoPriceForMessageDelivery;
use sp_core::H256;
use sp_runtime::{
	testing::Header as SubstrateHeader,
	traits::{BlakeTwo256, ConstU32, Convert, IdentityLookup},
	AccountId32, BuildStorage, StateVersion,
};
use sp_std::{cell::RefCell, marker::PhantomData};
use xcm::{latest::ROCOCO_GENESIS_HASH, prelude::*};
use xcm_builder::{
	AllowUnpaidExecutionFrom, DispatchBlob, DispatchBlobError, FixedWeightBounds,
	InspectMessageQueues, LocalExporter, NetworkExportTable, NetworkExportTableItem,
	ParentIsPreset, SiblingParachainConvertsVia, SovereignPaidRemoteExporter,
};
use xcm_executor::{
	traits::{ConvertLocation, ConvertOrigin},
	XcmExecutor,
};

pub type AccountId = AccountId32;
pub type Balance = u64;
type Block = frame_system::mocking::MockBlock<TestRuntime>;

/// Lane identifier type used for tests.
pub type TestLaneIdType = HashedLaneId;

pub const SIBLING_ASSET_HUB_ID: u32 = 2001;
pub const THIS_BRIDGE_HUB_ID: u32 = 2002;
pub const BRIDGED_ASSET_HUB_ID: u32 = 1001;

frame_support::construct_runtime! {
	pub enum TestRuntime {
		System: frame_system,
		Balances: pallet_balances,
		Messages: pallet_bridge_messages,
		XcmOverBridge: pallet_xcm_bridge,
		XcmOverBridgeWrappedWithExportMessageRouter: pallet_xcm_bridge_router = 57,
		XcmOverBridgeByExportXcmRouter: pallet_xcm_bridge_router::<Instance2> = 69,
	}
}

parameter_types! {
	pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight { read: 1, write: 2 };
	pub const ExistentialDeposit: Balance = 1;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for TestRuntime {
	type AccountId = AccountId;
	type AccountData = pallet_balances::AccountData<Balance>;
	type Block = Block;
	type Lookup = IdentityLookup<Self::AccountId>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for TestRuntime {
	type AccountStore = System;
}

impl pallet_bridge_messages::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = TestMessagesWeights;

	type ThisChain = ThisUnderlyingChain;
	type BridgedChain = BridgedUnderlyingChain;
	type BridgedHeaderChain = BridgedHeaderChain;

	type OutboundPayload = Vec<u8>;
	type InboundPayload = Vec<u8>;
	type LaneId = TestLaneIdType;

	type DeliveryPayments = ();
	type DeliveryConfirmationPayments = ();
	type OnMessagesDelivered = ();

	type MessageDispatch = TestMessageDispatch;
}

pub struct TestMessagesWeights;

impl pallet_bridge_messages::WeightInfo for TestMessagesWeights {
	fn receive_single_message_proof() -> Weight {
		Weight::zero()
	}
	fn receive_n_messages_proof(_: u32) -> Weight {
		Weight::zero()
	}
	fn receive_single_message_proof_with_outbound_lane_state() -> Weight {
		Weight::zero()
	}
	fn receive_single_n_bytes_message_proof(_: u32) -> Weight {
		Weight::zero()
	}
	fn receive_delivery_proof_for_single_message() -> Weight {
		Weight::zero()
	}
	fn receive_delivery_proof_for_two_messages_by_single_relayer() -> Weight {
		Weight::zero()
	}
	fn receive_delivery_proof_for_two_messages_by_two_relayers() -> Weight {
		Weight::zero()
	}
	fn receive_single_n_bytes_message_proof_with_dispatch(_n: u32) -> Weight {
		Weight::from_parts(1, 0)
	}
}

impl pallet_bridge_messages::WeightInfoExt for TestMessagesWeights {
	fn expected_extra_storage_proof_size() -> u32 {
		0
	}

	fn receive_messages_proof_overhead_from_runtime() -> Weight {
		Weight::zero()
	}

	fn receive_messages_delivery_proof_overhead_from_runtime() -> Weight {
		Weight::zero()
	}
}

parameter_types! {
	pub const HereLocation: Location = Location::here();
	pub const RelayNetwork: NetworkId = NetworkId::Kusama;
	pub UniversalLocation: InteriorLocation = [
		GlobalConsensus(RelayNetwork::get()),
		Parachain(THIS_BRIDGE_HUB_ID),
	].into();
	pub SiblingLocation: Location = Location::new(1, [Parachain(SIBLING_ASSET_HUB_ID)]);
	pub SiblingUniversalLocation: InteriorLocation = [GlobalConsensus(RelayNetwork::get()), Parachain(SIBLING_ASSET_HUB_ID)].into();

	pub const BridgedRelayNetwork: NetworkId = NetworkId::ByGenesis([1; 32]);
	pub BridgedRelayNetworkLocation: Location = (Parent, GlobalConsensus(BridgedRelayNetwork::get())).into();
	pub BridgedRelativeDestination: InteriorLocation = [Parachain(BRIDGED_ASSET_HUB_ID)].into();
	pub BridgedUniversalDestination: InteriorLocation = [GlobalConsensus(BridgedRelayNetwork::get()), Parachain(BRIDGED_ASSET_HUB_ID)].into();
	pub const NonBridgedRelayNetwork: NetworkId = NetworkId::ByGenesis(ROCOCO_GENESIS_HASH);

	pub const BridgeDeposit: Balance = 100_000;

	// configuration for pallet_xcm_bridge_hub_router
	pub BridgeHubLocation: Location = Here.into();
	pub BridgeTable: Vec<NetworkExportTableItem>
		= vec![
			NetworkExportTableItem::new(
				BridgedRelayNetwork::get(),
				None,
				BridgeHubLocation::get(),
				None
			)
		];
	pub UnitWeightCost: Weight = Weight::from_parts(10, 10);
	pub storage TestCongestionLimits: CongestionLimits = CongestionLimits::default();
}

/// **Universal** `InteriorLocation` of bridged asset hub.
pub fn bridged_asset_hub_universal_location() -> InteriorLocation {
	BridgedUniversalDestination::get()
}

pub(crate) type TestLocalXcmChannelManager = TestingLocalXcmChannelManager<
	BridgeId,
	HereOrLocalConsensusXcmChannelManager<
		BridgeId,
		// handles congestion for `XcmOverBridgeByExportXcmRouter`
		XcmOverBridgeByExportXcmRouter,
		// handles congestion for `XcmOverBridgeWrappedWithExportMessageRouter`
		UpdateBridgeStatusXcmChannelManager<
			TestRuntime,
			(),
			UpdateBridgeStatusXcmProvider,
			FromBridgeHubLocationXcmSender<ExecuteXcmOverSendXcm>,
		>,
	>,
>;

impl pallet_xcm_bridge::Config for TestRuntime {
	type WeightInfo = ();

	type UniversalLocation = UniversalLocation;
	type BridgedNetwork = BridgedRelayNetworkLocation;
	type BridgeMessagesPalletInstance = ();

	type MessageExportPrice = NoPriceForMessageDelivery<BridgeId>;
	type DestinationVersion = AlwaysLatest;

	type ForceOrigin = EnsureNever<()>;
	type OpenBridgeOrigin = EitherOf<
		// We want to translate `RuntimeOrigin::root()` to the `Location::here()`
		EnsureRootWithSuccess<AccountId, HereLocation>,
		OpenBridgeOrigin,
	>;
	type BridgeOriginAccountIdConverter = LocationToAccountId;

	type BridgeDeposit = BridgeDeposit;
	type Currency = Balances;
	type RuntimeHoldReason = RuntimeHoldReason;
	type AllowWithoutBridgeDeposit = (Equals<ParentRelayChainLocation>, Equals<HereLocation>);

	type LocalXcmChannelManager = TestLocalXcmChannelManager;
	type BlobDispatcher = BlobDispatcherWithChannelStatus<TestBlobDispatcher, TestBlobDispatcher>;
	type CongestionLimits = TestCongestionLimits;
}

#[cfg(feature = "runtime-benchmarks")]
impl crate::benchmarking::Config<()> for TestRuntime {
	fn open_bridge_origin() -> Option<(RuntimeOrigin, Balance)> {
		Some((OpenBridgeOrigin::sibling_parachain_origin(), ExistentialDeposit::get()))
	}
}

/// A router instance simulates a scenario where the router is deployed on a different chain than
/// the `MessageExporter`. This means that the router sends an `ExportMessage`.
pub type XcmOverBridgeWrappedWithExportMessageRouterInstance = ();
#[derive_impl(pallet_xcm_bridge_router::config_preludes::TestDefaultConfig)]
impl pallet_xcm_bridge_router::Config<XcmOverBridgeWrappedWithExportMessageRouterInstance>
	for TestRuntime
{
	// We use `SovereignPaidRemoteExporter` here to test and ensure that the `ExportMessage`
	// produced by `pallet_xcm_bridge_hub_router` is compatible with the `ExportXcm` implementation
	// of `pallet_xcm_bridge_hub`.
	type MessageExporter = SovereignPaidRemoteExporter<
		pallet_xcm_bridge_router::impls::ViaRemoteBridgeExporter<
			TestRuntime,
			// () - means `pallet_xcm_bridge_router::Config<()>`
			(),
			NetworkExportTable<BridgeTable>,
			BridgedRelayNetwork,
			BridgeHubLocation,
		>,
		// **Note**: The crucial part is that `ExportMessage` is processed by `XcmExecutor`, which
		// calls the `ExportXcm` implementation of `pallet_xcm_bridge_hub` as the
		// `MessageExporter`.
		ExecuteXcmOverSendXcm,
		ExportMessageOriginUniversalLocation,
	>;

	type BridgeIdResolver = pallet_xcm_bridge_router::impls::EnsureIsRemoteBridgeIdResolver<
		ExportMessageOriginUniversalLocation,
	>;
	// We convert to root here `BridgeHubLocationXcmOriginAsRoot`
	type UpdateBridgeStatusOrigin = EnsureRoot<AccountId>;
}

/// A router instance simulates a scenario where the router is deployed on the same chain as the
/// `MessageExporter`. This means that the router triggers `ExportXcm` trait directly.
pub type XcmOverBridgeByExportXcmRouterInstance = pallet_xcm_bridge_router::Instance2;
#[derive_impl(pallet_xcm_bridge_router::config_preludes::TestDefaultConfig)]
impl pallet_xcm_bridge_router::Config<XcmOverBridgeByExportXcmRouterInstance> for TestRuntime {
	// We use `LocalExporter` with `ViaLocalBridgeExporter` here to test and ensure that
	// `pallet_xcm_bridge_hub_router` can trigger directly `pallet_xcm_bridge_hub` as exporter.
	type MessageExporter = pallet_xcm_bridge_router::impls::ViaLocalBridgeExporter<
		TestRuntime,
		XcmOverBridgeByExportXcmRouterInstance,
		LocalExporter<XcmOverBridge, UniversalLocation>,
	>;

	type BridgeIdResolver =
		pallet_xcm_bridge_router::impls::EnsureIsRemoteBridgeIdResolver<UniversalLocation>;
	// We don't need to support here `update_bridge_status`.
	type UpdateBridgeStatusOrigin = EnsureNever<()>;
}

/// A dynamic way to set different universal location for the origin which sends `ExportMessage`.
pub struct ExportMessageOriginUniversalLocation;
impl ExportMessageOriginUniversalLocation {
	pub(crate) fn set(universal_location: Option<InteriorLocation>) {
		EXPORT_MESSAGE_ORIGIN_UNIVERSAL_LOCATION.with(|o| *o.borrow_mut() = universal_location);
	}
}
impl Get<InteriorLocation> for ExportMessageOriginUniversalLocation {
	fn get() -> InteriorLocation {
		EXPORT_MESSAGE_ORIGIN_UNIVERSAL_LOCATION.with(|o| {
			o.borrow()
				.clone()
				.expect("`EXPORT_MESSAGE_ORIGIN_UNIVERSAL_LOCATION` is not set!")
		})
	}
}
thread_local! {
	pub static EXPORT_MESSAGE_ORIGIN_UNIVERSAL_LOCATION: RefCell<Option<InteriorLocation>> = RefCell::new(None);
}

pub struct BridgeHubLocationXcmOriginAsRoot<RuntimeOrigin>(PhantomData<RuntimeOrigin>);
impl<RuntimeOrigin: OriginTrait> ConvertOrigin<RuntimeOrigin>
	for BridgeHubLocationXcmOriginAsRoot<RuntimeOrigin>
{
	fn convert_origin(
		origin: impl Into<Location>,
		kind: OriginKind,
	) -> Result<RuntimeOrigin, Location> {
		let origin = origin.into();
		if kind == OriginKind::Xcm && origin.eq(&BridgeHubLocation::get()) {
			Ok(RuntimeOrigin::root())
		} else {
			Err(origin)
		}
	}
}

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = ();
	type XcmEventEmitter = ();
	type AssetTransactor = ();
	type OriginConverter = BridgeHubLocationXcmOriginAsRoot<RuntimeOrigin>;
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = AllowUnpaidExecutionFrom<Everything>;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, ConstU32<100>>;
	type Trader = ();
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	type MaxAssetsIntoHolding = ();
	type AssetLocker = ();
	type AssetExchanger = ();
	type FeeManager = ();
	// We just set `MessageExporter` as our `pallet_xcm_bridge_hub` instance.
	type MessageExporter = (XcmOverBridge,);
	type UniversalAliases = ();
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = ();
	type TransactionalProcessor = ();
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = ();
}

thread_local! {
	pub static EXECUTE_XCM_ORIGIN: RefCell<Option<Location>> = RefCell::new(None);
}

/// The `SendXcm` implementation directly executes XCM using `XcmExecutor`.
pub struct ExecuteXcmOverSendXcm;
impl SendXcm for ExecuteXcmOverSendXcm {
	type Ticket = Xcm<()>;

	fn validate(
		_: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		Ok((message.take().unwrap(), Assets::new()))
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		let xcm: Xcm<RuntimeCall> = ticket.into();

		let origin = EXECUTE_XCM_ORIGIN.with(|o| o.borrow().clone().unwrap());
		let mut hash = xcm.using_encoded(sp_io::hashing::blake2_256);
		let outcome = XcmExecutor::<XcmConfig>::prepare_and_execute(
			origin,
			xcm,
			&mut hash,
			Weight::MAX,
			Weight::zero(),
		);
		assert_ok!(outcome.ensure_complete());

		Ok(hash)
	}
}
impl InspectMessageQueues for ExecuteXcmOverSendXcm {
	fn clear_messages() {
		todo!()
	}

	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)> {
		todo!()
	}
}
impl ExecuteXcmOverSendXcm {
	pub fn set_origin_for_execute(origin: Location) {
		EXECUTE_XCM_ORIGIN.with(|o| *o.borrow_mut() = Some(origin));
	}
}

/// Type for specifying how a `Location` can be converted into an `AccountId`. This is used
/// when determining ownership of accounts for asset transacting and when attempting to use XCM
/// `Transact` in order to determine the dispatch Origin.
pub type LocationToAccountId = (
	// The parent (Relay-chain) origin converts to the parent `AccountId`.
	ParentIsPreset<AccountId>,
	// Sibling parachain origins convert to AccountId via the `ParaId::into`.
	SiblingParachainConvertsVia<Sibling, AccountId>,
);

parameter_types! {
	pub ParentRelayChainLocation: Location = Location { parents: 1, interior: Here };
}
pub struct OpenBridgeOrigin;

impl OpenBridgeOrigin {
	pub fn parent_relay_chain_origin() -> RuntimeOrigin {
		RuntimeOrigin::signed([0u8; 32].into())
	}

	pub fn parent_relay_chain_universal_origin() -> RuntimeOrigin {
		RuntimeOrigin::signed([1u8; 32].into())
	}

	pub fn sibling_parachain_origin() -> RuntimeOrigin {
		let mut account = [0u8; 32];
		account[..4].copy_from_slice(&SIBLING_ASSET_HUB_ID.encode()[..4]);
		RuntimeOrigin::signed(account.into())
	}

	pub fn sibling_parachain_universal_origin() -> RuntimeOrigin {
		RuntimeOrigin::signed([2u8; 32].into())
	}

	pub fn origin_without_sovereign_account() -> RuntimeOrigin {
		RuntimeOrigin::signed([3u8; 32].into())
	}

	pub fn disallowed_origin() -> RuntimeOrigin {
		RuntimeOrigin::signed([42u8; 32].into())
	}
}

impl EnsureOrigin<RuntimeOrigin> for OpenBridgeOrigin {
	type Success = Location;

	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		let signer = o.clone().into_signer();
		if signer == Self::parent_relay_chain_origin().into_signer() {
			return Ok(ParentRelayChainLocation::get())
		} else if signer == Self::parent_relay_chain_universal_origin().into_signer() {
			return Ok(Location {
				parents: 2,
				interior: GlobalConsensus(RelayNetwork::get()).into(),
			})
		} else if signer == Self::sibling_parachain_universal_origin().into_signer() {
			return Ok(Location {
				parents: 2,
				interior: [GlobalConsensus(RelayNetwork::get()), Parachain(SIBLING_ASSET_HUB_ID)]
					.into(),
			})
		} else if signer == Self::origin_without_sovereign_account().into_signer() {
			return Ok(Location {
				parents: 1,
				interior: [Parachain(SIBLING_ASSET_HUB_ID), OnlyChild].into(),
			});
		} else if signer == Self::sibling_parachain_origin().into_signer() {
			return Ok(SiblingLocation::get());
		}

		Err(o)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		Ok(Self::parent_relay_chain_origin())
	}
}

pub(crate) type OpenBridgeOriginOf<T, I> = <T as pallet_xcm_bridge::Config<I>>::OpenBridgeOrigin;

pub(crate) fn fund_origin_sovereign_account(
	locations: &BridgeLocations,
	balance: Balance,
) -> AccountId {
	let bridge_owner_account =
		LocationToAccountId::convert_location(locations.bridge_origin_relative_location()).unwrap();
	assert_ok!(Balances::mint_into(&bridge_owner_account, balance));
	bridge_owner_account
}

/// Testing wrapper implementation of `LocalXcmChannelManager`, that supports storing flags in
/// storage to facilitate testing of `LocalXcmChannelManager` implementation.
pub struct TestingLocalXcmChannelManager<Bridge, Tested>(PhantomData<(Bridge, Tested)>);

impl<Bridge: Encode + sp_std::fmt::Debug, Tested> TestingLocalXcmChannelManager<Bridge, Tested> {
	fn suspended_key(bridge: &Bridge) -> Vec<u8> {
		[b"TestingLocalXcmChannelManager.Suspended", bridge.encode().as_slice()].concat()
	}
	fn resumed_key(bridge: &Bridge) -> Vec<u8> {
		[b"TestingLocalXcmChannelManager.Resumed", bridge.encode().as_slice()].concat()
	}

	pub fn is_bridge_suspened(bridge: &Bridge) -> bool {
		frame_support::storage::unhashed::get_or_default(&Self::suspended_key(bridge))
	}

	pub fn is_bridge_resumed(bridge: &Bridge) -> bool {
		frame_support::storage::unhashed::get_or_default(&Self::resumed_key(bridge))
	}
}

impl<Bridge: Encode + sp_std::fmt::Debug + Copy, Tested: LocalXcmChannelManager<Bridge>>
	LocalXcmChannelManager<Bridge> for TestingLocalXcmChannelManager<Bridge, Tested>
{
	type Error = Tested::Error;

	fn suspend_bridge(local_origin: &Location, bridge: Bridge) -> Result<(), Self::Error> {
		let result = Tested::suspend_bridge(local_origin, bridge);

		if result.is_ok() {
			frame_support::storage::unhashed::put(&Self::suspended_key(&bridge), &true);
		}

		result
	}

	fn resume_bridge(local_origin: &Location, bridge: Bridge) -> Result<(), Self::Error> {
		let result = Tested::resume_bridge(local_origin, bridge);

		if result.is_ok() {
			frame_support::storage::unhashed::put(&Self::resumed_key(&bridge), &true);
		}

		result
	}
}

/// Converts encoded call to the unpaid XCM `Transact`.
pub struct UpdateBridgeStatusXcmProvider;
impl Convert<Vec<u8>, Xcm<()>> for UpdateBridgeStatusXcmProvider {
	fn convert(encoded_call: Vec<u8>) -> Xcm<()> {
		Xcm(vec![
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			Transact {
				origin_kind: OriginKind::Xcm,
				fallback_max_weight: Some(Weight::from_parts(200_000_000, 6144)),
				call: encoded_call.into(),
			},
			ExpectTransactStatus(MaybeErrorCode::Success),
		])
	}
}

/// `SendXcm` implementation which sets `BridgeHubLocation` as origin for `ExecuteXcmOverSendXcm`.
pub struct FromBridgeHubLocationXcmSender<Inner>(PhantomData<Inner>);
impl<Inner: SendXcm> SendXcm for FromBridgeHubLocationXcmSender<Inner> {
	type Ticket = Inner::Ticket;

	fn validate(
		destination: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		Inner::validate(destination, message)
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		ExecuteXcmOverSendXcm::set_origin_for_execute(BridgeHubLocation::get());
		Inner::deliver(ticket)
	}
}

pub struct TestBlobDispatcher;

impl TestBlobDispatcher {
	pub fn is_dispatched() -> bool {
		frame_support::storage::unhashed::get_or_default(b"TestBlobDispatcher.Dispatched")
	}

	fn congestion_key(with: &Location) -> Vec<u8> {
		[b"TestBlobDispatcher.Congested.", with.encode().as_slice()].concat()
	}

	pub fn make_congested(with: &Location) {
		frame_support::storage::unhashed::put(&Self::congestion_key(with), &true);
	}
}

impl pallet_xcm_bridge::DispatchChannelStatusProvider for TestBlobDispatcher {
	fn is_congested(with: &Location) -> bool {
		frame_support::storage::unhashed::get_or_default(&Self::congestion_key(with))
	}
}

impl DispatchBlob for TestBlobDispatcher {
	fn dispatch_blob(_blob: Vec<u8>) -> Result<(), DispatchBlobError> {
		frame_support::storage::unhashed::put(b"TestBlobDispatcher.Dispatched", &true);
		Ok(())
	}
}

pub struct ThisUnderlyingChain;

impl Chain for ThisUnderlyingChain {
	const ID: ChainId = *b"tuch";

	type BlockNumber = u64;
	type Hash = H256;
	type Hasher = BlakeTwo256;
	type Header = SubstrateHeader;
	type AccountId = AccountId;
	type Balance = Balance;
	type Nonce = u64;
	type Signature = sp_runtime::MultiSignature;

	const STATE_VERSION: StateVersion = StateVersion::V1;

	fn max_extrinsic_size() -> u32 {
		u32::MAX
	}

	fn max_extrinsic_weight() -> Weight {
		Weight::MAX
	}
}

impl ChainWithMessages for ThisUnderlyingChain {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str = "WithThisChainBridgeMessages";
	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 16;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 128;
}

pub type BridgedHeaderHash = H256;
pub type BridgedChainHeader = SubstrateHeader;

pub struct BridgedUnderlyingChain;
impl Chain for BridgedUnderlyingChain {
	const ID: ChainId = *b"bgdc";
	type BlockNumber = u64;
	type Hash = BridgedHeaderHash;
	type Hasher = BlakeTwo256;
	type Header = BridgedChainHeader;
	type AccountId = AccountId;
	type Balance = Balance;
	type Nonce = u64;
	type Signature = sp_runtime::MultiSignature;

	const STATE_VERSION: StateVersion = StateVersion::V1;

	fn max_extrinsic_size() -> u32 {
		4096
	}

	fn max_extrinsic_weight() -> Weight {
		Weight::MAX
	}
}

impl ChainWithMessages for BridgedUnderlyingChain {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str = "WithBridgedChainBridgeMessages";
	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 16;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 128;
}

pub struct BridgedHeaderChain;
impl bp_header_chain::HeaderChain<BridgedUnderlyingChain> for BridgedHeaderChain {
	fn finalized_header_state_root(
		_hash: HashOf<BridgedUnderlyingChain>,
	) -> Option<HashOf<BridgedUnderlyingChain>> {
		unreachable!()
	}
}

/// Test message dispatcher.
pub struct TestMessageDispatch;

impl TestMessageDispatch {
	pub fn deactivate(lane: TestLaneIdType) {
		frame_support::storage::unhashed::put(&(b"inactive", lane).encode()[..], &false);
	}
}

impl MessageDispatch for TestMessageDispatch {
	type DispatchPayload = Vec<u8>;
	type DispatchLevelResult = ();
	type LaneId = TestLaneIdType;

	fn is_active(lane: Self::LaneId) -> bool {
		frame_support::storage::unhashed::take::<bool>(&(b"inactive", lane).encode()[..]) !=
			Some(false)
	}

	fn dispatch_weight(
		_message: &mut DispatchMessage<Self::DispatchPayload, Self::LaneId>,
	) -> Weight {
		Weight::zero()
	}

	fn dispatch(
		_: DispatchMessage<Self::DispatchPayload, Self::LaneId>,
	) -> MessageDispatchResult<Self::DispatchLevelResult> {
		MessageDispatchResult { unspent_weight: Weight::zero(), dispatch_level_result: () }
	}
}

/// Return test externalities to use in tests.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<TestRuntime>::default().build_storage().unwrap();
	sp_io::TestExternalities::new(t)
}

/// Run pallet test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	new_test_ext().execute_with(test)
}
