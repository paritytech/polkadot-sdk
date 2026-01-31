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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};
	use frame_support::traits::StorageVersion;
	use sp_runtime::FixedPointNumber;

	/// Test implementation of InitialVaultsConfig
	pub struct TestVaultsConfig;
	impl InitialVaultsConfig<Test> for TestVaultsConfig {
		fn minimum_collateralization_ratio() -> FixedU128 {
			FixedU128::saturating_from_rational(180, 100) // 180%
		}
		fn initial_collateralization_ratio() -> FixedU128 {
			FixedU128::saturating_from_rational(200, 100) // 200%
		}
		fn stability_fee() -> Permill {
			Permill::from_percent(4)
		}
		fn liquidation_penalty() -> Permill {
			Permill::from_percent(13)
		}
		fn maximum_issuance() -> BalanceOf<Test> {
			20_000_000_000_000 // 20M with 6 decimals
		}
		fn max_liquidation_amount() -> BalanceOf<Test> {
			20_000_000_000_000
		}
		fn max_position_amount() -> BalanceOf<Test> {
			10_000_000_000_000
		}
		fn minimum_deposit() -> BalanceOf<Test> {
			100_000_000_000_000 // 100 DOT with 10 decimals
		}
		fn minimum_mint() -> BalanceOf<Test> {
			5_000_000 // 5 pUSD with 6 decimals
		}
		fn stale_vault_threshold() -> u64 {
			14_400_000 // 4 hours in ms
		}
		fn oracle_staleness_threshold() -> u64 {
			3_600_000 // 1 hour in ms
		}
	}

	#[test]
	fn migration_v0_to_v1_works() {
		new_test_ext().execute_with(|| {
			// Clear storage to simulate pre-migration state (v0)
			StorageVersion::new(0).put::<Pallet<Test>>();
			MinimumCollateralizationRatio::<Test>::kill();
			InitialCollateralizationRatio::<Test>::kill();
			StabilityFee::<Test>::kill();
			LiquidationPenalty::<Test>::kill();
			MaximumIssuance::<Test>::kill();
			MaxLiquidationAmount::<Test>::kill();
			MaxPositionAmount::<Test>::kill();
			MinimumDeposit::<Test>::kill();
			MinimumMint::<Test>::kill();
			StaleVaultThreshold::<Test>::kill();
			OracleStalenessThreshold::<Test>::kill();

			// Verify storage is empty before migration
			assert!(MinimumCollateralizationRatio::<Test>::get().is_zero());
			assert!(InitialCollateralizationRatio::<Test>::get().is_zero());
			assert!(MaximumIssuance::<Test>::get().is_zero());
			assert!(MinimumDeposit::<Test>::get().is_zero());
			assert!(MinimumMint::<Test>::get().is_zero());
			assert!(StaleVaultThreshold::<Test>::get().is_zero());
			assert!(OracleStalenessThreshold::<Test>::get().is_zero());

			// Run migration
			let _weight = InnerMigrateV0ToV1::<Test, TestVaultsConfig>::on_runtime_upgrade();

			// Verify all parameters are set correctly
			assert_eq!(
				MinimumCollateralizationRatio::<Test>::get(),
				FixedU128::saturating_from_rational(180, 100)
			);
			assert_eq!(
				InitialCollateralizationRatio::<Test>::get(),
				FixedU128::saturating_from_rational(200, 100)
			);
			assert_eq!(StabilityFee::<Test>::get(), Permill::from_percent(4));
			assert_eq!(LiquidationPenalty::<Test>::get(), Permill::from_percent(13));
			assert_eq!(MaximumIssuance::<Test>::get(), 20_000_000_000_000);
			assert_eq!(MaxLiquidationAmount::<Test>::get(), 20_000_000_000_000);
			assert_eq!(MaxPositionAmount::<Test>::get(), 10_000_000_000_000);
			assert_eq!(MinimumDeposit::<Test>::get(), 100_000_000_000_000);
			assert_eq!(MinimumMint::<Test>::get(), 5_000_000);
			assert_eq!(StaleVaultThreshold::<Test>::get(), 14_400_000);
			assert_eq!(OracleStalenessThreshold::<Test>::get(), 3_600_000);
		});
	}
}
