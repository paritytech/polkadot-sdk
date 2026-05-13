//! Test runtime for `pallet-vaults`.
//!
//! Mirrors the production wiring in `troves.md` §10.2: native DOT lives in
//! `pallet-balances`, foreign tokens (and pUSD) live in
//! `pallet-assets` + `pallet-assets-holder`, and `fungible::UnionOf` glues
//! them into a single `fungibles::*` impl over a unified `AssetId`.
//!
//! Convention used in the tests: `AssetId == 0` means native DOT (routes
//! `Left(())` → `Balances`); any other `AssetId` routes
//! `Right(asset)` → `AssetsHolder`.

use crate as pallet_vaults;
pub use crate::{
	pallet::{BalanceOf, MomentOf, StableCreditOf},
	BranchConfig, BranchMode, Error, Event, HoldReason, Pallet, VaultsManagerLevel,
};
use frame::{
	deps::{
		frame_support::{
			derive_impl, parameter_types,
			traits::{
				fungible::{self, Credit, Inspect as FungibleInspect, ItemOf},
				fungibles::InspectHold,
				AsEnsureOriginWithArg, ConstU128, ConstU64, OnUnbalanced,
			},
			PalletId,
		},
		sp_runtime::{
			traits::{Convert, IdentityLookup},
			BuildStorage, DispatchError, DispatchResult, Either, FixedU128, Permill,
		},
	},
	testing_prelude::*,
};
pub use pallet_linked_list::Position;
use pusd_primitives::{
	KeeperCompensation, LiquidationAllocation, OffsetAllocation, RedemptionAllocation,
	VaultLiquidationInterface, VaultRedemptionInterface,
};

pub type AccountId = u64;
pub type Balance = u128;
pub type AssetId = u32;
pub type Block = MockBlock<Test>;
pub type Moment = u64;

#[frame::deps::frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeTask,
		RuntimeHoldReason,
		RuntimeFreezeReason
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system;

	#[runtime::pallet_index(1)]
	pub type Timestamp = pallet_timestamp;

	#[runtime::pallet_index(2)]
	pub type Balances = pallet_balances;

	#[runtime::pallet_index(3)]
	pub type Assets = pallet_assets;

	#[runtime::pallet_index(4)]
	pub type AssetsHolder = pallet_assets_holder;

	#[runtime::pallet_index(5)]
	pub type LinkedList = pallet_linked_list;

	#[runtime::pallet_index(6)]
	pub type Vaults = pallet_vaults;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId;
	type AccountData = pallet_balances::AccountData<Balance>;
	type Lookup = IdentityLookup<Self::AccountId>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::DefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
	type Balance = Balance;
	type ExistentialDeposit = ConstU128<1>;
	type RuntimeHoldReason = RuntimeHoldReason;
}

impl pallet_timestamp::Config for Test {
	type Moment = Moment;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<1>;
	type WeightInfo = ();
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig as pallet_assets::DefaultConfig)]
impl pallet_assets::Config for Test {
	type AssetId = AssetId;
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<u64>>;
	type ForceOrigin = frame_system::EnsureRoot<u64>;
	type Currency = Balances;
	type Holder = AssetsHolder;
	type Balance = Balance;
}

impl pallet_assets_holder::Config for Test {
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeEvent = RuntimeEvent;
}

parameter_types! {
	pub const MaxHintRepairSteps: u32 = 16;
	pub const MaxBranches: u32 = 8;
	pub const MaxOnIdleVaultRefresh: u32 = 4;
	pub const VaultsPalletId: PalletId = PalletId(*b"pusd/vlt");
	pub const PusdAssetId: AssetId = 1_000;
	pub SpYieldShare: Permill = Permill::from_percent(75);
}

impl pallet_linked_list::Config for Test {
	type WeightInfo = ();
	type ListId = AssetId;
	type ItemId = AccountId;
	type Priority = FixedU128;
	type MaxHintRepairSteps = MaxHintRepairSteps;
	type PriorityProvider = pallet_vaults::Pallet<Test>;
}

/// Routes `AssetId == 0` to native DOT (`Left(())`), every other id to the
/// pallet-assets multi-asset side (`Right(asset)`). Mirrors the production
/// `DotFromLeft` Convert in `troves.md` §10.2 — that one operates on XCM
/// `Location`, this one on `u32` for test ergonomics.
pub struct DotFromZero;
impl Convert<AssetId, Either<(), AssetId>> for DotFromZero {
	fn convert(asset: AssetId) -> Either<(), AssetId> {
		if asset == 0 {
			Either::Left(())
		} else {
			Either::Right(asset)
		}
	}
}

