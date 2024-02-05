// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use super::{
	AccountId, AllPalletsWithSystem, Balances, BaseDeliveryFee, FeeAssetId, ParachainInfo,
	ParachainSystem, PolkadotXcm, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin,
	TransactionByteFee, WeightToFee, XcmpQueue,
};
use crate::{
	bridge_common_config::{
		BridgeGrandpaRococoBulletinInstance, BridgeGrandpaWestendInstance,
		BridgeParachainWestendInstance, DeliveryRewardInBalance, RequiredStakeForStakeAndSlash,
	},
	bridge_to_bulletin_config::WithRococoBulletinMessagesInstance,
	bridge_to_westend_config::WithBridgeHubWestendMessagesInstance,
	EthereumGatewayAddress,
};
use bp_messages::LaneId;
use bp_relayers::{PayRewardFromAccount, RewardsAccountOwner, RewardsAccountParams};
use bp_runtime::ChainId;
use frame_support::{
	parameter_types,
	traits::{ConstU32, Contains, Equals, Everything, Nothing},
	StoragePrefixedMap,
};
use frame_system::EnsureRoot;
use pallet_xcm::XcmPassthrough;
use parachains_common::{
	impls::ToStakingPot,
	xcm_config::{
		AllSiblingSystemParachains, ConcreteAssetFromSystem, ParentRelayOrSiblingParachains,
		RelayOrOtherSystemParachains,
	},
	TREASURY_PALLET_ID,
};
use polkadot_parachain_primitives::primitives::Sibling;
use polkadot_runtime_common::xcm_sender::ExponentialPrice;
use snowbridge_runtime_common::XcmExportFeeToSibling;
use sp_core::Get;
use sp_runtime::traits::AccountIdConversion;
use sp_std::marker::PhantomData;
use testnet_parachains_constants::rococo::snowbridge::EthereumNetwork;
use xcm::latest::prelude::*;
#[allow(deprecated)]
use xcm_builder::{
	deposit_or_burn_fee, AccountId32Aliases, AllowExplicitUnpaidExecutionFrom,
	AllowKnownQueryResponses, AllowSubscriptionsFrom, AllowTopLevelPaidExecutionFrom,
	CurrencyAdapter, DenyReserveTransferToRelayChain, DenyThenTry, EnsureXcmOrigin,
	FrameTransactionalProcessor, HandleFee, IsConcrete, ParentAsSuperuser, ParentIsPreset,
	RelayChainAsNative, SiblingParachainAsNative, SiblingParachainConvertsVia,
	SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit,
	TrailingSetTopicAsId, UsingComponents, WeightInfoBounds, WithComputedOrigin, WithUniqueTopic,
	XcmFeeToAccount,
};
use xcm_executor::{
	traits::{FeeManager, FeeReason, FeeReason::Export, TransactAsset, WithOriginFilter},
	XcmExecutor,
};

parameter_types! {
	pub const TokenLocation: Location = Location::parent();
	pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
	pub RelayNetwork: NetworkId = NetworkId::Rococo;
	pub UniversalLocation: InteriorLocation =
		[GlobalConsensus(RelayNetwork::get()), Parachain(ParachainInfo::parachain_id().into())].into();
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 64;
	pub TreasuryAccount: AccountId = TREASURY_PALLET_ID.into_account_truncating();
	pub RelayTreasuryLocation: Location = (Parent, PalletInstance(rococo_runtime_constants::TREASURY_PALLET_ID)).into();
	pub SiblingPeople: Location = (Parent, Parachain(rococo_runtime_constants::system_parachain::PEOPLE_ID)).into();
}

/// Type for specifying how a `Location` can be converted into an `AccountId`. This is used
/// when determining ownership of accounts for asset transacting and when attempting to use XCM
/// `Transact` in order to determine the dispatch Origin.
pub type LocationToAccountId = (
	// The parent (Relay-chain) origin converts to the parent `AccountId`.
	ParentIsPreset<AccountId>,
	// Sibling parachain origins convert to AccountId via the `ParaId::into`.
	SiblingParachainConvertsVia<Sibling, AccountId>,
	// Straight up local `AccountId32` origins just alias directly to `AccountId`.
	AccountId32Aliases<RelayNetwork, AccountId>,
);

