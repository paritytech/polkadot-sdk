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

//! # Auctions Pallet
//!
//! Auction system for liquidating vault collateral and distributing protocol surplus.
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! The Auctions pallet implements `MakerDAO` Liquidation 2.0 style Dutch auctions for the pUSD
//! protocol. It handles two auction types: liquidation auctions (selling seized collateral to
//! cover vault debt) and surplus auctions (selling excess pUSD from the Insurance Fund for DOT).
//!
//! ### Key Concepts
//!
//! * **[`Auction`]**: A single auction instance tracking collateral, debt (tab), price curve
//!   parameters, and keeper information. Auctions are stored in [`Auctions`] storage.
//!
//! * **Dutch Auction**: Price starts high (oracle price × buffer) and decreases over time according
//!   to the [`PriceCurve`]. Buyers can purchase at the current price instantly.
//!
//! * **[`Tab`]**: The structured debt to be repaid, consisting of principal, accrued interest, and
//!   liquidation penalty. Payments are applied in order: principal first (burned), then interest
//!   (burned, since it was already minted to IF on accrual), then penalty (to Insurance Fund).
//!
//! * **Lot**: The amount of collateral available for purchase in an auction. Decreases as buyers
//!   take portions of the auction.
//!
//! * **[`CircuitBreakerLevel`]**: Four-level system for gradual shutdown:
//!   - [`AllEnabled`](CircuitBreakerLevel::AllEnabled): Normal operation
//!   - [`NoNewAuctions`](CircuitBreakerLevel::NoNewAuctions): Existing auctions continue, no new
//!     ones
//!   - [`NoNewAuctionsOrRestarts`](CircuitBreakerLevel::NoNewAuctionsOrRestarts): Only takes
//!     allowed
//!   - [`AllDisabled`](CircuitBreakerLevel::AllDisabled): Complete halt
//!
//! * **Keeper Incentives**: Rewards for maintaining auction health. Keepers who start or restart
//!   auctions receive `tip + chip × tab`, capped to the liquidation penalty and paid from the
//!   Insurance Fund at auction completion.
//!
//! ### Auction Types
//!
//! * **Liquidation Auction** ([`AuctionType::Liquidation`]): Sells seized DOT collateral for pUSD
//!   to cover vault debt. Started by `pallet-vaults` via [`AuctionsHandler::start_auction`]. Price
//!   is pUSD per DOT, decreasing over time.
//!
//! * **Surplus Auction** ([`AuctionType::Surplus`]): Sells excess pUSD from the Insurance Fund for
//!   DOT when IF balance exceeds [`Config::SurplusAuctionThreshold`] × total pUSD supply. Price is
//!   DOT per pUSD, decreasing over time. DOT proceeds go to Treasury.
//!
//! ### Auction Lifecycle
//!
//! 1. **Start**: Auction created with initial price = oracle price × buffer
//! 2. **Price Decay**: Price decreases according to [`PriceCurve`] (cubic polynomial)
//! 3. **Take**: Buyers call [`Pallet::take_liquidation`] or [`Pallet::take_surplus`] to purchase
//!    collateral/pUSD at current price. Partial purchases allowed.
//! 4. **Restart**: If auction exceeds `maximum_duration` or price falls below `minimum_price`,
//!    keepers can call [`Pallet::restart_auction`] to reset price
//! 5. **Completion**: When all collateral sold or debt fully covered:
//!    - Excess collateral returns to vault owner
//!    - Keeper incentive paid from Insurance Fund
//!    - Auction removed from storage
//!
//! ### Example
//!
//! The following example demonstrates a typical liquidation auction flow:
//!
//! ```ignore
//! // 1. Vault is liquidated (initiated by pallet-vaults, not directly callable)
//! // The vaults pallet calls start_auction with seized collateral and debt
//!
//! // 2. Buyer purchases collateral at current Dutch auction price
//! Auctions::take_liquidation(
//!     RuntimeOrigin::signed(buyer),
//!     auction_id,      // Auction to purchase from
//!     dot_amount,      // Maximum DOT to purchase
//!     max_price,       // Maximum pUSD per DOT willing to pay
//!     recipient,       // Account to receive the DOT
//! )?;
//!
//! // 3. If auction stalls, any keeper can restart it
//! Auctions::restart_auction(
//!     RuntimeOrigin::signed(keeper),
//!     auction_id,      // Stale auction to restart
//!     keeper,          // Account to receive incentive at completion
//! )?;
//! ```
//!
//! ## Low Level / Implementation Details
//!
//! ### Price Calculation
//!
//! The [`PriceCurve::SlowedExponentialDecrease`] uses a cubic polynomial for calculation:
//!
//! ```text
//! price = max(
//!     oracle_price × center_ratio - cubic_term - linear_term,
//!     starting_price × minimum_price
//! )
//! ```
//!
//! The curve inflects around `center` blocks, starting slow, accelerating, then slowing again.
//! A floor at `minimum_price` ratio prevents prices from going too low.
//!
//! ### Surplus Handling Modes
//!
//! The pallet supports two modes for handling Insurance Fund surplus via [`SurplusHandlingMode`]:
//!
//! * **Auction**: Surplus pUSD is auctioned for DOT (sent to Treasury)
//! * **`DirectTransfer`**: Surplus pUSD is transferred directly to Treasury (no auction)
//!
//! ### On-Idle Housekeeping
//!
//! The pallet uses `on_idle` to automatically restart stale liquidation auctions:
//! - Uses cursor-based pagination ([`OnIdleCursor`]) to continue across blocks
//! - Respects circuit breaker (blocked at level ≥ [`CircuitBreakerLevel::NoNewAuctionsOrRestarts`])
//! - Surplus auctions simply end when stale (unsold pUSD stays in Insurance Fund)
//! - Limited by [`Config::MaxOnIdleItems`] per block
//!
//! ### External Traits
//!
//! The pallet implements [`AuctionsHandler`] for the vaults pallet to:
//! - Start liquidation auctions via [`AuctionsHandler::start_auction`]
//!
//! During takes, it calls back to vaults via [`Config::CollateralManager`] to:
//! - Query oracle prices via [`CollateralManager::get_dot_price`]
//! - Execute purchases via [`CollateralManager::execute_purchase`]
//! - Complete auctions via [`CollateralManager::complete_auction`]
//!
//! This design keeps all asset operations centralized in the vaults pallet while
//! allowing the auction logic to remain reusable for other collateral sources.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
pub use price_calculators::PriceCurve;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

/// Price calculator implementations for Dutch auctions.
pub mod price_calculators;

/// Storage migrations.
pub mod migrations;

/// Helper trait for benchmarking setup.
///
/// Provides methods to set up the runtime state required for benchmarks,
/// such as funding accounts, creating holds, manipulating time, and setting prices.
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<AccountId, Balance> {
	/// Set the oracle price for DOT/pUSD.
	fn set_price(price: sp_runtime::FixedU128);

	/// Fund an account with native currency (DOT).
	fn fund_account(account: &AccountId, amount: Balance);

	/// Fund an account with pUSD stablecoin.
	fn fund_pusd(account: &AccountId, amount: Balance);

	/// Set up a liquidation auction: create seized hold and fund Insurance Fund.
	///
	/// This simulates the state after vaults pallet liquidates a vault:
	/// - Creates a seized collateral hold on `vault_owner`
	/// - Funds the Insurance Fund with pUSD for keeper payments
	fn setup_liquidation(
		vault_owner: &AccountId,
		collateral: Balance,
		insurance_fund_amount: Balance,
	);

	/// Set up surplus auction eligibility.
	///
	/// For surplus auctions to start, IF balance must exceed `threshold × pUSD_supply`.
	/// This method:
	/// 1. Funds the Insurance Fund with pUSD for the auction
	/// 2. Sets mock values for `CollateralManager::get_insurance_fund_balance()` and
	///    `CollateralManager::get_total_pusd_supply()` threshold checks
	fn setup_surplus_threshold(insurance_fund_amount: Balance, pusd_supply: Balance);
}

#[frame_support::pallet]
pub mod pallet {
	use crate::{price_calculators::PriceCurve, weights::WeightInfo};
	use alloc::boxed::Box;
	use frame_support::{pallet_prelude::*, weights::WeightMeter, DefaultNoBound};
	use frame_system::pallet_prelude::*;
	use sp_pusd::{AuctionsHandler, CollateralManager, DebtComponents, PaymentBreakdown};
	use sp_runtime::{
		traits::{One, SaturatedConversion, Saturating, Zero},
		FixedPointNumber, FixedU128, Permill,
	};

