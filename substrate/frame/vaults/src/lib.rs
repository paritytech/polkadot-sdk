//! # Vaults
//!
//! Vaults engine for the pUSD protocol. Users lock
//! collateral, mint pUSD, and pick a per-vault annual borrow rate. Redemptions
//! walk the rate index tail-first (lower-rate-first), with a `FinalRecovery`
//! FIFO served before the rate index for last-eligible-vault settlement.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod helpers;
mod interfaces;
mod math;
mod recovery;
pub mod types;
pub mod weights;

#[cfg(feature = "try-runtime")]
mod try_state;

#[cfg(test)]
pub mod mock;

#[cfg(test)]
mod tests;

pub use pallet::*;
pub use pusd_primitives;
pub use types::{
	BranchConfig, BranchConfigUpdate, BranchDebt, BranchMode, BranchQueues, BranchStakes,
	BranchState, DebtPayment, FrozenReason, FrozenState, ParameterId, RedistSnapshot, Vault,
	VaultDebt, VaultListId, VaultStatus, VaultsManagerLevel,
};
pub use weights::WeightInfo;

pub(crate) const LOG_TARGET: &str = "runtime::vaults";

/// Convenience macro mirroring `pallet-linked-list`'s log helper.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		frame::log::$level!(
			target: $crate::LOG_TARGET,
			concat!("[{:?}] [{}] ", $patter),
			<frame_system::Pallet<T>>::block_number(),
			<$crate::Pallet::<T> as frame::deps::frame_support::traits::PalletInfoAccess>::name()
			$(, $values)*
		)
	};
}

#[frame::pallet]
pub mod pallet {
	use super::*;
	use crate::{helpers, recovery, types::VaultsManagerLevel};
	use frame::{
		deps::{
			frame_support::{
				traits::{
					fungible::{self, Balanced as FungibleBalanced, Mutate as FungibleMutate},
					fungibles::{Inspect as FungiblesInspect, MutateHold as FungiblesMutateHold},
					OnUnbalanced, Time,
				},
				PalletId,
			},
			sp_runtime::{traits::AccountIdConversion, FixedPointOperand, FixedU128, Permill},
		},
		prelude::*,
	};
	use pallet_linked_list::{Position, PriorityProvider, SortedListInterface};
	use pusd_primitives::{BranchModeProvider, OnBranchYield, ProvidePrice};

	pub type BalanceOf<T> = <<T as Config>::CollateralAssets as FungiblesInspect<
		<T as frame_system::Config>::AccountId,
	>>::Balance;
	pub type MomentOf<T> = <<T as Config>::TimeProvider as Time>::Moment;
	pub type StableCreditOf<T> =
		fungible::Credit<<T as frame_system::Config>::AccountId, <T as Config>::StableAsset>;

	pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Outer hold-reason type. Must convert from the pallet's
		/// [`HoldReason`] enum so we can hold collateral on user accounts.
		type RuntimeHoldReason: From<HoldReason>;

		/// Identifier for collateral assets.
		type AssetId: Parameter + Member + Copy + Ord + MaxEncodedLen;

		/// Multi-asset collateral implementation. Balance must be a
		/// [`FixedPointOperand`] so the pallet's `FixedU128`-based math can
		/// operate on it directly without round-tripping through `u128`.
		type CollateralAssets: FungiblesMutateHold<
			Self::AccountId,
			AssetId = Self::AssetId,
			Balance: FixedPointOperand,
			Reason = Self::RuntimeHoldReason,
		>;

		/// The stable asset used to represent pUSD.
		type StableAsset: FungibleMutate<Self::AccountId, Balance = BalanceOf<Self>>
			+ FungibleBalanced<Self::AccountId>;

		/// The Oracle providing timestamped asset prices.
		type Oracle: ProvidePrice<AssetId = Self::AssetId, Moment = MomentOf<Self>>;

