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
//! * [`Call::create_agent`]: Create agent for a sibling parachain
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
use snowbridge_core::{AgentId, AssetMetadata, TokenId, TokenIdOf};
use snowbridge_outbound_primitives::{
	v2::{Command, Message, SendMessage},
	SendError,
};
use sp_core::H256;
use sp_runtime::traits::MaybeEquivalence;
use sp_std::prelude::*;
use xcm::prelude::*;
use xcm_executor::traits::ConvertLocation;

#[cfg(feature = "runtime-benchmarks")]
use frame_support::traits::OriginTrait;

pub use pallet::*;

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

pub fn agent_id_of<T: Config>(location: &Location) -> Result<H256, DispatchError> {
	T::AgentIdOf::convert_location(location).ok_or(Error::<T>::LocationConversionFailed.into())
}

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
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Send messages to Ethereum
		type OutboundQueue: SendMessage;

		/// Origin check for XCM locations that can create agents
		type SiblingOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Location>;

		/// Converts Location to AgentId
		type AgentIdOf: ConvertLocation<AgentId>;

		type WeightInfo: WeightInfo;

		/// This chain's Universal Location.
		type UniversalLocation: Get<InteriorLocation>;

		// The bridges configured Ethereum location
		type EthereumLocation: Get<Location>;

		#[cfg(feature = "runtime-benchmarks")]
		type Helper: BenchmarkHelper<Self::RuntimeOrigin>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An CreateAgent message was sent to the Gateway
		CreateAgent { location: Box<Location>, agent_id: AgentId },
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
		LocationConversionFailed,
		AgentAlreadyCreated,
		NoAgent,
		UnsupportedLocationVersion,
		InvalidLocation,
		Send(SendError),
	}

	/// The set of registered agents
	#[pallet::storage]
	#[pallet::getter(fn agents)]
	pub type Agents<T: Config> = StorageMap<_, Twox64Concat, AgentId, (), OptionQuery>;

	/// Lookup table for foreign token ID to native location relative to ethereum
	#[pallet::storage]
	pub type ForeignToNativeId<T: Config> =
		StorageMap<_, Blake2_128Concat, TokenId, xcm::v5::Location, OptionQuery>;

	/// Lookup table for native location relative to ethereum to foreign token ID
	#[pallet::storage]
	pub type NativeToForeignId<T: Config> =
		StorageMap<_, Blake2_128Concat, xcm::v5::Location, TokenId, OptionQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sends a command to the Gateway contract to instantiate a new agent contract representing
		/// `origin`.
		///
		/// Fee required: Yes
		///
		/// - `origin`: Must be `Location` of a sibling parachain
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::create_agent())]
		pub fn create_agent(
			origin: OriginFor<T>,
			location: Box<VersionedLocation>,
			fee: u128,
		) -> DispatchResult {
			T::SiblingOrigin::ensure_origin(origin)?;

			let origin_location: Location =
				(*location).try_into().map_err(|_| Error::<T>::UnsupportedLocationVersion)?;

			let ethereum_location = T::EthereumLocation::get();
			// reanchor to Ethereum context
			let location = origin_location
				.clone()
				.reanchored(&ethereum_location, &T::UniversalLocation::get())
				.map_err(|_| Error::<T>::LocationConversionFailed)?;

			let agent_id = agent_id_of::<T>(&location)?;

			// Record the agent id or fail if it has already been created
			ensure!(!Agents::<T>::contains_key(agent_id), Error::<T>::AgentAlreadyCreated);
			Agents::<T>::insert(agent_id, ());

			let command = Command::CreateAgent {};

			Self::send(origin_location.clone(), command, fee)?;

			Self::deposit_event(Event::<T>::CreateAgent {
				location: Box::new(origin_location),
				agent_id,
			});
			Ok(())
		}

		/// Registers a Polkadot-native token as a wrapped ERC20 token on Ethereum.
		/// Privileged. Can only be called by root.
		///
		/// Fee required: No
		///
		/// - `origin`: Must be root
		/// - `location`: Location of the asset (relative to this chain)
		/// - `metadata`: Metadata to include in the instantiated ERC20 contract on Ethereum
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::register_token())]
		pub fn register_token(
			origin: OriginFor<T>,
			asset_id: Box<VersionedLocation>,
			metadata: AssetMetadata,
			fee: u128,
		) -> DispatchResult {
			let origin_location = T::SiblingOrigin::ensure_origin(origin)?;

			let asset_location: Location =
				(*asset_id).try_into().map_err(|_| Error::<T>::UnsupportedLocationVersion)?;

			let ethereum_location = T::EthereumLocation::get();
			// reanchor to Ethereum context
			let location = asset_location
				.clone()
				.reanchored(&ethereum_location, &T::UniversalLocation::get())
				.map_err(|_| Error::<T>::LocationConversionFailed)?;

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
			Self::send(origin_location, command, fee)?;

			Self::deposit_event(Event::<T>::RegisterToken {
				location: location.clone().into(),
				foreign_token_id: token_id,
			});

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Send `command` to the Gateway on the Channel identified by `channel_id`
		fn send(origin_location: Location, command: Command, fee: u128) -> DispatchResult {
			let origin = agent_id_of::<T>(&origin_location)?;

			let mut message = Message {
				origin_location,
				origin,
				id: Default::default(),
				fee,
				commands: BoundedVec::try_from(vec![command]).unwrap(),
			};
			let hash = sp_io::hashing::blake2_256(&message.encode());
			message.id = hash.into();

			let (ticket, _) =
				T::OutboundQueue::validate(&message).map_err(|err| Error::<T>::Send(err))?;

			T::OutboundQueue::deliver(ticket).map_err(|err| Error::<T>::Send(err))?;
			Ok(())
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
