// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Pallet for committing outbound messages for delivery to Ethereum
//!
//! # Overview
//!
//! Messages come either from sibling parachains via XCM, or BridgeHub itself
//! via the `snowbridge-pallet-system`:
//!
//! 1. `snowbridge_router_primitives::outbound::EthereumBlobExporter::deliver`
//! 2. `snowbridge_pallet_system::Pallet::send`
//!
//! The message submission pipeline works like this:
//! 1. The message is first validated via the implementation for
//!    [`snowbridge_core::outbound::SendMessage::validate`]
//! 2. The message is then enqueued for later processing via the implementation for
//!    [`snowbridge_core::outbound::SendMessage::deliver`]
//! 3. The underlying message queue is implemented by [`Config::MessageQueue`]
//! 4. The message queue delivers messages back to this pallet via the implementation for
//!    [`frame_support::traits::ProcessMessage::process_message`]
//! 5. The message is processed in `Pallet::do_process_message`: a. Assigned a nonce b. ABI-encoded,
//!    hashed, and stored in the `MessageLeaves` vector
//! 6. At the end of the block, a merkle root is constructed from all the leaves in `MessageLeaves`.
//! 7. This merkle root is inserted into the parachain header as a digest item
//! 8. Offchain relayers are able to relay the message to Ethereum after: a. Generating a merkle
//!    proof for the committed message using the `prove_message` runtime API b. Reading the actual
//!    message content from the `Messages` vector in storage
//!
//! On the Ethereum side, the message root is ultimately the thing being
//! verified by the Polkadot light client.
//!
//! # Message Priorities
//!
//! The processing of governance commands can never be halted. This effectively
//! allows us to pause processing of normal user messages while still allowing
//! governance commands to be sent to Ethereum.
//!
//! # Fees
//!
//! An upfront fee must be paid for delivering a message. This fee covers several
//! components:
//! 1. The weight of processing the message locally
//! 2. The gas refund paid out to relayers for message submission
//! 3. An additional reward paid out to relayers for message submission
//!
//! Messages are weighed to determine the maximum amount of gas they could
//! consume on Ethereum. Using this upper bound, a final fee can be calculated.
//!
//! The fee calculation also requires the following parameters:
//! * Average ETH/DOT exchange rate over some period
//! * Max fee per unit of gas that bridge is willing to refund relayers for
//!
//! By design, it is expected that governance should manually update these
//! parameters every few weeks using the `set_pricing_parameters` extrinsic in the
//! system pallet.
//!
//! This is an interim measure. Once ETH/DOT liquidity pools are available in the Polkadot network,
//! we'll use them as a source of pricing info, subject to certain safeguards.
//!
//! ## Fee Computation Function
//!
//! ```text
//! LocalFee(Message) = WeightToFee(ProcessMessageWeight(Message))
//! RemoteFee(Message) = MaxGasRequired(Message) * Params.MaxFeePerGas + Params.Reward
//! RemoteFeeAdjusted(Message) = Params.Multiplier * (RemoteFee(Message) / Params.Ratio("ETH/DOT"))
//! Fee(Message) = LocalFee(Message) + RemoteFeeAdjusted(Message)
//! ```
//!
//! By design, the computed fee includes a safety factor (the `Multiplier`) to cover
//! unfavourable fluctuations in the ETH/DOT exchange rate.
//!
//! ## Fee Settlement
//!
//! On the remote side, in the gateway contract, the relayer accrues
//!
//! ```text
//! Min(GasPrice, Message.MaxFeePerGas) * GasUsed() + Message.Reward
//! ```
//! Or in plain english, relayers are refunded for gas consumption, using a
//! price that is a minimum of the actual gas price, or `Message.MaxFeePerGas`.
//!
//! # Extrinsics
//!
//! * [`Call::set_operating_mode`]: Set the operating mode
//!
//! # Runtime API
//!
//! * `prove_message`: Generate a merkle proof for a committed message
//! * `calculate_fee`: Calculate the delivery fee for a message
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

