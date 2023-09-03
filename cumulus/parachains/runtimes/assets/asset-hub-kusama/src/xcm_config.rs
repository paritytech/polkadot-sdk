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
	AccountId, AllPalletsWithSystem, Assets, Authorship, Balance, Balances, ForeignAssets,
	ForeignAssetsInstance, ParachainInfo, ParachainSystem, PolkadotXcm, PoolAssets, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeOrigin, ToPolkadotXcmRouter, TransactionByteFee,
	TrustBackedAssetsInstance, WeightToFee, XcmpQueue,
};
use assets_common::{
	local_and_foreign_assets::MatchesLocalAndForeignAssetsMultiLocation,
	matching::{
		FromSiblingParachain, IsForeignConcreteAsset, StartsWith, StartsWithExplicitGlobalConsensus,
	},
};
use frame_support::{
	match_types, parameter_types,
	traits::{ConstU32, Contains, Everything, EverythingBut, Nothing, PalletInfoAccess},
};
use frame_system::EnsureRoot;
use pallet_xcm::XcmPassthrough;
use parachains_common::{
	impls::ToStakingPot,
	xcm_config::{AllowUnpaidTransactsFrom, AssetFeeAsExistentialDepositMultiplier},
};
use polkadot_parachain_primitives::primitives::Sibling;
use sp_runtime::traits::ConvertInto;
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, AllowExplicitUnpaidExecutionFrom, AllowKnownQueryResponses,
	AllowSubscriptionsFrom, AllowTopLevelPaidExecutionFrom, CurrencyAdapter,
	DenyReserveTransferToRelayChain, DenyThenTry, DescribeAllTerminal, DescribeFamily,
	EnsureXcmOrigin, FungiblesAdapter, GlobalConsensusParachainConvertsFor, HashedDescription,
	IsConcrete, LocalMint, NativeAsset, NoChecking, ParentAsSuperuser, ParentIsPreset,
	RelayChainAsNative, SiblingParachainAsNative, SiblingParachainConvertsVia,
	SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit,
	TrailingSetTopicAsId, UsingComponents, WeightInfoBounds, WithComputedOrigin, WithUniqueTopic,
};
use xcm_executor::{traits::WithOriginFilter, XcmExecutor};

#[cfg(feature = "runtime-benchmarks")]
use {cumulus_primitives_core::ParaId, sp_core::Get};

parameter_types! {
	pub const KsmLocation: MultiLocation = MultiLocation::parent();
	pub const RelayNetwork: Option<NetworkId> = Some(NetworkId::Kusama);
	pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
	pub UniversalLocation: InteriorMultiLocation =
		X2(GlobalConsensus(RelayNetwork::get().unwrap()), Parachain(ParachainInfo::parachain_id().into()));
	pub UniversalLocationNetworkId: NetworkId = UniversalLocation::get().global_consensus().unwrap();
	pub TrustBackedAssetsPalletLocation: MultiLocation =
		PalletInstance(<Assets as PalletInfoAccess>::index() as u8).into();
	pub ForeignAssetsPalletLocation: MultiLocation =
		PalletInstance(<ForeignAssets as PalletInfoAccess>::index() as u8).into();
	pub PoolAssetsPalletLocation: MultiLocation =
		PalletInstance(<PoolAssets as PalletInfoAccess>::index() as u8).into();
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
	pub const GovernanceLocation: MultiLocation = MultiLocation::parent();
	pub const FellowshipLocation: MultiLocation = MultiLocation::parent();
}

/// Type for specifying how a `MultiLocation` can be converted into an `AccountId`. This is used
/// when determining ownership of accounts for asset transacting and when attempting to use XCM
/// `Transact` in order to determine the dispatch Origin.
pub type LocationToAccountId = (
	// The parent (Relay-chain) origin converts to the parent `AccountId`.
	ParentIsPreset<AccountId>,
	// Sibling parachain origins convert to AccountId via the `ParaId::into`.
	SiblingParachainConvertsVia<Sibling, AccountId>,
	// Straight up local `AccountId32` origins just alias directly to `AccountId`.
	AccountId32Aliases<RelayNetwork, AccountId>,
	// Foreign locations alias into accounts according to a hash of their standard description.
	HashedDescription<AccountId, DescribeFamily<DescribeAllTerminal>>,
	// Different global consensus parachain sovereign account.
	// (Used for over-bridge transfers and reserve processing)
	GlobalConsensusParachainConvertsFor<UniversalLocation, AccountId>,
);

