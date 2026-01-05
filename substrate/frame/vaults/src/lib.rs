// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! # Vaults Pallet
//!
//! A Collateralized Debt Position (CDP) system for minting over-collateralized stablecoins.
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! The Vaults pallet serves as the CDP engine for the pUSD protocol, enabling users to reserve
//! DOT as collateral to mint pUSD stablecoins. It includes risk management tools to help the
//! system stay well-collateralized, including liquidation mechanisms and emergency controls.
//!
//! ### Key Concepts
//!
//! * **[`Vault`]**: A per-account structure tracking collateralized debt. Each account can have at
//!   most one vault. Stores principal, accrued_interest, status, and last_fee_update timestamp.
//!
//! * **Collateral**: DOT held via [`MutateHold`](frame::traits::fungible::MutateHold) with the
//!   [`HoldReason::VaultDeposit`] reason. The pallet does not transfer funds to a pallet account;
//!   collateral stays in the user's account.
//!
//! * **Principal**: The pUSD debt excluding accrued interest.
//!
//! * **Accrued Interest**: Stability fees accumulated over time, calculated using [`StabilityFee`].
//!
//! * **Collateralization Ratio**: `CR = (Collateral × Price) / (Principal + AccruedInterest)`. Two
//!   ratios are enforced:
//!   - **Initial CR** ([`InitialCollateralizationRatio`]): Required when minting or withdrawing
//!   - **Minimum CR** ([`MinimumCollateralizationRatio`]): Liquidation threshold
//!
//! * **Insurance Fund**: An account ([`Config::InsuranceFund`]) that receives protocol revenue and
//!   serves as a backstop against bad debt.
//!
//! * **Bad Debt**: Unbacked pUSD recorded in [`BadDebt`] when liquidation auctions fail to cover
//!   vault debt. Can be healed via [`Pallet::heal`].
//!
//! ### Vault Lifecycle
//!
//! 1. **Create**: User deposits DOT (≥ [`Config::MinimumDeposit`]) via [`Pallet::create_vault`]
//! 2. **Mint**: User mints pUSD via [`Pallet::mint`], maintaining Initial CR
//! 3. **Repay**: User burns pUSD via [`Pallet::repay`]; interest goes to Insurance Fund
//! 4. **Withdraw**: User releases collateral via [`Pallet::withdraw_collateral`]
//! 5. **Close**: User closes debt-free vault via [`Pallet::close_vault`]
//! 6. **Liquidate**: Anyone can liquidate unsafe vaults via [`Pallet::liquidate_vault`]
//!
//! ### Hold Reasons
//!
//! The pallet uses two hold reasons for collateral management:
//!
//! * **[`HoldReason::VaultDeposit`]**: Collateral backing an active vault. Users can add/remove
//!   collateral while maintaining required ratios.
//!
//! * **[`HoldReason::Seized`]**: Collateral seized during liquidation, pending auction. The auction
//!   pallet operates on funds held with this reason.
//!
//! ### Example
//!
//! The following example demonstrates a typical vault lifecycle:
//!
//! ```ignore
//! // 1. Create a vault with initial collateral
//! Vaults::create_vault(RuntimeOrigin::signed(user), 100 * UNIT)?;
//!
//! // 2. Mint stablecoins against the collateral
//! Vaults::mint(RuntimeOrigin::signed(user), 20 * UNIT)?;
//!
//! // 3. Repay debt over time
//! Vaults::repay(RuntimeOrigin::signed(user), 20 * UNIT)?;
//!
//! // 4. Close the vault and withdraw all collateral
//! Vaults::close_vault(RuntimeOrigin::signed(user))?;
//! ```
//!
//! For more detailed examples, see the integration tests in the `tests` module.
//!
//! ## Low Level / Implementation Details
//!
//! ### Oracle Integration
//!
//! The pallet requires a price oracle implementing [`ProvidePrice`] that returns:
//! - **Normalized price**: `smallest_pUSD_units / smallest_collateral_unit`
//! - **Timestamp**: When the price was last updated
//!
//! Operations requiring price data fail with [`Error::OracleStale`] if the price is older than
//! [`Config::OracleStalenessThreshold`] (default: 1 hour).
//!
//! ### Fee Calculation
//!
//! Interest accrues continuously based on elapsed milliseconds:
//! ```text
//! Interest = Principal × StabilityFee × (DeltaMillis / 31,557,600,000)
//! ```
//!
//! Fees are updated lazily on vault interactions. Additionally:
//! - [`Pallet::poke`] allows anyone to force fee accrual on any vault
//! - `on_idle` updates stale vaults (unchanged for ≥ [`Config::StaleVaultThreshold`])
//!
//! ### Liquidation Flow
//!
//! When a vault's CR falls below [`MinimumCollateralizationRatio`]:
//!
//! 1. Keeper calls [`Pallet::liquidate_vault`]
//! 2. Fees are updated and CR verified below minimum
//! 3. Liquidation penalty is calculated: `Penalty = Principal × LiquidationPenalty`
//! 4. Hold reason changes from [`HoldReason::VaultDeposit`] to [`HoldReason::Seized`]
//! 5. [`AuctionsHandler::start_auction`] is called with collateral and debt details
//! 6. Auction pallet calls back via [`CollateralManager`] trait for purchases
//! 7. On completion, excess collateral returns to owner; shortfall becomes bad debt
//!
//! ### Liquidation Limits
//!
//! [`MaxLiquidationAmount`] is a **hard limit** on pUSD at risk in active auctions.
//! Liquidations are blocked when [`CurrentLiquidationAmount`] + new_debt >
//! [`MaxLiquidationAmount`].
//!
//! ### Governance Model
//!
//! The pallet supports tiered authorization via [`Config::ManagerOrigin`]:
//!
//! * **Full** ([`VaultsManagerLevel::Full`]): Can modify all parameters, raise or lower debt
//!   ceiling
//! * **Emergency** ([`VaultsManagerLevel::Emergency`]): Can only lower debt ceiling (defensive
//!   action)
//!
//! This enables fast-track emergency response to oracle attacks without full governance.
//!
//! ### External Traits
//!
//! The pallet implements [`CollateralManager`] for the auction pallet to:
//! - Query oracle prices via [`CollateralManager::get_dot_price`]
//! - Execute purchases via [`CollateralManager::execute_purchase`]
//! - Complete auctions via [`CollateralManager::complete_auction`]
//!
//! This design keeps all asset operations centralized in the vaults pallet while
//! allowing the auction logic to remain reusable for other collateral sources.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

extern crate alloc;

pub use pallet::*;
pub use sp_pusd::{AuctionsHandler, CollateralManager, PaymentBreakdown, PurchaseParams};
use xcm::v5::Location;

/// TODO: Update/import this trait from the Oracle as soon as it is implemented.
/// Trait for providing timestamped asset prices via oracle.
///
/// This trait abstracts the oracle interface for getting asset prices with their
/// last update timestamp. The price must be in "normalized" format:
/// smallest pUSD units per smallest asset unit.
///
/// # Example
/// For DOT at $4.21 with DOT (10 decimals) and pUSD (6 decimals):
/// - 1 DOT = 4.21 USD
/// - Price = 4.21 × 10^6 / 10^10 = 0.000421
///
/// Assets are identified by XCM `Location`, which can represent:
/// - Native token: `Location::here()` (DOT from AH perspective)
/// - Local assets: `Location::new(0, [PalletInstance(50), GeneralIndex(id)])`
///
/// The timestamp allows consumers to check for oracle staleness and pause
/// operations when the price data is too old.
pub trait ProvidePrice {
	type Price;
	/// The moment/timestamp type.
	type Moment;

	/// Get the current price and timestamp when it was last updated.
	///
	/// Returns `None` if the price is not available.
	/// The tuple contains (price, last_update_timestamp).
	fn get_price(asset: &Location) -> Option<(Self::Price, Self::Moment)>;
}

#[frame_support::pallet]
pub mod pallet {
	use super::{AuctionsHandler, CollateralManager, Location, ProvidePrice, PurchaseParams};
	use crate::weights::WeightInfo;
	use frame_support::{
		pallet_prelude::*,
		traits::{
			fungible::{Inspect, InspectHold, Mutate as FungibleMutate, MutateHold},
			fungibles::{Inspect as FungiblesInspect, Mutate as FungiblesMutate},
			tokens::{Fortitude, Precision, Preservation, Restriction},
			Time,
		},
		weights::WeightMeter,
		DefaultNoBound,
	};
	use frame_system::pallet_prelude::*;
	use sp_runtime::{
		traits::Zero, FixedPointNumber, FixedPointOperand, FixedU128, Permill, SaturatedConversion,
		Saturating,
	};

	/// Log target for this pallet.
	const LOG_TARGET: &str = "runtime::vaults";

	/// Milliseconds per year for timestamp-based fee calculations.
	const MILLIS_PER_YEAR: u64 = (365 * 24 + 6) * 60 * 60 * 1000;

