//! Migration to V1: Initialize pallet parameters.
//!
//! This migration sets initial values for all pallet parameters when deploying
//! to an existing chain. It should be included in the runtime's
//! migration list when first adding the vaults pallet.
//!
//! # Usage
//!
//! Include in the runtime migrations:
//!
//! ```ignore
//! pub type Migrations = (
//!     pallet_vaults::migrations::v1::MigrateV0ToV1<Runtime, VaultsInitialConfig>,
//!     // ... other migrations
//! );
//! ```
//!
//! Where `VaultsInitialConfig` implements [`InitialVaultsConfig`]:
//!
//! ```ignore
//! pub struct VaultsInitialConfig;
//! impl pallet_vaults::migrations::v1::InitialVaultsConfig<Runtime> for VaultsInitialConfig {
//!     fn minimum_collateralization_ratio() -> FixedU128 {
//!         FixedU128::saturating_from_rational(150, 100) // 150%
//!     }
//!     fn initial_collateralization_ratio() -> FixedU128 {
//!         FixedU128::saturating_from_rational(175, 100) // 175%
//!     }
//!     // ... etc
//! }
//! ```

use crate::{
	pallet::{
		InitialCollateralizationRatio, LiquidationPenalty, MaxLiquidationAmount, MaxPositionAmount,
		MaximumIssuance, MinimumCollateralizationRatio, MinimumDeposit, MinimumMint,
		OracleStalenessThreshold, StabilityFee, StaleVaultThreshold,
	},
	BalanceOf, Config, MomentOf, Pallet,
};
use frame_support::{pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade};
use sp_runtime::{traits::SaturatedConversion, FixedU128, Permill};

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// Configuration trait for initial parameter values.
///
/// Implement this trait in the runtime to specify the initial values
/// for vaults pallet parameters.
pub trait InitialVaultsConfig<T: Config> {
	/// Minimum collateralization ratio (e.g., 150% = 1.5).
	/// Below this ratio, vaults become eligible for liquidation.
	fn minimum_collateralization_ratio() -> FixedU128;

	/// Initial collateralization ratio (e.g., 175% = 1.75).
	/// Required when minting new debt. Must be >= minimum ratio.
	fn initial_collateralization_ratio() -> FixedU128;

	/// Annual stability fee as Permill (e.g., 5% = `Permill::from_percent(5)`).
	fn stability_fee() -> Permill;

	/// Liquidation penalty as Permill (e.g., 13% = `Permill::from_percent(13)`).
	fn liquidation_penalty() -> Permill;

	/// Maximum total pUSD debt allowed in the system.
	fn maximum_issuance() -> BalanceOf<T>;

	/// Maximum pUSD at risk in active auctions.
	fn max_liquidation_amount() -> BalanceOf<T>;

	/// Maximum pUSD debt for a single vault.
	fn max_position_amount() -> BalanceOf<T>;

	/// Minimum DOT required to create a vault.
	fn minimum_deposit() -> BalanceOf<T>;

	/// Minimum pUSD amount that can be minted in a single operation.
	fn minimum_mint() -> BalanceOf<T>;

	/// Milliseconds before a vault is considered stale for on_idle processing.
	fn stale_vault_threshold() -> u64;

	/// Maximum age (milliseconds) of oracle price before operations pause.
	fn oracle_staleness_threshold() -> u64;
}

/// Migration logic for V0 -> V1.
///
/// This struct implements the actual migration. It is wrapped by
/// [`MigrateToV1`] which uses [`VersionedMigration`] to handle version
/// checking and updating automatically.
pub struct InnerMigrateV0ToV1<T, I>(core::marker::PhantomData<(T, I)>);

impl<T: Config, I: InitialVaultsConfig<T>> UncheckedOnRuntimeUpgrade for InnerMigrateV0ToV1<T, I> {
	fn on_runtime_upgrade() -> Weight {
		log::info!(
			target: crate::pallet::LOG_TARGET,
			"Running MigrateToV1: initializing vaults pallet parameters"
		);

		// Set all parameters
		MinimumCollateralizationRatio::<T>::put(I::minimum_collateralization_ratio());
		InitialCollateralizationRatio::<T>::put(I::initial_collateralization_ratio());
		StabilityFee::<T>::put(I::stability_fee());
		LiquidationPenalty::<T>::put(I::liquidation_penalty());
		MaximumIssuance::<T>::put(I::maximum_issuance());
		MaxLiquidationAmount::<T>::put(I::max_liquidation_amount());
		MaxPositionAmount::<T>::put(I::max_position_amount());
		MinimumDeposit::<T>::put(I::minimum_deposit());
		MinimumMint::<T>::put(I::minimum_mint());
		StaleVaultThreshold::<T>::put(I::stale_vault_threshold().saturated_into::<MomentOf<T>>());
		OracleStalenessThreshold::<T>::put(
			I::oracle_staleness_threshold().saturated_into::<MomentOf<T>>(),
		);

		// Ensure Insurance Fund account exists (1 read + potentially 1 write)
		Pallet::<T>::ensure_insurance_fund_exists();

		log::info!(
			target: crate::pallet::LOG_TARGET,
			"MigrateToV1 complete"
		);

		// 11 writes (parameters) + 1 read + 1 write (insurance fund account check/creation)
		T::DbWeight::get().reads_writes(1, 12)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		// VersionedMigration ensures we only run when version is 0
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
		// Verify parameters are set (non-zero where applicable)
		ensure!(
			!MinimumCollateralizationRatio::<T>::get().is_zero(),
			"MinimumCollateralizationRatio not set"
		);
		ensure!(
			!InitialCollateralizationRatio::<T>::get().is_zero(),
			"InitialCollateralizationRatio not set"
		);
		// StabilityFee and LiquidationPenalty can legitimately be zero
		ensure!(!MaximumIssuance::<T>::get().is_zero(), "MaximumIssuance not set");
		ensure!(!MaxLiquidationAmount::<T>::get().is_zero(), "MaxLiquidationAmount not set");
		ensure!(!MaxPositionAmount::<T>::get().is_zero(), "MaxPositionAmount not set");
		ensure!(!MinimumDeposit::<T>::get().is_zero(), "MinimumDeposit not set");
		ensure!(!MinimumMint::<T>::get().is_zero(), "MinimumMint not set");
		ensure!(!StaleVaultThreshold::<T>::get().is_zero(), "StaleVaultThreshold not set");
		ensure!(
			!OracleStalenessThreshold::<T>::get().is_zero(),
			"OracleStalenessThreshold not set"
		);

		Ok(())
	}
}

/// [`UncheckedOnRuntimeUpgrade`] implementation [`InnerMigrateV0ToV1`] wrapped in a
/// [`VersionedMigration`](frame_support::migrations::VersionedMigration), which ensures that:
/// - The migration only runs once when the on-chain storage version is 0
/// - The on-chain storage version is updated to `1` after the migration executes
/// - Reads/Writes from checking/setting the on-chain storage version are accounted for
pub type MigrateV0ToV1<T, I> = frame_support::migrations::VersionedMigration<
	0, // The migration will only execute when the on-chain storage version is 0
	1, // The on-chain storage version will be set to 1 after the migration is complete
	InnerMigrateV0ToV1<T, I>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;