/// Means for transacting the native currency on this chain.
pub type CurrencyTransactor = CurrencyAdapter<
	// Use this currency:
	Balances,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<KsmLocation>,
	// Convert an XCM MultiLocation into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We don't track any teleports of `Balances`.
	(),
>;

/// `AssetId`/`Balance` converter for `PoolAssets`.
pub type TrustBackedAssetsConvertedConcreteId =
	assets_common::TrustBackedAssetsConvertedConcreteId<TrustBackedAssetsPalletLocation, Balance>;

/// Means for transacting assets besides the native currency on this chain.
pub type FungiblesTransactor = FungiblesAdapter<
	// Use this fungibles implementation:
	Assets,
	// Use this currency when it is a fungible asset matching the given location or name:
	TrustBackedAssetsConvertedConcreteId,
	// Convert an XCM MultiLocation into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We only want to allow teleports of known assets. We use non-zero issuance as an indication
	// that this asset is known.
	LocalMint<parachains_common::impls::NonZeroIssuance<AccountId, Assets>>,
	// The account to use for tracking teleports.
	CheckingAccount,
>;

/// `AssetId/Balance` converter for `TrustBackedAssets`
pub type ForeignAssetsConvertedConcreteId = assets_common::ForeignAssetsConvertedConcreteId<
	(
		// Ignore `TrustBackedAssets` explicitly
		StartsWith<TrustBackedAssetsPalletLocation>,
		// Ignore assets that start explicitly with our `GlobalConsensus(NetworkId)`, means:
		// - foreign assets from our consensus should be: `MultiLocation {parents: 1,
		//   X*(Parachain(xyz), ..)}`
		// - foreign assets outside our consensus with the same `GlobalConsensus(NetworkId)` won't
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
	// Convert an XCM MultiLocation into a local account id:
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
	// Convert an XCM MultiLocation into a local account id:
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
pub type AssetTransactors =
	(CurrencyTransactor, FungiblesTransactor, ForeignFungiblesTransactor, PoolFungiblesTransactor);

/// Simple `MultiLocation` matcher for Local and Foreign asset `MultiLocation`.
pub struct LocalAndForeignAssetsMultiLocationMatcher;
impl MatchesLocalAndForeignAssetsMultiLocation for LocalAndForeignAssetsMultiLocationMatcher {
	fn is_local(location: &MultiLocation) -> bool {
		use assets_common::fungible_conversion::MatchesMultiLocation;
		TrustBackedAssetsConvertedConcreteId::contains(location)
	}
	fn is_foreign(location: &MultiLocation) -> bool {
		use assets_common::fungible_conversion::MatchesMultiLocation;
		ForeignAssetsConvertedConcreteId::contains(location)
	}
}
impl Contains<MultiLocation> for LocalAndForeignAssetsMultiLocationMatcher {
	fn contains(location: &MultiLocation) -> bool {
		Self::is_local(location) || Self::is_foreign(location)
	}
}

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

match_types! {
	pub type ParentOrParentsPlurality: impl Contains<MultiLocation> = {
		MultiLocation { parents: 1, interior: Here } |
		MultiLocation { parents: 1, interior: X1(Plurality { .. }) }
	};
	pub type ParentOrSiblings: impl Contains<MultiLocation> = {
		MultiLocation { parents: 1, interior: Here } |
		MultiLocation { parents: 1, interior: X1(_) }
	};
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
				if items.iter().all(|(k, _)| k.eq(&bridging::XcmBridgeHubRouterByteFee::key())) =>
				return true,
			_ => (),
		};

		matches!(
			call,
			RuntimeCall::PolkadotXcm(pallet_xcm::Call::force_xcm_version { .. }) |
				RuntimeCall::System(
					frame_system::Call::set_heap_pages { .. } |
						frame_system::Call::set_code { .. } |
						frame_system::Call::set_code_without_checks { .. } |
						frame_system::Call::kill_prefix { .. },
				) | RuntimeCall::ParachainSystem(..) |
				RuntimeCall::Timestamp(..) |
				RuntimeCall::Balances(..) |
				RuntimeCall::CollatorSelection(
					pallet_collator_selection::Call::set_desired_candidates { .. } |
						pallet_collator_selection::Call::set_candidacy_bond { .. } |
						pallet_collator_selection::Call::register_as_candidate { .. } |
						pallet_collator_selection::Call::leave_intent { .. } |
						pallet_collator_selection::Call::set_invulnerables { .. } |
						pallet_collator_selection::Call::add_invulnerable { .. } |
						pallet_collator_selection::Call::remove_invulnerable { .. },
				) | RuntimeCall::Session(pallet_session::Call::purge_keys { .. }) |
				RuntimeCall::XcmpQueue(..) |
				RuntimeCall::DmpQueue(..) |
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
				pallet_assets::Call::force_create { .. } |
					pallet_assets::Call::block { .. } |
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
			) | RuntimeCall::ToPolkadotXcmRouter(
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
					// If the message is one that immediately attemps to pay for execution, then
					// allow it.
					AllowTopLevelPaidExecutionFrom<Everything>,
					// Parent and its pluralities (i.e. governance bodies) get free execution.
					AllowExplicitUnpaidExecutionFrom<ParentOrParentsPlurality>,
					// Subscriptions for version tracking are OK.
					AllowSubscriptionsFrom<ParentOrSiblings>,
					// Allows Transacts with `report_bridge_status` from sibling BridgeHub.
					bridging::AllowUnpaidStatusReportsFromSiblingBridgeHub<ToPolkadotXcmRouter>,
				),
				UniversalLocation,
				ConstU32<8>,
			>,
		),
	>,
