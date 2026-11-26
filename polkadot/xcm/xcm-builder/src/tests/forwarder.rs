// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use crate::{
	AccountId32Aliases, AllowUnpaidExecutionFrom, FixedWeightBounds, FrameTransactionalProcessor,
	FungibleAdapter, IsConcrete, MintLocation, TeleportAndBurnForwarderForAccountId32,
	TeleportForwarderForAccountId32,
};
use core::cell::{Cell, RefCell};
use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{fungible::Inspect, ConstU32, Everything, Get, Nothing},
};
use pallet_accumulate_and_forward::Forwarder;
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use xcm::latest::prelude::*;

type AccountId = AccountId32;
type Block = frame_system::mocking::MockBlock<Test>;

// Mock runtime
construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<AccountId>;
	type AccountData = pallet_balances::AccountData<u128>;
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 1;
}

impl pallet_balances::Config for Test {
	type MaxLocks = ConstU32<0>;
	type Balance = u128;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type MaxReserves = ConstU32<0>;
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type DoneSlashHandler = ();
}

// XCM mock config
parameter_types! {
	pub const HereLocation: Location = Location::here();
	pub static UniversalLocation: InteriorLocation = (ByGenesis([0; 32]), Parachain(42)).into();
	pub static AssetHubLocation: Location = Location::new(1, [Parachain(1000)]);
	// The remote beneficiary location does not matter for these tests since the XCM is never
	// delivered — we only test that local storage is rolled back on failure.
	pub const AccumulationInterior: InteriorLocation = Junctions::Here;
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 64;
	pub const BaseXcmWeight: Weight = Weight::from_parts(1_000, 1_000);
	pub const AnyNetwork: Option<NetworkId> = None;
}

thread_local! {
	static ROUTER_SHOULD_FAIL: Cell<bool> = Cell::new(false);
	static SENT_XCM: RefCell<Vec<(Location, Xcm<()>)>> = RefCell::new(Vec::new());
}

/// We use an XCM router whose behaviour is controlled by the `ROUTER_SHOULD_FAIL` thread-local.
/// When the flag is set, `validate` returns `Err(SendError::NotApplicable)`, simulating a delivery
/// failure that occurs after `WithdrawAsset` has already debited the source account.
struct ControllableRouter;
impl SendXcm for ControllableRouter {
	type Ticket = (Location, Xcm<()>);

	fn validate(
		dest: &mut Option<Location>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		if ROUTER_SHOULD_FAIL.with(|f| f.get()) {
			Err(SendError::NotApplicable)
		} else {
			let dest = dest.take().ok_or(SendError::MissingArgument)?;
			let msg = msg.take().ok_or(SendError::MissingArgument)?;
			let ticket = (dest, msg);
			Ok((ticket, Assets::new()))
		}
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		SENT_XCM.with(|q| q.borrow_mut().push(ticket));
		Ok([0u8; 32])
	}
}

fn sent_xcm() -> Vec<(Location, Xcm<()>)> {
	SENT_XCM.with(|q| q.borrow().clone())
}

/// Perform no teleport tracking by disabling the checking-account mechanism.
struct NoTeleportTracking;
impl Get<Option<(AccountId, MintLocation)>> for NoTeleportTracking {
	fn get() -> Option<(AccountId, MintLocation)> {
		None
	}
}

type DapAssetTransactor = FungibleAdapter<
	Balances,
	IsConcrete<HereLocation>,
	AccountId32Aliases<AnyNetwork, AccountId>,
	AccountId,
	NoTeleportTracking,
>;

struct TestXcmConfig;
impl xcm_executor::Config for TestXcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = ControllableRouter;
	type XcmEventEmitter = ();
	type AssetTransactor = DapAssetTransactor;
	type OriginConverter = ();
	type IsReserve = ();
	// `IsTeleporter` check is skipped in test builds by the executor, so any impl works here.
	type IsTeleporter = Everything;
	type UniversalLocation = UniversalLocation;
	type Barrier = AllowUnpaidExecutionFrom<Everything>;
	type Weigher = FixedWeightBounds<BaseXcmWeight, RuntimeCall, MaxInstructions>;
	type Trader = ();
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetLocker = ();
	type AssetExchanger = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = ();
}

fn new_test_ext(source: AccountId, balance: u128) -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(source, balance)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	SENT_XCM.with(|q| q.borrow_mut().clear());
	t.into()
}

type DepositForwarder = TeleportForwarderForAccountId32<
	TestXcmConfig,
	AssetHubLocation,
	HereLocation,
	AccumulationInterior,
>;
type BurnForwarder =
	TeleportAndBurnForwarderForAccountId32<TestXcmConfig, AssetHubLocation, HereLocation>;

fn remote_native_asset(amount: u128) -> Asset {
	(Location::new(1, [Parachain(42)]), amount).into()
}