/// Means for transacting the native currency on this chain.
#[allow(deprecated)]
pub type CurrencyTransactor = CurrencyAdapter<
	// Use this currency:
	Balances,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<TokenLocation>,
	// Do a simple punn to convert an AccountId32 Location into a native chain account ID:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We don't track any teleports of `Balances`.
	(),
>;

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// biases the kind of local `Origin` it will become.
pub type XcmOriginToTransactDispatchOrigin = (
	// Sovereign account converter; this attempts to derive an `AccountId` from the origin location
	// using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<LocationToAccountId, RuntimeOrigin>,
	// Native converter for Relay-chain (Parent) location; will convert to a `Relay` origin when
	// recognized.
	RelayChainAsNative<RelayChainOrigin, RuntimeOrigin>,
	// Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
	// recognized.
	SiblingParachainAsNative<cumulus_pallet_xcm::Origin, RuntimeOrigin>,
	// Superuser converter for the Relay-chain (Parent) location. This will allow it to issue a
	// transaction from the Root origin.
	ParentAsSuperuser<RuntimeOrigin>,
	// Native signed account converter; this just converts an `AccountId32` origin into a normal
	// `RuntimeOrigin::Signed` origin of the same 32-byte value.
	SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
	// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
	XcmPassthrough<RuntimeOrigin>,
);

pub struct ParentOrParentsPlurality;
impl Contains<Location> for ParentOrParentsPlurality {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (1, []) | (1, [Plurality { .. }]))
	}
}

