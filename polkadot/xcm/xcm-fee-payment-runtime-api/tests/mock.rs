// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Mock runtime for tests.
//! Implements both runtime APIs for fee estimation and getting the messages for transfers.

use codec::Encode;
use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{
		AsEnsureOriginWithArg, ConstU128, ConstU32, Contains, ContainsPair, Everything, Nothing,
		OriginTrait,
	},
	weights::WeightToFee as WeightToFeeT,
};
use frame_system::{EnsureRoot, RawOrigin as SystemRawOrigin};
use pallet_xcm::TestWeightInfo;
use sp_runtime::{
	traits::{Block as BlockT, Get, IdentityLookup, MaybeEquivalence, TryConvert},
	BuildStorage, SaturatedConversion,
};
use sp_std::{cell::RefCell, marker::PhantomData};
use xcm::{prelude::*, Version as XcmVersion};
use xcm_builder::{
	AllowTopLevelPaidExecutionFrom, ConvertedConcreteId, EnsureXcmOrigin, FixedRateOfFungible,
	FixedWeightBounds, FungibleAdapter, FungiblesAdapter, IsConcrete, MintLocation, NoChecking,
	TakeWeightCredit,
};
use xcm_executor::{
	traits::{ConvertLocation, JustTry},
	XcmExecutor,
};

use xcm_fee_payment_runtime_api::{
	dry_run::{Error as XcmDryRunApiError, ExtrinsicDryRunEffects, XcmDryRunApi, XcmDryRunEffects},
	fees::{Error as XcmPaymentApiError, XcmPaymentApi},
};

construct_runtime! {
	pub enum TestRuntime {
		System: frame_system,
		Balances: pallet_balances,
		AssetsPallet: pallet_assets,
		XcmPallet: pallet_xcm,
	}
}

pub type SignedExtra = (
	// frame_system::CheckEra<TestRuntime>,
	// frame_system::CheckNonce<TestRuntime>,
	frame_system::CheckWeight<TestRuntime>,
);
pub type TestXt = sp_runtime::testing::TestXt<RuntimeCall, SignedExtra>;
type Block = sp_runtime::testing::Block<TestXt>;
type Balance = u128;
type AssetIdForAssetsPallet = u32;
type AccountId = u64;

pub fn extra() -> SignedExtra {
	(frame_system::CheckWeight::new(),)
}

type Executive = frame_executive::Executive<
	TestRuntime,
	Block,
	frame_system::ChainContext<TestRuntime>,
	TestRuntime,
	AllPalletsWithSystem,
	(),
