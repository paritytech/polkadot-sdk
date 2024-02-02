// Copyright (C) Parity Technologies (UK) Ltd.
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

use super::{
	AccountId, AllPalletsWithSystem, Assets, Authorship, Balance, Balances, BaseDeliveryFee,
	CollatorSelection, FeeAssetId, ForeignAssets, ForeignAssetsInstance, ParachainInfo,
	ParachainSystem, PolkadotXcm, PoolAssets, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin,
	ToRococoXcmRouter, TransactionByteFee, TrustBackedAssetsInstance, Uniques, WeightToFee,
	XcmpQueue,
};
use assets_common::{
	matching::{FromSiblingParachain, IsForeignConcreteAsset},
	TrustBackedAssetsAsLocation,
};
use frame_support::{
	parameter_types,
	traits::{
		tokens::imbalance::ResolveAssetTo, ConstU32, Contains, Equals, Everything, Nothing,
		PalletInfoAccess,
	},
};
use frame_system::EnsureRoot;
use pallet_xcm::XcmPassthrough;
use parachains_common::{
	impls::ToStakingPot,
	xcm_config::{
		AllSiblingSystemParachains, AssetFeeAsExistentialDepositMultiplier,
		ConcreteAssetFromSystem, RelayOrOtherSystemParachains,
	},
	TREASURY_PALLET_ID,
};
use polkadot_parachain_primitives::primitives::Sibling;
use polkadot_runtime_common::xcm_sender::ExponentialPrice;
use sp_runtime::traits::{AccountIdConversion, ConvertInto};
use xcm::latest::prelude::*;
#[allow(deprecated)]
use xcm_builder::CurrencyAdapter;
use xcm_builder::{
	AccountId32Aliases, AllowExplicitUnpaidExecutionFrom, AllowKnownQueryResponses,
	AllowSubscriptionsFrom, AllowTopLevelPaidExecutionFrom, DenyReserveTransferToRelayChain,
	DenyThenTry, DescribeFamily, DescribePalletTerminal, EnsureXcmOrigin,
	FrameTransactionalProcessor, FungiblesAdapter, GlobalConsensusParachainConvertsFor,
	HashedDescription, IsConcrete, LocalMint, NetworkExportTableItem, NoChecking,
	NonFungiblesAdapter, ParentAsSuperuser, ParentIsPreset, RelayChainAsNative,
	SiblingParachainAsNative, SiblingParachainConvertsVia, SignedAccountId32AsNative,
	SignedToAccountId32, SovereignSignedViaLocation, StartsWith, StartsWithExplicitGlobalConsensus,
	TakeWeightCredit, TrailingSetTopicAsId, UsingComponents, WeightInfoBounds, WithComputedOrigin,
	WithUniqueTopic, XcmFeeManagerFromComponents, XcmFeeToAccount,
};
use xcm_executor::{traits::WithOriginFilter, XcmExecutor};

parameter_types! {
	pub const WestendLocation: Location = Location::parent();
	pub const WestendLocationV3: xcm::v3::Location = xcm::v3::Location::parent();
	pub const RelayNetwork: Option<NetworkId> = Some(NetworkId::Westend);
	pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
	pub UniversalLocation: InteriorLocation =
		[GlobalConsensus(RelayNetwork::get().unwrap()), Parachain(ParachainInfo::parachain_id().into())].into();
	pub UniversalLocationNetworkId: NetworkId = UniversalLocation::get().global_consensus().unwrap();
	pub TrustBackedAssetsPalletLocation: Location =
		PalletInstance(TrustBackedAssetsPalletIndex::get()).into();
	pub TrustBackedAssetsPalletIndex: u8 = <Assets as PalletInfoAccess>::index() as u8;
	pub TrustBackedAssetsPalletLocationV3: xcm::v3::Location =
		xcm::v3::Junction::PalletInstance(<Assets as PalletInfoAccess>::index() as u8).into();
	pub ForeignAssetsPalletLocation: Location =
		PalletInstance(<ForeignAssets as PalletInfoAccess>::index() as u8).into();
	pub PoolAssetsPalletLocation: Location =
		PalletInstance(<PoolAssets as PalletInfoAccess>::index() as u8).into();
	pub UniquesPalletLocation: Location =
		PalletInstance(<Uniques as PalletInfoAccess>::index() as u8).into();
	pub PoolAssetsPalletLocationV3: xcm::v3::Location =
		xcm::v3::Junction::PalletInstance(<PoolAssets as PalletInfoAccess>::index() as u8).into();
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
	pub StakingPot: AccountId = CollatorSelection::account_id();
	pub TreasuryAccount: AccountId = TREASURY_PALLET_ID.into_account_truncating();
	pub RelayTreasuryLocation: Location = (Parent, PalletInstance(westend_runtime_constants::TREASURY_PALLET_ID)).into();
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
	// Foreign chain account alias into local accounts according to a hash of their standard
	// description.
	HashedDescription<AccountId, DescribeFamily<DescribePalletTerminal>>,
	// Different global consensus parachain sovereign account.
	// (Used for over-bridge transfers and reserve processing)
	GlobalConsensusParachainConvertsFor<UniversalLocation, AccountId>,
);

