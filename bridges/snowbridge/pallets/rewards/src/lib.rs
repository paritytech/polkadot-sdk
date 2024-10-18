// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
#![cfg_attr(not(feature = "std"), no_std)]

pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use frame_support::PalletError;
use frame_system::pallet_prelude::*;
use snowbridge_core::rewards::RewardLedger;
use sp_core::H160;
pub use weights::WeightInfo;
use xcm::prelude::{send_xcm, SendError as XcmpSendError, *};

pub use pallet::*;

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use sp_core::H256;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type AssetHubParaId: Get<u32>;
		type EthereumNetwork: Get<NetworkId>;
		type WethAddress: Get<H160>;
		/// XCM message sender
		type XcmSender: SendXcm;
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A relayer reward was deposited
		RewardDeposited {
			/// The relayer account to which the reward was deposited.
			account_id: AccountIdOf<T>,
			/// The reward value.
			value: u128,
		},
		RewardClaimed {
			/// The relayer account that claimed the reward.
			account_id: AccountIdOf<T>,
			/// The address that received the reward on AH.
			deposit_address: AccountIdOf<T>,
			/// The claimed reward value.
			value: u128,
			/// The message ID that was provided, used to track the claim
			message_id: H256,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// XCMP send failure
		Send(SendError),
		/// The relayer rewards balance is lower than the claimed amount.
		InsufficientFunds,
	}

	#[derive(Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo, PalletError)]
	pub enum SendError {
		NotApplicable,
		NotRoutable,
		Transport,
		DestinationUnsupported,
		ExceedsMaxMessageSize,
		MissingArgument,
		Fees,
	}

	impl<T: Config> From<XcmpSendError> for Error<T> {
		fn from(e: XcmpSendError) -> Self {
			match e {
				XcmpSendError::NotApplicable => Error::<T>::Send(SendError::NotApplicable),
				XcmpSendError::Unroutable => Error::<T>::Send(SendError::NotRoutable),
				XcmpSendError::Transport(_) => Error::<T>::Send(SendError::Transport),
				XcmpSendError::DestinationUnsupported =>
					Error::<T>::Send(SendError::DestinationUnsupported),
				XcmpSendError::ExceedsMaxMessageSize =>
					Error::<T>::Send(SendError::ExceedsMaxMessageSize),
				XcmpSendError::MissingArgument => Error::<T>::Send(SendError::MissingArgument),
				XcmpSendError::Fees => Error::<T>::Send(SendError::Fees),
			}
		}
	}

	#[pallet::storage]
	pub type RewardsMapping<T: Config> = StorageMap<_, Identity, AccountIdOf<T>, u128, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((T::WeightInfo::claim(), DispatchClass::Operational))]
		pub fn claim(
			origin: OriginFor<T>,
			deposit_address: AccountIdOf<T>,
			value: u128,
			message_id: H256,
		) -> DispatchResult {
			let account_id = ensure_signed(origin)?;
			Self::process_claim(account_id, deposit_address, value, message_id)?;
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		fn process_claim(
			account_id: AccountIdOf<T>,
			deposit_address: AccountIdOf<T>,
			value: u128,
			message_id: H256,
		) -> DispatchResult {
			// Check if the claim value is equal to or less than the accumulated balance.
			let reward_balance = RewardsMapping::<T>::get(account_id.clone());
			if value > reward_balance {
				return Err(Error::<T>::InsufficientFunds.into());
			}

			let reward_asset = snowbridge_core::location::convert_token_address(
				T::EthereumNetwork::get(),
				T::WethAddress::get(),
			);
			let deposit: Asset = (reward_asset, value).into();
			let beneficiary: Location =
				Location::new(0, Parachain(T::AssetHubParaId::get().into()));

			let xcm: Xcm<()> = vec![
				DepositAsset { assets: Definite(deposit.into()), beneficiary },
				SetTopic(message_id.into()),
			]
			.into();

			let dest = Location::new(1, [Parachain(T::AssetHubParaId::get().into())]);
			let (_xcm_hash, _) = send_xcm::<T::XcmSender>(dest, xcm).map_err(Error::<T>::from)?;

			Self::deposit_event(Event::RewardClaimed {
				account_id,
				deposit_address,
				value,
				message_id,
			});
			Ok(())
		}
	}

	impl<T: Config> RewardLedger<T> for Pallet<T> {
		fn deposit(account_id: AccountIdOf<T>, value: u128) -> DispatchResult {
			RewardsMapping::<T>::mutate(account_id.clone(), |current_value| {
				*current_value = current_value.saturating_add(value);
			});
			Self::deposit_event(Event::RewardDeposited { account_id, value });

			Ok(())
		}
	}
}