use bridge_hub_common::{AggregateMessageOrigin, CustomDigestItem};
use codec::Decode;
use frame_support::{
	storage::StorageStreamIter,
	traits::{tokens::Balance, EnqueueMessage, Get, ProcessMessageError},
	weights::{Weight, WeightToFee},
};
use snowbridge_core::{
	outbound_v2::{CommandWrapper, Fee, GasMeter, Message},
	BasicOperatingMode,
};
use snowbridge_outbound_queue_merkle_tree_v2::merkle_root;
pub use snowbridge_outbound_queue_merkle_tree_v2::MerkleProof;
use sp_core::H256;
use sp_runtime::{traits::Hash, DigestItem};
use sp_std::prelude::*;
pub use types::{CommittedMessage, ProcessMessageOriginOf};
pub use weights::WeightInfo;

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
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Message has been queued and will be processed in the future
		MessageQueued {
			/// ID of the message. Usually the XCM message hash or a SetTopic.
			id: H256,
		},
		/// Message will be committed at the end of current block. From now on, to track the
		/// progress the message, use the `nonce` of `id`.
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
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The message is too large
		MessageTooLarge,
		/// The pallet is halted
		Halted,
		/// Invalid Channel
		InvalidChannel,
	}

	/// Messages to be committed in the current block. This storage value is killed in
	/// `on_initialize`, so should never go into block PoV.
	///
	/// Is never read in the runtime, only by offchain message relayers.
	///
	/// Inspired by the `frame_system::Pallet::Events` storage value
	#[pallet::storage]
	#[pallet::unbounded]
	pub(super) type Messages<T: Config> = StorageValue<_, Vec<CommittedMessage>, ValueQuery>;

	/// Hashes of the ABI-encoded messages in the [`Messages`] storage value. Used to generate a
	/// merkle root during `on_finalize`. This storage value is killed in
	/// `on_initialize`, so should never go into block PoV.
	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn message_leaves)]
	pub(super) type MessageLeaves<T: Config> = StorageValue<_, Vec<H256>, ValueQuery>;

	/// The current nonce for each message origin
	#[pallet::storage]
	pub type Nonce<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// The current operating mode of the pallet.
	#[pallet::storage]
	#[pallet::getter(fn operating_mode)]
	pub type OperatingMode<T: Config> = StorageValue<_, BasicOperatingMode, ValueQuery>;

	/// Fee locked by nonce
	#[pallet::storage]
	pub type LockedFee<T: Config> = StorageMap<_, Twox64Concat, u64, u128, ValueQuery>;

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
	impl<T: Config> Pallet<T> {
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
	}

	impl<T: Config> Pallet<T> {
		/// Generate a messages commitment and insert it into the header digest
		pub(crate) fn commit() {
			let count = MessageLeaves::<T>::decode_len().unwrap_or_default() as u64;
			if count == 0 {
				return
			}

			// Create merkle root of messages
			let root = merkle_root::<<T as Config>::Hashing, _>(MessageLeaves::<T>::stream_iter());

			let digest_item: DigestItem = CustomDigestItem::Snowbridge(root).into();

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

			// Decode bytes into versioned message
			let message: Message = Message::decode(&mut message).map_err(|_| Corrupt)?;

			let nonce = Nonce::<T>::get();

			let commands: Vec<CommandWrapper> = message
				.commands
				.into_iter()
				.map(|command| CommandWrapper {
					kind: command.index(),
					max_dispatch_gas: T::GasMeter::maximum_dispatch_gas_used_at_most(&command),
					command: command.clone(),
				})
				.collect();

			// Construct the final committed message
			let committed_message = CommittedMessage {
				origin: message.origin,
				nonce,
				id: message.id,
				commands: commands.try_into().expect("should work"),
			};

			// ABI-encode and hash the prepared message
			let message_abi_encoded = ethabi::encode(&[committed_message.clone().into()]);
			let message_abi_encoded_hash = <T as Config>::Hashing::hash(&message_abi_encoded);

			Messages::<T>::append(Box::new(committed_message.clone()));
			MessageLeaves::<T>::append(message_abi_encoded_hash);
			Nonce::<T>::set(nonce.saturating_add(1));
			<LockedFee<T>>::try_mutate(nonce, |amount| -> DispatchResult {
				*amount = amount.saturating_add(message.fee);
				Ok(())
			})
			.map_err(|_| Corrupt)?;

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