/// Means for transacting the native currency on this chain.
#[allow(deprecated)]
pub type CurrencyTransactor = CurrencyAdapter<
	// Use this currency:
	Balances,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<WestendLocation>,
	// Convert an XCM Location into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We don't track any teleports of `Balances`.
	(),
>;

/// `AssetId`/`Balance` converter for `TrustBackedAssets`.
pub type TrustBackedAssetsConvertedConcreteId =
	assets_common::TrustBackedAssetsConvertedConcreteId<TrustBackedAssetsPalletLocation, Balance>;

/// Means for transacting assets besides the native currency on this chain.
pub type FungiblesTransactor = FungiblesAdapter<
	// Use this fungibles implementation:
	Assets,
	// Use this currency when it is a fungible asset matching the given location or name:
	TrustBackedAssetsConvertedConcreteId,
	// Convert an XCM Location into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We only want to allow teleports of known assets. We use non-zero issuance as an indication
	// that this asset is known.
	LocalMint<parachains_common::impls::NonZeroIssuance<AccountId, Assets>>,
	// The account to use for tracking teleports.
	CheckingAccount,
>;

/// Matcher for converting `ClassId`/`InstanceId` into a uniques asset.
pub type UniquesConvertedConcreteId =
	assets_common::UniquesConvertedConcreteId<UniquesPalletLocation>;

/// Means for transacting unique assets.
pub type UniquesTransactor = NonFungiblesAdapter<
	// Use this non-fungibles implementation:
	Uniques,
	// This adapter will handle any non-fungible asset from the uniques pallet.
	UniquesConvertedConcreteId,
	// Convert an XCM Location into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// Does not check teleports.
	NoChecking,
	// The account to use for tracking teleports.
	CheckingAccount,
>;

/// `AssetId`/`Balance` converter for `ForeignAssets`.
pub type ForeignAssetsConvertedConcreteId = assets_common::ForeignAssetsConvertedConcreteId<
	(
		// Ignore `TrustBackedAssets` explicitly
		StartsWith<TrustBackedAssetsPalletLocation>,
		// Ignore asset which starts explicitly with our `GlobalConsensus(NetworkId)`, means:
		// - foreign assets from our consensus should be: `Location {parents: 1, X*(Parachain(xyz),
		//   ..)}
		// - foreign assets outside our consensus with the same `GlobalConsensus(NetworkId)` wont
		//   be accepted here
		StartsWithExplicitGlobalConsensus<UniversalLocationNetworkId>,
	),
	Balance,
>;

/// Means for transacting foreign assets from different global consensus.
pub type ForeignFungiblesTransactor = FungiblesAdapter<
	// Use this fungibles implementation:
	ForeignAssets,
	// Use this currency when it is a fungible asset matching the given location or name:
	ForeignAssetsConvertedConcreteId,
	// Convert an XCM Location into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We dont need to check teleports here.
	NoChecking,
	// The account to use for tracking teleports.
	CheckingAccount,
>;

/// `AssetId`/`Balance` converter for `PoolAssets`.
pub type PoolAssetsConvertedConcreteId =
	assets_common::PoolAssetsConvertedConcreteId<PoolAssetsPalletLocation, Balance>;

