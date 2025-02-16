// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Governance API for controlling the Ethereum side of the bridge
//!
//! # Extrinsics
//!
//! ## Agents
//!
//! Agents are smart contracts on Ethereum that act as proxies for consensus systems on Polkadot
//! networks.
//!
//! * [`Call::create_agent`]: Create agent for any kind of sovereign location on Polkadot network,
//!   can be a sibling parachain, pallet or smart contract or signed account in that parachain, etc

//! ## Polkadot-native tokens on Ethereum
//!
//! Tokens deposited on AssetHub pallet can be bridged to Ethereum as wrapped ERC20 tokens. As a
//! prerequisite, the token should be registered first.
//!
//! * [`Call::register_token`]: Register a token location as a wrapped ERC20 contract on Ethereum.
#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod api;
pub mod weights;
pub use weights::*;

use frame_support::{pallet_prelude::*, traits::EnsureOrigin};
use frame_system::pallet_prelude::*;
use snowbridge_core::{AgentIdOf as LocationHashOf, AssetMetadata, TokenId, TokenIdOf};
use snowbridge_outbound_queue_primitives::{
	v2::{Command, Message, SendMessage},
	SendError,
};
use sp_core::H256;
use sp_runtime::traits::MaybeEquivalence;
use sp_std::prelude::*;
use xcm::prelude::*;
use xcm_executor::traits::ConvertLocation;

use snowbridge_pallet_system::{Agents, ForeignToNativeId, NativeToForeignId};

#[cfg(feature = "runtime-benchmarks")]
use frame_support::traits::OriginTrait;

pub use pallet::*;

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<O>
where
	O: OriginTrait,
{
	fn make_xcm_origin(location: Location) -> O;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + snowbridge_pallet_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Send messages to Ethereum
		type OutboundQueue: SendMessage;

		/// Origin check for XCM locations that transact with this pallet
		type FrontendOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Location>;

		type WeightInfo: WeightInfo;
		#[cfg(feature = "runtime-benchmarks")]
		type Helper: BenchmarkHelper<Self::RuntimeOrigin>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An CreateAgent message was sent to the Gateway
		CreateAgent { location: Box<Location>, agent_id: H256 },
		/// Register Polkadot-native token as a wrapped ERC20 token on Ethereum
		RegisterToken {
			/// Location of Polkadot-native token
			location: VersionedLocation,
			/// ID of Polkadot-native token on Ethereum
			foreign_token_id: H256,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		LocationReanchorFailed,
		LocationConversionFailed,
		AgentAlreadyCreated,
		NoAgent,
		UnsupportedLocationVersion,
		InvalidLocation,
		Send(SendError),
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sends a command to the Gateway contract to instantiate a new agent contract representing
		/// `origin`.
		///
		/// - `location`: The location representing the agent
		/// - `fee`: Ether to pay for the execution cost on Ethereum
		#[pallet::call_index(1)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::create_agent())]
		pub fn create_agent(
			origin: OriginFor<T>,
			location: Box<VersionedLocation>,
			fee: u128,
		) -> DispatchResult {
			T::FrontendOrigin::ensure_origin(origin)?;

			let location: Location =
				(*location).try_into().map_err(|_| Error::<T>::UnsupportedLocationVersion)?;

			let message_origin = Self::location_to_message_origin(&location)?;

			// Record the agent id or fail if it has already been created
			ensure!(!Agents::<T>::contains_key(message_origin), Error::<T>::AgentAlreadyCreated);
			Agents::<T>::insert(message_origin, ());

			let command = Command::CreateAgent {};

			Self::send(message_origin, command, fee)?;

			Self::deposit_event(Event::<T>::CreateAgent {
				location: Box::new(location),
				agent_id: message_origin,
			});
			Ok(())
		}

		/// Registers a Polkadot-native token as a wrapped ERC20 token on Ethereum.
		///
		/// - `asset_id`: Location of the asset (relative to this chain)
		/// - `metadata`: Metadata to include in the instantiated ERC20 contract on Ethereum
		/// - `fee`: Ether to pay for the execution cost on Ethereum
		#[pallet::call_index(2)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::register_token())]
		pub fn register_token(
			origin: OriginFor<T>,
			asset_id: Box<VersionedLocation>,
			metadata: AssetMetadata,
			fee: u128,
		) -> DispatchResult {
			let origin_location = T::FrontendOrigin::ensure_origin(origin)?;
			let message_origin = Self::location_to_message_origin(&origin_location)?;

			let asset_location: Location =
				(*asset_id).try_into().map_err(|_| Error::<T>::UnsupportedLocationVersion)?;

			let location = Self::reanchor(&asset_location)?;

			let token_id = TokenIdOf::convert_location(&location)
				.ok_or(Error::<T>::LocationConversionFailed)?;

			if !ForeignToNativeId::<T>::contains_key(token_id) {
				NativeToForeignId::<T>::insert(location.clone(), token_id);
				ForeignToNativeId::<T>::insert(token_id, location.clone());
			}

			let command = Command::RegisterForeignToken {
				token_id,
				name: metadata.name.into_inner(),
				symbol: metadata.symbol.into_inner(),
				decimals: metadata.decimals,
			};
			Self::send(message_origin, command, fee)?;

			Self::deposit_event(Event::<T>::RegisterToken {
				location: location.clone().into(),
				foreign_token_id: token_id,
			});

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Send `command` to the Gateway from a specific origin/agent
		fn send(origin: H256, command: Command, fee: u128) -> DispatchResult {
			let mut message = Message {
				origin,
				id: Default::default(),
				fee,
				commands: BoundedVec::try_from(vec![command]).unwrap(),
			};
			let hash = sp_io::hashing::blake2_256(&message.encode());
			message.id = hash.into();

			let (ticket, _) = <T as pallet::Config>::OutboundQueue::validate(&message)
				.map_err(|err| Error::<T>::Send(err))?;

			<T as pallet::Config>::OutboundQueue::deliver(ticket)
				.map_err(|err| Error::<T>::Send(err))?;
			Ok(())
		}

		/// Reanchor the `location` in context of ethereum
		fn reanchor(location: &Location) -> Result<Location, Error<T>> {
			location
				.clone()
				.reanchored(&T::EthereumLocation::get(), &T::UniversalLocation::get())
				.map_err(|_| Error::<T>::LocationReanchorFailed)
		}

		pub fn location_to_message_origin(location: &Location) -> Result<H256, Error<T>> {
			let reanchored_location = Self::reanchor(location)?;
			LocationHashOf::convert_location(&reanchored_location)
				.ok_or(Error::<T>::LocationConversionFailed)
		}
	}

	impl<T: Config> MaybeEquivalence<TokenId, Location> for Pallet<T> {
		fn convert(foreign_id: &TokenId) -> Option<Location> {
			ForeignToNativeId::<T>::get(foreign_id)
		}
		fn convert_back(location: &Location) -> Option<TokenId> {
			NativeToForeignId::<T>::get(location)
		}
	}
}