fn expected_remote_prefix(source: &AccountId, amount: u128) -> Vec<Instruction<()>> {
	vec![
		ReceiveTeleportedAsset(remote_native_asset(amount).into()),
		AliasOrigin(Location::new(
			1,
			[Parachain(42), Junction::AccountId32 { network: None, id: source.clone().into() }],
		)),
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
	]
}

fn expected_remote_xcm(source: &AccountId, amount: u128, remote: Instruction<()>) -> Xcm<()> {
	let mut expected = expected_remote_prefix(source, amount);
	expected.extend([remote, SetTopic([0u8; 32])]);
	Xcm(expected)
}

/// Verify that when the XCM router fails (simulating a mid-program failure after `WithdrawAsset`
/// has debited the source account), `forward` rolls back all storage changes via
/// `with_transaction`. Both the source balance and total issuance must remain unchanged.
#[test]
fn forward_rolls_back_balance_and_issuance_on_xcm_failure() {
	let source: AccountId = AccountId32::from([1u8; 32]);
	let initial_balance = 1_000u128;

	new_test_ext(source.clone(), initial_balance).execute_with(|| {
		let initial_issuance = Balances::total_issuance();

		ROUTER_SHOULD_FAIL.with(|f| f.set(true));
		let result = DepositForwarder::forward(source.clone(), 500u128);
		ROUTER_SHOULD_FAIL.with(|f| f.set(false));

		assert!(result.is_err(), "expected forward to fail when router fails");
		assert_eq!(
			Balances::balance(&source),
			initial_balance,
			"source balance must be unchanged after XCM failure"
		);
		assert_eq!(
			Balances::total_issuance(),
			initial_issuance,
			"total issuance must be unchanged after XCM failure"
		);
	});
}

#[test]
fn deposit_forwarder_teleports_and_deposits_to_staging() {
	let source: AccountId = AccountId32::from([1u8; 32]);

	new_test_ext(source.clone(), 1_000).execute_with(|| {
		let initial_issuance = Balances::total_issuance();

		assert_eq!(DepositForwarder::forward(source.clone(), 500u128), Ok(()));

		assert_eq!(Balances::balance(&source), 500);
		assert_eq!(Balances::total_issuance(), initial_issuance - 500);

		let expected = expected_remote_xcm(
			&source,
			500,
			DepositAsset {
				assets: Wild(AllCounted(1)),
				beneficiary: AccumulationInterior::get().into_location(),
			},
		);
		assert_eq!(sent_xcm(), vec![(AssetHubLocation::get(), expected)]);
	});
}

#[test]
fn burn_forwarder_teleports_and_burns_reanchored_asset() {
	let source: AccountId = AccountId32::from([1u8; 32]);

	new_test_ext(source.clone(), 1_000).execute_with(|| {
		let initial_issuance = Balances::total_issuance();

		assert_eq!(BurnForwarder::forward(source.clone(), 500u128), Ok(()));

		assert_eq!(Balances::balance(&source), 500);
		assert_eq!(Balances::total_issuance(), initial_issuance - 500);

		let expected =
			expected_remote_xcm(&source, 500, BurnAsset(remote_native_asset(500).into()));
		assert_eq!(sent_xcm(), vec![(AssetHubLocation::get(), expected)]);
	});
}

#[test]
fn burn_forwarder_rolls_back_balance_and_issuance_on_xcm_failure() {
	let source: AccountId = AccountId32::from([1u8; 32]);
	let initial_balance = 1_000u128;

	new_test_ext(source.clone(), initial_balance).execute_with(|| {
		let initial_issuance = Balances::total_issuance();

		ROUTER_SHOULD_FAIL.with(|f| f.set(true));
		let result = BurnForwarder::forward(source.clone(), 500u128);
		ROUTER_SHOULD_FAIL.with(|f| f.set(false));

		assert!(result.is_err(), "expected forward to fail when router fails");
		assert_eq!(Balances::balance(&source), initial_balance);
		assert_eq!(Balances::total_issuance(), initial_issuance);
		assert!(sent_xcm().is_empty());
	});
}

#[test]
fn burn_forwarder_fails_without_side_effects_when_reanchor_fails() {
	let source: AccountId = AccountId32::from([1u8; 32]);
	let initial_balance = 1_000u128;

	new_test_ext(source.clone(), initial_balance).execute_with(|| {
		let initial_issuance = Balances::total_issuance();

		// More than `MAX_JUNCTIONS` parents cannot be reanchored to.
		AssetHubLocation::set(Location::new(9, Here));
		let result = BurnForwarder::forward(source.clone(), 500u128);

		assert!(result.is_err(), "expected forward to fail when reanchoring fails");
		assert_eq!(Balances::balance(&source), initial_balance);
		assert_eq!(Balances::total_issuance(), initial_issuance);
		assert!(sent_xcm().is_empty());
	});
}
