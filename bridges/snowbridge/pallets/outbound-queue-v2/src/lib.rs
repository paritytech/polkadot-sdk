// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Pallet for committing outbound messages for delivery to Ethereum
//!
//! # Overview
//!
//! Messages come either from sibling parachains via XCM, or BridgeHub itself
//! via the `snowbridge-pallet-system-v2`:
//!
//! 1. `snowbridge_outbound_queue_primitives::v2::EthereumBlobExporter::deliver`
//! 2. `snowbridge_pallet_system_v2::Pallet::send`
//!
//! The message submission pipeline works like this:
//! 1. The message is first validated via the implementation for
//!    [`snowbridge_outbound_queue_primitives::v2::SendMessage::validate`]
//! 2. The message is then enqueued for later processing via the implementation for
//!    [`snowbridge_outbound_queue_primitives::v2::SendMessage::deliver`]
//! 3. The underlying message queue is implemented by [`Config::MessageQueue`]
//! 4. The message queue delivers messages to this pallet via the implementation for
//!    [`frame_support::traits::ProcessMessage::process_message`]
//! 5. The message is processed in `Pallet::do_process_message`:
//! 	a. Convert to `OutboundMessage`, and stored into the `Messages` vector storage
//! 	b. ABI-encoded the OutboundMessage, with commited hash stored into the `MessageLeaves` storage
//! 	c. Generate `PendingOrder` with assigned nonce and fee attach, stored into the `PendingOrders`
//! 	   map storage, with nonce as the key
//! 	d. Increment nonce and update the `Nonce` storage
//! 6. At the end of the block, a merkle root is constructed from all the leaves in `MessageLeaves`,
//!    then `MessageLeaves` is dropped so that it is never committed to storage or included in PoV.
//! 7. This merkle root is inserted into the parachain header as a digest item
//! 8. Offchain relayers are able to relay the message to Ethereum after:
//! 	a. Generating a merkle proof for the committed message using the `prove_message` runtime API
//! 	b. Reading the actual message content from the `Messages` vector in storage
//! 9. On the Ethereum side, the message root is ultimately the thing being verified by the Beefy
//!    light client. When the message has been verified and executed, the relayer will call the
//!    extrinsic `submit_delivery_proof` work the way as follows:
//! 	a. Verify the message with proof for a transaction receipt containing the event log,
//! 	   same as the inbound queue verification flow
//! 	b. Fetch the pending order by nonce of the message, pay reward with fee attached in the order
//!    	c. Remove the order from `PendingOrders` map storage by nonce
//!
//! # Message Priorities
//!
//! The processing of governance commands can never be halted. This effectively
//! allows us to pause processing of normal user messages while still allowing
//! governance commands to be sent to Ethereum.
//!
//! # Extrinsics
//!
//! * [`Call::set_operating_mode`]: Set the operating mode
//! * [`Call::submit_delivery_proof`]: Submit delivery proof
//!
//! # Runtime API
//!
//! * `prove_message`: Generate a merkle proof for a committed message
//! * `dry_run`: Convert xcm to InboundMessage
#![cfg_attr(not(feature = "std"), no_std)]
pub mod api;
pub mod process_message_impl;
pub mod send_message_impl;
pub mod types;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod test;

use alloy_core::{
	primitives::{Bytes, FixedBytes},
	sol_types::SolValue,
};
use bridge_hub_common::{AggregateMessageOrigin, CustomDigestItem};
use codec::Decode;
use frame_support::{
	storage::StorageStreamIter,
	traits::{tokens::Balance, EnqueueMessage, Get, ProcessMessageError},
	weights::{Weight, WeightToFee},
};
use snowbridge_core::{ether_asset, BasicOperatingMode, PaymentProcedure, TokenId};
use snowbridge_merkle_tree::merkle_root;
use snowbridge_outbound_queue_primitives::{
	v2::{
		abi::{CommandWrapper, OutboundMessageWrapper},
		DeliveryReceipt, GasMeter, Message, OutboundCommandWrapper, OutboundMessage,
	},
	EventProof, VerificationError, Verifier,
};
use sp_core::{H160, H256};
use sp_runtime::{
	traits::{BlockNumberProvider, Hash, MaybeEquivalence},
	DigestItem,
};
use sp_std::prelude::*;
pub use types::{PendingOrder, ProcessMessageOriginOf};
pub use weights::WeightInfo;
use xcm::latest::{Location, NetworkId};

