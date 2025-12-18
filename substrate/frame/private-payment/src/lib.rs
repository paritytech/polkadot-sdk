// This file is part of Substrate.

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

//! # Private Payment Pallet
//!
//! A pallet for private payments using an anonymous coin-like system.
//!
//! This is a dummy implementation that provides the dispatchable interfaces
//! but uses placeholder types for ZK/ring-vrf cryptography.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::*;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::*,
	traits::tokens::fungibles::{Inspect, Mutate},
	PalletId,
};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::traits::{AccountIdConversion, AtLeast32BitUnsigned, CheckedAdd, CheckedSub};

/// Coin value exponent type (non-negative).
/// Value = BASE * 2^exponent where BASE = $0.01
pub type CoinExponent = u8;

/// Public key placeholder (32 bytes).
pub type PublicKey = [u8; 32];

/// Member key placeholder for ring-vrf.
pub type MemberKey = [u8; 32];

/// Alias/voucher placeholder.
pub type Alias = [u8; 32];

/// ZK proof placeholder.
pub type Proof = BoundedVec<u8, ConstU32<256>>;

/// A coin in the private payment system.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct Coin {
	/// Value exponent: actual value = BASE * 2^value_exponent
	pub value_exponent: CoinExponent,
	/// Number of times this coin has been transferred.
	pub age: u16,
}