/// Means for transacting asset conversion pool assets on this chain.
pub type PoolFungiblesTransactor = FungiblesAdapter<
	// Use this fungibles implementation:
	PoolAssets,
	// Use this currency when it is a fungible asset matching the given location or name:
	PoolAssetsConvertedConcreteId,
	// Convert an XCM Location into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We only want to allow teleports of known assets. We use non-zero issuance as an indication
	// that this asset is known.
	LocalMint<parachains_common::impls::NonZeroIssuance<AccountId, PoolAssets>>,
	// The account to use for tracking teleports.
	CheckingAccount,
>;

/// Means for transacting assets on this chain.
pub type AssetTransactors = (
	CurrencyTransactor,
	FungiblesTransactor,
	ForeignFungiblesTransactor,
	PoolFungiblesTransactor,
	UniquesTransactor,
);

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// biases the kind of local `Origin` it will become.
pub type XcmOriginToTransactDispatchOrigin = (
	// Sovereign account converter; this attempts to derive an `AccountId` from the origin location
	// using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<LocationToAccountId, RuntimeOrigin>,
	// Native converter for Relay-chain (Parent) location; will convert to a `Relay` origin when
	// recognised.
	RelayChainAsNative<RelayChainOrigin, RuntimeOrigin>,
	// Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
	// recognised.
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

parameter_types! {
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 64;
	pub XcmAssetFeesReceiver: Option<AccountId> = Authorship::author();
}

pub struct ParentOrParentsPlurality;
impl Contains<Location> for ParentOrParentsPlurality {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (1, []) | (1, [Plurality { .. }]))
	}
}

pub struct FellowshipEntities;
impl Contains<Location> for FellowshipEntities {
	fn contains(location: &Location) -> bool {
		matches!(
			location.unpack(),
			(1, [Parachain(1001), Plurality { id: BodyId::Technical, .. }]) |
				(1, [Parachain(1001), PalletInstance(64)]) |
				(1, [Parachain(1001), PalletInstance(65)])
		)
	}
}