		/// Branch-aware sink for the SP share of minted yield. Implemented by
		/// `pallet-stability-pool` in production. Must consume the credit and
		/// either resolve it (`Balanced::resolve`) or pair it against a
		/// rescind so the imbalance nets to zero.
		type SpYieldSink: OnBranchYield<Self::AssetId, StableCreditOf<Self>>;

		/// Fraction of newly minted pUSD fees routed to `SpYieldSink`. The
		/// remainder is forwarded to `FeeHandler`.
		type SpYieldShare: Get<Permill>;

		/// Runtime-configured destination for the residual (non-SP) share of
		/// minted pUSD fees.
		type FeeHandler: OnUnbalanced<StableCreditOf<Self>>;

		/// Time provider for fee accrual using UNIX timestamps in millis.
		/// Moments must convert to `u64` so the pallet can compute durations
		/// in milliseconds.
		type TimeProvider: Time;

		/// Origin allowed to update protocol parameters. Returns the manager
		/// tier so the call site can gate non-defensive operations.
		type ManagerOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = VaultsManagerLevel>;

		/// Sorted-DLL backing the per-branch rate index and FinalRecovery FIFO.
		/// Configured by the runtime to point at `pallet-linked-list` with
		/// `ListId = VaultListId<Self::AssetId>`, `ItemId = Self::AccountId`,
		/// `Priority = FixedU128`.
		type VaultLists: SortedListInterface<
			VaultListId<Self::AssetId>,
			Self::AccountId,
			Priority = FixedU128,
		>;

		/// Pallet-derived redistribution holding account (collateral parking
		/// during liquidation handoff).
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Maximum registered collateral branches.
		#[pallet::constant]
		type MaxBranches: Get<u32> + Get<Option<u32>>;

		/// Maximum vaults the `on_idle` cursor refreshes per block. Bounds
		/// idle-block weight regardless of branch count.
		#[pallet::constant]
		type MaxOnIdleVaultRefresh: Get<u32>;