/// A call filter for the XCM Transact instruction. This is a temporary measure until we properly
/// account for proof size weights.
///
/// Calls that are allowed through this filter must:
/// 1. Have a fixed weight;
/// 2. Cannot lead to another call being made;
/// 3. Have a defined proof size weight, e.g. no unbounded vecs in call parameters.
pub struct SafeCallFilter;
impl Contains<RuntimeCall> for SafeCallFilter {
	fn contains(call: &RuntimeCall) -> bool {
		#[cfg(feature = "runtime-benchmarks")]
		{
			if matches!(call, RuntimeCall::System(frame_system::Call::remark_with_event { .. })) {
				return true
			}
		}

		// Allow to change dedicated storage items (called by governance-like)
		match call {
			RuntimeCall::System(frame_system::Call::set_storage { items })
				if items.iter().all(|(k, _)| {
					k.eq(&DeliveryRewardInBalance::key()) ||
						k.eq(&RequiredStakeForStakeAndSlash::key()) ||
						k.eq(&EthereumGatewayAddress::key()) ||
						// Allow resetting of Ethereum nonces in Rococo only.
						k.starts_with(&snowbridge_pallet_inbound_queue::Nonce::<Runtime>::final_prefix()) ||
						k.starts_with(&snowbridge_pallet_outbound_queue::Nonce::<Runtime>::final_prefix())
				}) =>
				return true,
			_ => (),
		};

		matches!(
			call,
			RuntimeCall::PolkadotXcm(
				pallet_xcm::Call::force_xcm_version { .. } |
					pallet_xcm::Call::force_default_xcm_version { .. }
			) | RuntimeCall::System(
				frame_system::Call::set_heap_pages { .. } |
					frame_system::Call::set_code { .. } |
					frame_system::Call::set_code_without_checks { .. } |
					frame_system::Call::authorize_upgrade { .. } |
					frame_system::Call::authorize_upgrade_without_checks { .. } |
					frame_system::Call::kill_prefix { .. },
			) | RuntimeCall::ParachainSystem(..) |
				RuntimeCall::Timestamp(..) |
				RuntimeCall::Balances(..) |
				RuntimeCall::CollatorSelection(..) |
				RuntimeCall::Session(pallet_session::Call::purge_keys { .. }) |
				RuntimeCall::XcmpQueue(..) |
				RuntimeCall::MessageQueue(..) |
				RuntimeCall::BridgeWestendGrandpa(pallet_bridge_grandpa::Call::<
					Runtime,
					BridgeGrandpaWestendInstance,
				>::initialize { .. }) |
				RuntimeCall::BridgeWestendGrandpa(pallet_bridge_grandpa::Call::<
					Runtime,
					BridgeGrandpaWestendInstance,
				>::set_operating_mode { .. }) |
				RuntimeCall::BridgeWestendParachains(pallet_bridge_parachains::Call::<
					Runtime,
					BridgeParachainWestendInstance,
				>::set_operating_mode { .. }) |
				RuntimeCall::BridgeWestendMessages(pallet_bridge_messages::Call::<
					Runtime,
					WithBridgeHubWestendMessagesInstance,
				>::set_operating_mode { .. }) |
				RuntimeCall::BridgePolkadotBulletinGrandpa(pallet_bridge_grandpa::Call::<
					Runtime,
					BridgeGrandpaRococoBulletinInstance,
				>::initialize { .. }) |
				RuntimeCall::BridgePolkadotBulletinGrandpa(pallet_bridge_grandpa::Call::<
					Runtime,
					BridgeGrandpaRococoBulletinInstance,
				>::set_operating_mode { .. }) |
				RuntimeCall::BridgePolkadotBulletinMessages(pallet_bridge_messages::Call::<
					Runtime,
					WithRococoBulletinMessagesInstance,
				>::set_operating_mode { .. }) |
				RuntimeCall::EthereumBeaconClient(
					snowbridge_pallet_ethereum_client::Call::force_checkpoint { .. } |
						snowbridge_pallet_ethereum_client::Call::set_operating_mode { .. },
				) | RuntimeCall::EthereumInboundQueue(
				snowbridge_pallet_inbound_queue::Call::set_operating_mode { .. },
			) | RuntimeCall::EthereumOutboundQueue(
				snowbridge_pallet_outbound_queue::Call::set_operating_mode { .. },
			) | RuntimeCall::EthereumSystem(
				snowbridge_pallet_system::Call::upgrade { .. } |
					snowbridge_pallet_system::Call::set_operating_mode { .. } |
					snowbridge_pallet_system::Call::set_pricing_parameters { .. } |
					snowbridge_pallet_system::Call::force_update_channel { .. } |
					snowbridge_pallet_system::Call::force_transfer_native_from_agent { .. } |
					snowbridge_pallet_system::Call::set_token_transfer_fees { .. },
			)
		)
	}
}

pub type Barrier = TrailingSetTopicAsId<
	DenyThenTry<
		DenyReserveTransferToRelayChain,
		(
			// Allow local users to buy weight credit.
			TakeWeightCredit,
			// Expected responses are OK.
			AllowKnownQueryResponses<PolkadotXcm>,
			WithComputedOrigin<
				(
					// If the message is one that immediately attempts to pay for execution, then
					// allow it.
					AllowTopLevelPaidExecutionFrom<Everything>,
					// Parent, its pluralities (i.e. governance bodies), relay treasury pallet
					// and sibling People get free execution.
					AllowExplicitUnpaidExecutionFrom<(
						ParentOrParentsPlurality,
						Equals<RelayTreasuryLocation>,
						Equals<SiblingPeople>,
					)>,
					// Subscriptions for version tracking are OK.
					AllowSubscriptionsFrom<ParentRelayOrSiblingParachains>,
				),
				UniversalLocation,
				ConstU32<8>,
			>,
		),
	>,
>;

/// Locations that will not be charged fees in the executor,
/// either execution or delivery.
/// We only waive fees for system functions, which these locations represent.
pub type WaivedLocations = (
	RelayOrOtherSystemParachains<AllSiblingSystemParachains, Runtime>,
	Equals<RelayTreasuryLocation>,
);