pub struct AmbassadorEntities;
impl Contains<Location> for AmbassadorEntities {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (1, [Parachain(1001), PalletInstance(74)]))
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
				if items.iter().all(|(k, _)| k.eq(&bridging::XcmBridgeHubRouterByteFee::key())) ||
					items
						.iter()
						.all(|(k, _)| k.eq(&bridging::XcmBridgeHubRouterBaseFee::key())) =>
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
				RuntimeCall::Assets(
					pallet_assets::Call::create { .. } |
						pallet_assets::Call::force_create { .. } |
						pallet_assets::Call::start_destroy { .. } |
						pallet_assets::Call::destroy_accounts { .. } |
						pallet_assets::Call::destroy_approvals { .. } |
						pallet_assets::Call::finish_destroy { .. } |
						pallet_assets::Call::block { .. } |
						pallet_assets::Call::mint { .. } |
						pallet_assets::Call::burn { .. } |
						pallet_assets::Call::transfer { .. } |
						pallet_assets::Call::transfer_keep_alive { .. } |
						pallet_assets::Call::force_transfer { .. } |
						pallet_assets::Call::freeze { .. } |
						pallet_assets::Call::thaw { .. } |
						pallet_assets::Call::freeze_asset { .. } |
						pallet_assets::Call::thaw_asset { .. } |
						pallet_assets::Call::transfer_ownership { .. } |
						pallet_assets::Call::set_team { .. } |
						pallet_assets::Call::set_metadata { .. } |
						pallet_assets::Call::clear_metadata { .. } |
						pallet_assets::Call::force_set_metadata { .. } |
						pallet_assets::Call::force_clear_metadata { .. } |
						pallet_assets::Call::force_asset_status { .. } |
						pallet_assets::Call::approve_transfer { .. } |
						pallet_assets::Call::cancel_approval { .. } |
						pallet_assets::Call::force_cancel_approval { .. } |
						pallet_assets::Call::transfer_approved { .. } |
						pallet_assets::Call::touch { .. } |
						pallet_assets::Call::touch_other { .. } |
						pallet_assets::Call::refund { .. } |
						pallet_assets::Call::refund_other { .. },
				) | RuntimeCall::ForeignAssets(
				pallet_assets::Call::create { .. } |
					pallet_assets::Call::force_create { .. } |
					pallet_assets::Call::start_destroy { .. } |
					pallet_assets::Call::destroy_accounts { .. } |
					pallet_assets::Call::destroy_approvals { .. } |
					pallet_assets::Call::finish_destroy { .. } |
					pallet_assets::Call::block { .. } |
					pallet_assets::Call::mint { .. } |
					pallet_assets::Call::burn { .. } |
					pallet_assets::Call::transfer { .. } |
					pallet_assets::Call::transfer_keep_alive { .. } |
					pallet_assets::Call::force_transfer { .. } |
					pallet_assets::Call::freeze { .. } |
					pallet_assets::Call::thaw { .. } |
					pallet_assets::Call::freeze_asset { .. } |
					pallet_assets::Call::thaw_asset { .. } |
					pallet_assets::Call::transfer_ownership { .. } |
					pallet_assets::Call::set_team { .. } |
					pallet_assets::Call::set_metadata { .. } |
					pallet_assets::Call::clear_metadata { .. } |
					pallet_assets::Call::force_set_metadata { .. } |
					pallet_assets::Call::force_clear_metadata { .. } |
					pallet_assets::Call::force_asset_status { .. } |
					pallet_assets::Call::approve_transfer { .. } |
					pallet_assets::Call::cancel_approval { .. } |
					pallet_assets::Call::force_cancel_approval { .. } |
					pallet_assets::Call::transfer_approved { .. } |
					pallet_assets::Call::touch { .. } |
					pallet_assets::Call::touch_other { .. } |
					pallet_assets::Call::refund { .. } |
					pallet_assets::Call::refund_other { .. },
			) | RuntimeCall::PoolAssets(
				pallet_assets::Call::create { .. } |
					pallet_assets::Call::force_create { .. } |
					pallet_assets::Call::start_destroy { .. } |
					pallet_assets::Call::destroy_accounts { .. } |
					pallet_assets::Call::destroy_approvals { .. } |
					pallet_assets::Call::finish_destroy { .. } |
					pallet_assets::Call::block { .. } |
					pallet_assets::Call::mint { .. } |
					pallet_assets::Call::burn { .. } |
					pallet_assets::Call::transfer { .. } |
					pallet_assets::Call::transfer_keep_alive { .. } |
					pallet_assets::Call::force_transfer { .. } |
					pallet_assets::Call::freeze { .. } |
					pallet_assets::Call::thaw { .. } |
					pallet_assets::Call::freeze_asset { .. } |
					pallet_assets::Call::thaw_asset { .. } |
					pallet_assets::Call::transfer_ownership { .. } |
					pallet_assets::Call::set_team { .. } |
					pallet_assets::Call::set_metadata { .. } |
					pallet_assets::Call::clear_metadata { .. } |
					pallet_assets::Call::force_set_metadata { .. } |
					pallet_assets::Call::force_clear_metadata { .. } |
					pallet_assets::Call::force_asset_status { .. } |
					pallet_assets::Call::approve_transfer { .. } |
					pallet_assets::Call::cancel_approval { .. } |
					pallet_assets::Call::force_cancel_approval { .. } |
					pallet_assets::Call::transfer_approved { .. } |
					pallet_assets::Call::touch { .. } |
					pallet_assets::Call::touch_other { .. } |
					pallet_assets::Call::refund { .. } |
					pallet_assets::Call::refund_other { .. },
			) | RuntimeCall::AssetConversion(
				pallet_asset_conversion::Call::create_pool { .. } |
					pallet_asset_conversion::Call::add_liquidity { .. } |
					pallet_asset_conversion::Call::remove_liquidity { .. } |
					pallet_asset_conversion::Call::swap_tokens_for_exact_tokens { .. } |
					pallet_asset_conversion::Call::swap_exact_tokens_for_tokens { .. },
			) | RuntimeCall::NftFractionalization(
				pallet_nft_fractionalization::Call::fractionalize { .. } |
					pallet_nft_fractionalization::Call::unify { .. },
			) | RuntimeCall::Nfts(
				pallet_nfts::Call::create { .. } |
					pallet_nfts::Call::force_create { .. } |
					pallet_nfts::Call::destroy { .. } |
					pallet_nfts::Call::mint { .. } |
					pallet_nfts::Call::force_mint { .. } |
					pallet_nfts::Call::burn { .. } |
					pallet_nfts::Call::transfer { .. } |
					pallet_nfts::Call::lock_item_transfer { .. } |
					pallet_nfts::Call::unlock_item_transfer { .. } |
					pallet_nfts::Call::lock_collection { .. } |
					pallet_nfts::Call::transfer_ownership { .. } |
					pallet_nfts::Call::set_team { .. } |
					pallet_nfts::Call::force_collection_owner { .. } |
					pallet_nfts::Call::force_collection_config { .. } |
					pallet_nfts::Call::approve_transfer { .. } |
					pallet_nfts::Call::cancel_approval { .. } |
					pallet_nfts::Call::clear_all_transfer_approvals { .. } |
					pallet_nfts::Call::lock_item_properties { .. } |
					pallet_nfts::Call::set_attribute { .. } |
					pallet_nfts::Call::force_set_attribute { .. } |
					pallet_nfts::Call::clear_attribute { .. } |
					pallet_nfts::Call::approve_item_attributes { .. } |
					pallet_nfts::Call::cancel_item_attributes_approval { .. } |
					pallet_nfts::Call::set_metadata { .. } |
					pallet_nfts::Call::clear_metadata { .. } |
					pallet_nfts::Call::set_collection_metadata { .. } |
					pallet_nfts::Call::clear_collection_metadata { .. } |
					pallet_nfts::Call::set_accept_ownership { .. } |
					pallet_nfts::Call::set_collection_max_supply { .. } |
					pallet_nfts::Call::update_mint_settings { .. } |
					pallet_nfts::Call::set_price { .. } |
					pallet_nfts::Call::buy_item { .. } |
					pallet_nfts::Call::pay_tips { .. } |
					pallet_nfts::Call::create_swap { .. } |
					pallet_nfts::Call::cancel_swap { .. } |
					pallet_nfts::Call::claim_swap { .. },
			) | RuntimeCall::Uniques(
				pallet_uniques::Call::create { .. } |
					pallet_uniques::Call::force_create { .. } |
					pallet_uniques::Call::destroy { .. } |
					pallet_uniques::Call::mint { .. } |
					pallet_uniques::Call::burn { .. } |
					pallet_uniques::Call::transfer { .. } |
					pallet_uniques::Call::freeze { .. } |
					pallet_uniques::Call::thaw { .. } |
					pallet_uniques::Call::freeze_collection { .. } |
					pallet_uniques::Call::thaw_collection { .. } |
					pallet_uniques::Call::transfer_ownership { .. } |
					pallet_uniques::Call::set_team { .. } |
					pallet_uniques::Call::approve_transfer { .. } |
					pallet_uniques::Call::cancel_approval { .. } |
					pallet_uniques::Call::force_item_status { .. } |
					pallet_uniques::Call::set_attribute { .. } |
					pallet_uniques::Call::clear_attribute { .. } |
					pallet_uniques::Call::set_metadata { .. } |
					pallet_uniques::Call::clear_metadata { .. } |
					pallet_uniques::Call::set_collection_metadata { .. } |
					pallet_uniques::Call::clear_collection_metadata { .. } |
					pallet_uniques::Call::set_accept_ownership { .. } |
					pallet_uniques::Call::set_collection_max_supply { .. } |
					pallet_uniques::Call::set_price { .. } |
					pallet_uniques::Call::buy_item { .. }
			) | RuntimeCall::ToRococoXcmRouter(
				pallet_xcm_bridge_hub_router::Call::report_bridge_status { .. }
			)
		)
	}
}