		/// Weight metadata.
		type WeightInfo: weights::WeightInfo;
	}

	/// Hold reason used to lock collateral against the vault owner's
	/// account.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Held collateral backing an open vault.
		VaultCollateral,
	}

	/// Source-of-truth vault rows, keyed by `(collateral_id, owner)`.
	#[pallet::storage]
	pub type Vaults<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::AssetId,
		Blake2_128Concat,
		T::AccountId,
		Vault<BalanceOf<T>, MomentOf<T>>,
		OptionQuery,
	>;

	/// Per-branch governance/risk parameters. A registered branch always has
	/// a row.
	#[pallet::storage]
	pub type BranchConfigs<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AssetId,
		BranchConfig<BalanceOf<T>, MomentOf<T>>,
		OptionQuery,
		GetDefault,
		T::MaxBranches,
	>;

	/// Per-branch hot accounting state.
	#[pallet::storage]
	pub type BranchStates<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AssetId,
		BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
		OptionQuery,
		GetDefault,
		T::MaxBranches,
	>;

	/// Bounded registry of supported collateral branches.
	#[pallet::storage]
	pub type Branches<T: Config> =
		StorageValue<_, BoundedVec<T::AssetId, T::MaxBranches>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		VaultOpened {
			collateral_id: T::AssetId,
			owner: T::AccountId,
		},
		VaultStatusChanged {
			collateral_id: T::AssetId,
			owner: T::AccountId,
			old_status: VaultStatus,
			new_status: VaultStatus,
		},
		FinalRecoveryEntered {
			collateral_id: T::AssetId,
			owner: T::AccountId,
		},
		FinalRecoveryExited {
			collateral_id: T::AssetId,
			owner: T::AccountId,
		},
		BadDebtRecorded {
			collateral_id: T::AssetId,
			owner: T::AccountId,
			amount: BalanceOf<T>,
		},
		BadDebtHealed {
			collateral_id: T::AssetId,
			amount: BalanceOf<T>,
		},
		CollateralDeposited {
			collateral_id: T::AssetId,
			owner: T::AccountId,
			from: T::AccountId,
			amount: BalanceOf<T>,
		},
		CollateralWithdrawn {
			collateral_id: T::AssetId,
			owner: T::AccountId,
			recipient: T::AccountId,
			amount: BalanceOf<T>,
		},
		Borrowed {
			collateral_id: T::AssetId,
			owner: T::AccountId,
			recipient: T::AccountId,
			amount: BalanceOf<T>,
		},
		Repaid {
			collateral_id: T::AssetId,
			owner: T::AccountId,
			from: T::AccountId,
			amount: BalanceOf<T>,
		},
		VaultClosed {
			collateral_id: T::AssetId,
			owner: T::AccountId,
			recipient: T::AccountId,
		},
		InterestAccrued {
			collateral_id: T::AssetId,
			owner: T::AccountId,
			amount: BalanceOf<T>,
		},
		UpfrontFeeCharged {
			collateral_id: T::AssetId,
			owner: T::AccountId,
			amount: BalanceOf<T>,
		},
		BorrowRateChanged {
			collateral_id: T::AssetId,
			owner: T::AccountId,
			old_rate: FixedU128,
			new_rate: FixedU128,
		},
		ModeChanged {
			collateral_id: T::AssetId,
			old_mode: BranchMode,
			new_mode: BranchMode,
		},
		ParameterUpdated {
			collateral_id: T::AssetId,
			parameter: types::ParameterId,
		},
		DebtCeilingUpdated {
			collateral_id: T::AssetId,
			old_value: BalanceOf<T>,
			new_value: BalanceOf<T>,
		},
		BranchRegistered {
			collateral_id: T::AssetId,
		},
		VaultRedeemed {
			collateral_id: T::AssetId,
			owner: T::AccountId,
			redeemer: T::AccountId,
			debt_cancelled: BalanceOf<T>,
			collateral_to_redeemer: BalanceOf<T>,
			fee_collateral_retained: BalanceOf<T>,
			vault_annual_rate: FixedU128,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		VaultNotFound,
		VaultAlreadyExists,
		InvalidVaultStatus,
		VaultInFinalRecovery,
		UnknownCollateral,
		BranchAlreadyRegistered,
		TooManyBranches,
		DebtBelowMinimum,
		DebtWouldBecomeDust,
		DebtCeilingExceeded,
		RateOutOfBounds,
		UnsafeCollateralizationRatio,
		SafetyModeTcrWorsening,
		BranchFrozen,
		OraclePriceNotAvailable,
		OracleStale,
		InvalidPositionHints,
		RateIndexInvariantBroken,
		FinalRecoveryInvariantBroken,
		FinalRecoverySequenceOverflow,
		NotLastEligibleVault,
		InsufficientCollateral,
		InsufficientRepayment,
		ArithmeticOverflow,
		InsufficientPrivilege,
		DefensiveActionNotDefensive,
		InvalidLiquidationAllocation,
		InvalidRedemptionAllocation,
		LastVaultCannotBeLiquidated,
		RedistributionWouldOverflow,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			assert!(<T::MaxBranches as Get<u32>>::get() > 0, "`MaxBranches` must be > 0");
		}

		fn on_idle(
			_block: BlockNumberFor<T>,
			remaining: frame::deps::frame_support::weights::Weight,
		) -> frame::deps::frame_support::weights::Weight {
			helpers::on_idle_walk::<T>(remaining)
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_: BlockNumberFor<T>) -> Result<(), frame::try_runtime::TryRuntimeError> {
			crate::try_state::do_try_state::<T>()
		}
	}

	/// View functions exposed to runtime API consumers (wallets, indexers).
	#[pallet::view_functions]
	impl<T: Config> Pallet<T> {
		/// Fully-accrued collateralization ratio of `(collateral_id, owner)`.
		pub fn vault_cr(collateral_id: T::AssetId, owner: T::AccountId) -> Option<FixedU128> {
			helpers::view_vault_cr::<T>(&collateral_id, &owner)
		}

		/// Derived lifecycle status of `(collateral_id, owner)`.
		pub fn vault_status(collateral_id: T::AssetId, owner: T::AccountId) -> Option<VaultStatus> {
			helpers::view_vault_status::<T>(&collateral_id, &owner)
		}

		/// Branch TCR, including aggregate interest accrued since the last
		/// update so off-chain observers see the value the runtime would
		/// compute on the next write.
		pub fn branch_tcr(collateral_id: T::AssetId) -> Option<FixedU128> {
			helpers::view_branch_tcr::<T>(&collateral_id)
		}

		/// Registered branches.
		pub fn branches() -> alloc::vec::Vec<T::AssetId> {
			Branches::<T>::get().into_inner()
		}

		/// First `n` vault owners in actual redemption order: `FinalRecovery`
		/// FIFO first, then `last_dormant_vault_owner`, then the rate index
		/// tail-first.
		pub fn redemption_queue_head(
			collateral_id: T::AssetId,
			n: u32,
		) -> alloc::vec::Vec<T::AccountId> {
			helpers::view_redemption_queue_head::<T>(&collateral_id, n)
		}

		/// First `n` `FinalRecovery` owners in FIFO order.
		pub fn final_recovery_queue_head(
			collateral_id: T::AssetId,
			n: u32,
		) -> alloc::vec::Vec<T::AccountId> {
			recovery::queue_head::<T>(&collateral_id, n)
		}

		/// Rate-index insert hint for `rate` on `collateral_id`.
		pub fn find_rate_position(
			collateral_id: T::AssetId,
			rate: FixedU128,
		) -> Position<T::AccountId> {
			T::VaultLists::find_position(&VaultListId::Rate(collateral_id), rate)
		}

		/// Rate-index re-insert hint for moving `(collateral_id, owner)` to
		/// `new_rate`. `None` if the vault is not in the rate index.
		pub fn find_re_insert_position(
			collateral_id: T::AssetId,
			owner: T::AccountId,
			new_rate: FixedU128,
		) -> Option<Position<T::AccountId>> {
			T::VaultLists::find_re_insert_position(
				&VaultListId::Rate(collateral_id),
				&owner,
				new_rate,
			)
		}

		/// Steps the on-chain repair walk would take for `(rate, hint)` on
		/// `collateral_id`.
		pub fn repair_steps_needed(
			collateral_id: T::AssetId,
			rate: FixedU128,
			hint: Position<T::AccountId>,
		) -> u32 {
			T::VaultLists::repair_steps_needed(&VaultListId::Rate(collateral_id), rate, hint)
		}

		/// Current rate-index neighbors of `(collateral_id, owner)`. `None`
		/// when the vault is not in the rate index.
		pub fn vault_rate_index_neighbors(
			collateral_id: T::AssetId,
			owner: T::AccountId,
		) -> Option<Position<T::AccountId>> {
			T::VaultLists::neighbors(&VaultListId::Rate(collateral_id), &owner)
		}

		/// Total active-vault interest-bearing debt at rates strictly less
		/// than `rate`.
		pub fn debt_in_front(collateral_id: T::AssetId, rate: FixedU128) -> BalanceOf<T> {
			helpers::view_debt_in_front::<T>(&collateral_id, rate)
		}

		/// Predict the upfront fee `open_vault` would charge for
		/// `(initial_debt, annual_rate)` against the current branch state.
		pub fn predict_open_upfront_fee(
			collateral_id: T::AssetId,
			initial_debt: BalanceOf<T>,
			annual_rate: FixedU128,
		) -> BalanceOf<T> {
			helpers::predict_upfront_fee_open::<T>(&collateral_id, initial_debt, annual_rate)
		}

		/// Predict the upfront fee `borrow` would charge.
		pub fn predict_borrow_upfront_fee(
			collateral_id: T::AssetId,
			owner: T::AccountId,
			debt_increase: BalanceOf<T>,
			maybe_new_rate: Option<FixedU128>,
		) -> BalanceOf<T> {
			helpers::predict_upfront_fee_borrow::<T>(
				&collateral_id,
				&owner,
				debt_increase,
				maybe_new_rate,
			)
		}

		/// Predict the upfront fee `change_rate` would charge — `0` when the
		/// cooldown has elapsed.
		pub fn predict_rate_change_upfront_fee(
			collateral_id: T::AssetId,
			owner: T::AccountId,
			new_rate: FixedU128,
		) -> BalanceOf<T> {
			helpers::predict_upfront_fee_rate_change::<T>(&collateral_id, &owner, new_rate)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Open a new vault.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::open_vault())]
		pub fn open_vault(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			initial_collateral: BalanceOf<T>,
			initial_debt: BalanceOf<T>,
			annual_rate: FixedU128,
			hint: Position<T::AccountId>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			helpers::open_vault::<T>(
				who,
				collateral_id,
				initial_collateral,
				initial_debt,
				annual_rate,
				hint,
			)
		}

		/// Permissionless deposit-into-vault: caller spends their own
		/// collateral to deposit into `(collateral_id, owner)`.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::deposit_collateral_for())]
		pub fn deposit_collateral_for(
			origin: OriginFor<T>,
			owner: T::AccountId,
			collateral_id: T::AssetId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let from = ensure_signed(origin)?;
			helpers::deposit_collateral_for::<T>(from, owner, collateral_id, amount)
		}

		/// Withdraw collateral from caller's vault on `collateral_id`.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::withdraw_collateral())]
		pub fn withdraw_collateral(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			amount: BalanceOf<T>,
			recipient: Option<T::AccountId>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			helpers::withdraw_collateral::<T>(who, collateral_id, amount, recipient)
		}

		/// Borrow more pUSD from caller's vault, optionally adjusting the
		/// rate. May revive a `Dormant` vault.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::borrow())]
		pub fn borrow(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			amount: BalanceOf<T>,
			maybe_new_rate: Option<FixedU128>,
			recipient: Option<T::AccountId>,
			hint: Position<T::AccountId>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			helpers::borrow::<T>(who, collateral_id, amount, maybe_new_rate, recipient, hint)
		}

		/// Permissionless repay-into-vault.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::repay_for())]
		pub fn repay_for(
			origin: OriginFor<T>,
			owner: T::AccountId,
			collateral_id: T::AssetId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let from = ensure_signed(origin)?;
			helpers::repay_for::<T>(from, owner, collateral_id, amount)
		}

		/// Change the borrow rate of caller's vault.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::change_rate())]
		pub fn change_rate(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			new_rate: FixedU128,
			hint: Position<T::AccountId>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			helpers::change_rate::<T>(who, collateral_id, new_rate, hint)
		}

		/// Close caller's vault. Vault must have zero debt or the caller
		/// must repay-to-close in the same operation.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::close_vault())]
		pub fn close_vault(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			recipient: Option<T::AccountId>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			helpers::close_vault::<T>(who, collateral_id, recipient)
		}

		/// Permissionless: refresh aggregate/vault interest and apply pending
		/// redistribution to `(collateral_id, owner)`.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::poke())]
		pub fn poke(
			origin: OriginFor<T>,
			owner: T::AccountId,
			collateral_id: T::AssetId,
		) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			helpers::poke::<T>(owner, collateral_id)
		}

		/// Permissionless: move an unsafe last-eligible vault into
		/// `FinalRecovery`.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::enter_final_recovery())]
		pub fn enter_final_recovery(
			origin: OriginFor<T>,
			owner: T::AccountId,
			collateral_id: T::AssetId,
		) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			helpers::enter_final_recovery::<T>(owner, collateral_id)
		}

		/// Permissionless: exit `FinalRecovery` once the fully-accrued vault CR
		/// is back above `MinimumCollateralizationRatio`. Caller supplies the
		/// rate-index `hint` used to reinsert in O(1).
		#[pallet::call_index(22)]
		#[pallet::weight(T::WeightInfo::enter_final_recovery())]
		pub fn exit_final_recovery(
			origin: OriginFor<T>,
			owner: T::AccountId,
			collateral_id: T::AssetId,
			hint: Position<T::AccountId>,
		) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			helpers::exit_final_recovery::<T>(owner, collateral_id, hint)
		}

		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::register_branch())]
		pub fn register_branch(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			config: BranchConfig<BalanceOf<T>, MomentOf<T>>,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(matches!(level, VaultsManagerLevel::Full), Error::<T>::InsufficientPrivilege);
			helpers::register_branch::<T>(collateral_id, config)
		}

		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::set_param())]
		pub fn set_minimum_collateralization_ratio(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			value: FixedU128,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			let cfg = helpers::current_branch_config::<T>(collateral_id)?;
			ensure!(
				matches!(level, VaultsManagerLevel::Full) ||
					value >= cfg.minimum_collateralization_ratio,
				Error::<T>::DefensiveActionNotDefensive
			);
			helpers::update_branch_config::<T>(
				collateral_id,
				BranchConfigUpdate::MinimumCollateralizationRatio(value),
			)
		}

		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::set_param())]
		pub fn set_initial_collateralization_ratio(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			value: FixedU128,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			let cfg = helpers::current_branch_config::<T>(collateral_id)?;
			ensure!(
				matches!(level, VaultsManagerLevel::Full) ||
					value >= cfg.initial_collateralization_ratio,
				Error::<T>::DefensiveActionNotDefensive
			);
			helpers::update_branch_config::<T>(
				collateral_id,
				BranchConfigUpdate::InitialCollateralizationRatio(value),
			)
		}

		#[pallet::call_index(13)]
		#[pallet::weight(T::WeightInfo::set_param())]
		pub fn set_safety_collateralization_ratio(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			value: FixedU128,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			let cfg = helpers::current_branch_config::<T>(collateral_id)?;
			ensure!(
				matches!(level, VaultsManagerLevel::Full) ||
					value >= cfg.safety_collateralization_ratio,
				Error::<T>::DefensiveActionNotDefensive
			);
			helpers::update_branch_config::<T>(
				collateral_id,
				BranchConfigUpdate::SafetyCollateralizationRatio(value),
			)
		}

		#[pallet::call_index(14)]
		#[pallet::weight(T::WeightInfo::set_param())]
		pub fn set_debt_ceiling(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			value: BalanceOf<T>,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			let cfg = helpers::current_branch_config::<T>(collateral_id)?;
			let old = cfg.debt_ceiling;
			ensure!(
				matches!(level, VaultsManagerLevel::Full) || value <= old,
				Error::<T>::DefensiveActionNotDefensive
			);
			BranchConfigs::<T>::mutate(collateral_id, |maybe| {
				if let Some(c) = maybe {
					c.debt_ceiling = value;
				}
			});
			Self::deposit_event(Event::DebtCeilingUpdated {
				collateral_id,
				old_value: old,
				new_value: value,
			});
			Ok(())
		}

		#[pallet::call_index(15)]
		#[pallet::weight(T::WeightInfo::set_param())]
		pub fn set_minimum_debt(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			value: BalanceOf<T>,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(matches!(level, VaultsManagerLevel::Full), Error::<T>::InsufficientPrivilege);
			helpers::update_branch_config::<T>(
				collateral_id,
				BranchConfigUpdate::MinimumDebt(value),
			)
		}

		#[pallet::call_index(16)]
		#[pallet::weight(T::WeightInfo::set_param())]
		pub fn set_minimum_collateral(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			value: BalanceOf<T>,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(matches!(level, VaultsManagerLevel::Full), Error::<T>::InsufficientPrivilege);
			helpers::update_branch_config::<T>(
				collateral_id,
				BranchConfigUpdate::MinimumCollateral(value),
			)
		}

		#[pallet::call_index(23)]
		#[pallet::weight(T::WeightInfo::set_param())]
		pub fn set_minimum_total_stakes(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			value: BalanceOf<T>,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(matches!(level, VaultsManagerLevel::Full), Error::<T>::InsufficientPrivilege);
			helpers::update_branch_config::<T>(
				collateral_id,
				BranchConfigUpdate::MinimumTotalStakes(value),
			)
		}

		#[pallet::call_index(17)]
		#[pallet::weight(T::WeightInfo::set_param())]
		pub fn set_borrow_rate_bounds(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			min_rate: FixedU128,
			max_rate: FixedU128,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			let cfg = helpers::current_branch_config::<T>(collateral_id)?;
			ensure!(
				matches!(level, VaultsManagerLevel::Full) ||
					(max_rate <= cfg.maximum_borrow_rate && min_rate >= cfg.minimum_borrow_rate),
				Error::<T>::DefensiveActionNotDefensive
			);
			helpers::update_branch_config::<T>(
				collateral_id,
				BranchConfigUpdate::BorrowRateBounds { min: min_rate, max: max_rate },
			)
		}

		#[pallet::call_index(18)]
		#[pallet::weight(T::WeightInfo::set_param())]
		pub fn set_upfront_fee_period(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			value: MomentOf<T>,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(matches!(level, VaultsManagerLevel::Full), Error::<T>::InsufficientPrivilege);
			helpers::update_branch_config::<T>(
				collateral_id,
				BranchConfigUpdate::UpfrontFeePeriod(value),
			)
		}

		#[pallet::call_index(19)]
		#[pallet::weight(T::WeightInfo::set_param())]
		pub fn set_rate_adjustment_cooldown(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			value: MomentOf<T>,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(matches!(level, VaultsManagerLevel::Full), Error::<T>::InsufficientPrivilege);
			helpers::update_branch_config::<T>(
				collateral_id,
				BranchConfigUpdate::RateAdjustmentCooldown(value),
			)
		}

		#[pallet::call_index(20)]
		#[pallet::weight(T::WeightInfo::set_param())]
		pub fn set_redistribution_penalty(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
			value: FixedU128,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(matches!(level, VaultsManagerLevel::Full), Error::<T>::InsufficientPrivilege);
			helpers::update_branch_config::<T>(
				collateral_id,
				BranchConfigUpdate::RedistributionPenalty(value),
			)
		}

		/// Force the branch into `Frozen` mode. Any manager tier may issue
		/// this — defensive override.
		#[pallet::call_index(21)]
		#[pallet::weight(T::WeightInfo::enable_frozen_mode())]
		pub fn enable_frozen_mode(
			origin: OriginFor<T>,
			collateral_id: T::AssetId,
		) -> DispatchResult {
			let _level = T::ManagerOrigin::ensure_origin(origin)?;
			helpers::enable_frozen_mode::<T>(collateral_id)
		}
	}

	/// `BranchModeProvider` implementation so other pallets can query the
	/// derived/persisted mode without depending on us at the trait surface.
	impl<T: Config> BranchModeProvider<T::AssetId> for Pallet<T> {
		fn mode(collateral_id: &T::AssetId) -> Option<BranchMode> {
			helpers::current_mode::<T>(collateral_id).ok()
		}
	}

	/// `PriorityProvider` so `pallet-linked-list` can read authoritative rates
	/// from us when relisting a drifted node.
	impl<T: Config> PriorityProvider<VaultListId<T::AssetId>, T::AccountId> for Pallet<T> {
		type Priority = FixedU128;
		fn priority(list_id: &VaultListId<T::AssetId>, item: &T::AccountId) -> Option<FixedU128> {
			match list_id {
				VaultListId::Rate(collateral_id) => {
					Vaults::<T>::get(collateral_id, item).map(|v| v.annual_rate)
				},
				VaultListId::FinalRecovery(_) => T::VaultLists::priority(list_id, item),
			}
		}
	}

	impl<T: Config> Pallet<T> {
		/// Account that holds redistribution-pending collateral and any
		/// transient fee surpluses.
		pub fn redistribution_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}
	}
}