>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for TestRuntime {
	type Block = Block;
	type AccountId = AccountId;
	type AccountData = pallet_balances::AccountData<Balance>;
	type Lookup = IdentityLookup<AccountId>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for TestRuntime {
	type AccountStore = System;
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
impl pallet_assets::Config for TestRuntime {
	type AssetId = AssetIdForAssetsPallet;
	type Balance = Balance;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type Freezer = ();
	type AssetDeposit = ConstU128<1>;
	type AssetAccountDeposit = ConstU128<10>;
	type MetadataDepositBase = ConstU128<1>;
	type MetadataDepositPerByte = ConstU128<1>;
	type ApprovalDeposit = ConstU128<1>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

thread_local! {
	pub static SENT_XCM: RefCell<Vec<(Location, Xcm<()>)>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn sent_xcm() -> Vec<(Location, Xcm<()>)> {
	SENT_XCM.with(|q| (*q.borrow()).clone())
}

pub struct TestXcmSender;
impl SendXcm for TestXcmSender {
	type Ticket = (Location, Xcm<()>);
	fn validate(
		dest: &mut Option<Location>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let ticket = (dest.take().unwrap(), msg.take().unwrap());
		let fees: Assets = (HereLocation::get(), DeliveryFees::get()).into();
		Ok((ticket, fees))
	}
	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		let hash = fake_message_hash(&ticket.1);
		SENT_XCM.with(|q| q.borrow_mut().push(ticket));
		Ok(hash)
	}
}

pub(crate) fn fake_message_hash<Call>(message: &Xcm<Call>) -> XcmHash {
	message.using_encoded(sp_io::hashing::blake2_256)
}

pub type XcmRouter = TestXcmSender;

parameter_types! {
	pub const DeliveryFees: u128 = 20; // Random value.
	pub const ExistentialDeposit: u128 = 1; // Random value.
	pub const BaseXcmWeight: Weight = Weight::from_parts(100, 10); // Random value.
	pub const MaxInstructions: u32 = 100;
	pub const NativeTokenPerSecondPerByte: (AssetId, u128, u128) = (AssetId(HereLocation::get()), 1, 1);
	pub UniversalLocation: InteriorLocation = [GlobalConsensus(NetworkId::Westend), Parachain(2000)].into();
	pub static AdvertisedXcmVersion: XcmVersion = 4;
	pub const HereLocation: Location = Location::here();
	pub const RelayLocation: Location = Location::parent();
	pub const MaxAssetsIntoHolding: u32 = 64;
	pub CheckAccount: AccountId = XcmPallet::check_account();
	pub LocalCheckAccount: (AccountId, MintLocation) = (CheckAccount::get(), MintLocation::Local);
	pub const AnyNetwork: Option<NetworkId> = None;
}

/// Simple `WeightToFee` implementation that adds the ref_time by the proof_size.
pub struct WeightToFee;
impl WeightToFeeT for WeightToFee {
	type Balance = Balance;
	fn weight_to_fee(weight: &Weight) -> Self::Balance {
		Self::Balance::saturated_from(weight.ref_time())
			.saturating_add(Self::Balance::saturated_from(weight.proof_size()))
	}
}

type Weigher = FixedWeightBounds<BaseXcmWeight, RuntimeCall, MaxInstructions>;

/// Matches the pair (NativeToken, AssetHub).
/// This is used in the `IsTeleporter` configuration item, meaning we accept our native token
/// coming from AssetHub as a teleport.
pub struct NativeTokenToAssetHub;
impl ContainsPair<Asset, Location> for NativeTokenToAssetHub {
	fn contains(asset: &Asset, origin: &Location) -> bool {
		matches!(asset.id.0.unpack(), (0, [])) && matches!(origin.unpack(), (1, [Parachain(1000)]))
	}
}

/// Matches the pair (RelayToken, AssetHub).
/// This is used in the `IsReserve` configuration item, meaning we accept the relay token
/// coming from AssetHub as a reserve asset transfer.
pub struct RelayTokenToAssetHub;
impl ContainsPair<Asset, Location> for RelayTokenToAssetHub {
	fn contains(asset: &Asset, origin: &Location) -> bool {
		matches!(asset.id.0.unpack(), (1, [])) && matches!(origin.unpack(), (1, [Parachain(1000)]))
	}
}

/// Converts locations that are only the `AccountIndex64` junction into local u64 accounts.
pub struct AccountIndex64Aliases<Network, AccountId>(PhantomData<(Network, AccountId)>);
impl<Network: Get<Option<NetworkId>>, AccountId: From<u64>> ConvertLocation<AccountId>
	for AccountIndex64Aliases<Network, AccountId>
{
	fn convert_location(location: &Location) -> Option<AccountId> {
		let index = match location.unpack() {
			(0, [AccountIndex64 { index, network: None }]) => index,
			(0, [AccountIndex64 { index, network }]) if *network == Network::get() => index,
			_ => return None,
		};
		Some((*index).into())
	}
}

/// Custom location converter to turn sibling chains into u64 accounts.
pub struct SiblingChainToIndex64;
impl ConvertLocation<AccountId> for SiblingChainToIndex64 {
	fn convert_location(location: &Location) -> Option<AccountId> {
		let index = match location.unpack() {
			(1, [Parachain(id)]) => id,
			_ => return None,
		};
		Some((*index).into())
	}
}

/// We alias local account locations to actual local accounts.
/// We also allow sovereign accounts for other sibling chains.
pub type LocationToAccountId = (AccountIndex64Aliases<AnyNetwork, u64>, SiblingChainToIndex64);

pub type NativeTokenTransactor = FungibleAdapter<
	// We use pallet-balances for handling this fungible asset.
	Balances,
	// The fungible asset handled by this transactor is the native token of the chain.
	IsConcrete<HereLocation>,
	// How we convert locations to accounts.
	LocationToAccountId,
	// We need to specify the AccountId type.
	AccountId,
	// We mint the native tokens locally, so we track how many we've sent away via teleports.
	LocalCheckAccount,
>;

pub struct LocationToAssetIdForAssetsPallet;
impl MaybeEquivalence<Location, AssetIdForAssetsPallet> for LocationToAssetIdForAssetsPallet {
	fn convert(location: &Location) -> Option<AssetIdForAssetsPallet> {
		match location.unpack() {
			(1, []) => Some(1 as AssetIdForAssetsPallet),
			_ => None,
		}
	}

	fn convert_back(id: &AssetIdForAssetsPallet) -> Option<Location> {
		match id {
			1 => Some(Location::new(1, [])),
			_ => None,
		}
	}
}

/// AssetTransactor for handling the relay chain token.
pub type RelayTokenTransactor = FungiblesAdapter<
	// We use pallet-assets for handling the relay token.
	AssetsPallet,
	// Matches the relay token.
	ConvertedConcreteId<AssetIdForAssetsPallet, Balance, LocationToAssetIdForAssetsPallet, JustTry>,
	// How we convert locations to accounts.
	LocationToAccountId,
	// We need to specify the AccountId type.
	AccountId,
	// We don't track teleports.
	NoChecking,
	(),
>;

pub type AssetTransactors = (NativeTokenTransactor, RelayTokenTransactor);

pub struct HereAndInnerLocations;
impl Contains<Location> for HereAndInnerLocations {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (0, []) | (0, _))
	}
}

pub type Barrier = (
	TakeWeightCredit, // We need this for pallet-xcm's extrinsics to work.
	AllowTopLevelPaidExecutionFrom<HereAndInnerLocations>, /* TODO: Technically, we should allow
	                   * messages from "AssetHub". */
);

pub type Trader = FixedRateOfFungible<NativeTokenPerSecondPerByte, ()>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = AssetTransactors;
	type OriginConverter = ();
	type IsReserve = RelayTokenToAssetHub;
	type IsTeleporter = NativeTokenToAssetHub;
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = Weigher;
	type Trader = Trader;
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetLocker = ();
	type AssetExchanger = ();
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = ();
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Nothing;
	type Aliasers = Nothing;
	type TransactionalProcessor = ();
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = XcmPallet;
}