pub type Barrier = TrailingSetTopicAsId<
	DenyThenTry<
		DenyReserveTransferToRelayChain,
		(
			TakeWeightCredit,
			// Expected responses are OK.
			AllowKnownQueryResponses<PolkadotXcm>,
			// Allow XCMs with some computed origins to pass through.
			WithComputedOrigin<
				(
					// If the message is one that immediately attempts to pay for execution, then
					// allow it.
					AllowTopLevelPaidExecutionFrom<Everything>,
					// Parent, its pluralities (i.e. governance bodies), relay treasury pallet and
					// BridgeHub get free execution.
					AllowExplicitUnpaidExecutionFrom<(
						ParentOrParentsPlurality,
						Equals<RelayTreasuryLocation>,
						Equals<bridging::SiblingBridgeHub>,
						FellowshipEntities,
						AmbassadorEntities,
					)>,
					// Subscriptions for version tracking are OK.
					AllowSubscriptionsFrom<Everything>,
				),
				UniversalLocation,
				ConstU32<8>,
			>,
		),
	>,
>;

// TODO: This calls into the Assets pallet's default `BalanceToAssetBalance` implementation, which
// uses the ratio of minimum balances and requires asset sufficiency. This means that purchasing
// weight within XCM programs will still use the old way, and paying fees via asset conversion will
// only be possible when transacting locally. We should add an impl of this trait that does asset
// conversion.
pub type AssetFeeAsExistentialDepositMultiplierFeeCharger = AssetFeeAsExistentialDepositMultiplier<
	Runtime,
	WeightToFee,
	pallet_assets::BalanceToAssetBalance<Balances, Runtime, ConvertInto, TrustBackedAssetsInstance>,
	TrustBackedAssetsInstance,