/// Unified collateral surface: `Balances` (single-asset, native) on the
/// left; `AssetsHolder` (multi-asset, hold-aware) on the right.
pub type VaultCollateralAssets =
	fungible::UnionOf<Balances, AssetsHolder, DotFromZero, AssetId, AccountId>;

pub type Pusd = ItemOf<Assets, PusdAssetId, AccountId>;

/// Naive oracle: tests poke `set_price(asset_id, price)`.
pub struct MockOracle;
parameter_types! {
	pub static MockPrices: alloc::collections::BTreeMap<AssetId, FixedU128> =
		alloc::collections::BTreeMap::new();
	pub static MockOracleAvailable: bool = true;
}
impl pusd_primitives::ProvidePrice for MockOracle {
	type AssetId = AssetId;
	type Moment = Moment;

	fn provide_price(
		collateral_id: &AssetId,
	) -> Result<pusd_primitives::PriceFeed<Moment>, frame::deps::sp_runtime::DispatchError> {
		if !MockOracleAvailable::get() {
			return Err(crate::pallet::Error::<Test>::OraclePriceNotAvailable.into());
		}
		match MockPrices::get().get(collateral_id).copied() {
			Some(p) => Ok(pusd_primitives::PriceFeed {
				price: p,
				observed_at: pallet_timestamp::Pallet::<Test>::get(),
			}),
			None => Err(crate::pallet::Error::<Test>::OraclePriceNotAvailable.into()),
		}
	}
}

pub fn set_price(asset: AssetId, price: FixedU128) {
	MockPrices::mutate(|m| {
		m.insert(asset, price);
	});
}

pub fn set_oracle_available(v: bool) {
	MockOracleAvailable::set(v);
}

/// Drops yield credits on the floor: tests don't need a Stability Pool.
pub struct DropYieldSink;
impl pusd_primitives::OnBranchYield<AssetId, Credit<AccountId, Pusd>> for DropYieldSink {
	fn on_branch_yield(
		_collateral_id: AssetId,
		credit: Credit<AccountId, Pusd>,
	) -> frame::deps::frame_support::pallet_prelude::DispatchResult {
		drop(credit);
		Ok(())
	}
}

/// `OnUnbalanced` impl that drops residual fee credits on the floor.
pub struct DropFeeHandler;
impl OnUnbalanced<Credit<AccountId, Pusd>> for DropFeeHandler {
	fn on_nonzero_unbalanced(amount: Credit<AccountId, Pusd>) {
		drop(amount);
	}
}

pub struct VaultsManagerOrigin;
impl frame::deps::frame_support::traits::EnsureOrigin<RuntimeOrigin> for VaultsManagerOrigin {
	type Success = pallet_vaults::VaultsManagerLevel;
	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		match Into::<Result<frame_system::RawOrigin<u64>, RuntimeOrigin>>::into(o.clone()) {
			Ok(frame_system::RawOrigin::Root) => Ok(pallet_vaults::VaultsManagerLevel::Full),
			Ok(frame_system::RawOrigin::Signed(999)) => {
				Ok(pallet_vaults::VaultsManagerLevel::Defensive)
			},
			_ => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		Ok(RuntimeOrigin::root())
	}
}

impl pallet_vaults::Config for Test {
	type RuntimeHoldReason = RuntimeHoldReason;
	type AssetId = AssetId;
	type CollateralAssets = VaultCollateralAssets;
	type StableAsset = Pusd;
	type Oracle = MockOracle;
	type SpYieldSink = DropYieldSink;
	type SpYieldShare = SpYieldShare;
	type FeeHandler = DropFeeHandler;
	type TimeProvider = Timestamp;
	type ManagerOrigin = VaultsManagerOrigin;
	type PalletId = VaultsPalletId;
	type RateIndex = LinkedList;
	type MaxBranches = MaxBranches;
	type MaxOnIdleVaultRefresh = MaxOnIdleVaultRefresh;
	type WeightInfo = ();
}

/// DOT-equivalent collateral asset id used across tests. `0` routes to the
/// native `Balances` pallet via [`DotFromZero`].
pub const DOT: AssetId = 0;

/// A non-native test collateral that lives in `pallet-assets`. Used in tests
/// that exercise the multi-asset side of the union.
pub const TOKEN_X: AssetId = 1;