>;

/// Multiplier used for dedicated `TakeFirstAssetTrader` with `Assets` instance.
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

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = AssetTransactors;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	// Asset Hub trusts only particular configured bridge locations as reserve locations.
	// Asset Hub may _act_ as a reserve location for KSM and assets created under `pallet-assets`.
	// Users must use teleport where allowed (e.g. KSM with the Relay Chain).
	type IsReserve = bridging::IsTrustedBridgedReserveLocationForConcreteAsset;
	// We allow:
	// - teleportation of KSM
	// - teleportation of sibling parachain's assets (as ForeignCreators)
	type IsTeleporter = (
		NativeAsset,
		IsForeignConcreteAsset<FromSiblingParachain<parachain_info::Pallet<Runtime>>>,
	);
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = WeightInfoBounds<
		crate::weights::xcm::AssetHubKusamaXcmWeight<RuntimeCall>,
		RuntimeCall,
		MaxInstructions,
	>;
	type Trader = (
		UsingComponents<WeightToFee, KsmLocation, AccountId, Balances, ToStakingPot<Runtime>>,
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
	// TODO:check-parameter: change and assert in tests when (https://github.com/paritytech/polkadot/pull/7005) merged
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = bridging::UniversalAliases;
	type CallDispatcher = WithOriginFilter<SafeCallFilter>;
	type SafeCallFilter = SafeCallFilter;
	type Aliasers = Nothing;
}

/// Converts a local signed origin into an XCM multilocation.
/// Forms the basis for local origins sending/executing XCMs.
pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

/// For routing XCM messages which do not cross local consensus boundary.
type LocalXcmRouter = (
	// Two routers - use UMP to communicate with the relay chain:
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, ()>,
	// ..and XCMP to communicate with the sibling chains.
	XcmpQueue,
);

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = WithUniqueTopic<(
	LocalXcmRouter,
	// Router which wraps and sends xcm to BridgeHub to be delivered to the Polkadot
	// GlobalConsensus
	ToPolkadotXcmRouter,
)>;