	/// The reason for this pallet placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// The funds are held as collateral in an active vault.
		#[codec(index = 0)]
		VaultDeposit,
		/// The funds have been seized during liquidation and are pending auction.
		/// The auction pallet operates on funds held with this reason.
		#[codec(index = 1)]
		Seized,
	}

	/// Status of a vault in its lifecycle.
	#[derive(
		Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, Debug, Default,
	)]
	pub enum VaultStatus {
		/// Vault is active and healthy.
		#[default]
		Healthy,
		/// Vault has been liquidated and collateral is being auctioned.
		/// No operations are allowed on the vault in this state.
		InLiquidation,
	}

	/// Privilege level returned by ManagerOrigin.
	///
	/// This enables tiered authorization where different origins have different
	/// capabilities for managing vault parameters.
	#[derive(
		Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, Debug, Default,
	)]
	pub enum VaultsManagerLevel {
		/// Full administrative access via GeneralAdmin origin.
		/// Can modify all parameters, raise or lower debt ceiling.
		#[default]
		Full,
		/// Emergency access via EmergencyAction origin.
		/// Can only lower the debt ceiling (defensive action).
		Emergency,
	}

	/// Unified balance type for both collateral (DOT) and stablecoin (pUSD).
	pub type BalanceOf<T> =
		<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

	/// Type alias for the timestamp moment type from the time provider.
	pub type MomentOf<T> = <<T as Config>::TimeProvider as Time>::Moment;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The currency used for collateral (native DOT).
		/// Collateral is managed via pallet_balances using holds.
		/// The Balance type is derived from this and must implement `FixedPointOperand`.
		type Currency: FungibleMutate<Self::AccountId, Balance: FixedPointOperand>
			+ MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// The overarching runtime hold reason.
		type RuntimeHoldReason: From<HoldReason>;

		/// The asset used for pUSD debt.
		/// Constrained to use the same Balance type as Currency.
		type Asset: FungiblesMutate<
			Self::AccountId,
			AssetId = Self::AssetId,
			Balance = BalanceOf<Self>,
		>;

		/// The AssetId type for pallet_assets (used for pUSD).
		type AssetId: Parameter + Member + Copy + MaybeSerializeDeserialize + MaxEncodedLen;

		/// The AssetId for the stablecoin (pUSD).
		#[pallet::constant]
		type StablecoinAssetId: Get<Self::AssetId>;

		/// Account that receives protocol revenue (interest and penalties).
		#[pallet::constant]
		type InsuranceFund: Get<Self::AccountId>;

		/// Minimum collateral deposit required to create a vault.
		#[pallet::constant]
		type MinimumDeposit: Get<BalanceOf<Self>>;

		/// Minimum amount of stablecoin that can be minted in a single operation.
		#[pallet::constant]
		type MinimumMint: Get<BalanceOf<Self>>;

		/// Time provider for fee accrual using UNIX timestamps.
		type TimeProvider: Time;

		/// Duration (in milliseconds) before a vault is considered stale for on_idle fee accrual.
		/// Default: 14,400,000 ms (~4 hours).
		#[pallet::constant]
		type StaleVaultThreshold: Get<MomentOf<Self>>;

		/// Maximum age (in milliseconds) of oracle price before operations are paused.
		/// When the oracle price is older than this threshold, price-dependent operations
		/// (mint, withdraw with debt, liquidate) will fail.
		#[pallet::constant]
		type OracleStalenessThreshold: Get<MomentOf<Self>>;

		/// The Oracle providing timestamped asset prices.
		///
		/// **Important**: The oracle must return prices in "normalized" format:
		/// `smallest_pUSD_units per smallest_asset_unit`
		///
		/// For example, with DOT (10 decimals) at $4.21 and pUSD (6 decimals):
		/// - 1 DOT = 4.21 USD
		/// - Price = 4.21 × 10^6 / 10^10 = 0.000421
		///
		/// This format allows the vault to perform decimal-agnostic calculations.
		/// The oracle must also return a timestamp indicating when the price was last updated.
		type Oracle: ProvidePrice<Price = FixedU128, Moment = MomentOf<Self>>;

		/// The XCM Location of the collateral asset.
		#[pallet::constant]
		type CollateralLocation: Get<Location>;

		/// The Auctions handler for liquidating collateral.
		type AuctionsHandler: AuctionsHandler<Self::AccountId, BalanceOf<Self>>;

		/// Origin allowed to update protocol parameters.
		///
		/// Returns `VaultsManagerLevel` to distinguish privilege levels:
		/// - `Full` (via GeneralAdmin): Can modify all parameters
		/// - `Emergency` (via EmergencyAction): Can only lower debt ceiling
		type ManagerOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = VaultsManagerLevel>;

		/// A type representing the weights required by the dispatchables of this pallet.
		type WeightInfo: crate::weights::WeightInfo;
	}

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// A Vault struct representing a CDP.
	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
	#[scale_info(skip_type_params(T))]
	pub struct Vault<T: Config> {
		/// Current status of the vault in its lifecycle.
		pub status: VaultStatus,
		/// Principal pUSD owed (excluding accrued interest).
		pub principal: BalanceOf<T>,
		/// Accrued interest in pUSD.
		pub accrued_interest: BalanceOf<T>,
		/// Timestamp (milliseconds since Unix epoch) when fees were last updated.
		pub last_fee_update: MomentOf<T>,
	}

	impl<T: Config> Default for Vault<T> {
		fn default() -> Self {
			Self::new()
		}
	}

	impl<T: Config> Vault<T> {
		/// Create a new healthy vault with zero debt and the current timestamp.
		pub fn new() -> Self {
			Self {
				status: VaultStatus::Healthy,
				principal: Zero::zero(),
				accrued_interest: Zero::zero(),
				last_fee_update: T::TimeProvider::now(),
			}
		}

		/// Get the total collateral held by the Balances pallet for this vault.
		pub fn get_held_collateral(&self, who: &T::AccountId) -> BalanceOf<T> {
			T::Currency::balance_on_hold(&HoldReason::VaultDeposit.into(), who)
		}
	}

	/// Map of AccountId -> Vault.
	/// Each account can only have one vault.
	#[pallet::storage]
	pub type Vaults<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, Vault<T>>;

	/// Minimum collateralization ratio
	/// Below this ratio, a vault becomes eligible for liquidation.
	/// Also used as the threshold for collateral withdrawals.
	#[pallet::storage]
	pub type MinimumCollateralizationRatio<T: Config> = StorageValue<_, FixedU128, ValueQuery>;

	/// Initial collateralization ratio
	/// Required when minting new debt. This is higher than the minimum ratio
	/// to create a safety buffer preventing immediate liquidation after minting.
	#[pallet::storage]
	pub type InitialCollateralizationRatio<T: Config> = StorageValue<_, FixedU128, ValueQuery>;

	/// Stability fee (annual interest rate).
	#[pallet::storage]
	pub type StabilityFee<T: Config> = StorageValue<_, Permill, ValueQuery>;

	/// Liquidation penalty
	/// Applied to the debt during liquidation. The penalty is converted to DOT
	/// and deducted from the collateral returned to the vault owner.
	/// This incentivizes vault owners to maintain safe collateral levels.
	#[pallet::storage]
	pub type LiquidationPenalty<T: Config> = StorageValue<_, Permill, ValueQuery>;

	/// Maximum total debt allowed in the system.
	#[pallet::storage]
	pub type MaximumIssuance<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	/// Accumulated bad debt in pUSD.
	/// This represents unbacked principal left after liquidation auctions.
	#[pallet::storage]
	pub type BadDebt<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	/// Maximum pUSD that can be at risk in active auctions.
	///
	/// This is a **hard limit** - liquidations are blocked when exceeded.
	/// Governance can adjust this parameter to control auction exposure.
	#[pallet::storage]
	pub type MaxLiquidationAmount<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	/// Current pUSD at risk in active auctions.
	///
	/// This accumulator tracks the sum of debt for all active auctions.
	/// It increases when auctions start and decreases when auctions complete
	/// or are cancelled (via callbacks from the Auctions pallet).
	#[pallet::storage]
	pub type CurrentLiquidationAmount<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	/// Cursor for `on_idle` pagination.
	///
	/// Stores the last processed vault owner's AccountId to continue iteration
	/// across blocks. This prevents restarting from the beginning each block
	/// and ensures all vaults are eventually processed.
	#[pallet::storage]
	pub type OnIdleCursor<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	/// Genesis configuration for the vaults pallet.
	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Minimum collateralization ratio.
		/// Below this ratio, a vault becomes eligible for liquidation.
		pub minimum_collateralization_ratio: FixedU128,
		/// Initial collateralization ratio.
		/// Required when minting new debt.
		pub initial_collateralization_ratio: FixedU128,
		/// Stability fee (annual interest rate).
		pub stability_fee: Permill,
		/// Liquidation penalty.
		pub liquidation_penalty: Permill,
		/// Maximum total debt allowed in the system.
		pub maximum_issuance: BalanceOf<T>,
		/// Maximum pUSD at risk in active auctions.
		pub max_liquidation_amount: BalanceOf<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			MinimumCollateralizationRatio::<T>::put(self.minimum_collateralization_ratio);
			InitialCollateralizationRatio::<T>::put(self.initial_collateralization_ratio);
			StabilityFee::<T>::put(self.stability_fee);
			LiquidationPenalty::<T>::put(self.liquidation_penalty);
			MaximumIssuance::<T>::put(self.maximum_issuance);
			MaxLiquidationAmount::<T>::put(self.max_liquidation_amount);

			// Ensure Insurance Fund account exists so it can receive any amount.
			Pallet::<T>::ensure_insurance_fund_exists();
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new vault was created with initial collateral deposit.
		VaultCreated { owner: T::AccountId },
		/// Collateral (DOT) was deposited into a vault.
		CollateralDeposited { owner: T::AccountId, amount: BalanceOf<T> },
		/// Collateral (DOT) was withdrawn from a vault.
		CollateralWithdrawn { owner: T::AccountId, amount: BalanceOf<T> },
		/// Stablecoin (pUSD) was minted against vault collateral.
		Minted { owner: T::AccountId, amount: BalanceOf<T> },
		/// Debt (pUSD) was repaid and burned.
		Repaid { owner: T::AccountId, amount: BalanceOf<T> },
		/// Excess pUSD returned when repayment exceeded debt.
		ReturnedExcess { owner: T::AccountId, amount: BalanceOf<T> },
		/// A vault was liquidated due to undercollateralization.
		InLiquidation {
			owner: T::AccountId,
			/// Outstanding debt at time of liquidation.
			debt: BalanceOf<T>,
			/// Collateral seized for auction (after interest and penalty).
			collateral_seized: BalanceOf<T>,
		},
		/// A vault was closed and all collateral returned to owner.
		VaultClosed { owner: T::AccountId },
		/// Interest was updated on a vault.
		InterestUpdated {
			owner: T::AccountId,
			/// Interest amount in pUSD.
			amount: BalanceOf<T>,
		},
		/// Liquidation penalty collected during liquidation.
		LiquidationPenaltyAdded { owner: T::AccountId, amount: BalanceOf<T> },
		/// Minimum collateralization ratio was updated by governance.
		MinimumCollateralizationRatioUpdated { old_value: FixedU128, new_value: FixedU128 },
		/// Initial collateralization ratio was updated by governance.
		InitialCollateralizationRatioUpdated { old_value: FixedU128, new_value: FixedU128 },
		/// Stability fee was updated by governance.
		StabilityFeeUpdated { old_value: Permill, new_value: Permill },
		/// Liquidation penalty was updated by governance.
		LiquidationPenaltyUpdated { old_value: Permill, new_value: Permill },
		/// Maximum system debt ceiling was updated by governance.
		MaximumIssuanceUpdated { old_value: BalanceOf<T>, new_value: BalanceOf<T> },
		/// Maximum liquidation amount was updated by governance.
		MaxLiquidationAmountUpdated { old_value: BalanceOf<T>, new_value: BalanceOf<T> },
		/// Bad debt accrued when auctions leave unbacked principal.
		BadDebtAccrued {
			owner: T::AccountId,
			/// Uncollectable principal amount in pUSD.
			amount: BalanceOf<T>,
		},
		/// Bad debt was healed by burning pUSD from InsuranceFund.
		BadDebtRepaid { amount: BalanceOf<T> },
		/// A Dutch auction was started for liquidated collateral.
		AuctionStarted {
			owner: T::AccountId,
			auction_id: u32,
			/// Collateral available for auction (lot).
			collateral: BalanceOf<T>,
			/// Debt to raise from auction (tab).
			tab: BalanceOf<T>,
		},
		/// pUSD collected from auction purchase; CurrentLiquidationAmount reduced.
		AuctionDebtCollected { amount: BalanceOf<T> },
		/// Auction completed with principal shortfall; recorded as BadDebt.
		AuctionShortfall { shortfall: BalanceOf<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// No vault exists for the specified account.
		///
		/// Create a vault first using [`Pallet::create_vault`] before attempting other operations.
		VaultNotFound,
		/// Insufficient collateral for the requested operation.
		///
		/// Deposit more collateral or reduce the withdrawal amount.
		InsufficientCollateral,
		/// Minting would exceed the system-wide maximum debt ceiling.
		///
		/// Wait for system debt to decrease or governance to raise [`MaximumIssuance`].
		ExceedsMaxDebt,
		/// Operation would breach the required collateralization ratio.
		///
		/// Deposit more collateral, reduce mint amount, or reduce withdrawal amount to maintain
		/// the required ratio (either [`InitialCollateralizationRatio`] for minting/withdrawals
		/// or [`MinimumCollateralizationRatio`] for liquidation safety).
		UnsafeCollateralizationRatio,
		/// Account already has an active vault.
		///
		/// Each account can only have one vault. Use the existing vault or close it first.
		VaultAlreadyExists,
		/// Arithmetic operation overflowed.
		///
		/// This indicates an internal calculation exceeded safe bounds. Try different amounts.
		ArithmeticOverflow,
		/// Vault is sufficiently collateralized and cannot be liquidated.
		///
		/// The vault's collateralization ratio is above [`MinimumCollateralizationRatio`].
		/// Liquidation is only possible when the ratio falls below this threshold.
		VaultIsSafe,
		/// Oracle price not available for collateral asset.
		///
		/// The oracle has not reported a price for the collateral. Wait for oracle update.
		PriceNotAvailable,
		/// Oracle price is stale.
		///
		/// The oracle price is older than [`Config::OracleStalenessThreshold`].
		/// Wait for the oracle to provide a fresh price update.
		OracleStale,
		/// Cannot close vault with outstanding debt.
		///
		/// Repay all principal debt using [`Pallet::repay`] before closing the vault.
		VaultHasDebt,
		/// Deposit or remaining collateral below minimum threshold.
		///
		/// Ensure deposit amount is at least [`Config::MinimumDeposit`], or when withdrawing,
		/// leave at least that amount (or withdraw everything to close the vault).
		BelowMinimumDeposit,
		/// Mint amount below minimum threshold.
		///
		/// Ensure mint amount is at least [`Config::MinimumMint`].
		BelowMinimumMint,
		/// Vault is in liquidation; operations blocked until auction completes.
		///
		/// Wait for the auction to complete. The vault will be removed once the auction ends.
		VaultInLiquidation,
		/// Origin lacks required privilege level.
		///
		/// This operation requires [`VaultsManagerLevel::Full`] privilege. Emergency origins
		/// cannot perform this action.
		InsufficientPrivilege,
		/// Emergency origin can only lower the maximum debt, not raise it.
		///
		/// Use a Full privilege origin to raise the debt ceiling, or specify a lower value.
		CanOnlyLowerMaxDebt,
		/// Liquidation would exceed maximum liquidation amount.
		///
		/// The system has reached its limit for debt at risk in active auctions. Wait for
		/// existing auctions to complete or governance to raise [`MaxLiquidationAmount`].
		ExceedsMaxLiquidationAmount,
		/// Initial collateralization ratio must be >= minimum ratio.
		///
		/// The [`InitialCollateralizationRatio`] cannot be set below
		/// [`MinimumCollateralizationRatio`] as it would prevent any borrowing.
		InitialRatioMustExceedMinimum,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// This ensures the Insurance Fund account is created with a provider reference so it can
		/// receive any amount (including below ED) without risk of being reaped.
		fn on_runtime_upgrade() -> Weight {
			let on_chain_version = StorageVersion::get::<Pallet<T>>();

			if on_chain_version < 1 {
				Self::ensure_insurance_fund_exists();
				StorageVersion::new(1).put::<Pallet<T>>();

				log::info!(
					target: LOG_TARGET,
					"Migrated storage from version {:?} to 1",
					on_chain_version
				);

				// Weight: 1 read (storage version) + 1 read (account_exists) + 2 writes
				// (inc_providers + storage version)
				T::DbWeight::get().reads_writes(2, 2)
			} else {
				log::debug!(
					target: LOG_TARGET,
					"No migration needed, on-chain version {:?}",
					on_chain_version
				);
				// Weight: 1 read (storage version check)
				T::DbWeight::get().reads(1)
			}
		}

		/// Idle block housekeeping: update fees for stale vaults.
		///
		/// Vaults inactive for >= StaleVaultThreshold get their fees updated.
		/// Uses cursor-based pagination to continue across blocks, ensuring all
		/// vaults are eventually processed without unbounded iteration.
		fn on_idle(_now: BlockNumberFor<T>, limit: Weight) -> Weight {
			let mut meter = WeightMeter::with_limit(limit);

			// Early exit if not enough weight for base overhead
			let base_weight = Self::on_idle_base_weight();
			if meter.try_consume(base_weight).is_err() {
				return meter.consumed();
			}

			let current_timestamp = T::TimeProvider::now();
			let stale_threshold = T::StaleVaultThreshold::get();
			let per_vault_weight = Self::on_idle_per_vault_weight();

			// Build iterator from cursor position
			let cursor = OnIdleCursor::<T>::get();
			let mut iter = match cursor {
				Some(ref last_key) => Vaults::<T>::iter_from(Vaults::<T>::hashed_key_for(last_key)),
				None => Vaults::<T>::iter(),
			};

			let mut last_processed: Option<T::AccountId> = None;
			let mut stopped_for_weight = false;

			loop {
				let Some((owner, mut vault)) = iter.next() else { break };

				// Skip cursor key itself if it still exists
				if cursor.as_ref() == Some(&owner) {
					continue;
				}

				// Check weight budget for processing this vault.
				if meter.try_consume(per_vault_weight).is_err() {
					stopped_for_weight = true;
					break;
				}

				// Only process healthy vaults that are stale.
				if vault.status == VaultStatus::Healthy {
					let time_since = current_timestamp.saturating_sub(vault.last_fee_update);
					if time_since >= stale_threshold {
						Self::update_vault_fees(&mut vault, &owner, Some(current_timestamp));
						log::debug!(
							target: LOG_TARGET,
							"on_idle: updated stale vault fees for {:?}, time_since={:?}ms",
							owner,
							time_since
						);
						Vaults::<T>::insert(&owner, vault);
					}
				}

				last_processed = Some(owner);
			}

			// Update cursor based on how we exited
			if stopped_for_weight {
				if let Some(last) = last_processed {
					OnIdleCursor::<T>::put(last);
				}
			} else {
				// Natural end of iteration - clear cursor to restart next time
				OnIdleCursor::<T>::kill();
			}

			meter.consumed()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new vault with initial collateral deposit.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed` by the account that will own the vault.
		///
		/// ## Details
		///
		/// Creates a new vault for the caller with the specified initial collateral deposit.
		/// The collateral is held using the [`HoldReason::VaultDeposit`] reason. Each account
		/// can only have one vault at a time.
		///
		/// ## Errors
		///
		/// - [`Error::BelowMinimumDeposit`]: If `initial_deposit` is less than
		///   [`Config::MinimumDeposit`].
		/// - [`Error::VaultAlreadyExists`]: If the caller already has an active vault.
		///
		/// ## Events
		///
		/// - [`Event::VaultCreated`]: Emitted when the vault is successfully created.
		/// - [`Event::CollateralDeposited`]: Emitted for the initial collateral deposit.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::create_vault())]
		pub fn create_vault(origin: OriginFor<T>, initial_deposit: BalanceOf<T>) -> DispatchResult {
			ensure!(initial_deposit >= T::MinimumDeposit::get(), Error::<T>::BelowMinimumDeposit);

			let who = ensure_signed(origin)?;

			Vaults::<T>::try_mutate_exists(&who, |maybe_vault| -> DispatchResult {
				ensure!(maybe_vault.is_none(), Error::<T>::VaultAlreadyExists);
				T::Currency::hold(&HoldReason::VaultDeposit.into(), &who, initial_deposit)?;
				*maybe_vault = Some(Vault::new());

				Self::deposit_event(Event::VaultCreated { owner: who.clone() });
				Self::deposit_event(Event::CollateralDeposited {
					owner: who.clone(),
					amount: initial_deposit,
				});

				Ok(())
			})
		}

		/// Deposit additional collateral into an existing vault.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed` by the vault owner.
		///
		/// ## Details
		///
		/// Adds collateral to an existing vault. The amount is held using the
		/// [`HoldReason::VaultDeposit`] reason. Any accrued stability fees are updated
		/// before the deposit is processed.
		///
		/// ## Errors
		///
		/// - [`Error::VaultNotFound`]: If the caller does not have a vault.
		/// - [`Error::VaultInLiquidation`]: If the vault is currently being liquidated.
		///
		/// ## Events
		///
		/// - [`Event::CollateralDeposited`]: Emitted when collateral is successfully deposited.
		/// - [`Event::InterestUpdated`]: Emitted if stability fees were accrued.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::deposit_collateral())]
		pub fn deposit_collateral(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Vaults::<T>::try_mutate(&who, |maybe_vault| -> DispatchResult {
				let vault = maybe_vault.as_mut().ok_or(Error::<T>::VaultNotFound)?;

				ensure!(vault.status == VaultStatus::Healthy, Error::<T>::VaultInLiquidation);

				Self::update_vault_fees(vault, &who, None);

				T::Currency::hold(&HoldReason::VaultDeposit.into(), &who, amount)?;

				Self::deposit_event(Event::CollateralDeposited { owner: who.clone(), amount });
				Ok(())
			})
		}

		/// Withdraw collateral from a vault.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed` by the vault owner.
		///
		/// ## Details
		///
		/// Releases collateral from the vault back to the owner's free balance. Any accrued
		/// stability fees are updated first. If the vault has outstanding debt, the withdrawal
		/// must maintain the [`InitialCollateralizationRatio`] to preserve a safety buffer.
		/// If remaining collateral is non-zero, it must meet [`Config::MinimumDeposit`].
		/// Withdrawing all collateral when debt is zero will auto-close the vault.
		///
		/// ## Errors
		///
		/// - [`Error::VaultNotFound`]: If the caller does not have a vault.
		/// - [`Error::VaultInLiquidation`]: If the vault is currently being liquidated.
		/// - [`Error::InsufficientCollateral`]: If `amount` exceeds available collateral.
		/// - [`Error::BelowMinimumDeposit`]: If remaining collateral is below the minimum.
		/// - [`Error::VaultHasDebt`]: If attempting to withdraw all collateral while debt exists.
		/// - [`Error::UnsafeCollateralizationRatio`]: If withdrawal would breach initial ratio.
		/// - [`Error::PriceNotAvailable`]: If the oracle price is unavailable.
		/// - [`Error::OracleStale`]: If the oracle price is too old.
		///
		/// ## Events
		///
		/// - [`Event::CollateralWithdrawn`]: Emitted when collateral is released.
		/// - [`Event::VaultClosed`]: Emitted if the vault is auto-closed (zero collateral, zero
		///   debt).
		/// - [`Event::InterestUpdated`]: Emitted if stability fees were accrued.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::withdraw_collateral())]
		pub fn withdraw_collateral(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Vaults::<T>::try_mutate_exists(&who, |maybe_vault| -> DispatchResult {
				let vault = maybe_vault.as_mut().ok_or(Error::<T>::VaultNotFound)?;

				ensure!(vault.status == VaultStatus::Healthy, Error::<T>::VaultInLiquidation);

				Self::update_vault_fees(vault, &who, None);

				let available = vault.get_held_collateral(&who);
				ensure!(available >= amount, Error::<T>::InsufficientCollateral);

				let remaining_collateral = available.saturating_sub(amount);
				// Prevent dust vaults: if any collateral remains, it must meet MinimumDeposit.
				// Withdrawing all collateral (when debt == 0) auto-closes the vault.
				if !remaining_collateral.is_zero() {
					ensure!(
						remaining_collateral >= T::MinimumDeposit::get(),
						Error::<T>::BelowMinimumDeposit
					);
				}

				// Check collateralization ratio if vault has principal or accrued interest
				let total_obligation = vault
					.principal
					.checked_add(&vault.accrued_interest)
					.ok_or(Error::<T>::ArithmeticOverflow)?;

				if !total_obligation.is_zero() {
					// Cannot withdraw all collateral if there's outstanding obligation
					ensure!(!remaining_collateral.is_zero(), Error::<T>::VaultHasDebt);

					// CR = remaining_collateral × Price / (Principal + AccruedInterest)
					let ratio = Self::calculate_collateralization_ratio(
						remaining_collateral,
						total_obligation,
					)?
					.ok_or(Error::<T>::ArithmeticOverflow)?;
					let initial_ratio = InitialCollateralizationRatio::<T>::get();
					ensure!(ratio >= initial_ratio, Error::<T>::UnsafeCollateralizationRatio);
				}

				T::Currency::release(
					&HoldReason::VaultDeposit.into(),
					&who,
					amount,
					Precision::Exact,
				)?;

				Self::deposit_event(Event::CollateralWithdrawn { owner: who.clone(), amount });

				// Remove empty vaults immediately (no collateral + no debt).
				if remaining_collateral.is_zero() {
					*maybe_vault = None;
					Self::deposit_event(Event::VaultClosed { owner: who.clone() });
				}

				Ok(())
			})
		}

		/// Mint stablecoin (pUSD) against collateral.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed` by the vault owner.
		///
		/// ## Details
		///
		/// Mints pUSD stablecoins by increasing the vault's principal debt. Any accrued
		/// stability fees are updated first. The vault must maintain the
		/// [`InitialCollateralizationRatio`] to create a safety buffer
		/// preventing immediate liquidation after minting. The total system debt cannot
		/// exceed [`MaximumIssuance`].
		///
		/// ## Errors
		///
		/// - [`Error::VaultNotFound`]: If the caller does not have a vault.
		/// - [`Error::VaultInLiquidation`]: If the vault is currently being liquidated.
		/// - [`Error::BelowMinimumMint`]: If `amount` is below [`Config::MinimumMint`].
		/// - [`Error::ExceedsMaxDebt`]: If minting would exceed the system debt ceiling.
		/// - [`Error::UnsafeCollateralizationRatio`]: If minting would breach initial ratio.
		/// - [`Error::PriceNotAvailable`]: If the oracle price is unavailable.
		/// - [`Error::OracleStale`]: If the oracle price is too old.
		///
		/// ## Events
		///
		/// - [`Event::Minted`]: Emitted when pUSD is successfully minted.
		/// - [`Event::InterestUpdated`]: Emitted if stability fees were accrued.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::mint())]
		pub fn mint(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Vaults::<T>::try_mutate(&who, |maybe_vault| -> DispatchResult {
				let vault = maybe_vault.as_mut().ok_or(Error::<T>::VaultNotFound)?;

				ensure!(vault.status == VaultStatus::Healthy, Error::<T>::VaultInLiquidation);

				ensure!(amount >= T::MinimumMint::get(), Error::<T>::BelowMinimumMint);

				Self::update_vault_fees(vault, &who, None);

				let new_principal =
					vault.principal.checked_add(&amount).ok_or(Error::<T>::ArithmeticOverflow)?;

				// Check max debt using total issuance of pUSD
				// Total debt = Total pUSD in circulation
				let total_issuance = T::Asset::total_issuance(T::StablecoinAssetId::get());
				ensure!(
					total_issuance.saturating_add(amount) <= MaximumIssuance::<T>::get(),
					Error::<T>::ExceedsMaxDebt
				);

				vault.principal = new_principal;

				// Check collateralization ratio (CR). Use InitialCollateralizationRatio for minting
				// to create safety buffer.
				let ratio = Self::get_collateralization_ratio(vault, &who)?
					.ok_or(Error::<T>::ArithmeticOverflow)?;
				let initial_ratio = InitialCollateralizationRatio::<T>::get();
				ensure!(ratio >= initial_ratio, Error::<T>::UnsafeCollateralizationRatio);

				T::Asset::mint_into(T::StablecoinAssetId::get(), &who, amount)?;

				Self::deposit_event(Event::Minted { owner: who.clone(), amount });
				Ok(())
			})
		}

		/// Repay debt by burning pUSD.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed` by the vault owner.
		///
		/// ## Details
		///
		/// Reduces vault debt by burning pUSD from the caller. Payment is applied in order:
		/// accrued interest first (transferred to [`Config::InsuranceFund`]), then principal
		/// (burned). Any accrued stability fees are updated before repayment is processed.
		/// If `amount` exceeds total obligation, the excess is reported but not consumed.
		///
		/// ## Errors
		///
		/// - [`Error::VaultNotFound`]: If the caller does not have a vault.
		/// - [`Error::VaultInLiquidation`]: If the vault is currently being liquidated.
		///
		/// ## Events
		///
		/// - [`Event::InterestUpdated`]: Emitted for interest payment to InsuranceFund.
		/// - [`Event::Repaid`]: Emitted for principal portion burned.
		/// - [`Event::ReturnedExcess`]: Emitted if `amount` exceeded total obligation.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::repay())]
		pub fn repay(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Vaults::<T>::try_mutate(&who, |maybe_vault| -> DispatchResult {
				let vault = maybe_vault.as_mut().ok_or(Error::<T>::VaultNotFound)?;

				ensure!(vault.status == VaultStatus::Healthy, Error::<T>::VaultInLiquidation);

				Self::update_vault_fees(vault, &who, None);

				// Payment order: interest first, then principal
				// 1. Calculate how much interest to pay (capped by available amount)
				let interest_to_pay = vault.accrued_interest.min(amount);
				let remaining_after_interest = amount.saturating_sub(interest_to_pay);

				// 2. Calculate how much principal to pay (capped by remaining amount)
				let principal_to_pay = vault.principal.min(remaining_after_interest);

				// 3. Calculate true excess (unused after interest + principal)
				let true_excess = remaining_after_interest.saturating_sub(principal_to_pay);

				// Transfer interest to InsuranceFund first
				if !interest_to_pay.is_zero() {
					T::Asset::transfer(
						T::StablecoinAssetId::get(),
						&who,
						&T::InsuranceFund::get(),
						interest_to_pay,
						Preservation::Expendable,
					)?;
					vault.accrued_interest = vault.accrued_interest.saturating_sub(interest_to_pay);
				}

				// Burn pUSD for principal repayment
				if !principal_to_pay.is_zero() {
					T::Asset::burn_from(
						T::StablecoinAssetId::get(),
						&who,
						principal_to_pay,
						Preservation::Expendable,
						Precision::Exact,
						Fortitude::Force,
					)?;
					vault.principal = vault.principal.saturating_sub(principal_to_pay);
				}

				if !interest_to_pay.is_zero() {
					Self::deposit_event(Event::InterestUpdated {
						owner: who.clone(),
						amount: interest_to_pay,
					})
				};

				if !principal_to_pay.is_zero() {
					Self::deposit_event(Event::Repaid {
						owner: who.clone(),
						amount: principal_to_pay,
					});
				}

				if !true_excess.is_zero() {
					Self::deposit_event(Event::ReturnedExcess {
						owner: who.clone(),
						amount: true_excess,
					});
				}

				Ok(())
			})
		}

		/// Liquidate an undercollateralized vault.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed`. Anyone can call this function to liquidate an unsafe vault
		/// (acting as a "keeper").
		///
		/// ## Details
		///
		/// Initiates an auction for the vault's collateral when the vault's
		/// collateralization ratio falls below [`MinimumCollateralizationRatio`].
		/// The auction will attempt to raise enough pUSD to cover the debt plus the
		/// [`LiquidationPenalty`]. The collateral hold reason changes from
		/// [`HoldReason::VaultDeposit`] to [`HoldReason::Seized`].
		///
		/// **Process:**
		/// 1. Verify vault is undercollateralized (ratio < [`MinimumCollateralizationRatio`])
		/// 2. Calculate liquidation penalty based on principal
		/// 3. Update [`CurrentLiquidationAmount`] accumulator
		/// 4. Seize collateral and start auction via [`Config::AuctionsHandler`]
		///
		/// ## Errors
		///
		/// - [`Error::VaultNotFound`]: If the target vault does not exist.
		/// - [`Error::VaultInLiquidation`]: If the vault is already being liquidated.
		/// - [`Error::VaultIsSafe`]: If the vault's ratio is above the minimum threshold.
		/// - [`Error::ExceedsMaxLiquidationAmount`]: If liquidation would exceed the hard limit.
		/// - [`Error::PriceNotAvailable`]: If the oracle price is unavailable.
		/// - [`Error::OracleStale`]: If the oracle price is too old.
		///
		/// ## Events
		///
		/// - [`Event::LiquidationPenaltyAdded`]: Emitted with the calculated penalty amount.
		/// - [`Event::InLiquidation`]: Emitted when the vault enters liquidation state.
		/// - [`Event::AuctionStarted`]: Emitted with auction details.
		/// - [`Event::InterestUpdated`]: Emitted if stability fees were accrued.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::liquidate_vault())]
		pub fn liquidate_vault(origin: OriginFor<T>, vault_owner: T::AccountId) -> DispatchResult {
			let keeper = ensure_signed(origin)?;

			Vaults::<T>::try_mutate(&vault_owner, |maybe_vault| -> DispatchResult {
				let vault = maybe_vault.as_mut().ok_or(Error::<T>::VaultNotFound)?;

				ensure!(vault.status == VaultStatus::Healthy, Error::<T>::VaultInLiquidation);

				Self::update_vault_fees(vault, &vault_owner, None);

				let principal = vault.principal;
				let accrued_interest = vault.accrued_interest;
				let collateral_seized = vault.get_held_collateral(&vault_owner);
				let total_obligation = principal
					.checked_add(&accrued_interest)
					.ok_or(Error::<T>::ArithmeticOverflow)?;

				// Check if vault is undercollateralized
				// CR = HeldCollateral × Price / (Principal + AccruedInterest)
				// A vault with no debt is always safe
				ensure!(!total_obligation.is_zero(), Error::<T>::VaultIsSafe);
				let ratio =
					Self::calculate_collateralization_ratio(collateral_seized, total_obligation)?
						.ok_or(Error::<T>::ArithmeticOverflow)?;
				let min_ratio = MinimumCollateralizationRatio::<T>::get();
				ensure!(ratio < min_ratio, Error::<T>::VaultIsSafe);

				// Calculate liquidation penalty in pUSD (applied to principal)
				let liquidation_penalty = LiquidationPenalty::<T>::get();
				let penalty_pusd = liquidation_penalty.mul_floor(principal);

				// Total debt for the auction includes principal + interest + penalty
				let total_debt = total_obligation
					.checked_add(&penalty_pusd)
					.ok_or(Error::<T>::ArithmeticOverflow)?;

				// Check if liquidation would exceed hard limit
				let current_liquidation = CurrentLiquidationAmount::<T>::get();
				let max_liquidation = MaxLiquidationAmount::<T>::get();
				let new_liquidation_amount = current_liquidation
					.checked_add(&total_debt)
					.ok_or(Error::<T>::ArithmeticOverflow)?;
				ensure!(
					new_liquidation_amount <= max_liquidation,
					Error::<T>::ExceedsMaxLiquidationAmount
				);

				CurrentLiquidationAmount::<T>::put(new_liquidation_amount);

				// Emit penalty collected event
				if !penalty_pusd.is_zero() {
					Self::deposit_event(Event::LiquidationPenaltyAdded {
						owner: vault_owner.clone(),
						amount: penalty_pusd,
					});
				}

				// Change hold reason from VaultDeposit to Seized
				// The collateral stays in the user's account but is now controlled by the auction
				// pallet
				T::Currency::release(
					&HoldReason::VaultDeposit.into(),
					&vault_owner,
					collateral_seized,
					Precision::Exact,
				)?;

				// Immediately re-hold with Seized reason
				T::Currency::hold(&HoldReason::Seized.into(), &vault_owner, collateral_seized)?;

				// Start the auction - collateral (native DOT) is held with Seized reason
				// Pass separate Tab components: principal, accrued_interest, penalty
				let auction_id = T::AuctionsHandler::start_auction(
					&vault_owner,
					collateral_seized,
					principal,
					accrued_interest,
					penalty_pusd,
					&keeper,
				)?;

				// Mark vault as in liquidation (will be removed when auction completes)
				vault.status = VaultStatus::InLiquidation;

				log::info!(
					target: LOG_TARGET,
					"Vault liquidated: owner={:?}, principal={:?}, collateral={:?}, auction_id={}, ratio={:?}",
					vault_owner,
					principal,
					collateral_seized,
					auction_id,
					ratio
				);

				Self::deposit_event(Event::InLiquidation {
					owner: vault_owner.clone(),
					debt: total_debt,
					collateral_seized,
				});

				Self::deposit_event(Event::AuctionStarted {
					owner: vault_owner.clone(),
					auction_id,
					collateral: collateral_seized,
					tab: total_debt,
				});

				Ok(())
			})
		}

		/// Close a vault with no debt and withdraw all collateral.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed` by the vault owner.
		///
		/// ## Details
		///
		/// Closes the vault and releases all collateral to the owner. Can only be called
		/// when `principal == 0`. Any accrued interest is transferred to
		/// [`Config::InsuranceFund`] before closing. The vault is removed from storage.
		///
		/// ## Errors
		///
		/// - [`Error::VaultNotFound`]: If the caller does not have a vault.
		/// - [`Error::VaultInLiquidation`]: If the vault is currently being liquidated.
		/// - [`Error::VaultHasDebt`]: If the vault has outstanding principal debt.
		///
		/// ## Events
		///
		/// - [`Event::InterestUpdated`]: Emitted if accrued interest was paid.
		/// - [`Event::CollateralWithdrawn`]: Emitted when collateral is released.
		/// - [`Event::VaultClosed`]: Emitted when the vault is removed.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::close_vault())]
		pub fn close_vault(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Vaults::<T>::try_mutate_exists(&who, |maybe_vault| -> DispatchResult {
				let vault = maybe_vault.as_mut().ok_or(Error::<T>::VaultNotFound)?;

				ensure!(vault.status == VaultStatus::Healthy, Error::<T>::VaultInLiquidation);

				Self::update_vault_fees(vault, &who, None);

				ensure!(vault.principal.is_zero(), Error::<T>::VaultHasDebt);

				// Collect any accrued interest before closing.
				if !vault.accrued_interest.is_zero() {
					let interest = vault.accrued_interest;
					T::Asset::transfer(
						T::StablecoinAssetId::get(),
						&who,
						&T::InsuranceFund::get(),
						interest,
						Preservation::Expendable,
					)?;
					vault.accrued_interest = Zero::zero();
					Self::deposit_event(Event::InterestUpdated {
						owner: who.clone(),
						amount: interest,
					});
				}

				let released = T::Currency::release_all(
					&HoldReason::VaultDeposit.into(),
					&who,
					Precision::BestEffort,
				)?;

				Self::deposit_event(Event::CollateralWithdrawn {
					owner: who.clone(),
					amount: released,
				});

				*maybe_vault = None;
				Self::deposit_event(Event::VaultClosed { owner: who.clone() });

				Ok(())
			})
		}

		/// Set the minimum collateralization ratio.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`] with [`VaultsManagerLevel::Full`] privilege
		/// (typically GeneralAdmin). Emergency origin cannot modify this parameter.
		///
		/// ## Details
		///
		/// Sets the [`MinimumCollateralizationRatio`] below which vaults become eligible
		/// for liquidation. The ratio is expressed as [`FixedU128`] (e.g., 1.8 for 180%).
		///
		/// ## Errors
		///
		/// - [`Error::InsufficientPrivilege`]: If called by Emergency origin.
		///
		/// ## Events
		///
		/// - [`Event::MinimumCollateralizationRatioUpdated`]: Emitted with old and new values.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::set_minimum_collateralization_ratio())]
		pub fn set_minimum_collateralization_ratio(
			origin: OriginFor<T>,
			ratio: FixedU128,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level == VaultsManagerLevel::Full, Error::<T>::InsufficientPrivilege);
			let old_value = MinimumCollateralizationRatio::<T>::get();
			MinimumCollateralizationRatio::<T>::put(ratio);
			Self::deposit_event(Event::MinimumCollateralizationRatioUpdated {
				old_value,
				new_value: ratio,
			});
			Ok(())
		}

		/// Set the stability fee (annual interest rate).
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`] with [`VaultsManagerLevel::Full`] privilege
		/// (typically GeneralAdmin). Emergency origin cannot modify this parameter.
		///
		/// ## Details
		///
		/// Sets the [`StabilityFee`] used to calculate interest accrual on vault debt.
		/// The fee is expressed as [`Permill`] (e.g., `Permill::from_percent(5)` for 5% APR).
		///
		/// ## Errors
		///
		/// - [`Error::InsufficientPrivilege`]: If called by Emergency origin.
		///
		/// ## Events
		///
		/// - [`Event::StabilityFeeUpdated`]: Emitted with old and new values.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::set_stability_fee())]
		pub fn set_stability_fee(origin: OriginFor<T>, fee: Permill) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level == VaultsManagerLevel::Full, Error::<T>::InsufficientPrivilege);
			let old_value = StabilityFee::<T>::get();
			StabilityFee::<T>::put(fee);
			Self::deposit_event(Event::StabilityFeeUpdated { old_value, new_value: fee });
			Ok(())
		}

		/// Set the initial collateralization ratio.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`] with [`VaultsManagerLevel::Full`] privilege
		/// (typically GeneralAdmin). Emergency origin cannot modify this parameter.
		///
		/// ## Details
		///
		/// Sets the [`InitialCollateralizationRatio`] required when minting new debt or
		/// withdrawing collateral with existing debt. This ratio must be greater than or
		/// equal to [`MinimumCollateralizationRatio`] to create a safety buffer preventing
		/// immediate liquidation. Expressed as [`FixedU128`] (e.g., 2.0 for 200%).
		///
		/// ## Errors
		///
		/// - [`Error::InsufficientPrivilege`]: If called by Emergency origin.
		/// - [`Error::InitialRatioMustExceedMinimum`]: If ratio is below minimum.
		///
		/// ## Events
		///
		/// - [`Event::InitialCollateralizationRatioUpdated`]: Emitted with old and new values.
		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::set_initial_collateralization_ratio())]
		pub fn set_initial_collateralization_ratio(
			origin: OriginFor<T>,
			ratio: FixedU128,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level == VaultsManagerLevel::Full, Error::<T>::InsufficientPrivilege);
			// Initial ratio must be >= minimum ratio to allow borrowing
			let min_ratio = MinimumCollateralizationRatio::<T>::get();
			ensure!(ratio >= min_ratio, Error::<T>::InitialRatioMustExceedMinimum);
			let old_value = InitialCollateralizationRatio::<T>::get();
			InitialCollateralizationRatio::<T>::put(ratio);
			Self::deposit_event(Event::InitialCollateralizationRatioUpdated {
				old_value,
				new_value: ratio,
			});
			Ok(())
		}

		/// Set the liquidation penalty.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`] with [`VaultsManagerLevel::Full`] privilege
		/// (typically GeneralAdmin). Emergency origin cannot modify this parameter.
		///
		/// ## Details
		///
		/// Sets the [`LiquidationPenalty`] applied to debt during liquidation. The penalty
		/// is added to the auction tab and incentivizes vault owners to maintain safe
		/// collateral levels. Expressed as [`Permill`] (e.g., `Permill::from_percent(13)`
		/// for 13%).
		///
		/// ## Errors
		///
		/// - [`Error::InsufficientPrivilege`]: If called by Emergency origin.
		///
		/// ## Events
		///
		/// - [`Event::LiquidationPenaltyUpdated`]: Emitted with old and new values.
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::set_liquidation_penalty())]
		pub fn set_liquidation_penalty(origin: OriginFor<T>, penalty: Permill) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level == VaultsManagerLevel::Full, Error::<T>::InsufficientPrivilege);
			let old_value = LiquidationPenalty::<T>::get();
			LiquidationPenalty::<T>::put(penalty);
			Self::deposit_event(Event::LiquidationPenaltyUpdated { old_value, new_value: penalty });
			Ok(())
		}

		/// Repay accumulated bad debt by burning pUSD from the InsuranceFund.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed`. Anyone can trigger bad debt repayment.
		///
		/// ## Details
		///
		/// Burns pUSD from [`Config::InsuranceFund`] to reduce [`BadDebt`] accumulated
		/// from auction shortfalls. If `amount` exceeds current bad debt, only the bad
		/// debt amount is burned.
		///
		/// ## Events
		///
		/// - [`Event::BadDebtRepaid`]: Emitted with the amount of bad debt healed.
		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::heal())]
		pub fn heal(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
			ensure_signed(origin)?;

			let current_bad_debt = BadDebt::<T>::get();
			let repay_amount = amount.min(current_bad_debt);

			if repay_amount.is_zero() {
				return Ok(());
			}

			// Burn pUSD from the InsuranceFund to cover the bad debt
			let burned = T::Asset::burn_from(
				T::StablecoinAssetId::get(),
				&T::InsuranceFund::get(),
				repay_amount,
				Preservation::Expendable,
				Precision::Exact,
				Fortitude::Force,
			)?;

			// Reduce bad debt
			BadDebt::<T>::mutate(|debt| {
				*debt = debt.saturating_sub(burned);
			});

			Self::deposit_event(Event::BadDebtRepaid { amount: burned });

			Ok(())
		}

		/// Set the maximum pUSD at risk in active auctions.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`] with [`VaultsManagerLevel::Full`] privilege
		/// (typically GeneralAdmin). Emergency origin cannot modify this parameter.
		///
		/// ## Details
		///
		/// Sets the [`MaxLiquidationAmount`] which is a **hard limit** on total pUSD debt
		/// that can be at risk in active auctions. Liquidations are blocked when this limit
		/// would be exceeded. Governance can adjust this to control auction exposure.
		///
		/// ## Errors
		///
		/// - [`Error::InsufficientPrivilege`]: If called by Emergency origin.
		///
		/// ## Events
		///
		/// - [`Event::MaxLiquidationAmountUpdated`]: Emitted with old and new values.
		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::set_max_liquidation_amount())]
		pub fn set_max_liquidation_amount(
			origin: OriginFor<T>,
			new_value: BalanceOf<T>,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level == VaultsManagerLevel::Full, Error::<T>::InsufficientPrivilege);
			let old_value = MaxLiquidationAmount::<T>::get();
			MaxLiquidationAmount::<T>::put(new_value);
			Self::deposit_event(Event::MaxLiquidationAmountUpdated { old_value, new_value });
			Ok(())
		}

		/// Force fee accrual on any vault.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed`. Anyone can poke any vault.
		///
		/// ## Details
		///
		/// Forces stability fee accrual on the specified vault. This is useful for:
		/// - Updating inactive vault owners who still need to accrue fees
		/// - Keeping vault state fresh for accurate collateralization queries
		/// - Protocol monitoring and maintenance before liquidation checks
		///
		/// ## Errors
		///
		/// - [`Error::VaultNotFound`]: If the target vault does not exist.
		/// - [`Error::VaultInLiquidation`]: If the vault is currently being liquidated.
		///
		/// ## Events
		///
		/// - [`Event::InterestUpdated`]: Emitted if interest was accrued.
		#[pallet::call_index(13)]
		#[pallet::weight(T::WeightInfo::poke())]
		pub fn poke(origin: OriginFor<T>, vault_owner: T::AccountId) -> DispatchResult {
			ensure_signed(origin)?;

			Vaults::<T>::try_mutate(&vault_owner, |maybe_vault| -> DispatchResult {
				let vault = maybe_vault.as_mut().ok_or(Error::<T>::VaultNotFound)?;

				ensure!(vault.status == VaultStatus::Healthy, Error::<T>::VaultInLiquidation);

				Self::update_vault_fees(vault, &vault_owner, None);

				Ok(())
			})
		}

		/// Set the maximum total debt allowed in the system (debt ceiling).
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`]. Both Full and Emergency privilege levels
		/// are supported with different capabilities:
		/// - **Full (GeneralAdmin)**: Can set any value (raise or lower).
		/// - **Emergency (EmergencyAction)**: Can only lower the ceiling, enabling fast-track
		///   emergency response to oracle attacks without full governance approval.
		///
		/// ## Details
		///
		/// Sets the [`MaximumIssuance`] which is the system-wide cap on total pUSD issuance.
		/// No new debt can be minted once this limit is reached.
		///
		/// ## Errors
		///
		/// - [`Error::CanOnlyLowerMaxDebt`]: If Emergency origin tries to raise the ceiling.
		///
		/// ## Events
		///
		/// - [`Event::MaximumIssuanceUpdated`]: Emitted with old and new values.
		#[pallet::call_index(14)]
		#[pallet::weight(T::WeightInfo::set_maximum_issuance())]
		pub fn set_maximum_issuance(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			let old_value = MaximumIssuance::<T>::get();

			// Emergency can only lower the ceiling
			if level == VaultsManagerLevel::Emergency {
				ensure!(amount <= old_value, Error::<T>::CanOnlyLowerMaxDebt);
			}

			MaximumIssuance::<T>::put(amount);
			Self::deposit_event(Event::MaximumIssuanceUpdated { old_value, new_value: amount });
			Ok(())
		}
	}

	// Implement CollateralManager for the Vaults pallet
	impl<T: Config> CollateralManager<T::AccountId> for Pallet<T> {
		type Balance = BalanceOf<T>;
		/// Get current collateral price from oracle.
		/// Returns `None` if the price is unavailable or zero.
		fn get_dot_price() -> Option<FixedU128> {
			T::Oracle::get_price(&T::CollateralLocation::get())
				.map(|(price, _timestamp)| price)
				.filter(|p| !p.is_zero())
		}

		fn execute_purchase(params: PurchaseParams<T::AccountId, BalanceOf<T>>) -> DispatchResult {
			let PurchaseParams {
				vault_owner,
				buyer,
				recipient,
				keeper,
				collateral_amount,
				payment,
			} = params;

			// 1. Burn principal pUSD from buyer
			if !payment.burn.is_zero() {
				T::Asset::burn_from(
					T::StablecoinAssetId::get(),
					&buyer,
					payment.burn,
					Preservation::Expendable,
					Precision::Exact,
					Fortitude::Force,
				)?;
			}

			// 2. Transfer interest/penalty to Insurance Fund
			if !payment.insurance_fund.is_zero() {
				T::Asset::transfer(
					T::StablecoinAssetId::get(),
					&buyer,
					&T::InsuranceFund::get(),
					payment.insurance_fund,
					Preservation::Expendable,
				)?;
			}

			// 3. Transfer keeper incentive from buyer to keeper
			if !payment.keeper.is_zero() {
				T::Asset::transfer(
					T::StablecoinAssetId::get(),
					&buyer,
					&keeper,
					payment.keeper,
					Preservation::Expendable,
				)?;
			}

			// 4. Release collateral from Seized hold and transfer to recipient
			if vault_owner != recipient {
				// Atomic: release from hold and transfer to recipient in one operation
				T::Currency::transfer_on_hold(
					&HoldReason::Seized.into(),
					&vault_owner,
					&recipient,
					collateral_amount,
					Precision::Exact,
					Restriction::Free, // Recipient receives as free balance
					Fortitude::Polite,
				)?;
			} else {
				// Just release back to vault_owner
				T::Currency::release(
					&HoldReason::Seized.into(),
					&vault_owner,
					collateral_amount,
					Precision::Exact,
				)?;
			}

			// Reduce CurrentLiquidationAmount as debt is collected
			let total_collected = payment
				.burn
				.saturating_add(payment.insurance_fund)
				.saturating_add(payment.keeper);
			CurrentLiquidationAmount::<T>::mutate(|current| {
				*current = current.saturating_sub(total_collected);
			});

			Self::deposit_event(Event::AuctionDebtCollected { amount: total_collected });

			Ok(())
		}

		/// Complete an auction: return excess collateral, record any shortfall.
		///
		/// This function handles the end-of-auction operations:
		/// - Returns remaining collateral to vault owner (if any)
		/// - Records shortfall as bad debt (if any)
		/// - Marks the vault as InLiquidation for cleanup
		fn complete_auction(
			vault_owner: &T::AccountId,
			remaining_collateral: BalanceOf<T>,
			shortfall: BalanceOf<T>,
		) -> DispatchResult {
			// Return excess collateral to vault owner
			if !remaining_collateral.is_zero() {
				// Release from Seized hold - collateral goes back to vault owner
				T::Currency::release(
					&HoldReason::Seized.into(),
					vault_owner,
					remaining_collateral,
					Precision::Exact,
				)?;
			}

			// Record shortfall as bad debt
			if !shortfall.is_zero() {
				// Reduce CurrentLiquidationAmount by shortfall (was never collected)
				CurrentLiquidationAmount::<T>::mutate(|current| {
					*current = current.saturating_sub(shortfall);
				});

				// Record as bad debt
				BadDebt::<T>::mutate(|bad_debt| {
					bad_debt.saturating_accrue(shortfall);
				});

				Self::deposit_event(Event::BadDebtAccrued {
					owner: vault_owner.clone(),
					amount: shortfall,
				});

				log::warn!(
					target: LOG_TARGET,
					"Auction shortfall: owner={:?}, shortfall={:?}",
					vault_owner,
					shortfall
				);

				Self::deposit_event(Event::AuctionShortfall { shortfall });
			}

			Vaults::<T>::remove(vault_owner);
			Self::deposit_event(Event::VaultClosed { owner: vault_owner.clone() });

			Ok(())
		}
	}

	// Test-only helper functions for internal logic testing
	#[cfg(test)]
	impl<T: Config> Pallet<T> {
		/// Reduce CurrentLiquidationAmount (simulates debt collection in auction).
		/// Test-only helper for isolated unit testing.
		pub fn test_reduce_liquidation_amount(amount: BalanceOf<T>) -> DispatchResult {
			CurrentLiquidationAmount::<T>::mutate(|current| {
				*current = current.saturating_sub(amount);
			});
			Self::deposit_event(Event::AuctionDebtCollected { amount });
			Ok(())
		}

		/// Record auction shortfall (simulates auction completion with shortfall).
		/// Test-only helper for isolated unit testing.
		pub fn test_record_shortfall(
			vault_owner: T::AccountId,
			shortfall: BalanceOf<T>,
		) -> DispatchResult {
			if !shortfall.is_zero() {
				CurrentLiquidationAmount::<T>::mutate(|current| {
					*current = current.saturating_sub(shortfall);
				});
				BadDebt::<T>::mutate(|bad_debt| {
					bad_debt.saturating_accrue(shortfall);
				});
				Self::deposit_event(Event::BadDebtAccrued {
					owner: vault_owner,
					amount: shortfall,
				});
				Self::deposit_event(Event::AuctionShortfall { shortfall });
			}
			Ok(())
		}
	}

	// Helper functions
	impl<T: Config> Pallet<T> {
		/// Calculate collateralization ratio from explicit collateral and debt values.
		///
		/// Formula:
		/// ```text
		/// collateral_value = collateral × price
		/// debt = principal + accrued_interest
		/// ratio = collateral_value / debt
		/// ```
		///
		/// Returns the ratio as FixedU128 (e.g., 150% = 1.5).
		pub fn calculate_collateralization_ratio(
			collateral: BalanceOf<T>,
			debt: BalanceOf<T>,
		) -> Result<Option<FixedU128>, DispatchError> {
			if debt.is_zero() {
				return Ok(None);
			}

			// Get fresh normalized price.
			let price = Self::get_fresh_price()?;

			// Convert collateral to stablecoin value using FixedPointOperand
			let collateral_value = price.saturating_mul_int(collateral);

			// Calculate ratio: collateral_value / debt (both in stablecoin smallest units)
			let ratio = FixedU128::saturating_from_rational(collateral_value, debt);

			Ok(Some(ratio))
		}

		/// Get the collateralization ratio for a vault.
		///
		/// Formula:
		/// ```text
		/// debt = principal + accrued_interest
		/// collateralization_ratio = collateral × price / debt
		/// ```
		///
		/// Returns `Ok(None)` if the vault has no principal and no accrued interest.
		pub fn get_collateralization_ratio(
			vault: &Vault<T>,
			who: &T::AccountId,
		) -> Result<Option<FixedU128>, DispatchError> {
			let held_collateral = vault.get_held_collateral(who);
			let total_debt = vault
				.principal
				.checked_add(&vault.accrued_interest)
				.ok_or(Error::<T>::ArithmeticOverflow)?;
			Self::calculate_collateralization_ratio(held_collateral, total_debt)
		}

		/// Update the accrued interest for a vault based on elapsed time.
		///
		/// Calculates interest in pUSD and adds it to the vault's accrued_interest.
		/// Uses actual timestamps for accurate time-based interest calculation.
		/// Emits an `InterestUpdated` event if interest was accrued.
		///
		/// # Parameters
		/// - `vault`: The vault to update
		/// - `who`: The vault owner (for event emission)
		/// - `now`: Optional timestamp; if `None`, fetches current time
		pub fn update_vault_fees(
			vault: &mut Vault<T>,
			who: &T::AccountId,
			now: Option<MomentOf<T>>,
		) {
			let now = now.unwrap_or_else(T::TimeProvider::now);
			if now <= vault.last_fee_update {
				return;
			}

			let millis_elapsed = now.saturating_sub(vault.last_fee_update);
			let stability_fee = StabilityFee::<T>::get();
			let annual_interest_pusd = stability_fee.mul_floor(vault.principal);

			let elapsed_ratio = FixedU128::saturating_from_rational(
				millis_elapsed.saturated_into::<u64>(),
				MILLIS_PER_YEAR,
			);
			let accrued = elapsed_ratio.saturating_mul_int(annual_interest_pusd);

			vault.accrued_interest.saturating_accrue(accrued);
			vault.last_fee_update = now;

			if !accrued.is_zero() {
				Self::deposit_event(Event::InterestUpdated { owner: who.clone(), amount: accrued });
			}
		}

		/// Base weight for `on_idle` overhead.
		///
		/// Includes:
		/// - Reading the cursor (1 read)
		/// - Writing cursor update (1 write, worst case)
		pub fn on_idle_base_weight() -> Weight {
			T::DbWeight::get().reads_writes(1, 1)
		}

		/// Benchmarked weight to process one stale vault.
		///
		/// This is derived from the `on_idle_one_vault` benchmark which measures
		/// the worst case: a stale vault with debt requiring fee calculation.
		pub fn on_idle_per_vault_weight() -> Weight {
			T::WeightInfo::on_idle_one_vault().saturating_sub(Self::on_idle_base_weight())
		}

		/// Get a price from the oracle.
		///
		/// Returns the price if it's available, non-zero, and within the staleness threshold.
		///
		/// # Errors
		/// - `PriceNotAvailable`: Oracle returned None or zero price
		/// - `OracleStale`: Price timestamp is older than `OracleStalenessThreshold`
		pub fn get_fresh_price() -> Result<FixedU128, DispatchError> {
			let (price, price_timestamp) = T::Oracle::get_price(&T::CollateralLocation::get())
				.filter(|(p, _)| !p.is_zero())
				.ok_or(Error::<T>::PriceNotAvailable)?;

			let current_time = T::TimeProvider::now();
			let threshold = T::OracleStalenessThreshold::get();
			let price_age = current_time.saturating_sub(price_timestamp);

			ensure!(price_age <= threshold, Error::<T>::OracleStale);

			Ok(price)
		}

		/// Ensure the Insurance Fund account exists by incrementing its provider count.
		///
		/// This is called at genesis and on runtime upgrade.
		/// It's idempotent - calling it multiple times is safe.
		///
		/// By using `inc_providers`, the account can receive any amount including
		/// those below the existential deposit (ED), preventing potential issues
		/// where transfers to the Insurance Fund could fail if it was reaped.
		pub fn ensure_insurance_fund_exists() {
			let insurance_fund = T::InsuranceFund::get();
			if !frame_system::Pallet::<T>::account_exists(&insurance_fund) {
				frame_system::Pallet::<T>::inc_providers(&insurance_fund);
				log::info!(
					target: LOG_TARGET,
					"Created Insurance Fund account: {:?}",
					insurance_fund
				);
			}
		}
	}
}