pub fn new_test_ext() -> TestState {
	let t = RuntimeGenesisConfig {
		assets: pallet_assets::GenesisConfig {
			assets: vec![(TOKEN_X, 1, true, 1), (PusdAssetId::get(), 1, true, 1)],
			metadata: vec![],
			accounts: vec![],
			next_asset_id: None,
			reserves: vec![],
		},
		system: Default::default(),
		balances: pallet_balances::GenesisConfig {
			balances: (1u64..=10u64).map(|i| (i, 1_000_000_000_000)).collect(),
			..Default::default()
		},
	}
	.build_storage()
	.unwrap();
	let mut ext: TestState = t.into();
	ext.execute_with(|| {
		System::set_block_number(1);
		Timestamp::set_timestamp(1_000);
		// Mint Token X to test accounts. Native DOT was already minted via
		// the balances genesis above.
		for who in 1u64..=10 {
			<Assets as frame::deps::frame_support::traits::fungibles::Mutate<u64>>::mint_into(
				TOKEN_X,
				&who,
				1_000_000_000_000,
			)
			.expect("mint Token X in test setup");
		}
		MockPrices::set(alloc::collections::BTreeMap::new());
		MockOracleAvailable::set(true);
	});
	ext
}

/// Run `test` and check post-state invariants under `try-runtime`.
pub fn build_and_execute(test: impl FnOnce()) {
	new_test_ext().execute_with(|| {
		test();
		#[cfg(feature = "try-runtime")]
		crate::try_state::do_try_state::<Test>().expect("post-test invariants hold");
	});
}

/// Default branch config: MCR=110%, ICR=120%, Safety=130%, ceiling 100M,
/// MinDebt=200, MinColl=1, rate bounds 0.1%-100%, 7d upfront fee, 1d
/// rate-cooldown, 5% redistribution penalty.
pub fn default_branch_config() -> pallet_vaults::BranchConfig<Balance, Moment> {
	pallet_vaults::BranchConfig {
		minimum_collateralization_ratio: FixedU128::from_rational(110u128, 100u128),
		initial_collateralization_ratio: FixedU128::from_rational(120u128, 100u128),
		safety_collateralization_ratio: FixedU128::from_rational(130u128, 100u128),
		debt_ceiling: 100_000_000,
		minimum_debt: 200,
		minimum_collateral: 1,
		minimum_borrow_rate: FixedU128::from_rational(1u128, 1_000u128),
		maximum_borrow_rate: FixedU128::from_rational(100u128, 100u128),
		upfront_fee_period: 7 * 24 * 3_600 * 1_000,
		rate_adjustment_cooldown: 24 * 3_600 * 1_000,
		redistribution_penalty: FixedU128::from_rational(5u128, 100u128),
	}
}

pub fn register_default_branch() {
	pallet_vaults::Pallet::<Test>::register_branch(
		RuntimeOrigin::root(),
		DOT,
		default_branch_config(),
	)
	.expect("register_branch ok");
	set_price(DOT, FixedU128::from_rational(10u128, 1u128));
	fund_redistribution_account_for(DOT);
}

pub fn register_branch_for(asset: AssetId) {
	pallet_vaults::Pallet::<Test>::register_branch(
		RuntimeOrigin::root(),
		asset,
		default_branch_config(),
	)
	.expect("register_branch ok");
	set_price(asset, FixedU128::from_rational(10u128, 1u128));
	fund_redistribution_account_for(asset);
}

/// Mint a single existential deposit unit to the redistribution sub-account
/// so it can receive on-hold transfers during a liquidation that redistributes
/// collateral. The pallet's `register_branch` is expected to handle this in
/// production (per DESIGN.md §5.3 "ensures the redistribution account can hold
/// `c`") but currently doesn't — covered by Phase 3 grooming.
pub fn fund_redistribution_account_for(asset: AssetId) {
	use frame::deps::{
		frame_support::traits::fungible::Mutate as FungibleMutate,
		sp_runtime::traits::AccountIdConversion,
	};
	let pallet_account: AccountId = VaultsPalletId::get().into_account_truncating();
	if asset == DOT {
		let _ = <Balances as FungibleMutate<AccountId>>::mint_into(&pallet_account, 1);
	} else {
		let _ =
			<Assets as frame::deps::frame_support::traits::fungibles::Mutate<AccountId>>::mint_into(
				asset,
				&pallet_account,
				1,
			);
	}
}

/// Advance `pallet_timestamp` by `ms` milliseconds without touching block #.
/// Use this for interest-accrual tests where only wall-clock matters.
pub fn advance_time(ms: Moment) {
	let now = pallet_timestamp::Pallet::<Test>::get();
	Timestamp::set_timestamp(now + ms);
}

/// Advance both block number and timestamp by `n` blocks of `ms_per_block`.
/// Use this for `on_idle`-sensitive tests.
pub fn advance_blocks(n: u64, ms_per_block: Moment) {
	for _ in 0..n {
		let block = System::block_number();
		System::set_block_number(block + 1);
		advance_time(ms_per_block);
	}
}