#[cfg(feature = "runtime-benchmarks")]
parameter_types! {
	pub ReachableDest: Option<MultiLocation> = Some(Parent.into());
}

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// We want to disallow users sending (arbitrary) XCMs from this chain.
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, ()>;
	type XcmRouter = XcmRouter;
	// We support local origins dispatching XCM executions in principle...
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	// ... but disallow generic XCM execution. As a result only teleports and reserve transfers are
	// allowed.
	type XcmExecuteFilter = Nothing;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	// Allow reserve based transfer to everywhere except for bridging, here we strictly check what
	// assets are allowed.
	type XcmReserveTransferFilter =
		EverythingBut<bridging::IsNotAllowedExplicitlyForReserveTransfer>;
	type Weigher = WeightInfoBounds<
		crate::weights::xcm::AssetHubKusamaXcmWeight<RuntimeCall>,
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
	#[cfg(feature = "runtime-benchmarks")]
	type ReachableDest = ReachableDest;
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
impl pallet_assets::BenchmarkHelper<MultiLocation> for XcmBenchmarkHelper {
	fn create_asset_id_parameter(id: u32) -> MultiLocation {
		MultiLocation { parents: 1, interior: X1(Parachain(id)) }
	}
}

#[cfg(feature = "runtime-benchmarks")]
pub struct BenchmarkMultiLocationConverter<SelfParaId> {
	_phantom: sp_std::marker::PhantomData<SelfParaId>,
}

#[cfg(feature = "runtime-benchmarks")]
impl<SelfParaId>
	pallet_asset_conversion::BenchmarkHelper<MultiLocation, sp_std::boxed::Box<MultiLocation>>
	for BenchmarkMultiLocationConverter<SelfParaId>
where
	SelfParaId: Get<ParaId>,
{
	fn asset_id(asset_id: u32) -> MultiLocation {
		MultiLocation {
			parents: 1,
			interior: X3(
				Parachain(SelfParaId::get().into()),
				PalletInstance(<Assets as PalletInfoAccess>::index() as u8),
				GeneralIndex(asset_id.into()),
			),
		}
	}
	fn multiasset_id(asset_id: u32) -> sp_std::boxed::Box<MultiLocation> {
		sp_std::boxed::Box::new(Self::asset_id(asset_id))
	}
}

/// All configuration related to bridging
pub mod bridging {
	use super::*;
	use assets_common::{matching, matching::*};
	use parachains_common::xcm_config::LocationFilter;
	use sp_std::collections::btree_set::BTreeSet;

	parameter_types! {
		pub BridgeHubKusamaParaId: u32 = 1002;
		pub BridgeHubKusama: MultiLocation = MultiLocation::new(1, X1(Parachain(BridgeHubKusamaParaId::get())));
		pub BridgeHubKusamaWithBridgeHubPolkadotInstance: MultiLocation = MultiLocation::new(1, X2(Parachain(BridgeHubKusamaParaId::get()), PalletInstance(53)));
		pub const PolkadotNetwork: NetworkId = NetworkId::Polkadot;
		pub AssetHubPolkadot: MultiLocation =  MultiLocation::new(2, X2(GlobalConsensus(PolkadotNetwork::get()), Parachain(1000)));
		pub DotLocation: MultiLocation =  MultiLocation::new(2, X1(GlobalConsensus(PolkadotNetwork::get())));

		/// Router expects payment with this `AssetId`.
		/// (`AssetId` has to be aligned with `BridgeTable`)
		pub XcmBridgeHubRouterFeeAssetId: AssetId = KsmLocation::get().into();
		/// Price per byte - can be adjusted via governance `set_storage` call.
		pub storage XcmBridgeHubRouterByteFee: Balance = TransactionByteFee::get();

		/// Set up exporters configuration.
		/// `Option<MultiAsset>` represents static "base fee" which is used for total delivery fee calculation.
		pub BridgeTable: sp_std::vec::Vec<(NetworkId, LocationFilter<InteriorMultiLocation>, MultiLocation, Option<MultiAsset>)> = sp_std::vec![
			(
				PolkadotNetwork::get(),
				LocationFilter::default()
					// allow to bridge to AssetHubPolkadot
					.add_equals(AssetHubPolkadot::get().interior.split_global().expect("invalid configuration for AssetHubPolkadot").1),
					// and nothing else
				BridgeHubKusama::get(),
				// base delivery fee to local `BridgeHub`
				Some((
					XcmBridgeHubRouterFeeAssetId::get(),
					bp_asset_hub_kusama::BridgeHubKusamaBaseFeeInDots::get(),
				).into())
			)
		];

		/// Set up trusted bridged reserve locations.
		/// Locations from which the runtime accepts reserved assets.
		pub TrustedBridgedReserveLocations: sp_std::vec::Vec<FilteredLocation> = sp_std::vec![
			// trust assets from AssetHubPolkadot
			(
				AssetHubPolkadot::get(),
				AssetFilter::ByMultiLocation(
					LocationFilter::default()
						// allow receive DOT
						.add_equals(DotLocation::get())
						// and nothing else
				)
			)
		];

		/// Allowed reserve transfer assets per destination.
		/// Means that runtime allows to transfer reserve assets to these locations.
		pub AllowedReserveTransferAssetsLocations: sp_std::vec::Vec<FilteredLocation> = sp_std::vec![
			// allow to transfer assets to AssetHubPolkadot
			(
				AssetHubPolkadot::get(),
				AssetFilter::ByMultiLocation(
					LocationFilter::default()
						// allow send only KSM
						.add_equals(KsmLocation::get())
						// and nothing else
				)
			)
		];

		/// Universal aliases
		pub UniversalAliases: BTreeSet<(MultiLocation, Junction)> = BTreeSet::from_iter(
			sp_std::vec![
				(BridgeHubKusamaWithBridgeHubPolkadotInstance::get(), GlobalConsensus(PolkadotNetwork::get()))
			]
		);

	}