	/// Balance type alias derived from `CollateralManager`.
	pub type BalanceOf<T> = <<T as Config>::CollateralManager as CollateralManager<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	/// Structured debt components for an auction.
	///
	/// Tracks the breakdown of debt to ensure correct payment distribution:
	/// - Principal: burned to maintain pUSD peg (priority 1)
	/// - Interest: burned (was already minted to IF on accrual) (priority 2)
	/// - Penalty: transferred to Insurance Fund (priority 3)
	///
	/// Keeper incentives are NOT included in the payment priority during takes.
	/// Instead, the keeper is paid a fixed amount at auction completion from the
	/// Insurance Fund, capped to the penalty actually collected.
	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug, Default)]
	#[scale_info(skip_type_params(T))]
	pub struct Tab<T: Config> {
		/// Original principal debt - burned to maintain pUSD peg.
		pub principal: BalanceOf<T>,
		/// Accrued interest at liquidation time - burned (was already minted to IF on accrual).
		pub accrued_interest: BalanceOf<T>,
		/// Liquidation penalty - goes to Insurance Fund.
		pub penalty: BalanceOf<T>,
	}

	impl<T: Config> Tab<T> {
		/// Create a new Tab from liquidation components.
		pub(crate) const fn new(
			principal: BalanceOf<T>,
			accrued_interest: BalanceOf<T>,
			penalty: BalanceOf<T>,
		) -> Self {
			Self { principal, accrued_interest, penalty }
		}

		/// Total debt to raise from the auction (buyers pay base tab only).
		pub fn total(&self) -> BalanceOf<T> {
			self.principal
				.saturating_add(self.accrued_interest)
				.saturating_add(self.penalty)
		}

		/// Compute payment distribution without mutating state.
		///
		/// Payment priority: principal first (burn), then interest (burn),
		/// then penalty (to IF).
		///
		/// Note: Keeper incentive is NOT included in the payment priority during takes.
		/// The keeper is paid a fixed amount at auction completion from the Insurance Fund.
		fn compute_payment(&self, amount: BalanceOf<T>) -> PaymentBreakdown<BalanceOf<T>> {
			let mut remaining = amount;

			// Principal is paid first (burned)
			let principal = remaining.min(self.principal);
			remaining = remaining.saturating_sub(principal);

			// Interest is paid next (transferred to IF)
			let interest = remaining.min(self.accrued_interest);
			remaining = remaining.saturating_sub(interest);

			// Penalty goes to Insurance Fund (keeper paid at completion)
			let penalty = remaining.min(self.penalty);

			PaymentBreakdown::new(principal, interest, penalty)
		}

		/// Simulate what `total()` would be after applying a payment.
		pub(crate) fn remaining_after_payment(&self, amount: BalanceOf<T>) -> BalanceOf<T> {
			let breakdown = self.compute_payment(amount);
			self.principal
				.saturating_sub(breakdown.principal_paid)
				.saturating_add(self.accrued_interest.saturating_sub(breakdown.interest_paid))
				.saturating_add(self.penalty.saturating_sub(breakdown.penalty_paid))
		}

		/// Apply a payment with priority: principal first, then interest, then penalty.
		/// Returns a `PaymentBreakdown` showing how the payment is distributed.
		pub(crate) fn apply_payment(
			&mut self,
			amount: BalanceOf<T>,
		) -> PaymentBreakdown<BalanceOf<T>> {
			let breakdown = self.compute_payment(amount);

			self.principal = self.principal.saturating_sub(breakdown.principal_paid);
			self.accrued_interest = self.accrued_interest.saturating_sub(breakdown.interest_paid);
			self.penalty = self.penalty.saturating_sub(breakdown.penalty_paid);

			breakdown
		}
	}

	/// A single auction instance (equivalent to `MakerDAO`'s `Sale` struct).
	///
	/// For liquidation auctions: DOT collateral is held via `HoldReason::Seized` on the vault
	/// owner's account. For surplus auctions: pUSD is held in the Insurance Fund.
	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
	#[scale_info(skip_type_params(T))]
	pub struct Auction<T: Config> {
		/// Type of auction (Liquidation or Surplus).
		pub auction_type: AuctionType,
		/// Structured debt components (principal, interest, penalty).
		/// For liquidation: debt to repay. For surplus: pUSD amount being sold.
		pub tab: Tab<T>,
		/// Amount of DOT collateral available for purchase.
		/// For liquidation: seized DOT. For surplus: unused (Zero).
		pub auctionable_collateral: BalanceOf<T>,
		/// Original vault owner (receives excess collateral).
		/// For liquidation: vault owner. For surplus: None (protocol-owned).
		pub vault_owner: Option<T::AccountId>,
		/// Auction start block (for price calculation).
		pub starting_block: BlockNumberFor<T>,
		/// Initial price (oracle price * buffer).
		/// For liquidation: pUSD per DOT. For surplus: DOT per pUSD.
		pub starting_price: FixedU128,
		/// Keeper who will receive the incentive at completion.
		/// Initially the liquidation/surplus auction starter, updated on restart.
		pub keeper: T::AccountId,
		/// Keeper incentive amount (tip + chip × tab, capped to penalty).
		/// Fixed at auction start, paid from IF at completion (capped to `penalty_collected`).
		pub keeper_incentive: BalanceOf<T>,
		/// Penalty actually collected during takes.
		pub penalty_collected: BalanceOf<T>,
	}

	/// Configuration parameters for an auction type.
	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
	#[scale_info(skip_type_params(T))]
	pub struct AuctionConfigRecord<T: Config> {
		/// Price buffer: multiplier for initial price (e.g., 1.2 = 20% above oracle).
		pub buffer: FixedU128,
		/// Maximum auction duration before reset needed (in blocks).
		pub maximum_duration: BlockNumberFor<T>,
		/// Minimum price ratio before reset needed (e.g., 0.65 = 65%).
		pub minimum_price: FixedU128,
		/// Percentage of base tab for keeper incentive when starting a liquidation auction.
		pub chip: Permill,
		/// Flat keeper incentive in pUSD when starting a liquidation auction.
		pub tip: BalanceOf<T>,
		/// Price decay curve for auctions.
		pub curve: PriceCurve,
	}

	impl<T: Config> AuctionConfigRecord<T> {
		/// Default config for liquidation auctions.
		pub fn default_liquidation() -> Self {
			Self {
				buffer: FixedU128::from_rational(120, 100), // 20% above oracle
				maximum_duration: 300u32.into(),            // ~30 minutes at 6s blocks
				minimum_price: FixedU128::from_rational(65, 100), // 65% of initial
				chip: Permill::from_parts(1000),            // 0.1%
				tip: One::one(),                            // 1 pUSD flat fee
				curve: PriceCurve::default(),
			}
		}

		/// Default config for surplus auctions.
		pub fn default_surplus() -> Self {
			Self {
				minimum_price: FixedU128::from_rational(795, 1000),
				chip: Permill::zero(),
				tip: Zero::zero(),
				..Self::default_liquidation()
			}
		}
	}

	impl<T: Config> Default for AuctionConfigRecord<T> {
		fn default() -> Self {
			Self::default_liquidation()
		}
	}

	/// Configuration parameters that can be updated.
	#[derive(
		Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
	)]
	pub enum ConfigParameter {
		Buffer,
		MaximumDuration,
		MinimumPrice,
		Chip,
		Tip,
		Curve,
	}

	/// Circuit breaker levels for gradual shutdown.
	///
	/// The circuit breaker provides a mechanism for gradual system shutdown:
	/// - `AllEnabled`: Normal operation, all actions permitted
	/// - `NoNewAuctions`: New auctions blocked, existing auctions can proceed
	/// - `NoNewAuctionsOrRestarts`: New auctions and restarts blocked, only takes allowed
	/// - `AllDisabled`: Emergency stop, all auction operations blocked
	#[derive(
		Encode,
		Decode,
		DecodeWithMemTracking,
		MaxEncodedLen,
		TypeInfo,
		Clone,
		Copy,
		PartialEq,
		Eq,
		PartialOrd,
		Ord,
		Debug,
		Default,
	)]
	pub enum CircuitBreakerLevel {
		/// Level 0: All operations enabled (normal operation).
		#[default]
		AllEnabled,
		/// Level 1: New auctions blocked.
		NoNewAuctions,
		/// Level 2: New auctions and restarts blocked.
		NoNewAuctionsOrRestarts,
		/// Level 3: All operations blocked (emergency stop).
		AllDisabled,
	}

	/// Surplus handling mode for governance-controlled distribution.
	///
	/// Controls how surplus pUSD from the Insurance Fund is distributed:
	/// - **Auction**: Surplus pUSD is auctioned for DOT
	/// - **DirectTransfer**: Surplus pUSD is transferred directly via `SurplusHandler` (default)
	#[derive(
		Encode,
		Decode,
		DecodeWithMemTracking,
		MaxEncodedLen,
		TypeInfo,
		Clone,
		Copy,
		PartialEq,
		Eq,
		Debug,
		Default,
	)]
	pub enum SurplusHandlingMode {
		/// Surplus pUSD is auctioned for DOT (sent to Treasury).
		/// Allows price discovery and potentially better rates.
		Auction,
		/// Surplus pUSD is transferred directly to the configured handler (DAP).
		/// Simpler, faster, but no price discovery.
		#[default]
		DirectTransfer,
	}

	/// Type of auction being conducted.
	///
	/// The auction system supports two types of auctions:
	/// - **Liquidation**: Sells DOT collateral for pUSD to repay vault debt
	/// - **Surplus**: Sells excess pUSD from the Insurance Fund for DOT (sent to Treasury)
	#[derive(
		Encode,
		Decode,
		DecodeWithMemTracking,
		MaxEncodedLen,
		TypeInfo,
		Clone,
		Copy,
		PartialEq,
		Eq,
		Debug,
		Default,
	)]
	pub enum AuctionType {
		/// Liquidation auction: sells seized DOT collateral for pUSD.
		/// - Price is in pUSD per DOT (decreases over time)
		/// - pUSD received is burned (principal) or sent to Insurance Fund (interest/penalty)
		/// - DOT is transferred from vault owner's seized hold to buyer
		#[default]
		Liquidation,
		/// Surplus auction: sells excess pUSD from Insurance Fund for DOT.
		/// - Price is in DOT per pUSD (decreases over time, meaning less DOT per pUSD)
		/// - pUSD is transferred from Insurance Fund to buyer
		/// - DOT received from buyer is sent to Treasury
		Surplus,
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Collateral manager providing all asset operations.
		/// Abstracts oracle, holds, stablecoin, and insurance fund interactions.
		/// The Balance type is derived from this trait.
		type CollateralManager: CollateralManager<Self::AccountId>;

		/// Minimum tab (debt) that must remain after a partial take.
		/// Prevents auctions from being left in unattractive "dusty" states.
		#[pallet::constant]
		type MinAuctionTab: Get<BalanceOf<Self>>;

		/// Minimum collateral amount per purchase for liquidation auctions.
		/// Prevents micro-purchases that would be economically inefficient.
		#[pallet::constant]
		type MinPurchaseAmount: Get<BalanceOf<Self>>;

		/// Origin allowed to execute emergency actions (`set_stopped`).
		/// Should be a multisig or technical committee for fast response.
		type ManagerOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Threshold for starting surplus auctions.
		/// Insurance Fund balance must exceed this percentage of total pUSD supply.
		/// E.g., 5% means IF must have > 5% of total pUSD supply as surplus.
		#[pallet::constant]
		type SurplusAuctionThreshold: Get<Permill>;

		/// Amount of pUSD to auction per surplus auction.
		/// E.g., 100,000 pUSD per auction.
		#[pallet::constant]
		type SurplusAuctionAmount: Get<BalanceOf<Self>>;

		/// Minimum pUSD amount per surplus auction purchase.
		/// Prevents micro-purchases in surplus auctions.
		#[pallet::constant]
		type MinSurplusPurchaseAmount: Get<BalanceOf<Self>>;

		/// Weight info.
		type WeightInfo: WeightInfo;

		/// Maximum number of auctions to process per `on_idle` call.
		///
		/// This is a safety limit independent of weight to guard against benchmarking
		/// inaccuracies. Even if weight budget allows more, iteration stops after this
		/// many auctions. Set to `u32::MAX` to effectively disable this limit.
		#[pallet::constant]
		type MaxOnIdleItems: Get<u32>;

		/// Helper type for benchmarking setup.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::BenchmarkHelper<Self::AccountId, BalanceOf<Self>>;
	}

	/// Current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// Configuration parameters per auction type.
	#[pallet::storage]
	pub type AuctionConfig<T: Config> =
		StorageMap<_, Twox64Concat, AuctionType, AuctionConfigRecord<T>, ValueQuery>;

	/// The next auction ID to be assigned.
	#[pallet::storage]
	pub type NextAuctionId<T: Config> = StorageValue<_, u32, ValueQuery, NextAuctionIdDefault>;

	/// Default value for `NextAuctionId` (starts at 1).
	#[pallet::type_value]
	pub fn NextAuctionIdDefault() -> u32 {
		1
	}

	/// Map of auction ID -> Auction data.
	#[pallet::storage]
	pub type Auctions<T: Config> = CountedStorageMap<_, Blake2_128Concat, u32, Auction<T>>;

	/// Circuit breaker status for gradual shutdown.
	#[pallet::storage]
	pub type Stopped<T: Config> = StorageValue<_, CircuitBreakerLevel, ValueQuery>;

	/// Cursor for `on_idle` pagination.
	///
	/// Stores the last processed auction ID to continue iteration across blocks.
	/// This prevents restarting from the beginning each block and ensures all
	/// auctions are eventually scanned for staleness without unbounded iteration.
	#[pallet::storage]
	pub type OnIdleCursor<T: Config> = StorageValue<_, u32, OptionQuery>;

	/// ID of the currently active surplus auction.
	///
	/// Only one surplus auction can be active at a time to prevent race conditions
	/// where multiple auctions could "promise" more pUSD than the Insurance Fund
	/// has available as surplus. This limitation could be lifted if `pallet-assets-holder`
	/// becomes available, allowing pUSD to be held (reserved) for each auction.
	#[pallet::storage]
	pub type ActiveSurplusAuctionId<T: Config> = StorageValue<_, u32, OptionQuery>;

	/// Current surplus handling mode.
	///
	/// Controls whether surplus pUSD is distributed via auction or direct transfer.
	/// - `Auction`: `start_surplus_auction()` enabled
	/// - `DirectTransfer` (default): `transfer_surplus()` enabled
	///
	/// Changed via governance using `set_surplus_mode()`.
	#[pallet::storage]
	pub type SurplusMode<T: Config> = StorageValue<_, SurplusHandlingMode, ValueQuery>;

	/// Genesis configuration for the auctions pallet.
	///
	/// Initializes auction config with sensible defaults. For custom configuration,
	/// use governance calls after genesis or the v1 migration with custom parameters.
	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		#[serde(skip)]
		_marker: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			AuctionConfig::<T>::insert(
				AuctionType::Liquidation,
				AuctionConfigRecord::<T>::default_liquidation(),
			);
			AuctionConfig::<T>::insert(
				AuctionType::Surplus,
				AuctionConfigRecord::<T>::default_surplus(),
			);
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Auction started (liquidation or surplus).
		///
		/// For liquidation: `lot` is DOT collateral, `tab` is debt to repay.
		/// For surplus: `lot` is zero, `tab` is pUSD amount being sold.
		AuctionStarted {
			/// Liquidation or Surplus.
			auction_type: AuctionType,
			/// Unique auction identifier.
			id: u32,
			/// Target amount to raise (debt for liquidation, pUSD for surplus).
			tab: BalanceOf<T>,
			/// Collateral for sale (DOT for liquidation, zero for surplus).
			lot: BalanceOf<T>,
			/// Vault owner (liquidation only, None for surplus).
			owner: Option<T::AccountId>,
			/// Block at which price decay begins.
			starting_block: BlockNumberFor<T>,
			/// Initial price (oracle price × buffer).
			starting_price: FixedU128,
			/// Account receiving keeper incentive at completion.
			keeper: T::AccountId,
		},
		/// Purchase from auction.
		///
		/// For liquidation: buyer pays pUSD (`payment`), receives DOT (`received`).
		/// For surplus: buyer pays DOT (`payment`), receives pUSD (`received`).
		Take {
			/// Liquidation or Surplus.
			auction_type: AuctionType,
			/// Auction identifier.
			id: u32,
			/// Maximum acceptable price.
			max: FixedU128,
			/// Actual price at time of purchase.
			price: FixedU128,
			/// Amount buyer paid (pUSD for liquidation, DOT for surplus).
			payment: BalanceOf<T>,
			/// Amount buyer received (DOT for liquidation, pUSD for surplus).
			received: BalanceOf<T>,
			/// Account that received the purchased asset.
			recipient: T::AccountId,
		},
		/// Auction completed.
		///
		/// For liquidation: `remaining` is unsold DOT, `shortfall` is unpaid principal +
		/// interest (bad debt). Penalty shortfall is excluded (no pUSD minted against it).
		/// For surplus: `remaining` is unsold pUSD, `shortfall` is always zero.
		AuctionCompleted {
			/// Liquidation or Surplus.
			auction_type: AuctionType,
			/// Auction identifier.
			id: u32,
			/// Remaining unsold amount (DOT for liquidation, pUSD for surplus).
			remaining: BalanceOf<T>,
			/// Unpaid principal + interest (liquidation only, zero for surplus).
			shortfall: BalanceOf<T>,
		},
		/// Auction restarted by keeper.
		AuctionRestarted {
			/// Liquidation or Surplus.
			auction_type: AuctionType,
			/// Auction identifier.
			id: u32,
			/// New initial price (oracle price × buffer).
			starting_price: FixedU128,
			/// Remaining tab (debt for liquidation, pUSD for surplus).
			tab: BalanceOf<T>,
			/// Remaining lot (DOT for liquidation, zero for surplus).
			lot: BalanceOf<T>,
			/// Vault owner (liquidation only, None for surplus).
			owner: Option<T::AccountId>,
			/// New keeper who will receive incentive at completion.
			keeper: T::AccountId,
			/// Fixed incentive amount (set at auction start, paid at completion).
			incentive: BalanceOf<T>,
		},
		/// Configuration parameter updated for an auction type.
		ConfigUpdated { auction_type: AuctionType, parameter: ConfigParameter },
		/// Circuit breaker status changed.
		StoppedUpdated { level: CircuitBreakerLevel },
		/// Surplus handling mode changed by governance.
		SurplusModeUpdated { mode: SurplusHandlingMode },
		/// Surplus pUSD transferred directly to treasury (`DirectTransfer` mode).
		SurplusTransferred { amount: BalanceOf<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Auction not found.
		AuctionNotFound,
		/// Auction needs to be restarted first (stale).
		AuctionNeedsRestart,
		/// Price higher than max acceptable.
		PriceTooHigh,
		/// Circuit breaker: new auctions stopped (level >= 1).
		AuctionsStopped,
		/// Circuit breaker: restart stopped (level >= 2).
		RestartStopped,
		/// Circuit breaker: take stopped (level >= 3).
		TakeStopped,
		/// Auction does not need restart.
		DoesNotNeedRestart,
		/// Remaining auction would be too small (dust).
		DustyAuction,
		/// Purchase amount below `MinPurchaseAmount`.
		PurchaseTooSmall,
		/// Price not available from oracle.
		PriceNotAvailable,
		/// Insurance Fund balance is below the surplus auction threshold.
		InsufficientSurplus,
		/// Wrong auction type for this operation.
		InvalidAuctionType,
		/// A surplus auction is already in progress; only one can be active at a time.
		SurplusAuctionAlreadyActive,
		/// Surplus auctions are disabled in `DirectTransfer` mode.
		SurplusAuctionsDisabled,
		/// Direct transfer is disabled in Auction mode.
		DirectTransferDisabled,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Idle block housekeeping: restart stale auctions.
		///
		/// Uses cursor-based pagination to continue across blocks, ensuring all
		/// auctions are eventually processed.
		fn on_idle(now: BlockNumberFor<T>, limit: Weight) -> Weight {
			let mut meter = WeightMeter::with_limit(limit);

			// Early exit if not enough weight for minimum work.
			let min_weight = Self::on_idle_weight();
			if meter.try_consume(min_weight).is_err() {
				return meter.consumed();
			}

			// Respect circuit breaker: restarts blocked at level >= NoNewAuctionsOrRestarts.
			if Stopped::<T>::get() >= CircuitBreakerLevel::NoNewAuctionsOrRestarts {
				return meter.consumed();
			}

			let cursor = OnIdleCursor::<T>::get();

			let per_auction_read = T::DbWeight::get().reads(1);
			let per_restart = T::WeightInfo::restart_auction();

			let iter: Box<dyn Iterator<Item = (u32, Auction<T>)>> = match cursor {
				Some(ref last_key) => Box::new(
					Auctions::<T>::iter_from(Auctions::<T>::hashed_key_for(last_key)).skip(1),
				),
				None => Box::new(Auctions::<T>::iter()),
			};

			let max_items = T::MaxOnIdleItems::get();
			let mut items_processed: u32 = 0;
			let mut last_processed: Option<u32> = None;

			for (id, auction) in iter {
				if items_processed >= max_items {
					break;
				}

				if meter.try_consume(per_auction_read).is_err() {
					break;
				}

				items_processed = items_processed.saturating_add(1);

				// Get config for this auction type
				let config = AuctionConfig::<T>::get(auction.auction_type);

				// Determine staleness using auction-type-specific config.
				let elapsed = now.saturating_sub(auction.starting_block);
				let mut is_stale = elapsed >= config.maximum_duration;

				if !is_stale {
					let elapsed_u64: u64 = elapsed.saturated_into();
					let current = config.curve.calculate_price(
						auction.starting_price,
						config.buffer,
						elapsed_u64,
					);
					if let Some(ratio) = current.checked_div(&auction.starting_price) {
						is_stale = ratio <= config.minimum_price;
					} else {
						// starting_price is zero => stale.
						is_stale = true;
					}
				}

				if is_stale {
					if meter.try_consume(per_restart).is_err() {
						break;
					}

					match auction.auction_type {
						AuctionType::Liquidation => {
							// Restart using the existing keeper so on_idle does not change who is
							// paid.
							let _ = Self::do_restart_auction(id, auction.keeper.clone());
						},
						AuctionType::Surplus => {
							// Surplus auctions simply end when stale - unsold pUSD stays in IF
							Self::deposit_event(Event::AuctionCompleted {
								auction_type: AuctionType::Surplus,
								id,
								remaining: auction.tab.principal,
								shortfall: Zero::zero(),
							});
							Auctions::<T>::remove(id);
							ActiveSurplusAuctionId::<T>::kill();
						},
					}
				}

				last_processed = Some(id);
			}

			match last_processed {
				Some(last) => {
					if Auctions::<T>::iter_from(Auctions::<T>::hashed_key_for(last))
						.nth(1)
						.is_none()
					{
						OnIdleCursor::<T>::kill();
					} else {
						OnIdleCursor::<T>::put(last);
					}
				},
				None => {
					OnIdleCursor::<T>::kill();
				},
			}

			meter.consumed()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Purchase collateral from a liquidation auction.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed` by the buyer.
		///
		/// ## Details
		///
		/// Purchases DOT collateral from a liquidation auction at the current Dutch auction price.
		/// The buyer specifies a maximum amount of DOT to purchase and a maximum price they're
		/// willing to pay (pUSD per DOT). The actual purchase may be less than requested if the
		/// auction doesn't have enough collateral or the debt would be overpaid.
		///
		///
		/// ## Arguments
		///
		/// - `id`: Auction ID
		/// - `dot_amount`: Maximum DOT collateral to purchase
		/// - `max_pusd_per_dot`: Maximum acceptable price (pUSD per DOT)
		/// - `recipient`: Account to receive the purchased DOT
		///
		/// ## Errors
		///
		/// - [`Error::TakeStopped`]: If circuit breaker is at `AllDisabled`
		/// - [`Error::AuctionNotFound`]: If auction ID doesn't exist
		/// - [`Error::InvalidAuctionType`]: If auction is not a liquidation auction
		/// - [`Error::AuctionNeedsRestart`]: If auction has gone stale
		/// - [`Error::PriceTooHigh`]: If current price exceeds `max_pusd_per_dot`
		/// - [`Error::PurchaseTooSmall`]: If purchase amount is below minimum
		/// - [`Error::DustyAuction`]: If purchase would leave a dusty remaining auction
		///
		/// ## Events
		///
		/// - [`Event::Take`]: Emitted when purchase succeeds
		/// - [`Event::AuctionCompleted`]: Emitted if auction completes with this purchase
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::take_liquidation())]
		pub fn take_liquidation(
			origin: OriginFor<T>,
			id: u32,
			dot_amount: BalanceOf<T>,
			max_pusd_per_dot: FixedU128,
			recipient: T::AccountId,
		) -> DispatchResult {
			let buyer = ensure_signed(origin)?;

			// 1. Check circuit breaker (take blocked only at AllDisabled)
			ensure!(
				Stopped::<T>::get() != CircuitBreakerLevel::AllDisabled,
				Error::<T>::TakeStopped
			);

			// 2. Load and validate auction
			let auction = Auctions::<T>::get(id).ok_or(Error::<T>::AuctionNotFound)?;
			ensure!(
				auction.auction_type == AuctionType::Liquidation,
				Error::<T>::InvalidAuctionType
			);
			ensure!(!Self::needs_restart(&auction), Error::<T>::AuctionNeedsRestart);

			Self::do_take_liquidation(&buyer, id, auction, dot_amount, max_pusd_per_dot, recipient)
		}

		/// Purchase pUSD from a surplus auction.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed` by the buyer.
		///
		/// ## Details
		///
		/// Purchases pUSD from a surplus auction at the current Dutch auction price.
		/// The buyer pays DOT and receives pUSD from the Insurance Fund. The price
		/// decreases over time (DOT per pUSD), allowing buyers to purchase at their
		/// desired rate.
		///
		/// The buyer specifies a maximum amount of pUSD to purchase and a maximum
		/// price (DOT per pUSD). The DOT paid is sent to the Treasury.
		///
		/// ## Arguments
		///
		/// - `id`: Auction ID
		/// - `pusd_amount`: Maximum pUSD to purchase
		/// - `max_dot_per_pusd`: Maximum acceptable price (DOT per pUSD)
		/// - `recipient`: Account to receive the purchased pUSD
		///
		/// ## Errors
		///
		/// - [`Error::TakeStopped`]: If circuit breaker is at `AllDisabled`
		/// - [`Error::AuctionNotFound`]: If auction ID doesn't exist
		/// - [`Error::InvalidAuctionType`]: If auction is not a surplus auction
		/// - [`Error::AuctionNeedsRestart`]: If auction has gone stale
		/// - [`Error::PriceTooHigh`]: If current price exceeds `max_dot_per_pusd`
		/// - [`Error::PurchaseTooSmall`]: If purchase amount is below minimum
		/// - [`Error::DustyAuction`]: If purchase would leave a dusty remaining auction
		///
		/// ## Events
		///
		/// - [`Event::Take`]: Emitted when purchase succeeds
		/// - [`Event::AuctionCompleted`]: Emitted if auction completes with this purchase
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::take_surplus())]
		pub fn take_surplus(
			origin: OriginFor<T>,
			id: u32,
			pusd_amount: BalanceOf<T>,
			max_dot_per_pusd: FixedU128,
			recipient: T::AccountId,
		) -> DispatchResult {
			let buyer = ensure_signed(origin)?;

			// 1. Check circuit breaker (take blocked only at AllDisabled)
			ensure!(
				Stopped::<T>::get() != CircuitBreakerLevel::AllDisabled,
				Error::<T>::TakeStopped
			);

			// 2. Load and validate auction
			let auction = Auctions::<T>::get(id).ok_or(Error::<T>::AuctionNotFound)?;
			ensure!(auction.auction_type == AuctionType::Surplus, Error::<T>::InvalidAuctionType);
			ensure!(!Self::needs_restart(&auction), Error::<T>::AuctionNeedsRestart);

			Self::do_take_surplus(&buyer, id, auction, pusd_amount, max_dot_per_pusd, recipient)
		}

		/// Restart a stale auction.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed`. Anyone can restart a stale auction (keeper incentive).
		///
		/// ## Details
		///
		/// Restarts an auction that has exceeded `maximum_duration` or whose price has
		/// fallen below `minimum_price` ratio of the starting price. The auction is
		/// reset with:
		/// - New `starting_block` = current block
		/// - New `starting_price` = current oracle price × buffer
		/// - New `keeper` = caller (receives incentive at auction completion)
		///
		/// The new keeper replaces the previous one and will receive the incentive
		/// when the auction completes, encouraging keepers to maintain auction health.
		///
		/// ## Arguments
		///
		/// - `id`: Auction ID to restart
		/// - `keeper`: Account to receive keeper incentive at completion
		///
		/// ## Errors
		///
		/// - [`Error::RestartStopped`]: If circuit breaker is at `NoNewAuctionsOrRestarts` or
		///   higher
		/// - [`Error::AuctionNotFound`]: If auction ID doesn't exist
		/// - [`Error::DoesNotNeedRestart`]: If auction is not stale
		///
		/// ## Events
		///
		/// - [`Event::AuctionRestarted`]: Emitted when auction is successfully restarted
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::restart_auction())]
		pub fn restart_auction(
			origin: OriginFor<T>,
			id: u32,
			keeper: T::AccountId,
		) -> DispatchResult {
			ensure_signed(origin)?;
			Self::do_restart_auction(id, keeper)
		}

		/// Start a surplus auction to sell excess pUSD for DOT.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed`. Anyone can start a surplus auction when conditions are met.
		///
		/// ## Details
		///
		/// Starts a Dutch auction to sell excess pUSD from the Insurance Fund for DOT.
		/// Only one surplus auction can be active at a time. The auction price decreases
		/// over time (DOT per pUSD), allowing market participants to purchase at their
		/// desired rate.
		///
		/// Only works when `SurplusMode` is set to `Auction`. For direct transfers,
		/// use [`Pallet::transfer_surplus`] instead.
		///
		/// ## Arguments
		///
		/// - `keeper`: Account to receive keeper incentive at auction completion
		///
		/// ## Errors
		///
		/// - [`Error::AuctionsStopped`]: If circuit breaker is not at `AllEnabled`
		/// - [`Error::SurplusAuctionsDisabled`]: If `SurplusMode` is `DirectTransfer`
		/// - [`Error::InsufficientSurplus`]: If IF balance is below threshold
		/// - [`Error::SurplusAuctionAlreadyActive`]: If a surplus auction is already running
		/// - [`Error::PriceNotAvailable`]: If oracle price is unavailable
		///
		/// ## Events
		///
		/// - [`Event::AuctionStarted`]: Emitted when auction starts
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::start_surplus_auction())]
		pub fn start_surplus_auction(origin: OriginFor<T>, keeper: T::AccountId) -> DispatchResult {
			ensure_signed(origin)?;
			Self::do_start_surplus_auction(keeper)
		}

		/// Update the price buffer parameter for an auction type.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Root`.
		///
		/// ## Details
		///
		/// Sets the buffer multiplier applied to oracle price for the initial auction price.
		/// For example, a buffer of 1.2 means the auction starts at 120% of oracle price.
		///
		/// ## Arguments
		///
		/// - `auction_type`: Which auction type to configure (`Liquidation` or `Surplus`)
		/// - `buffer`: New buffer value (e.g., 1.2 for 20% above oracle)
		///
		/// ## Events
		///
		/// - [`Event::ConfigUpdated`]: Emitted with `Buffer` parameter
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::set_buffer())]
		pub fn set_buffer(
			origin: OriginFor<T>,
			auction_type: AuctionType,
			buffer: FixedU128,
		) -> DispatchResult {
			ensure_root(origin)?;
			AuctionConfig::<T>::mutate(auction_type, |config| config.buffer = buffer);
			Self::deposit_event(Event::ConfigUpdated {
				auction_type,
				parameter: ConfigParameter::Buffer,
			});
			Ok(())
		}

		/// Update the maximum auction duration.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Root`.
		///
		/// ## Details
		///
		/// Sets how long an auction can run before it becomes stale and eligible for restart.
		/// After this duration, the auction must be restarted by a keeper to continue.
		///
		/// ## Arguments
		///
		/// - `auction_type`: Which auction type to configure (`Liquidation` or `Surplus`)
		/// - `maximum_duration`: Maximum duration in blocks before restart is required
		///
		/// ## Events
		///
		/// - [`Event::ConfigUpdated`]: Emitted with `MaximumDuration` parameter
		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::set_maximum_duration())]
		pub fn set_maximum_duration(
			origin: OriginFor<T>,
			auction_type: AuctionType,
			maximum_duration: BlockNumberFor<T>,
		) -> DispatchResult {
			ensure_root(origin)?;
			AuctionConfig::<T>::mutate(auction_type, |config| {
				config.maximum_duration = maximum_duration;
			});
			Self::deposit_event(Event::ConfigUpdated {
				auction_type,
				parameter: ConfigParameter::MaximumDuration,
			});
			Ok(())
		}

		/// Update the minimum price ratio for auction reset.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Root`.
		///
		/// ## Details
		///
		/// Sets the minimum ratio of current price to starting price before an auction
		/// becomes stale. For example, 0.65 means the auction becomes stale when price
		/// drops below 65% of the starting price.
		///
		/// ## Arguments
		///
		/// - `auction_type`: Which auction type to configure (`Liquidation` or `Surplus`)
		/// - `minimum_price`: Minimum price ratio (e.g., 0.65 for 65% floor)
		///
		/// ## Events
		///
		/// - [`Event::ConfigUpdated`]: Emitted with `MinimumPrice` parameter
		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::set_minimum_price())]
		pub fn set_minimum_price(
			origin: OriginFor<T>,
			auction_type: AuctionType,
			minimum_price: FixedU128,
		) -> DispatchResult {
			ensure_root(origin)?;
			AuctionConfig::<T>::mutate(auction_type, |config| config.minimum_price = minimum_price);
			Self::deposit_event(Event::ConfigUpdated {
				auction_type,
				parameter: ConfigParameter::MinimumPrice,
			});
			Ok(())
		}

		/// Update the percentage-based keeper incentive.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Root`.
		///
		/// ## Details
		///
		/// Sets the percentage of the auction's tab (debt) that keepers receive as
		/// incentive when starting or restarting auctions. This is calculated as:
		/// `incentive = tip + (chip × tab)`, capped to the penalty amount.
		///
		/// ## Arguments
		///
		/// - `auction_type`: Which auction type to configure (`Liquidation` or `Surplus`)
		/// - `chip`: Percentage incentive (e.g., 0.1% = Permill::from_parts(1000))
		///
		/// ## Events
		///
		/// - [`Event::ConfigUpdated`]: Emitted with `Chip` parameter
		#[pallet::call_index(13)]
		#[pallet::weight(T::WeightInfo::set_chip())]
		pub fn set_chip(
			origin: OriginFor<T>,
			auction_type: AuctionType,
			chip: Permill,
		) -> DispatchResult {
			ensure_root(origin)?;
			AuctionConfig::<T>::mutate(auction_type, |config| config.chip = chip);
			Self::deposit_event(Event::ConfigUpdated {
				auction_type,
				parameter: ConfigParameter::Chip,
			});
			Ok(())
		}

		/// Update the flat keeper incentive amount.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Root`.
		///
		/// ## Details
		///
		/// Sets a flat fee paid to keepers for starting or restarting auctions.
		/// This is added to the percentage-based incentive (`chip`). The total
		/// incentive is capped to the penalty amount: `incentive = tip + (chip × tab)`.
		///
		/// ## Arguments
		///
		/// - `auction_type`: Which auction type to configure (`Liquidation` or `Surplus`)
		/// - `tip`: Flat incentive amount in pUSD
		///
		/// ## Events
		///
		/// - [`Event::ConfigUpdated`]: Emitted with `Tip` parameter
		#[pallet::call_index(14)]
		#[pallet::weight(T::WeightInfo::set_tip())]
		pub fn set_tip(
			origin: OriginFor<T>,
			auction_type: AuctionType,
			tip: BalanceOf<T>,
		) -> DispatchResult {
			ensure_root(origin)?;
			AuctionConfig::<T>::mutate(auction_type, |config| config.tip = tip);
			Self::deposit_event(Event::ConfigUpdated {
				auction_type,
				parameter: ConfigParameter::Tip,
			});
			Ok(())
		}

		/// Update the price decay curve for auctions.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Root`.
		///
		/// ## Details
		///
		/// Sets the price curve used to calculate auction prices over time.
		/// The curve determines how quickly the auction price decreases from
		/// the starting price (oracle × buffer) toward the minimum price floor.
		///
		/// ## Arguments
		///
		/// - `auction_type`: Which auction type to configure (`Liquidation` or `Surplus`)
		/// - `curve`: New price curve configuration
		///
		/// ## Events
		///
		/// - [`Event::ConfigUpdated`]: Emitted with `Curve` parameter
		#[pallet::call_index(16)]
		#[pallet::weight(T::WeightInfo::set_curve())]
		pub fn set_curve(
			origin: OriginFor<T>,
			auction_type: AuctionType,
			curve: PriceCurve,
		) -> DispatchResult {
			ensure_root(origin)?;
			AuctionConfig::<T>::mutate(auction_type, |config| config.curve = curve);
			Self::deposit_event(Event::ConfigUpdated {
				auction_type,
				parameter: ConfigParameter::Curve,
			});
			Ok(())
		}

		/// Set the circuit breaker level for emergency control.
		///
		/// ## Dispatch Origin
		///
		/// Must be `ManagerOrigin`.
		///
		/// ## Details
		///
		/// Sets the circuit breaker level for gradual system shutdown in emergencies.
		/// Unlike governance changes, this can be called quickly by authorized
		/// emergency responders without waiting for full governance processes.
		///
		/// ## Levels
		///
		/// - [`CircuitBreakerLevel::AllEnabled`]: Normal operation
		/// - [`CircuitBreakerLevel::NoNewAuctions`]: New auctions blocked
		/// - [`CircuitBreakerLevel::NoNewAuctionsOrRestarts`]: New auctions and restarts blocked
		/// - [`CircuitBreakerLevel::AllDisabled`]: All operations blocked (emergency stop)
		///
		/// ## Arguments
		///
		/// - `level`: New circuit breaker level
		///
		/// ## Events
		///
		/// - [`Event::StoppedUpdated`]: Emitted with the new level
		#[pallet::call_index(15)]
		#[pallet::weight(T::WeightInfo::set_stopped())]
		pub fn set_stopped(origin: OriginFor<T>, level: CircuitBreakerLevel) -> DispatchResult {
			T::ManagerOrigin::ensure_origin(origin)?;
			Stopped::<T>::put(level);
			Self::deposit_event(Event::StoppedUpdated { level });
			Ok(())
		}

		/// Set surplus handling mode.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Root` (governance decision).
		///
		/// ## Details
		///
		/// Controls how surplus pUSD is distributed:
		/// - `Auction`: Surplus is auctioned for DOT (`start_surplus_auction()` enabled)
		/// - `DirectTransfer`: Surplus is transferred directly (`transfer_surplus()` enabled)
		///
		/// ## Events
		///
		/// - [`Event::SurplusModeUpdated`]: Emitted with the new mode.
		#[pallet::call_index(17)]
		#[pallet::weight(T::WeightInfo::set_surplus_mode())]
		pub fn set_surplus_mode(origin: OriginFor<T>, mode: SurplusHandlingMode) -> DispatchResult {
			ensure_root(origin)?;
			SurplusMode::<T>::put(mode);
			Self::deposit_event(Event::SurplusModeUpdated { mode });
			Ok(())
		}

		/// Transfer surplus pUSD directly to treasury.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed`. Anyone can trigger (permissionless keeper).
		///
		/// ## Details
		///
		/// Only works when `SurplusMode` is `DirectTransfer`. Transfers
		/// `SurplusAuctionAmount` pUSD from the Insurance Fund to the treasury
		/// via the configured `SurplusHandler`.
		///
		/// ## Prerequisites
		///
		/// - `SurplusMode` must be `DirectTransfer`
		/// - Circuit breaker must allow new auctions (level = `AllEnabled`)
		/// - Insurance Fund balance must exceed threshold after transfer
		///
		/// ## Events
		///
		/// - [`Event::SurplusTransferred`]: Emitted with the amount transferred.
		///
		/// ## Errors
		///
		/// - [`Error::DirectTransferDisabled`]: If mode is `Auction`
		/// - [`Error::AuctionsStopped`]: If circuit breaker is active
		/// - [`Error::InsufficientSurplus`]: If IF balance is below threshold
		#[pallet::call_index(18)]
		#[pallet::weight(T::WeightInfo::transfer_surplus())]
		pub fn transfer_surplus(origin: OriginFor<T>) -> DispatchResult {
			ensure_signed(origin)?;
			Self::do_transfer_surplus()
		}
	}

	// Implement AuctionsHandler trait for vaults pallet to call
	impl<T: Config> AuctionsHandler<T::AccountId, BalanceOf<T>> for Pallet<T> {
		fn start_auction(
			vault_owner: T::AccountId,
			collateral_amount: BalanceOf<T>,
			debt: DebtComponents<BalanceOf<T>>,
			keeper: T::AccountId,
		) -> Result<u32, DispatchError> {
			// 1. Check circuit breaker allows new auctions (only AllEnabled permits new auctions)
			ensure!(
				Stopped::<T>::get() == CircuitBreakerLevel::AllEnabled,
				Error::<T>::AuctionsStopped
			);

			// 2. Get current oracle price via CollateralManager
			let price =
				T::CollateralManager::get_dot_price().ok_or(Error::<T>::PriceNotAvailable)?;
			ensure!(!price.is_zero(), Error::<T>::PriceNotAvailable);

			// 3. Calculate initial price (oracle * buffer)
			let config = AuctionConfig::<T>::get(AuctionType::Liquidation);
			let starting_price = price.saturating_mul(config.buffer);

			// 4. Compute keeper incentive: tip + (chip × base_tab), capped to penalty since
			// keeper incentives are funded from the penalty pool.
			let base_tab = debt.total();
			let chip_incentive = config.chip.mul_floor(base_tab);
			let keeper_incentive_raw = config.tip.saturating_add(chip_incentive);
			let keeper_incentive = keeper_incentive_raw.min(debt.penalty);

			// 5. Create Tab with structured debt components
			let tab = Tab::new(debt.principal, debt.interest, debt.penalty);

			// 6. Create auction record
			let id = NextAuctionId::<T>::mutate(|n| {
				let id = *n;
				*n = n.saturating_add(1);
				id
			});
			let now = frame_system::Pallet::<T>::block_number();

			let auction = Auction {
				auction_type: AuctionType::Liquidation,
				tab,
				auctionable_collateral: collateral_amount,
				vault_owner: Some(vault_owner.clone()),
				starting_block: now,
				starting_price,
				keeper: keeper.clone(),
				keeper_incentive,
				penalty_collected: Zero::zero(),
			};

			// 7. Store auction
			Auctions::<T>::insert(id, auction);

			// 8. Note: Collateral is already held with HoldReason::Seized by vaults pallet
			// The auctions pallet operates on that hold

			Self::deposit_event(Event::AuctionStarted {
				auction_type: AuctionType::Liquidation,
				id,
				tab: base_tab,
				lot: collateral_amount,
				owner: Some(vault_owner),
				starting_block: now,
				starting_price,
				keeper,
			});

			Ok(id)
		}
	}

	// Internal helper functions
	impl<T: Config> Pallet<T> {
		/// Minimum weight required for `on_idle` to do useful work.
		///
		/// Includes:
		/// - Reading the cursor
		/// - Reading Stopped
		/// - Reading `AuctionConfig`
		/// - Writing cursor update
		pub(crate) fn on_idle_weight() -> Weight {
			T::DbWeight::get().reads_writes(3, 1)
		}

		/// Get current auction price using configured price curve.
		pub fn current_price(auction: &Auction<T>) -> FixedU128 {
			let config = AuctionConfig::<T>::get(auction.auction_type);
			let now = frame_system::Pallet::<T>::block_number();
			let elapsed: u64 = now.saturating_sub(auction.starting_block).saturated_into();
			config.curve.calculate_price(auction.starting_price, config.buffer, elapsed)
		}

		/// Check if auction needs restart (stale).
		pub(crate) fn needs_restart(auction: &Auction<T>) -> bool {
			let config = AuctionConfig::<T>::get(auction.auction_type);
			let now = frame_system::Pallet::<T>::block_number();
			let elapsed = now.saturating_sub(auction.starting_block);

			// Too much time elapsed
			if elapsed >= config.maximum_duration {
				return true;
			}

			// Price fallen too low relative to initial
			let current = Self::current_price(auction);
			if let Some(ratio) = current.checked_div(&auction.starting_price) {
				return ratio <= config.minimum_price;
			}

			// If division fails (starting_price is zero), definitely needs restart
			true
		}

		/// Execute take logic for liquidation auctions.
		///
		/// Buyer pays pUSD, receives DOT collateral.
		fn do_take_liquidation(
			buyer: &T::AccountId,
			id: u32,
			mut auction: Auction<T>,
			amt: BalanceOf<T>,
			max: FixedU128,
			recipient: T::AccountId,
		) -> DispatchResult {
			// 1. Get current price (pUSD per DOT) and validate against max
			let price = Self::current_price(&auction);
			ensure!(max >= price, Error::<T>::PriceTooHigh);

			// Get current tab total (principal + accrued interest + penalty)
			let tab_total = auction.tab.total();

			// 2. Calculate slice (amount of collateral) and owe (pUSD required)
			// Round UP owe using ceil() to minimize bad debt (buyer pays at least fair value)
			let mut slice = amt.min(auction.auctionable_collateral);

			let mut owe = price
				.saturating_mul(FixedU128::saturating_from_integer(slice))
				.ceil()
				.saturating_mul_int(One::one());

			// 3. If owe > tab total, cap it and adjust slice.
			// This occurs when collateral value exceeds debt (e.g., 100 DOT worth 505 pUSD, debt 50
			// pUSD).
			if owe > tab_total {
				owe = tab_total;
				// slice = tab / price (round DOWN - buyer pays exact tab, gets slightly less
				// collateral)
				if !price.is_zero() {
					let tab_fixed = FixedU128::saturating_from_integer(tab_total);
					if let Some(slice_fixed) = tab_fixed.checked_div(&price) {
						slice = slice_fixed.saturating_mul_int(One::one());
					}
				}
			}

			// 4. Minimum purchase check on final slice.
			// Reject if slice < MinPurchaseAmount unless taking entire remaining collateral.
			let min_purchase = T::MinPurchaseAmount::get();
			if slice < min_purchase && slice < auction.auctionable_collateral {
				return Err(Error::<T>::PurchaseTooSmall.into());
			}

			// 5. Dust prevention: all-or-nothing remainder.
			// If this purchase would leave a dusty remaining_tab, require taking all remaining
			// collateral.
			let remaining_tab = auction.tab.remaining_after_payment(owe);
			let remaining_collateral = auction.auctionable_collateral.saturating_sub(slice);
			let min_tab = T::MinAuctionTab::get();

			let would_leave_dust = !remaining_tab.is_zero() &&
				remaining_tab < min_tab &&
				!remaining_collateral.is_zero();

			if would_leave_dust {
				return Err(Error::<T>::DustyAuction.into());
			}

			// 6. Apply payment to Tab and get distribution (burn, insurance_fund)
			let payment = auction.tab.apply_payment(owe);

			// Track penalty collected (penalty_paid is the penalty portion)
			auction.penalty_collected =
				auction.penalty_collected.saturating_add(payment.insurance_fund());

			// 7. Execute purchase via CollateralManager
			// This burns principal+interest pUSD, transfers penalty to IF,
			// and transfers collateral to recipient. Keeper is paid at completion.
			let vault_owner = auction
				.vault_owner
				.clone()
				.expect("liquidation auctions always have vault_owner");

			T::CollateralManager::execute_purchase(
				buyer,
				slice,
				payment,
				&recipient,
				&vault_owner,
			)?;

			// 8. Emit take event
			Self::deposit_event(Event::Take {
				auction_type: AuctionType::Liquidation,
				id,
				max,
				price,
				payment: owe,
				received: slice,
				recipient,
			});

			// 9. Check if auction is complete
			if remaining_collateral.is_zero() || remaining_tab.is_zero() {
				// Complete auction via CollateralManager.
				// Shortfall = remaining principal + remaining interest (both are bad debt).
				// Principal: pUSD was minted against it; unpaid principal = peg risk.
				// Interest: was already minted into IF on accrual; unpaid = excess pUSD in
				// circulation.
				// Penalty: NOT included — no pUSD was minted against it, so not bad debt.
				let shortfall = auction.tab.principal.saturating_add(auction.tab.accrued_interest);

				// Cap keeper incentive to actual penalty collected (avoid overpaying if shortfall)
				let actual_keeper_incentive =
					auction.keeper_incentive.min(auction.penalty_collected);

				T::CollateralManager::complete_auction(
					&vault_owner,
					remaining_collateral,
					shortfall,
					&auction.keeper,
					actual_keeper_incentive,
				)?;

				Self::deposit_event(Event::AuctionCompleted {
					auction_type: AuctionType::Liquidation,
					id,
					remaining: remaining_collateral,
					shortfall,
				});

				// Remove auction from storage
				Auctions::<T>::remove(id);
			} else {
				// Update auction with reduced tab/auctionable_collateral
				auction.auctionable_collateral = remaining_collateral;
				Auctions::<T>::insert(id, auction);
			}

			Ok(())
		}

		/// Execute take logic for surplus auctions.
		///
		/// Buyer pays DOT, receives pUSD from Insurance Fund.
		fn do_take_surplus(
			buyer: &T::AccountId,
			id: u32,
			mut auction: Auction<T>,
			amt: BalanceOf<T>,
			max: FixedU128,
			recipient: T::AccountId,
		) -> DispatchResult {
			// 1. Get current price (DOT per pUSD) and validate against max
			let price = Self::current_price(&auction);
			ensure!(max >= price, Error::<T>::PriceTooHigh);

			// For surplus auctions, tab.principal tracks the remaining pUSD to sell
			let remaining_pusd = auction.tab.principal;

			// 2. Calculate pusd_amount (pUSD to transfer) and dot_amount (DOT buyer pays)
			// Cap to remaining if buyer wants more than available
			let pusd_amount = amt.min(remaining_pusd);

			// dot_amount = pusd_amount × price (DOT per pUSD)
			// Round UP to favor the protocol (buyer pays slightly more DOT)
			let dot_amount: BalanceOf<T> = price
				.saturating_mul(FixedU128::saturating_from_integer(pusd_amount))
				.ceil()
				.saturating_mul_int(One::one());

			// 3. Minimum purchase check
			let min_purchase = T::MinSurplusPurchaseAmount::get();
			if pusd_amount < min_purchase && pusd_amount < remaining_pusd {
				return Err(Error::<T>::PurchaseTooSmall.into());
			}

			// 4. Dust prevention: if remaining would be below threshold, take all
			let remaining_after = remaining_pusd.saturating_sub(pusd_amount);
			let min_tab = T::MinAuctionTab::get();

			let would_leave_dust = !remaining_after.is_zero() &&
				remaining_after < min_tab &&
				pusd_amount < remaining_pusd;

			if would_leave_dust {
				return Err(Error::<T>::DustyAuction.into());
			}

			// 5. Update the auction tab (reduce principal by pusd_amount sold)
			auction.tab.principal = remaining_after;

			// 6. Execute surplus purchase via CollateralManager
			// This transfers pUSD from IF to recipient, and DOT from buyer to Treasury
			T::CollateralManager::execute_surplus_purchase(
				buyer,
				&recipient,
				pusd_amount,
				dot_amount,
			)?;

			Self::deposit_event(Event::Take {
				auction_type: AuctionType::Surplus,
				id,
				max,
				price,
				payment: dot_amount,
				received: pusd_amount,
				recipient,
			});

			// 7. Check if auction is complete (all pUSD sold)
			if remaining_after.is_zero() {
				Self::deposit_event(Event::AuctionCompleted {
					auction_type: AuctionType::Surplus,
					id,
					remaining: remaining_after,
					shortfall: Zero::zero(),
				});

				// Remove auction from storage and clear active surplus auction tracking
				Auctions::<T>::remove(id);
				ActiveSurplusAuctionId::<T>::kill();
			} else {
				// Update auction with reduced pUSD amount
				Auctions::<T>::insert(id, auction);
			}

			Ok(())
		}

		/// Execute the restart auction logic for liquidation auctions only.
		fn do_restart_auction(id: u32, keeper: T::AccountId) -> DispatchResult {
			// 1. Check circuit breaker allows restart (blocked at NoNewAuctionsOrRestarts and
			//    AllDisabled)
			ensure!(
				Stopped::<T>::get() < CircuitBreakerLevel::NoNewAuctionsOrRestarts,
				Error::<T>::RestartStopped
			);

			// 2. Load auction
			let mut auction = Auctions::<T>::get(id).ok_or(Error::<T>::AuctionNotFound)?;

			// 3. Block restart for surplus auctions - they simply end when stale
			ensure!(
				auction.auction_type == AuctionType::Liquidation,
				Error::<T>::InvalidAuctionType
			);

			// 4. Verify auction needs restart
			ensure!(Self::needs_restart(&auction), Error::<T>::DoesNotNeedRestart);

			// 5. Get current oracle price via CollateralManager
			let dot_price_pusd =
				T::CollateralManager::get_dot_price().ok_or(Error::<T>::PriceNotAvailable)?;
			ensure!(!dot_price_pusd.is_zero(), Error::<T>::PriceNotAvailable);

			// 6. Calculate new starting_price for liquidation auction
			let config = AuctionConfig::<T>::get(AuctionType::Liquidation);
			let new_starting_price = dot_price_pusd.saturating_mul(config.buffer);

			// 7. Get base tab for event (keeper incentive is NOT recalculated on restart)
			let base_tab = auction.tab.total();

			// 8. Update auction state (keeper incentive remains fixed from auction start)
			let now = frame_system::Pallet::<T>::block_number();
			auction.starting_block = now;
			auction.starting_price = new_starting_price;
			// Note: keeper_incentive is NOT updated - it's fixed at auction start
			auction.keeper = keeper.clone();

			Self::deposit_event(Event::AuctionRestarted {
				auction_type: AuctionType::Liquidation,
				id,
				starting_price: new_starting_price,
				tab: base_tab,
				lot: auction.auctionable_collateral,
				owner: auction.vault_owner.clone(),
				keeper,
				incentive: auction.keeper_incentive,
			});

			Auctions::<T>::insert(id, auction);

			Ok(())
		}

		/// Start a surplus auction to sell pUSD from Insurance Fund for DOT.
		///
		/// # Errors
		///
		/// - [`Error::SurplusAuctionsDisabled`] - Surplus mode is not set to `Auction`
		/// - [`Error::AuctionsStopped`] - Circuit breaker is not `AllEnabled`
		/// - [`Error::SurplusAuctionAlreadyActive`] - A surplus auction is already in progress
		/// - [`Error::InsufficientSurplus`] - Insurance Fund balance is insufficient
		/// - [`Error::PriceNotAvailable`] - DOT price is unavailable or zero
		pub(crate) fn do_start_surplus_auction(keeper: T::AccountId) -> DispatchResult {
			// 1. Check surplus mode allows auctions
			ensure!(
				SurplusMode::<T>::get() == SurplusHandlingMode::Auction,
				Error::<T>::SurplusAuctionsDisabled
			);

			// 2. Check circuit breaker allows new auctions (only AllEnabled permits new auctions)
			ensure!(
				Stopped::<T>::get() == CircuitBreakerLevel::AllEnabled,
				Error::<T>::AuctionsStopped
			);

			// 3. Ensure no surplus auction is currently active
			ensure!(
				ActiveSurplusAuctionId::<T>::get().is_none(),
				Error::<T>::SurplusAuctionAlreadyActive
			);

			// 4. Check IF balance exceeds surplus threshold after removing auction amount
			let if_balance = T::CollateralManager::get_insurance_fund_balance();
			let total_supply = T::CollateralManager::get_total_pusd_supply();
			let threshold = T::SurplusAuctionThreshold::get();
			let auction_amount = T::SurplusAuctionAmount::get();

			// After removing auction_amount, IF must still have at least threshold × total_supply
			let required_surplus = threshold.mul_floor(total_supply);
			ensure!(
				if_balance >= required_surplus.saturating_add(auction_amount),
				Error::<T>::InsufficientSurplus
			);

			// 5. Get current DOT price (pUSD per DOT) and calculate inverse (DOT per pUSD)
			let dot_price_pusd =
				T::CollateralManager::get_dot_price().ok_or(Error::<T>::PriceNotAvailable)?;
			ensure!(!dot_price_pusd.is_zero(), Error::<T>::PriceNotAvailable);

			// Inverse price: DOT per pUSD = 1 / (pUSD per DOT)
			// For surplus auctions, price decreases means buyers pay less DOT per pUSD.
			let inverse_price = FixedU128::one()
				.checked_div(&dot_price_pusd)
				.ok_or(Error::<T>::PriceNotAvailable)?;

			// Apply buffer: starting_price = inverse_price * buffer
			// (start with buyer paying more DOT per pUSD, price decreases to favor buyer over time)
			let config = AuctionConfig::<T>::get(AuctionType::Surplus);
			let starting_price = inverse_price.saturating_mul(config.buffer);

			// 6. Determine auction amount
			let pusd_amount = T::SurplusAuctionAmount::get();

			// 7. Create Tab for surplus auction
			// - principal: pUSD amount being sold (not burned, just tracked)
			// - accrued_interest: Zero (no debt)
			// - penalty: Zero (no penalty)
			let tab = Tab::new(pusd_amount, Zero::zero(), Zero::zero());

			// 8. Surplus keeper incentive is tip only (no chip).
			// Surplus auctions are not time-sensitive, so no percentage-based incentive.
			let keeper_incentive = config.tip;

			// 9. Create auction record
			let id = NextAuctionId::<T>::mutate(|n| {
				let id = *n;
				*n = n.saturating_add(1);
				id
			});
			let now = frame_system::Pallet::<T>::block_number();

			let auction = Auction {
				auction_type: AuctionType::Surplus,
				tab,
				auctionable_collateral: Zero::zero(),
				vault_owner: None,
				starting_block: now,
				starting_price,
				keeper: keeper.clone(),
				keeper_incentive,
				penalty_collected: Zero::zero(),
			};

			// 10. Store auction and mark as active surplus auction
			Auctions::<T>::insert(id, auction);
			ActiveSurplusAuctionId::<T>::put(id);

			// 11. Emit event
			Self::deposit_event(Event::AuctionStarted {
				auction_type: AuctionType::Surplus,
				id,
				tab: pusd_amount,
				lot: Zero::zero(),
				owner: None,
				starting_block: now,
				starting_price,
				keeper,
			});

			Ok(())
		}

		/// Transfer surplus pUSD directly to treasury via the `CollateralManager`.
		///
		/// Only available in `DirectTransfer` mode. Checks the same threshold conditions
		/// as surplus auctions.
		fn do_transfer_surplus() -> DispatchResult {
			// 1. Check surplus mode allows direct transfer
			ensure!(
				SurplusMode::<T>::get() == SurplusHandlingMode::DirectTransfer,
				Error::<T>::DirectTransferDisabled
			);

			// 2. Check circuit breaker (same as auctions - blocked at NoNewAuctions and above)
			ensure!(
				Stopped::<T>::get() == CircuitBreakerLevel::AllEnabled,
				Error::<T>::AuctionsStopped
			);

			// 3. Check IF balance exceeds surplus threshold after transfer
			let if_balance = T::CollateralManager::get_insurance_fund_balance();
			let total_supply = T::CollateralManager::get_total_pusd_supply();
			let threshold = T::SurplusAuctionThreshold::get();
			let transfer_amount = T::SurplusAuctionAmount::get();

			// After removing transfer_amount, IF must still have at least threshold × total_supply
			let required_surplus = threshold.mul_floor(total_supply);
			ensure!(
				if_balance >= required_surplus.saturating_add(transfer_amount),
				Error::<T>::InsufficientSurplus
			);

			// 4. Execute transfer via CollateralManager
			T::CollateralManager::transfer_surplus(transfer_amount)?;

			// 5. Emit event
			Self::deposit_event(Event::SurplusTransferred { amount: transfer_amount });

			Ok(())
		}
	}
}