/// Cases where a remote origin is accepted as trusted Teleporter for a given asset:
/// - NativeToken with the parent Relay Chain and sibling parachains.
pub type TrustedTeleporters = ConcreteAssetFromSystem<TokenLocation>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = CurrencyTransactor;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	// BridgeHub does not recognize a reserve location for any asset. Users must teleport Native
	// token where allowed (e.g. with the Relay Chain).
	type IsReserve = ();
	type IsTeleporter = TrustedTeleporters;
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = WeightInfoBounds<
		crate::weights::xcm::BridgeHubRococoXcmWeight<RuntimeCall>,
		RuntimeCall,
		MaxInstructions,
	>;
	type Trader =
		UsingComponents<WeightToFee, TokenLocation, AccountId, Balances, ToStakingPot<Runtime>>;
	type ResponseHandler = PolkadotXcm;
	type AssetTrap = PolkadotXcm;
	type AssetLocker = ();
	type AssetExchanger = ();
	type AssetClaims = PolkadotXcm;
	type SubscriptionService = PolkadotXcm;
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = XcmFeeManagerFromComponentsBridgeHub<
		WaivedLocations,
		(
			XcmExportFeeToRelayerRewardAccounts<
				Self::AssetTransactor,
				crate::bridge_to_westend_config::WestendGlobalConsensusNetwork,
				crate::bridge_to_westend_config::AssetHubWestendParaId,
				crate::bridge_to_westend_config::BridgeHubWestendChainId,
				crate::bridge_to_westend_config::AssetHubRococoToAssetHubWestendMessagesLane,
			>,
			XcmExportFeeToSibling<
				bp_rococo::Balance,
				AccountId,
				TokenLocation,
				EthereumNetwork,
				Self::AssetTransactor,
				crate::EthereumOutboundQueue,
			>,
			XcmFeeToAccount<Self::AssetTransactor, AccountId, TreasuryAccount>,
		),
	>;
	type MessageExporter = (
		crate::bridge_to_westend_config::ToBridgeHubWestendHaulBlobExporter,
		crate::bridge_to_bulletin_config::ToRococoBulletinHaulBlobExporter,
		crate::bridge_to_ethereum_config::SnowbridgeExporter,
	);
	type UniversalAliases = Nothing;
	type CallDispatcher = WithOriginFilter<SafeCallFilter>;
	type SafeCallFilter = SafeCallFilter;
	type Aliasers = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
}

pub type PriceForParentDelivery =
	ExponentialPrice<FeeAssetId, BaseDeliveryFee, TransactionByteFee, ParachainSystem>;

/// Converts a local signed origin into an XCM location.
/// Forms the basis for local origins sending/executing XCMs.
pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = WithUniqueTopic<(
	// Two routers - use UMP to communicate with the relay chain:
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, PriceForParentDelivery>,
	// ..and XCMP to communicate with the sibling chains.
	XcmpQueue,
)>;

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmRouter = XcmRouter;
	// We want to disallow users sending (arbitrary) XCMs from this chain.
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, ()>;
	// We support local origins dispatching XCM executions in principle...
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmExecuteFilter = Nothing;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	type XcmReserveTransferFilter = Nothing; // This parachain is not meant as a reserve location.
	type Weigher = WeightInfoBounds<
		crate::weights::xcm::BridgeHubRococoXcmWeight<RuntimeCall>,
		RuntimeCall,
		MaxInstructions,
	>;
	type UniversalLocation = UniversalLocation;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type Currency = Balances;
	type CurrencyMatcher = ();
	type TrustedLockers = ();
	type SovereignAccountOf = LocationToAccountId;
	type MaxLockers = ConstU32<8>;
	type WeightInfo = crate::weights::pallet_xcm::WeightInfo<Runtime>;
	type AdminOrigin = EnsureRoot<AccountId>;
	type MaxRemoteLockConsumers = ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
}

impl cumulus_pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}