/// Converts a signed origin of a u64 account into a location with only the `AccountIndex64`
/// junction.
pub struct SignedToAccountIndex64<RuntimeOrigin, AccountId>(
	PhantomData<(RuntimeOrigin, AccountId)>,
);
impl<RuntimeOrigin: OriginTrait + Clone, AccountId: Into<u64>> TryConvert<RuntimeOrigin, Location>
	for SignedToAccountIndex64<RuntimeOrigin, AccountId>
where
	RuntimeOrigin::PalletsOrigin: From<SystemRawOrigin<AccountId>>
		+ TryInto<SystemRawOrigin<AccountId>, Error = RuntimeOrigin::PalletsOrigin>,
{
	fn try_convert(origin: RuntimeOrigin) -> Result<Location, RuntimeOrigin> {
		origin.try_with_caller(|caller| match caller.try_into() {
			Ok(SystemRawOrigin::Signed(who)) =>
				Ok(Junction::AccountIndex64 { network: None, index: who.into() }.into()),
			Ok(other) => Err(other.into()),
			Err(other) => Err(other),
		})
	}
}

pub type LocalOriginToLocation = SignedToAccountIndex64<RuntimeOrigin, AccountId>;

impl pallet_xcm::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, ()>;
	type XcmRouter = XcmRouter;
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmExecuteFilter = Nothing;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything; // Put everything instead of something more restricted.
	type XcmReserveTransferFilter = Everything; // Same.
	type Weigher = Weigher;
	type UniversalLocation = UniversalLocation;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	type AdvertisedXcmVersion = AdvertisedXcmVersion;
	type AdminOrigin = EnsureRoot<AccountId>;
	type TrustedLockers = ();
	type SovereignAccountOf = ();
	type Currency = Balances;
	type CurrencyMatcher = IsConcrete<HereLocation>;
	type MaxLockers = ConstU32<0>;
	type MaxRemoteLockConsumers = ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
	type WeightInfo = TestWeightInfo;
}