	impl Contains<(MultiLocation, Junction)> for UniversalAliases {
		fn contains(alias: &(MultiLocation, Junction)) -> bool {
			UniversalAliases::get().contains(alias)
		}
	}

	pub type FilteredNetworkExportTable =
		parachains_common::xcm_config::FilteredNetworkExportTable<BridgeTable>;

	/// Reserve locations filter for `xcm_executor::Config::IsReserve`.
	pub type IsTrustedBridgedReserveLocationForConcreteAsset =
		matching::IsTrustedBridgedReserveLocationForConcreteAsset<
			UniversalLocation,
			TrustedBridgedReserveLocations,
		>;

	/// Filter out those assets which are not allowed for bridged reserve based transfer.
	/// (asset, location) filter for `pallet_xcm::Config::XcmReserveTransferFilter`.
	pub type IsNotAllowedExplicitlyForReserveTransfer = ExcludeOnlyForRemoteDestination<
		UniversalLocation,
		FilteredNetworkExportTable,
		IsNotAllowedConcreteAssetBy<AllowedReserveTransferAssetsLocations>,
	>;

	impl Contains<RuntimeCall> for ToPolkadotXcmRouter {
		fn contains(call: &RuntimeCall) -> bool {
			matches!(
				call,
				RuntimeCall::ToPolkadotXcmRouter(
					pallet_xcm_bridge_hub_router::Call::report_bridge_status { .. }
				)
			)
		}
	}

	/// Barrier for `pallet_xcm_bridge_hub_router::Pallet` to receive congestion statuses from
	/// sibling BridgeHub.
	pub type AllowUnpaidStatusReportsFromSiblingBridgeHub<XcmBridgeHubRouter> =
		AllowUnpaidTransactsFrom<RuntimeCall, XcmBridgeHubRouter, Equals<BridgeHubKusama>>;

	/// Benchmarks helper for bridging configuration.
	#[cfg(feature = "runtime-benchmarks")]
	pub struct BridgingBenchmarksHelper;

	#[cfg(feature = "runtime-benchmarks")]
	impl BridgingBenchmarksHelper {
		pub fn prepare_universal_alias() -> Option<(MultiLocation, Junction)> {
			let alias = UniversalAliases::get().into_iter().find_map(|(location, junction)| {
				match BridgeHubKusamaWithBridgeHubPolkadotInstance::get().eq(&location) {
					true => Some((location, junction)),
					false => None,
				}
			});
			assert!(alias.is_some(), "we expect here BridgeHubKusama to Polkadot mapping at least");
			Some(alias.unwrap())
		}
	}
}