/// A `HandleFee` implementation that simply deposits the fees for `ExportMessage` XCM instructions
/// into the accounts that are used for paying the relayer rewards.
/// Burns the fees in case of a failure.
pub struct XcmExportFeeToRelayerRewardAccounts<
	AssetTransactor,
	DestNetwork,
	DestParaId,
	DestBridgedChainId,
	BridgeLaneId,
>(PhantomData<(AssetTransactor, DestNetwork, DestParaId, DestBridgedChainId, BridgeLaneId)>);

impl<
		AssetTransactor: TransactAsset,
		DestNetwork: Get<NetworkId>,
		DestParaId: Get<cumulus_primitives_core::ParaId>,
		DestBridgedChainId: Get<ChainId>,
		BridgeLaneId: Get<LaneId>,
	> HandleFee
	for XcmExportFeeToRelayerRewardAccounts<
		AssetTransactor,
		DestNetwork,
		DestParaId,
		DestBridgedChainId,
		BridgeLaneId,
	>
{
	fn handle_fee(fee: Assets, maybe_context: Option<&XcmContext>, reason: FeeReason) -> Assets {
		if matches!(reason, FeeReason::Export { network: bridged_network, destination }
				if bridged_network == DestNetwork::get() &&
					destination == [Parachain(DestParaId::get().into())])
		{
			// We have 2 relayer rewards accounts:
			// - the SA of the source parachain on this BH: this pays the relayers for delivering
			//   Source para -> Target Para message delivery confirmations
			// - the SA of the destination parachain on this BH: this pays the relayers for
			//   delivering Target para -> Source Para messages
			// We split the `ExportMessage` fee between these 2 accounts.
			let source_para_account = PayRewardFromAccount::<
				pallet_balances::Pallet<Runtime>,
				AccountId,
			>::rewards_account(RewardsAccountParams::new(
				BridgeLaneId::get(),
				DestBridgedChainId::get(),
				RewardsAccountOwner::ThisChain,
			));

			let dest_para_account = PayRewardFromAccount::<
				pallet_balances::Pallet<Runtime>,
				AccountId,
			>::rewards_account(RewardsAccountParams::new(
				BridgeLaneId::get(),
				DestBridgedChainId::get(),
				RewardsAccountOwner::BridgedChain,
			));

			for asset in fee.into_inner() {
				match asset.fun {
					Fungible(total_fee) => {
						let source_fee = total_fee / 2;
						deposit_or_burn_fee::<AssetTransactor, _>(
							Asset { id: asset.id.clone(), fun: Fungible(source_fee) }.into(),
							maybe_context,
							source_para_account.clone(),
						);

						let dest_fee = total_fee - source_fee;
						deposit_or_burn_fee::<AssetTransactor, _>(
							Asset { id: asset.id, fun: Fungible(dest_fee) }.into(),
							maybe_context,
							dest_para_account.clone(),
						);
					},
					NonFungible(_) => {
						deposit_or_burn_fee::<AssetTransactor, _>(
							asset.into(),
							maybe_context,
							source_para_account.clone(),
						);
					},
				}
			}

			return Assets::new()
		}

		fee
	}
}

pub struct XcmFeeManagerFromComponentsBridgeHub<WaivedLocations, HandleFee>(
	PhantomData<(WaivedLocations, HandleFee)>,
);
impl<WaivedLocations: Contains<Location>, FeeHandler: HandleFee> FeeManager
	for XcmFeeManagerFromComponentsBridgeHub<WaivedLocations, FeeHandler>
{
	fn is_waived(origin: Option<&Location>, fee_reason: FeeReason) -> bool {
		let Some(loc) = origin else { return false };
		if let Export { network, destination: Here } = fee_reason {
			return !(network == EthereumNetwork::get())
		}
		WaivedLocations::contains(loc)
	}

	fn handle_fee(fee: Assets, context: Option<&XcmContext>, reason: FeeReason) {
		FeeHandler::handle_fee(fee, context, reason);
	}
}