/// Open a vault with default `(None, None)` hints — the most common path in
/// tests that don't care about rate-index ordering. Returns `DispatchResult`
/// so callers can use either `assert_ok!` or `assert_noop!`.
pub fn open(
	who: AccountId,
	asset: AssetId,
	coll: Balance,
	debt: Balance,
	rate: FixedU128,
) -> DispatchResult {
	pallet_vaults::Pallet::<Test>::open_vault(
		RuntimeOrigin::signed(who),
		asset,
		coll,
		debt,
		rate,
		Position::endpoints_only(),
	)
}

/// Drive a liquidation through the trait surface. The allocation absorbs all
/// post-touch debt into the offset path (orchestrator-side coll movement is
/// not modeled — there is no Stability Pool in the unit-test mock); held
/// collateral falls through to the owner as surplus. Use this when a test
/// just needs the vault row removed.
pub fn liquidate(asset: AssetId, owner: AccountId) -> DispatchResult {
	let post_touch =
		<Pallet<Test> as VaultLiquidationInterface<AccountId, AssetId, Balance>>::prepare_liquidation(
			asset, owner,
		)?;
	let alloc = LiquidationAllocation {
		offset: OffsetAllocation { recipient: owner, debt: post_touch, collateral: 0 },
		redistribution_collateral: 0,
		keeper: KeeperCompensation { recipient: owner, collateral: 0 },
	};
	<Pallet<Test> as VaultLiquidationInterface<AccountId, AssetId, Balance>>::finalize_liquidation(
		asset, owner, alloc,
	)
}

/// Same as `liquidate` but the caller supplies the post-touch debt allocation.
/// Used by redistribution-accounting tests that need an explicit
/// offset / redistribution / keeper split. The `build` closure receives the
/// post-touch debt and returns the `LiquidationAllocation`.
pub fn liquidate_with(
	asset: AssetId,
	owner: AccountId,
	build: impl FnOnce(Balance) -> LiquidationAllocation<AccountId, Balance>,
) -> DispatchResult {
	let post_touch =
		<Pallet<Test> as VaultLiquidationInterface<AccountId, AssetId, Balance>>::prepare_liquidation(
			asset, owner,
		)?;
	let alloc = build(post_touch);
	<Pallet<Test> as VaultLiquidationInterface<AccountId, AssetId, Balance>>::finalize_liquidation(
		asset, owner, alloc,
	)
}

/// Drive a redemption through the trait surface. Picks the next target via
/// `next_redemption_target`, sizes `debt_to_cancel` to `min(amount, post_touch_debt)`,
/// and pays `floor(debt_to_cancel / price)` collateral to the redeemer with
/// no fee retained. Returns the owner that was redeemed against.
pub fn redeem(
	asset: AssetId,
	redeemer: AccountId,
	amount: Balance,
) -> Result<AccountId, DispatchError> {
	let target = <Pallet<Test> as VaultRedemptionInterface<AccountId, AssetId, Balance>>::next_redemption_target(
		asset, None,
	)
	.ok_or(DispatchError::Other("no redemption target"))?;
	let post_touch = <Pallet<Test> as VaultRedemptionInterface<AccountId, AssetId, Balance>>::touch_for_redemption(
		asset, target,
	)?;
	let debt_to_cancel = core::cmp::min(amount, post_touch);
	let price = MockPrices::get().get(&asset).copied().expect("price set");
	let coll_to_redeemer =
		(FixedU128::saturating_from_integer(debt_to_cancel) / price).saturating_mul_int(1u128);
	let alloc = RedemptionAllocation {
		debt_to_cancel,
		collateral_to_redeemer: coll_to_redeemer,
		fee_collateral_retained: 0,
	};
	<Pallet<Test> as VaultRedemptionInterface<AccountId, AssetId, Balance>>::apply_redemption(
		asset, target, redeemer, alloc,
	)?;
	Ok(target)
}

/// Held collateral on `(asset, who)` for the `VaultCollateral` reason.
pub fn held(asset: AssetId, who: AccountId) -> Balance {
	<VaultCollateralAssets as InspectHold<AccountId>>::balance_on_hold(
		asset,
		&HoldReason::VaultCollateral.into(),
		&who,
	)
}

/// Total balance of `(asset, who)` on the collateral surface (includes any hold).
pub fn collateral_balance(asset: AssetId, who: AccountId) -> Balance {
	use frame::deps::frame_support::traits::fungibles::Inspect as FungiblesInspect;
	<VaultCollateralAssets as FungiblesInspect<AccountId>>::balance(asset, &who)
}

/// pUSD balance of `who`.
pub fn pusd_balance(who: AccountId) -> Balance {
	<Pusd as FungibleInspect<AccountId>>::balance(&who)
}