type DeliveryReceiptOf<T> = DeliveryReceipt<<T as frame_system::Config>::AccountId>;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type Hashing: Hash<Output = H256>;

		type MessageQueue: EnqueueMessage<AggregateMessageOrigin>;

		/// Measures the maximum gas used to execute a command on Ethereum
		type GasMeter: GasMeter;

		type Balance: Balance + From<u128>;

		/// Max bytes in a message payload
		#[pallet::constant]
		type MaxMessagePayloadSize: Get<u32>;

		/// Max number of messages processed per block
		#[pallet::constant]
		type MaxMessagesPerBlock: Get<u32>;

		/// Convert a weight value into a deductible fee based.
		type WeightToFee: WeightToFee<Balance = Self::Balance>;

		/// Weight information for extrinsics in this pallet
		type WeightInfo: WeightInfo;

		/// The verifier for delivery proof from Ethereum
		type Verifier: Verifier;

		/// Address of the Gateway contract
		#[pallet::constant]
		type GatewayAddress: Get<H160>;

		/// Means of paying a relayer
		type RewardPayment: PaymentProcedure<Self::AccountId>;

		type ConvertAssetId: MaybeEquivalence<TokenId, Location>;

		type EthereumNetwork: Get<NetworkId>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Message has been queued and will be processed in the future
		MessageQueued {
			/// The message
			message: Message,
		},
		/// Message will be committed at the end of current block. From now on, to track the
		/// progress the message, use the `nonce` or the `id`.
		MessageAccepted {
			/// ID of the message
			id: H256,
			/// The nonce assigned to this message
			nonce: u64,
		},
		/// Some messages have been committed
		MessagesCommitted {
			/// Merkle root of the committed messages
			root: H256,
			/// number of committed messages
			count: u64,
		},
		/// Set OperatingMode
		OperatingModeChanged { mode: BasicOperatingMode },
		/// Delivery Proof received
		MessageDeliveryProofReceived { nonce: u64 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The message is too large
		MessageTooLarge,
		/// The pallet is halted
		Halted,
		/// Invalid Channel
		InvalidChannel,
		/// Invalid Envelope
		InvalidEnvelope,
		/// Message verification error
		Verification(VerificationError),
		/// Invalid Gateway
		InvalidGateway,
		/// Pending nonce does not exist
		InvalidPendingNonce,
		/// Reward payment failed
		RewardPaymentFailed,
	}

	/// Messages to be committed in the current block. This storage value is killed in
	/// `on_initialize`, so will not end up bloating state.
	///
	/// Is never read in the runtime, only by offchain message relayers.
	/// Because of this, it will never go into the PoV of a block.
	///
	/// Inspired by the `frame_system::Pallet::Events` storage value
	#[pallet::storage]
	#[pallet::unbounded]
	pub(super) type Messages<T: Config> = StorageValue<_, Vec<OutboundMessage>, ValueQuery>;

	/// Hashes of the ABI-encoded messages in the [`Messages`] storage value. Used to generate a
	/// merkle root during `on_finalize`. This storage value is killed in
	/// `on_initialize`, so should never go into block PoV.
	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn message_leaves)]
	pub(super) type MessageLeaves<T: Config> = StorageValue<_, Vec<H256>, ValueQuery>;

	/// The current nonce for the messages
	#[pallet::storage]
	pub type Nonce<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// The current operating mode of the pallet.
	#[pallet::storage]
	#[pallet::getter(fn operating_mode)]
	pub type OperatingMode<T: Config> = StorageValue<_, BasicOperatingMode, ValueQuery>;

	/// Pending orders to relay
	#[pallet::storage]
	pub type PendingOrders<T: Config> =
		StorageMap<_, Identity, u64, PendingOrder<BlockNumberFor<T>>, OptionQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
	where
		T::AccountId: AsRef<[u8]>,
	{
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			// Remove storage from previous block
			Messages::<T>::kill();
			MessageLeaves::<T>::kill();
			// Reserve some weight for the `on_finalize` handler
			T::WeightInfo::commit()
		}

		fn on_finalize(_: BlockNumberFor<T>) {
			Self::commit();
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: From<[u8; 32]>,
	{
		/// Halt or resume all pallet operations. May only be called by root.
		#[pallet::call_index(0)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_operating_mode(
			origin: OriginFor<T>,
			mode: BasicOperatingMode,
		) -> DispatchResult {
			ensure_root(origin)?;
			OperatingMode::<T>::put(mode);
			Self::deposit_event(Event::OperatingModeChanged { mode });
			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::submit_delivery_receipt())]
		pub fn submit_delivery_receipt(
			origin: OriginFor<T>,
			event: Box<EventProof>,
		) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(!Self::operating_mode().is_halted(), Error::<T>::Halted);

			// submit message to verifier for verification
			T::Verifier::verify(&event.event_log, &event.proof)
				.map_err(|e| Error::<T>::Verification(e))?;

			let receipt = DeliveryReceiptOf::<T>::try_from(&event.event_log)
				.map_err(|_| Error::<T>::InvalidEnvelope)?;

			// Verify that the message was submitted from the known Gateway contract
			ensure!(T::GatewayAddress::get() == receipt.gateway, Error::<T>::InvalidGateway);

			let nonce = receipt.nonce;

			let order = <PendingOrders<T>>::get(nonce).ok_or(Error::<T>::InvalidPendingNonce)?;

			if order.fee > 0 {
				let ether = ether_asset(T::EthereumNetwork::get(), order.fee);
				T::RewardPayment::pay_reward(receipt.reward_address, ether)
					.map_err(|_| Error::<T>::RewardPaymentFailed)?;
			}

			<PendingOrders<T>>::remove(nonce);

			Self::deposit_event(Event::MessageDeliveryProofReceived { nonce });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Generate a messages commitment and insert it into the header digest
		pub(crate) fn commit() {
			let count = MessageLeaves::<T>::decode_len().unwrap_or_default() as u64;
			if count == 0 {
				return;
			}

			// Create merkle root of messages
			let root = merkle_root::<<T as Config>::Hashing, _>(MessageLeaves::<T>::stream_iter());

			let digest_item: DigestItem = CustomDigestItem::SnowbridgeV2(root).into();

			// Insert merkle root into the header digest
			<frame_system::Pallet<T>>::deposit_log(digest_item);

			Self::deposit_event(Event::MessagesCommitted { root, count });
		}

		/// Process a message delivered by the MessageQueue pallet
		pub(crate) fn do_process_message(
			_: ProcessMessageOriginOf<T>,
			mut message: &[u8],
		) -> Result<bool, ProcessMessageError> {
			use ProcessMessageError::*;

			// Yield if the maximum number of messages has been processed this block.
			// This ensures that the weight of `on_finalize` has a known maximum bound.
			ensure!(
				MessageLeaves::<T>::decode_len().unwrap_or(0) <
					T::MaxMessagesPerBlock::get() as usize,
				Yield
			);

			let nonce = Nonce::<T>::get();

			// Decode bytes into Message and
			// a. Convert to OutboundMessage and save into Messages
			// b. Convert to committed hash and save into MessageLeaves
			// c. Save nonce&fee into PendingOrders
			let message: Message = Message::decode(&mut message).map_err(|_| Corrupt)?;
			let commands: Vec<OutboundCommandWrapper> = message
				.commands
				.clone()
				.into_iter()
				.map(|command| OutboundCommandWrapper {
					kind: command.index(),
					gas: T::GasMeter::maximum_dispatch_gas_used_at_most(&command),
					payload: command.abi_encode(),
				})
				.collect();

			let abi_commands: Vec<CommandWrapper> = commands
				.clone()
				.into_iter()
				.map(|command| CommandWrapper {
					kind: command.kind,
					gas: command.gas,
					payload: Bytes::from(command.payload),
				})
				.collect();
			let committed_message = OutboundMessageWrapper {
				origin: FixedBytes::from(message.origin.as_fixed_bytes()),
				nonce,
				commands: abi_commands,
			};
			let message_abi_encoded_hash =
				<T as Config>::Hashing::hash(&committed_message.abi_encode());
			MessageLeaves::<T>::append(message_abi_encoded_hash);

			let outbound_message = OutboundMessage {
				origin: message.origin,
				nonce,
				commands: commands.try_into().map_err(|_| Corrupt)?,
			};
			Messages::<T>::append(Box::new(outbound_message));

			// Generate `PendingOrder` with fee attached in the message, stored
			// into the `PendingOrders` map storage, with assigned nonce as the key.
			// When the message is processed on ethereum side, the relayer will send the nonce
			// back with delivery proof, only after that the order can
			// be resolved and the fee will be rewarded to the relayer.
			let order = PendingOrder {
				nonce,
				fee: message.fee,
				block_number: frame_system::Pallet::<T>::current_block_number(),
			};
			<PendingOrders<T>>::insert(nonce, order);

			Nonce::<T>::set(nonce.checked_add(1).ok_or(Unsupported)?);

			Self::deposit_event(Event::MessageAccepted { id: message.id, nonce });

			Ok(true)
		}

		/// The local component of the message processing fees in native currency
		pub(crate) fn calculate_local_fee() -> T::Balance {
			T::WeightToFee::weight_to_fee(
				&T::WeightInfo::do_process_message().saturating_add(T::WeightInfo::commit_single()),
			)
		}
	}
}