/// Token used to claim from the recycler.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum RecyclerClaimToken {
	/// Free token distributed to verified persons.
	FreelyDistributedToPerson { ring_index: u32, counter: u32, period: u32, proof: Proof },
	/// Free token distributed to lite persons.
	FreelyDistributedToLitePerson { ring_index: u32, counter: u32, period: u32, proof: Proof },
	/// Paid token purchased with DOT/stable/coin.
	Paid { ring_index: u32, proof: Proof },
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Minimum age before a coin can be recycled.
		#[pallet::constant]
		type MinimumAgeForRecycling: Get<u16>;

		/// Maximum age - coins at or above this age cannot be transferred.
		#[pallet::constant]
		type MaximumAge: Get<u16>;

		/// Maximum coin exponent (e.g., 14 for max $163.84 with $0.01 base).
		#[pallet::constant]
		type MaxCoinExponent: Get<CoinExponent>;

		/// The fungible assets implementation for the backing stablecoin.
		type Assets: Inspect<Self::AccountId, AssetId = Self::AssetId, Balance = Self::Balance>
			+ Mutate<Self::AccountId>;

		/// The asset ID type.
		type AssetId: Parameter + Member + Copy + MaxEncodedLen;

		/// The balance type for assets.
		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Copy
			+ MaxEncodedLen
			+ Default
			+ CheckedAdd
			+ CheckedSub
			+ From<u128>;

		/// The asset ID of the backing stablecoin.
		#[pallet::constant]
		type BackingAssetId: Get<Self::AssetId>;

		/// Base value in asset units (representing $0.01).
		#[pallet::constant]
		type BaseValue: Get<Self::Balance>;

		/// The pallet's ID for holding assets.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Weight information for extrinsics.
		type WeightInfo: WeightInfo;
	}

	/// Coins owned by public keys.
	#[pallet::storage]
	pub type CoinsByOwner<T: Config> = StorageMap<_, Blake2_128Concat, PublicKey, Coin>;

	/// Recycler rings by coin exponent.
	/// Maps exponent -> list of member keys waiting in recycler.
	#[pallet::storage]
	pub type RecyclerRings<T: Config> =
		StorageMap<_, Twox64Concat, CoinExponent, BoundedVec<MemberKey, ConstU32<10000>>>;

	/// Recycler ring index tracker.
	#[pallet::storage]
	pub type RecyclerRingIndex<T: Config> = StorageMap<_, Twox64Concat, CoinExponent, u32>;

	/// Vouchers available in recycler (alias -> (exponent, ring_index)).
	#[pallet::storage]
	pub type RecyclerVouchers<T: Config> =
		StorageMap<_, Blake2_128Concat, Alias, (CoinExponent, u32)>;

	/// Paid token ring for claim tokens purchased with fees.
	#[pallet::storage]
	pub type PaidTokenRing<T: Config> =
		StorageValue<_, BoundedVec<MemberKey, ConstU32<10000>>, ValueQuery>;

	/// Paid token ring index tracker.
	#[pallet::storage]
	pub type PaidTokenRingIndex<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Consumed claim tokens to prevent double-spending.
	#[pallet::storage]
	pub type ConsumedClaimTokens<T: Config> = StorageMap<_, Blake2_128Concat, Alias, ()>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new coin was minted.
		CoinMinted { owner: PublicKey, value_exponent: CoinExponent },
		/// A coin was split into smaller denominations.
		CoinSplit { from: PublicKey, into: Vec<(PublicKey, CoinExponent)> },
		/// A coin was transferred to a new owner.
		CoinTransferred { from: PublicKey, to: PublicKey, value_exponent: CoinExponent },
		/// A coin was loaded into the recycler.
		RecyclerLoadedWithCoin { value_exponent: CoinExponent, member_key: MemberKey },
		/// External asset was loaded into the recycler.
		RecyclerLoadedWithAsset {
			who: T::AccountId,
			value_exponent: CoinExponent,
			member_key: MemberKey,
		},
		/// Coins were unloaded from the recycler.
		RecyclerUnloadedIntoCoin {
			value_exponent: CoinExponent,
			voucher_count: u32,
			dest: PublicKey,
		},
		/// External asset was unloaded from the recycler.
		RecyclerUnloadedIntoAsset {
			value_exponent: CoinExponent,
			voucher_count: u32,
			dest: T::AccountId,
		},
		/// A recycler claim token was purchased.
		ClaimTokenPurchased { member_key: MemberKey },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The specified coin was not found.
		CoinNotFound,
		/// The coin is too young to be recycled.
		CoinTooYoungToRecycle,
		/// The coin is too old to be transferred.
		CoinTooOldToTransfer,
		/// The split amounts don't sum to the original coin value.
		InvalidSplitAmount,
		/// The coin denomination (exponent) is invalid.
		InvalidCoinDenomination,
		/// The voucher is invalid or doesn't exist.
		InvalidVoucher,
		/// The claim token has already been used.
		ClaimTokenAlreadyUsed,
		/// Insufficient balance to perform the operation.
		InsufficientBalance,
		/// Arithmetic overflow occurred.
		ArithmeticOverflow,
		/// The recycler ring is full.
		RecyclerRingFull,
		/// The paid token ring is full.
		PaidTokenRingFull,
		/// Invalid claim token proof.
		InvalidClaimToken,
		/// Split would produce too many coins.
		TooManyOutputCoins,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Split a coin into multiple smaller coins.
		///
		/// The sum of output coin values must equal the input coin value.
		/// All output coins will have age = input_age + 1.
		///
		/// - `coin`: The public key of the coin to split.
		/// - `split_into`: Vector of (exponent, destinations) pairs.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::split(split_into.len() as u32))]
		pub fn split(
			origin: OriginFor<T>,
			coin: PublicKey,
			split_into: Vec<(CoinExponent, Vec<PublicKey>)>,
		) -> DispatchResult {
			ensure_signed(origin)?;

			let original = CoinsByOwner::<T>::get(coin).ok_or(Error::<T>::CoinNotFound)?;

			// Calculate original value: BASE * 2^exponent
			let original_value = Self::coin_value(original.value_exponent)?;

			// Calculate sum of split values
			let mut total_split_value: u128 = 0;
			let mut outputs: Vec<(PublicKey, CoinExponent)> = Vec::new();

			for (exponent, destinations) in &split_into {
				ensure!(
					*exponent <= T::MaxCoinExponent::get(),
					Error::<T>::InvalidCoinDenomination
				);
				let value = Self::coin_value(*exponent)?;
				for dest in destinations {
					total_split_value = total_split_value
						.checked_add(value)
						.ok_or(Error::<T>::ArithmeticOverflow)?;
					outputs.push((*dest, *exponent));
				}
			}

			ensure!(outputs.len() <= 100, Error::<T>::TooManyOutputCoins);
			ensure!(total_split_value == original_value, Error::<T>::InvalidSplitAmount);

			// Remove original coin
			CoinsByOwner::<T>::remove(coin);

			// Create new coins with incremented age
			let new_age = original.age.saturating_add(1);
			for (dest, exponent) in &outputs {
				CoinsByOwner::<T>::insert(dest, Coin { value_exponent: *exponent, age: new_age });
			}

			Self::deposit_event(Event::CoinSplit { from: coin, into: outputs });
			Ok(())
		}

		/// Transfer a coin to a new owner.
		///
		/// The coin's age will be incremented by 1.
		/// Fails if the coin's age >= MaximumAge.
		///
		/// - `coin`: The public key of the coin to transfer.
		/// - `to`: The new owner's public key.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::transfer())]
		pub fn transfer(origin: OriginFor<T>, coin: PublicKey, to: PublicKey) -> DispatchResult {
			ensure_signed(origin)?;

			let mut coin_data = CoinsByOwner::<T>::get(coin).ok_or(Error::<T>::CoinNotFound)?;

			ensure!(coin_data.age < T::MaximumAge::get(), Error::<T>::CoinTooOldToTransfer);

			// Remove from old owner
			CoinsByOwner::<T>::remove(coin);

			// Increment age and insert for new owner
			coin_data.age = coin_data.age.saturating_add(1);
			CoinsByOwner::<T>::insert(to, coin_data.clone());

			Self::deposit_event(Event::CoinTransferred {
				from: coin,
				to,
				value_exponent: coin_data.value_exponent,
			});
			Ok(())
		}

		/// Load a coin into the recycler.
		///
		/// The coin is burned and a voucher is created for the member_key.
		/// The coin must have age >= MinimumAgeForRecycling.
		///
		/// - `coin`: The public key of the coin to recycle.
		/// - `member_key`: The ring-vrf member key to receive the voucher.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::load_recycler_with_coin())]
		pub fn load_recycler_with_coin(
			origin: OriginFor<T>,
			coin: PublicKey,
			member_key: MemberKey,
		) -> DispatchResult {
			ensure_signed(origin)?;

			let coin_data = CoinsByOwner::<T>::get(coin).ok_or(Error::<T>::CoinNotFound)?;

			ensure!(
				coin_data.age >= T::MinimumAgeForRecycling::get(),
				Error::<T>::CoinTooYoungToRecycle
			);

			// Remove coin
			CoinsByOwner::<T>::remove(coin);

			// Add member key to recycler ring
			let exponent = coin_data.value_exponent;
			RecyclerRings::<T>::try_mutate(exponent, |ring| -> DispatchResult {
				let ring = ring.get_or_insert_with(|| BoundedVec::default());
				ring.try_push(member_key).map_err(|_| Error::<T>::RecyclerRingFull)?;
				Ok(())
			})?;

			// Create voucher (dummy: use member_key as alias)
			let ring_index = RecyclerRingIndex::<T>::get(exponent).unwrap_or(0);
			RecyclerVouchers::<T>::insert(member_key, (exponent, ring_index));
			RecyclerRingIndex::<T>::insert(exponent, ring_index.saturating_add(1));

			Self::deposit_event(Event::RecyclerLoadedWithCoin {
				value_exponent: exponent,
				member_key,
			});
			Ok(())
		}

		/// Load external stablecoin into the recycler.
		///
		/// Transfers the asset from the caller to the pallet account.
		///
		/// - `recycler_value`: The coin exponent for the recycler to use.
		/// - `member_key`: The ring-vrf member key to receive the voucher.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::load_recycler_with_external_asset())]
		pub fn load_recycler_with_external_asset(
			origin: OriginFor<T>,
			recycler_value: CoinExponent,
			member_key: MemberKey,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(
				recycler_value <= T::MaxCoinExponent::get(),
				Error::<T>::InvalidCoinDenomination
			);

			// Calculate asset amount
			let amount = Self::coin_balance(recycler_value)?;

			// Transfer asset to pallet account
			let pallet_account = Self::account_id();
			T::Assets::transfer(
				T::BackingAssetId::get(),
				&who,
				&pallet_account,
				amount,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			// Add member key to recycler ring
			RecyclerRings::<T>::try_mutate(recycler_value, |ring| -> DispatchResult {
				let ring = ring.get_or_insert_with(|| BoundedVec::default());
				ring.try_push(member_key).map_err(|_| Error::<T>::RecyclerRingFull)?;
				Ok(())
			})?;

			// Create voucher
			let ring_index = RecyclerRingIndex::<T>::get(recycler_value).unwrap_or(0);
			RecyclerVouchers::<T>::insert(member_key, (recycler_value, ring_index));
			RecyclerRingIndex::<T>::insert(recycler_value, ring_index.saturating_add(1));

			Self::deposit_event(Event::RecyclerLoadedWithAsset {
				who,
				value_exponent: recycler_value,
				member_key,
			});
			Ok(())
		}

		/// Unload vouchers from the recycler into a new coin.
		///
		/// - `claim_token`: The token authorizing the claim.
		/// - `vouchers`: The voucher aliases to consume.
		/// - `recycler_value`: The coin exponent of the recycler.
		/// - `_recycler_index`: The recycler ring index (unused in dummy).
		/// - `dest`: The destination public key for the new coin.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::unload_recycler_into_coin(vouchers.len() as u32))]
		pub fn unload_recycler_into_coin(
			origin: OriginFor<T>,
			claim_token: RecyclerClaimToken,
			vouchers: Vec<Alias>,
			recycler_value: CoinExponent,
			_recycler_index: u32,
			dest: PublicKey,
		) -> DispatchResult {
			ensure_none(origin)?;

			// Verify claim token (dummy: just check it's not consumed)
			let token_alias = Self::claim_token_alias(&claim_token);
			ensure!(
				!ConsumedClaimTokens::<T>::contains_key(token_alias),
				Error::<T>::ClaimTokenAlreadyUsed
			);
			ConsumedClaimTokens::<T>::insert(token_alias, ());

			// Verify and consume vouchers
			for voucher in &vouchers {
				let (exp, _) =
					RecyclerVouchers::<T>::get(voucher).ok_or(Error::<T>::InvalidVoucher)?;
				ensure!(exp == recycler_value, Error::<T>::InvalidVoucher);
				RecyclerVouchers::<T>::remove(voucher);
			}

			// Calculate new coin exponent
			let voucher_count = vouchers.len() as u32;
			let new_exponent = Self::calculate_combined_exponent(recycler_value, voucher_count)?;

			// Mint new coin with age 0
			CoinsByOwner::<T>::insert(dest, Coin { value_exponent: new_exponent, age: 0 });

			Self::deposit_event(Event::RecyclerUnloadedIntoCoin {
				value_exponent: new_exponent,
				voucher_count,
				dest,
			});
			Ok(())
		}

		/// Unload vouchers from the recycler into external stablecoin.
		///
		/// - `claim_token`: The token authorizing the claim.
		/// - `vouchers`: The voucher aliases to consume.
		/// - `recycler_value`: The coin exponent of the recycler.
		/// - `_recycler_index`: The recycler ring index (unused in dummy).
		/// - `dest`: The destination account for the stablecoin.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::unload_recycler_into_external_asset(vouchers.len() as u32))]
		pub fn unload_recycler_into_external_asset(
			origin: OriginFor<T>,
			claim_token: RecyclerClaimToken,
			vouchers: Vec<Alias>,
			recycler_value: CoinExponent,
			_recycler_index: u32,
			dest: T::AccountId,
		) -> DispatchResult {
			ensure_none(origin)?;

			// Verify claim token
			let token_alias = Self::claim_token_alias(&claim_token);
			ensure!(
				!ConsumedClaimTokens::<T>::contains_key(token_alias),
				Error::<T>::ClaimTokenAlreadyUsed
			);
			ConsumedClaimTokens::<T>::insert(token_alias, ());

			// Verify and consume vouchers
			for voucher in &vouchers {
				let (exp, _) =
					RecyclerVouchers::<T>::get(voucher).ok_or(Error::<T>::InvalidVoucher)?;
				ensure!(exp == recycler_value, Error::<T>::InvalidVoucher);
				RecyclerVouchers::<T>::remove(voucher);
			}

			// Calculate total amount
			let single_value = Self::coin_balance(recycler_value)?;
			let total_amount = single_value
				.checked_mul(&(vouchers.len() as u128).into())
				.ok_or(Error::<T>::ArithmeticOverflow)?;

			// Transfer from pallet account to destination
			let pallet_account = Self::account_id();
			T::Assets::transfer(
				T::BackingAssetId::get(),
				&pallet_account,
				&dest,
				total_amount,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			Self::deposit_event(Event::RecyclerUnloadedIntoAsset {
				value_exponent: recycler_value,
				voucher_count: vouchers.len() as u32,
				dest,
			});
			Ok(())
		}

		/// Unload vouchers from recycler into external asset, paying fees from output.
		///
		/// - `vouchers`: The voucher aliases to consume.
		/// - `recycler_value`: The coin exponent of the recycler.
		/// - `_recycler_index`: The recycler ring index (unused in dummy).
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::unload_recycler_into_external_asset_and_pay_fees(vouchers.len() as u32))]
		pub fn unload_recycler_into_external_asset_and_pay_fees(
			origin: OriginFor<T>,
			vouchers: Vec<Alias>,
			recycler_value: CoinExponent,
			_recycler_index: u32,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// Verify and consume vouchers
			for voucher in &vouchers {
				let (exp, _) =
					RecyclerVouchers::<T>::get(voucher).ok_or(Error::<T>::InvalidVoucher)?;
				ensure!(exp == recycler_value, Error::<T>::InvalidVoucher);
				RecyclerVouchers::<T>::remove(voucher);
			}

			// Calculate total amount (dummy: no fee deduction)
			let single_value = Self::coin_balance(recycler_value)?;
			let total_amount = single_value
				.checked_mul(&(vouchers.len() as u128).into())
				.ok_or(Error::<T>::ArithmeticOverflow)?;

			// Transfer from pallet account to caller
			let pallet_account = Self::account_id();
			T::Assets::transfer(
				T::BackingAssetId::get(),
				&pallet_account,
				&who,
				total_amount,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			Self::deposit_event(Event::RecyclerUnloadedIntoAsset {
				value_exponent: recycler_value,
				voucher_count: vouchers.len() as u32,
				dest: who,
			});
			Ok(())
		}

		/// Pay for a recycler claim token using DOT.
		///
		/// - `member_key`: The member key to add to the paid token ring.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::pay_for_recycler_claim_token())]
		pub fn pay_for_recycler_claim_token_in_dot(
			origin: OriginFor<T>,
			member_key: MemberKey,
		) -> DispatchResult {
			let _who = ensure_signed(origin)?;

			// Dummy: just add to paid token ring (no actual DOT transfer)
			PaidTokenRing::<T>::try_mutate(|ring| -> DispatchResult {
				ring.try_push(member_key).map_err(|_| Error::<T>::PaidTokenRingFull)?;
				Ok(())
			})?;

			let _index = PaidTokenRingIndex::<T>::get();
			PaidTokenRingIndex::<T>::set(_index.saturating_add(1));

			Self::deposit_event(Event::ClaimTokenPurchased { member_key });
			Ok(())
		}

		/// Pay for a recycler claim token using stablecoin.
		///
		/// - `member_key`: The member key to add to the paid token ring.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::pay_for_recycler_claim_token())]
		pub fn pay_for_recycler_claim_token_in_stable(
			origin: OriginFor<T>,
			member_key: MemberKey,
		) -> DispatchResult {
			let _who = ensure_signed(origin)?;

			// Dummy: just add to paid token ring (no actual stablecoin transfer)
			PaidTokenRing::<T>::try_mutate(|ring| -> DispatchResult {
				ring.try_push(member_key).map_err(|_| Error::<T>::PaidTokenRingFull)?;
				Ok(())
			})?;

			let _index = PaidTokenRingIndex::<T>::get();
			PaidTokenRingIndex::<T>::set(_index.saturating_add(1));

			Self::deposit_event(Event::ClaimTokenPurchased { member_key });
			Ok(())
		}

		/// Pay for a recycler claim token using a coin.
		///
		/// - `coin`: The coin to pay with (must cover the fee).
		/// - `member_key`: The member key to add to the paid token ring.
		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::pay_for_recycler_claim_token())]
		pub fn pay_for_recycler_claim_token_in_coin(
			origin: OriginFor<T>,
			coin: PublicKey,
			member_key: MemberKey,
		) -> DispatchResult {
			ensure_signed(origin)?;

			// Verify coin exists
			let _coin_data = CoinsByOwner::<T>::get(coin).ok_or(Error::<T>::CoinNotFound)?;

			// Dummy: burn the coin and add to paid token ring
			CoinsByOwner::<T>::remove(coin);

			PaidTokenRing::<T>::try_mutate(|ring| -> DispatchResult {
				ring.try_push(member_key).map_err(|_| Error::<T>::PaidTokenRingFull)?;
				Ok(())
			})?;

			let _index = PaidTokenRingIndex::<T>::get();
			PaidTokenRingIndex::<T>::set(_index.saturating_add(1));

			Self::deposit_event(Event::ClaimTokenPurchased { member_key });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Get the pallet's account ID.
		pub fn account_id() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// Calculate coin value as u128 from exponent.
		/// Value = 2^exponent (in base units, where 1 base unit = $0.01)
		fn coin_value(exponent: CoinExponent) -> Result<u128, DispatchError> {
			Ok(1u128.checked_shl(exponent as u32).ok_or(Error::<T>::ArithmeticOverflow)?)
		}

		/// Calculate coin balance in asset units from exponent.
		fn coin_balance(exponent: CoinExponent) -> Result<T::Balance, DispatchError> {
			let base: T::Balance = T::BaseValue::get();
			let multiplier = Self::coin_value(exponent)?;
			Ok(base.checked_mul(&multiplier.into()).ok_or(Error::<T>::ArithmeticOverflow)?)
		}

		/// Calculate the combined exponent when consolidating vouchers.
		/// voucher_count vouchers of value 2^exponent = 2^exponent * voucher_count
		/// If voucher_count is a power of 2, result = exponent + log2(voucher_count)
		fn calculate_combined_exponent(
			base_exponent: CoinExponent,
			voucher_count: u32,
		) -> Result<CoinExponent, DispatchError> {
			// For simplicity, require voucher_count to be a power of 2
			ensure!(voucher_count.is_power_of_two(), Error::<T>::InvalidSplitAmount);
			let additional_exponent = voucher_count.trailing_zeros() as u8;
			let new_exponent = base_exponent
				.checked_add(additional_exponent)
				.ok_or(Error::<T>::ArithmeticOverflow)?;
			ensure!(new_exponent <= T::MaxCoinExponent::get(), Error::<T>::InvalidCoinDenomination);
			Ok(new_exponent)
		}

		/// Get alias from claim token for tracking consumption.
		fn claim_token_alias(token: &RecyclerClaimToken) -> Alias {
			// Dummy: generate a unique alias based on token contents
			let mut alias = [0u8; 32];
			match token {
				RecyclerClaimToken::FreelyDistributedToPerson {
					ring_index,
					counter,
					period,
					..
				} => {
					alias[0] = 0;
					alias[1..5].copy_from_slice(&ring_index.to_le_bytes());
					alias[5..9].copy_from_slice(&counter.to_le_bytes());
					alias[9..13].copy_from_slice(&period.to_le_bytes());
				},
				RecyclerClaimToken::FreelyDistributedToLitePerson {
					ring_index,
					counter,
					period,
					..
				} => {
					alias[0] = 1;
					alias[1..5].copy_from_slice(&ring_index.to_le_bytes());
					alias[5..9].copy_from_slice(&counter.to_le_bytes());
					alias[9..13].copy_from_slice(&period.to_le_bytes());
				},
				RecyclerClaimToken::Paid { ring_index, .. } => {
					alias[0] = 2;
					alias[1..5].copy_from_slice(&ring_index.to_le_bytes());
				},
			}
			alias
		}
	}
}