>;

/// Multiplier used for dedicated `TakeFirstAssetTrader` with `ForeignAssets` instance.
pub type ForeignAssetFeeAsExistentialDepositMultiplierFeeCharger =
	AssetFeeAsExistentialDepositMultiplier<
		Runtime,
		WeightToFee,
		pallet_assets::BalanceToAssetBalance<Balances, Runtime, ConvertInto, ForeignAssetsInstance>,
		ForeignAssetsInstance,
	>;

/// Locations that will not be charged fees in the executor,
/// either execution or delivery.
/// We only waive fees for system functions, which these locations represent.
pub type WaivedLocations = (
	RelayOrOtherSystemParachains<AllSiblingSystemParachains, Runtime>,
	Equals<RelayTreasuryLocation>,
	FellowshipEntities,
	AmbassadorEntities,
);

/// Cases where a remote origin is accepted as trusted Teleporter for a given asset:
///
/// - WND with the parent Relay Chain and sibling system parachains; and
/// - Sibling parachains' assets from where they originate (as `ForeignCreators`).
pub type TrustedTeleporters = (
	ConcreteAssetFromSystem<WestendLocation>,
	IsForeignConcreteAsset<FromSiblingParachain<parachain_info::Pallet<Runtime>>>,
);

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = AssetTransactors;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	// Asset Hub trusts only particular, pre-configured bridged locations from a different consensus
	// as reserve locations (we trust the Bridge Hub to relay the message that a reserve is being
	// held). Asset Hub may _act_ as a reserve location for WND and assets created
	// under `pallet-assets`. Users must use teleport where allowed (e.g. WND with the Relay Chain).
	type IsReserve = (bridging::to_rococo::IsTrustedBridgedReserveLocationForConcreteAsset,);
	type IsTeleporter = TrustedTeleporters;
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = WeightInfoBounds<
		crate::weights::xcm::AssetHubWestendXcmWeight<RuntimeCall>,
		RuntimeCall,
		MaxInstructions,
	>;
	type Trader = (
		UsingComponents<WeightToFee, WestendLocation, AccountId, Balances, ToStakingPot<Runtime>>,
		cumulus_primitives_utility::SwapFirstAssetTrader<
			WestendLocationV3,
			crate::AssetConversion,
			WeightToFee,
			crate::NativeAndAssets,
			(
				TrustBackedAssetsAsLocation<TrustBackedAssetsPalletLocation, Balance>,
				ForeignAssetsConvertedConcreteId,
			),
			ResolveAssetTo<StakingPot, crate::NativeAndAssets>,
			AccountId,
		>,
		// This trader allows to pay with `is_sufficient=true` "Trust Backed" assets from dedicated
		// `pallet_assets` instance - `Assets`.
		cumulus_primitives_utility::TakeFirstAssetTrader<
			AccountId,
			AssetFeeAsExistentialDepositMultiplierFeeCharger,
			TrustBackedAssetsConvertedConcreteId,
			Assets,
			cumulus_primitives_utility::XcmFeesTo32ByteAccount<
				FungiblesTransactor,
				AccountId,
				XcmAssetFeesReceiver,
			>,
		>,
		// This trader allows to pay with `is_sufficient=true` "Foreign" assets from dedicated
		// `pallet_assets` instance - `ForeignAssets`.
		cumulus_primitives_utility::TakeFirstAssetTrader<
			AccountId,
			ForeignAssetFeeAsExistentialDepositMultiplierFeeCharger,
			ForeignAssetsConvertedConcreteId,
			ForeignAssets,
			cumulus_primitives_utility::XcmFeesTo32ByteAccount<
				ForeignFungiblesTransactor,
				AccountId,
				XcmAssetFeesReceiver,
			>,
		>,
	);
	type ResponseHandler = PolkadotXcm;
	type AssetTrap = PolkadotXcm;
	type AssetClaims = PolkadotXcm;
	type SubscriptionService = PolkadotXcm;
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type AssetLocker = ();
	type AssetExchanger = ();
	type FeeManager = XcmFeeManagerFromComponents<
		WaivedLocations,
		XcmFeeToAccount<Self::AssetTransactor, AccountId, TreasuryAccount>,
	>;
	type MessageExporter = ();
	type UniversalAliases = (bridging::to_rococo::UniversalAliases,);
	type CallDispatcher = WithOriginFilter<SafeCallFilter>;
	type SafeCallFilter = SafeCallFilter;
	type Aliasers = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
}