pub fn new_test_ext_with_balances(balances: Vec<(AccountId, Balance)>) -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<TestRuntime>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<TestRuntime> { balances }
		.assimilate_storage(&mut t)
		.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub fn new_test_ext_with_balances_and_assets(
	balances: Vec<(AccountId, Balance)>,
	assets: Vec<(AssetIdForAssetsPallet, AccountId, Balance)>,
) -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<TestRuntime>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<TestRuntime> { balances }
		.assimilate_storage(&mut t)
		.unwrap();

	pallet_assets::GenesisConfig::<TestRuntime> {
		assets: vec![
			// id, owner, is_sufficient, min_balance.
			// We don't actually need this to be sufficient, since we use the native assets in
			// tests for the existential deposit.
			(1, 0, true, 1),
		],
		metadata: vec![
			// id, name, symbol, decimals.
			(1, "Relay Token".into(), "RLY".into(), 12),
		],
		accounts: assets,
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

#[derive(Clone)]
pub(crate) struct TestClient;

pub(crate) struct RuntimeApi {
	_inner: TestClient,
}

impl sp_api::ProvideRuntimeApi<Block> for TestClient {
	type Api = RuntimeApi;
	fn runtime_api(&self) -> sp_api::ApiRef<Self::Api> {
		RuntimeApi { _inner: self.clone() }.into()
	}
}

sp_api::mock_impl_runtime_apis! {
	impl XcmPaymentApi<Block> for RuntimeApi {
		fn query_acceptable_payment_assets(xcm_version: XcmVersion) -> Result<Vec<VersionedAssetId>, XcmPaymentApiError> {
			Ok(vec![
				VersionedAssetId::from(AssetId(HereLocation::get()))
					.into_version(xcm_version)
					.map_err(|_| XcmPaymentApiError::VersionedConversionFailed)?
			])
		}

		fn query_xcm_weight(message: VersionedXcm<()>) -> Result<Weight, XcmPaymentApiError> {
			XcmPallet::query_xcm_weight(message)
		}

		fn query_weight_to_asset_fee(weight: Weight, asset: VersionedAssetId) -> Result<u128, XcmPaymentApiError> {
			match asset.try_as::<AssetId>() {
				Ok(asset_id) if asset_id.0 == HereLocation::get() => {
					Ok(WeightToFee::weight_to_fee(&weight))
				},
				Ok(asset_id) => {
					log::trace!(
						target: "xcm::XcmPaymentApi::query_weight_to_asset_fee",
						"query_weight_to_asset_fee - unhandled asset_id: {asset_id:?}!"
					);
					Err(XcmPaymentApiError::AssetNotFound)
				},
				Err(_) => {
					log::trace!(
						target: "xcm::XcmPaymentApi::query_weight_to_asset_fee",
						"query_weight_to_asset_fee - failed to convert asset: {asset:?}!"
					);
					Err(XcmPaymentApiError::VersionedConversionFailed)
				}
			}
		}

		fn query_delivery_fees(destination: VersionedLocation, message: VersionedXcm<()>) -> Result<VersionedAssets, XcmPaymentApiError> {
			XcmPallet::query_delivery_fees(destination, message)
		}
	}

	impl XcmDryRunApi<Block, RuntimeCall, RuntimeEvent> for RuntimeApi {
		fn dry_run_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> Result<ExtrinsicDryRunEffects<RuntimeEvent>, XcmDryRunApiError> {
			use xcm_executor::RecordXcm;
			// We want to record the XCM that's executed, so we can return it.
			pallet_xcm::Pallet::<TestRuntime>::set_record_xcm(true);
			let result = Executive::apply_extrinsic(extrinsic).map_err(|error| {
				log::error!(
					target: "xcm::XcmDryRunApi::dry_run_extrinsic",
					"Applying extrinsic failed with error {:?}",
					error,
				);
				XcmDryRunApiError::InvalidExtrinsic
			})?;
			// Nothing gets committed to storage in runtime APIs, so there's no harm in leaving the flag as true.
			let local_xcm = pallet_xcm::Pallet::<TestRuntime>::recorded_xcm();
			let forwarded_xcms = sent_xcm()
				.into_iter()
				.map(|(location, message)| (
					VersionedLocation::from(location),
					vec![VersionedXcm::from(message)],
				)).collect();
			let events: Vec<RuntimeEvent> = System::events().iter().map(|record| record.event.clone()).collect();
			Ok(ExtrinsicDryRunEffects {
				local_xcm: local_xcm.map(VersionedXcm::<()>::from),
				forwarded_xcms,
				emitted_events: events,
				execution_result: result,
			})
		}

		fn dry_run_xcm(origin_location: VersionedLocation, xcm: VersionedXcm<RuntimeCall>) -> Result<XcmDryRunEffects<RuntimeEvent>, XcmDryRunApiError> {
			let origin_location: Location = origin_location.try_into().map_err(|error| {
				log::error!(
					target: "xcm::XcmDryRunApi::dry_run_xcm",
					"Location version conversion failed with error: {:?}",
					error,
				);
				XcmDryRunApiError::VersionedConversionFailed
			})?;
			let xcm: Xcm<RuntimeCall> = xcm.try_into().map_err(|error| {
				log::error!(
					target: "xcm::XcmDryRunApi::dry_run_xcm",
					"Xcm version conversion failed with error {:?}",
					error,
				);
				XcmDryRunApiError::VersionedConversionFailed
			})?;
			let mut hash = fake_message_hash(&xcm);
			let result = XcmExecutor::<XcmConfig>::prepare_and_execute(
				origin_location,
				xcm,
				&mut hash,
				Weight::MAX, // Max limit available for execution.
				Weight::zero(),
			);
			let forwarded_xcms = sent_xcm()
				.into_iter()
				.map(|(location, message)| (
					VersionedLocation::from(location),
					vec![VersionedXcm::from(message)],
				)).collect();
			let events: Vec<RuntimeEvent> = System::events().iter().map(|record| record.event.clone()).collect();
			Ok(XcmDryRunEffects {
				forwarded_xcms,
				emitted_events: events,
				execution_result: result,
			})
		}
	}
}