/// Local origins on this chain are allowed to dispatch XCM sends/executions.
pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

pub type PriceForParentDelivery =
	ExponentialPrice<FeeAssetId, BaseDeliveryFee, TransactionByteFee, ParachainSystem>;

/// For routing XCM messages which do not cross local consensus boundary.
type LocalXcmRouter = (
	// Two routers - use UMP to communicate with the relay chain:
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, PriceForParentDelivery>,
	// ..and XCMP to communicate with the sibling chains.
	XcmpQueue,
);

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = WithUniqueTopic<(
	LocalXcmRouter,
	// Router which wraps and sends xcm to BridgeHub to be delivered to the Rococo
	// GlobalConsensus
	ToRococoXcmRouter,
)>;

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmExecuteFilter = Everything;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	type XcmReserveTransferFilter = Everything;
	type Weigher = WeightInfoBounds<
		crate::weights::xcm::AssetHubWestendXcmWeight<RuntimeCall>,
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

pub type ForeignCreatorsSovereignAccountOf = (
	SiblingParachainConvertsVia<Sibling, AccountId>,
	AccountId32Aliases<RelayNetwork, AccountId>,
	ParentIsPreset<AccountId>,
);

/// Simple conversion of `u32` into an `AssetId` for use in benchmarking.
pub struct XcmBenchmarkHelper;
#[cfg(feature = "runtime-benchmarks")]
impl pallet_assets::BenchmarkHelper<xcm::v3::Location> for XcmBenchmarkHelper {
	fn create_asset_id_parameter(id: u32) -> xcm::v3::Location {
		xcm::v3::Location::new(1, [xcm::v3::Junction::Parachain(id)])
	}
}

/// All configuration related to bridging
pub mod bridging {
	use super::*;
	use assets_common::matching;
	use sp_std::collections::btree_set::BTreeSet;

	parameter_types! {
		/// Base price of every byte of the Westend -> Rococo message. Can be adjusted via
		/// governance `set_storage` call.
		///
		/// Default value is our estimation of the:
		///
		/// 1) an approximate cost of XCM execution (`ExportMessage` and surroundings) at Westend bridge hub;
		///
		/// 2) the approximate cost of Westend -> Rococo message delivery transaction on Rococo Bridge Hub,
		///    converted into WNDs using 1:1 conversion rate;
		///
		/// 3) the approximate cost of Westend -> Rococo message confirmation transaction on Westend Bridge Hub.
		pub storage XcmBridgeHubRouterBaseFee: Balance =
			bp_bridge_hub_westend::BridgeHubWestendBaseXcmFeeInWnds::get()
				.saturating_add(bp_bridge_hub_rococo::BridgeHubRococoBaseDeliveryFeeInRocs::get())
				.saturating_add(bp_bridge_hub_westend::BridgeHubWestendBaseConfirmationFeeInWnds::get());
		/// Price of every byte of the Westend -> Rococo message. Can be adjusted via
		/// governance `set_storage` call.
		pub storage XcmBridgeHubRouterByteFee: Balance = TransactionByteFee::get();

		pub SiblingBridgeHubParaId: u32 = bp_bridge_hub_westend::BRIDGE_HUB_WESTEND_PARACHAIN_ID;
		pub SiblingBridgeHub: Location = Location::new(1, [Parachain(SiblingBridgeHubParaId::get())]);
		/// Router expects payment with this `AssetId`.
		/// (`AssetId` has to be aligned with `BridgeTable`)
		pub XcmBridgeHubRouterFeeAssetId: AssetId = WestendLocation::get().into();

		pub BridgeTable: sp_std::vec::Vec<NetworkExportTableItem> =
			sp_std::vec::Vec::new().into_iter()
			.chain(to_rococo::BridgeTable::get())
			.collect();
	}

	pub type NetworkExportTable = xcm_builder::NetworkExportTable<BridgeTable>;

	pub mod to_rococo {
		use super::*;

		parameter_types! {
			pub SiblingBridgeHubWithBridgeHubRococoInstance: Location = Location::new(
				1,
				[
					Parachain(SiblingBridgeHubParaId::get()),
					PalletInstance(bp_bridge_hub_westend::WITH_BRIDGE_WESTEND_TO_ROCOCO_MESSAGES_PALLET_INDEX)
				]
			);

			pub const RococoNetwork: NetworkId = NetworkId::Rococo;
			pub AssetHubRococo: Location = Location::new(2, [GlobalConsensus(RococoNetwork::get()), Parachain(bp_asset_hub_rococo::ASSET_HUB_ROCOCO_PARACHAIN_ID)]);
			pub RocLocation: Location = Location::new(2, [GlobalConsensus(RococoNetwork::get())]);

			pub RocFromAssetHubRococo: (AssetFilter, Location) = (
				Wild(AllOf { fun: WildFungible, id: AssetId(RocLocation::get()) }),
				AssetHubRococo::get()
			);

			/// Set up exporters configuration.
			/// `Option<Asset>` represents static "base fee" which is used for total delivery fee calculation.
			pub BridgeTable: sp_std::vec::Vec<NetworkExportTableItem> = sp_std::vec![
				NetworkExportTableItem::new(
					RococoNetwork::get(),
					Some(sp_std::vec![
						AssetHubRococo::get().interior.split_global().expect("invalid configuration for AssetHubRococo").1,
					]),
					SiblingBridgeHub::get(),
					// base delivery fee to local `BridgeHub`
					Some((
						XcmBridgeHubRouterFeeAssetId::get(),
						XcmBridgeHubRouterBaseFee::get(),
					).into())
				)
			];

			/// Universal aliases
			pub UniversalAliases: BTreeSet<(Location, Junction)> = BTreeSet::from_iter(
				sp_std::vec![
					(SiblingBridgeHubWithBridgeHubRococoInstance::get(), GlobalConsensus(RococoNetwork::get()))
				]
			);
		}

		impl Contains<(Location, Junction)> for UniversalAliases {
			fn contains(alias: &(Location, Junction)) -> bool {
				UniversalAliases::get().contains(alias)
			}
		}

		/// Reserve locations filter for `xcm_executor::Config::IsReserve`.
		/// Locations from which the runtime accepts reserved assets.
		pub type IsTrustedBridgedReserveLocationForConcreteAsset =
			matching::IsTrustedBridgedReserveLocationForConcreteAsset<
				UniversalLocation,
				(
					// allow receive ROC from AssetHubRococo
					xcm_builder::Case<RocFromAssetHubRococo>,
					// and nothing else
				),
			>;

		impl Contains<RuntimeCall> for ToRococoXcmRouter {
			fn contains(call: &RuntimeCall) -> bool {
				matches!(
					call,
					RuntimeCall::ToRococoXcmRouter(
						pallet_xcm_bridge_hub_router::Call::report_bridge_status { .. }
					)
				)
			}
		}
	}

	/// Benchmarks helper for bridging configuration.
	#[cfg(feature = "runtime-benchmarks")]
	pub struct BridgingBenchmarksHelper;

	#[cfg(feature = "runtime-benchmarks")]
	impl BridgingBenchmarksHelper {
		pub fn prepare_universal_alias() -> Option<(Location, Junction)> {
			let alias =
				to_rococo::UniversalAliases::get().into_iter().find_map(|(location, junction)| {
					match to_rococo::SiblingBridgeHubWithBridgeHubRococoInstance::get()
						.eq(&location)
					{
						true => Some((location, junction)),
						false => None,
					}
				});
			assert!(alias.is_some(), "we expect here BridgeHubWestend to Rococo mapping at least");
			Some(alias.unwrap())
		}
	}
}
