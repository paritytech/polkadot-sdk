// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]

//! cumulus-pallet-parachain-system is a base pallet for cumulus-based parachains.
//!
//! This pallet handles low-level details of being a parachain. It's responsibilities include:
//!
//! - ingestion of the parachain validation data
//! - ingestion of incoming downward and lateral messages and dispatching them
//! - coordinating upgrades with the relay-chain
//! - communication of parachain outputs, such as sent messages, signalling an upgrade, etc.
//!
//! Users must ensure that they register this pallet as an inherent provider.

use codec::Encode;
use cumulus_primitives_core::{
	relay_chain, AbridgedHostConfiguration, ChannelStatus, CollationInfo, DmpMessageHandler,
	GetChannelInfo, InboundDownwardMessage, InboundHrmpMessage, MessageSendError,
	OutboundHrmpMessage, ParaId, PersistedValidationData, UpwardMessage, UpwardMessageSender,
	XcmpMessageHandler, XcmpMessageSource,
};
use cumulus_primitives_parachain_inherent::{MessageQueueChain, ParachainInherentData};
use frame_support::{
	dispatch::{DispatchError, DispatchResult},
	ensure,
	inherent::{InherentData, InherentIdentifier, ProvideInherent},
	storage,
	traits::Get,
	weights::{Pays, PostDispatchInfo, Weight},
};
use frame_system::{ensure_none, ensure_root};
use polkadot_parachain::primitives::RelayChainBlockNumber;
use relay_state_snapshot::MessagingStateSnapshot;
use sp_runtime::{
	traits::{Block as BlockT, BlockNumberProvider, Hash},
	transaction_validity::{
		InvalidTransaction, TransactionLongevity, TransactionSource, TransactionValidity,
		ValidTransaction,
	},
};
use sp_std::{cmp, collections::btree_map::BTreeMap, prelude::*};

mod migration;
mod relay_state_snapshot;
#[macro_use]
pub mod validate_block;
#[cfg(test)]
mod tests;

/// Register the `validate_block` function that is used by parachains to validate blocks on a
/// validator.
///
/// Does *nothing* when `std` feature is enabled.
///
/// Expects as parameters the runtime, a block executor and an inherent checker.
///
/// # Example
///
/// ```
///     struct BlockExecutor;
///     struct Runtime;
///     struct CheckInherents;
///
///     cumulus_pallet_parachain_system::register_validate_block! {
///         Runtime = Runtime,
///         BlockExecutor = Executive,
///         CheckInherents = CheckInherents,
///     }
///
/// # fn main() {}
/// ```
pub use cumulus_pallet_parachain_system_proc_macro::register_validate_block;
pub use relay_state_snapshot::RelayChainStateProof;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::storage_version(migration::STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<OnSetCode = ParachainSetCode<Self>> {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Something which can be notified when the validation data is set.
		type OnSystemEvent: OnSystemEvent;

		/// Returns the parachain ID we are running with.
		type SelfParaId: Get<ParaId>;

		/// The place where outbound XCMP messages come from. This is queried in `finalize_block`.
		type OutboundXcmpMessageSource: XcmpMessageSource;

		/// The message handler that will be invoked when messages are received via DMP.
		type DmpMessageHandler: DmpMessageHandler;

		/// The weight we reserve at the beginning of the block for processing DMP messages.
		type ReservedDmpWeight: Get<Weight>;

		/// The message handler that will be invoked when messages are received via XCMP.
		///
		/// The messages are dispatched in the order they were relayed by the relay chain. If
		/// multiple messages were relayed at one block, these will be dispatched in ascending
		/// order of the sender's para ID.
		type XcmpMessageHandler: XcmpMessageHandler;

		/// The weight we reserve at the beginning of the block for processing XCMP messages.
		type ReservedXcmpWeight: Get<Weight>;
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			migration::on_runtime_upgrade::<T>()
		}

		fn on_finalize(_: T::BlockNumber) {
			<DidSetValidationCode<T>>::kill();
			<UpgradeRestrictionSignal<T>>::kill();

			assert!(
				<ValidationData<T>>::exists(),
				"set_validation_data inherent needs to be present in every block!"
			);

			let host_config = match Self::host_configuration() {
				Some(ok) => ok,
				None => {
					debug_assert!(
						false,
						"host configuration is promised to set until `on_finalize`; qed",
					);
					return
				},
			};
			let relevant_messaging_state = match Self::relevant_messaging_state() {
				Some(ok) => ok,
				None => {
					debug_assert!(
						false,
						"relevant messaging state is promised to be set until `on_finalize`; \
							qed",
					);
					return
				},
			};

			<PendingUpwardMessages<T>>::mutate(|up| {
				let (count, size) = relevant_messaging_state.relay_dispatch_queue_size;

				let available_capacity = cmp::min(
					host_config.max_upward_queue_count.saturating_sub(count),
					host_config.max_upward_message_num_per_candidate,
				);
				let available_size = host_config.max_upward_queue_size.saturating_sub(size);

				// Count the number of messages we can possibly fit in the given constraints, i.e.
				// available_capacity and available_size.
				let num = up
					.iter()
					.scan((available_capacity as usize, available_size as usize), |state, msg| {
						let (cap_left, size_left) = *state;
						match (cap_left.checked_sub(1), size_left.checked_sub(msg.len())) {
							(Some(new_cap), Some(new_size)) => {
								*state = (new_cap, new_size);
								Some(())
							},
							_ => None,
						}
					})
					.count();

				// TODO: #274 Return back messages that do not longer fit into the queue.

				UpwardMessages::<T>::put(&up[..num]);
				*up = up.split_off(num);
			});

			// Sending HRMP messages is a little bit more involved. There are the following
			// constraints:
			//
			// - a channel should exist (and it can be closed while a message is buffered),
			// - at most one message can be sent in a channel,
			// - the sent out messages should be ordered by ascension of recipient para id.
			// - the capacity and total size of the channel is limited,
			// - the maximum size of a message is limited (and can potentially be changed),

			let maximum_channels = host_config
				.hrmp_max_message_num_per_candidate
				.min(<AnnouncedHrmpMessagesPerCandidate<T>>::take()) as usize;

			let outbound_messages =
				T::OutboundXcmpMessageSource::take_outbound_messages(maximum_channels)
					.into_iter()
					.map(|(recipient, data)| OutboundHrmpMessage { recipient, data })
					.collect::<Vec<_>>();

			HrmpOutboundMessages::<T>::put(outbound_messages);
		}

		fn on_initialize(_n: T::BlockNumber) -> Weight {
			let mut weight = 0;

			// To prevent removing `NewValidationCode` that was set by another `on_initialize`
			// like for example from scheduler, we only kill the storage entry if it was not yet
			// updated in the current block.
			if !<DidSetValidationCode<T>>::get() {
				NewValidationCode::<T>::kill();
				weight += T::DbWeight::get().writes(1);
			}

			// Remove the validation from the old block.
			<ValidationData<T>>::kill();
			ProcessedDownwardMessages::<T>::kill();
			HrmpWatermark::<T>::kill();
			UpwardMessages::<T>::kill();
			HrmpOutboundMessages::<T>::kill();
			CustomValidationHeadData::<T>::kill();

			weight += T::DbWeight::get().writes(6);

			// Here, in `on_initialize` we must report the weight for both `on_initialize` and
			// `on_finalize`.
			//
			// One complication here, is that the `host_configuration` is updated by an inherent
			// and those are processed after the block initialization phase. Therefore, we have to
			// be content only with the configuration as per the previous block. That means that
			// the configuration can be either stale (or be abscent altogether in case of the
			// beginning of the chain).
			//
			// In order to mitigate this, we do the following. At the time, we are only concerned
			// about `hrmp_max_message_num_per_candidate`. We reserve the amount of weight to
			// process the number of HRMP messages according to the potentially stale
			// configuration. In `on_finalize` we will process only the maximum between the
			// announced number of messages and the actual received in the fresh configuration.
			//
			// In the common case, they will be the same. In the case the actual value is smaller
			// than the announced, we would waste some of weight. In the case the actual value is
			// greater than the announced, we will miss opportunity to send a couple of messages.
			weight += T::DbWeight::get().reads_writes(1, 1);
			let hrmp_max_message_num_per_candidate = Self::host_configuration()
				.map(|cfg| cfg.hrmp_max_message_num_per_candidate)
				.unwrap_or(0);
			<AnnouncedHrmpMessagesPerCandidate<T>>::put(hrmp_max_message_num_per_candidate);

			// NOTE that the actual weight consumed by `on_finalize` may turn out lower.
			weight += T::DbWeight::get().reads_writes(
				3 + hrmp_max_message_num_per_candidate as u64,
				4 + hrmp_max_message_num_per_candidate as u64,
			);

			weight
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the current validation data.
		///
		/// This should be invoked exactly once per block. It will panic at the finalization
		/// phase if the call was not invoked.
		///
		/// The dispatch origin for this call must be `Inherent`
		///
		/// As a side effect, this function upgrades the current validation function
		/// if the appropriate time has come.
		#[pallet::weight((0, DispatchClass::Mandatory))]
		// TODO: This weight should be corrected.
		pub fn set_validation_data(
			origin: OriginFor<T>,
			data: ParachainInherentData,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;
			assert!(
				!<ValidationData<T>>::exists(),
				"ValidationData must be updated only once in a block",
			);

			let ParachainInherentData {
				validation_data: vfp,
				relay_chain_state,
				downward_messages,
				horizontal_messages,
			} = data;

			Self::validate_validation_data(&vfp);

			let relay_state_proof = RelayChainStateProof::new(
				T::SelfParaId::get(),
				vfp.relay_parent_storage_root,
				relay_chain_state,
			)
			.expect("Invalid relay chain state proof");

			// initialization logic: we know that this runs exactly once every block,
			// which means we can put the initialization logic here to remove the
			// sequencing problem.
			let upgrade_go_ahead_signal = relay_state_proof
				.read_upgrade_go_ahead_signal()
				.expect("Invalid upgrade go ahead signal");
			match upgrade_go_ahead_signal {
				Some(relay_chain::v1::UpgradeGoAhead::GoAhead) => {
					assert!(
						<PendingValidationCode<T>>::exists(),
						"No new validation function found in storage, GoAhead signal is not expected",
					);
					let validation_code = <PendingValidationCode<T>>::take();

					Self::put_parachain_code(&validation_code);
					<T::OnSystemEvent as OnSystemEvent>::on_validation_code_applied();
					Self::deposit_event(Event::ValidationFunctionApplied(vfp.relay_parent_number));
				},
				Some(relay_chain::v1::UpgradeGoAhead::Abort) => {
					<PendingValidationCode<T>>::kill();
					Self::deposit_event(Event::ValidationFunctionDiscarded);
				},
				None => {},
			}
			<UpgradeRestrictionSignal<T>>::put(
				relay_state_proof
					.read_upgrade_restriction_signal()
					.expect("Invalid upgrade restriction signal"),
			);

			let host_config = relay_state_proof
				.read_abridged_host_configuration()
				.expect("Invalid host configuration in relay chain state proof");
			let relevant_messaging_state = relay_state_proof
				.read_messaging_state_snapshot()
				.expect("Invalid messaging state in relay chain state proof");

			<ValidationData<T>>::put(&vfp);
			<RelevantMessagingState<T>>::put(relevant_messaging_state.clone());
			<HostConfiguration<T>>::put(host_config);

			<T::OnSystemEvent as OnSystemEvent>::on_validation_data(&vfp);

			// TODO: This is more than zero, but will need benchmarking to figure out what.
			let mut total_weight = 0;
			total_weight += Self::process_inbound_downward_messages(
				relevant_messaging_state.dmq_mqc_head,
				downward_messages,
			);
			total_weight += Self::process_inbound_horizontal_messages(
				&relevant_messaging_state.ingress_channels,
				horizontal_messages,
				vfp.relay_parent_number,
			);

			Ok(PostDispatchInfo { actual_weight: Some(total_weight), pays_fee: Pays::No })
		}

		#[pallet::weight((1_000, DispatchClass::Operational))]
		pub fn sudo_send_upward_message(
			origin: OriginFor<T>,
			message: UpwardMessage,
		) -> DispatchResult {
			ensure_root(origin)?;
			let _ = Self::send_upward_message(message);
			Ok(())
		}

		#[pallet::weight((1_000_000, DispatchClass::Operational))]
		pub fn authorize_upgrade(origin: OriginFor<T>, code_hash: T::Hash) -> DispatchResult {
			ensure_root(origin)?;

			AuthorizedUpgrade::<T>::put(&code_hash);

			Self::deposit_event(Event::UpgradeAuthorized(code_hash));
			Ok(())
		}

		#[pallet::weight(1_000_000)]
		pub fn enact_authorized_upgrade(
			_: OriginFor<T>,
			code: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			Self::validate_authorized_upgrade(&code[..])?;
			Self::schedule_code_upgrade(code)?;
			AuthorizedUpgrade::<T>::kill();
			Ok(Pays::No.into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The validation function has been scheduled to apply.
		ValidationFunctionStored,
		/// The validation function was applied as of the contained relay chain block number.
		ValidationFunctionApplied(RelayChainBlockNumber),
		/// The relay-chain aborted the upgrade process.
		ValidationFunctionDiscarded,
		/// An upgrade has been authorized.
		UpgradeAuthorized(T::Hash),
		/// Some downward messages have been received and will be processed.
		/// \[ count \]
		DownwardMessagesReceived(u32),
		/// Downward messages were processed using the given weight.
		/// \[ weight_used, result_mqc_head \]
		DownwardMessagesProcessed(Weight, relay_chain::Hash),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Attempt to upgrade validation function while existing upgrade pending
		OverlappingUpgrades,
		/// Polkadot currently prohibits this parachain from upgrading its validation function
		ProhibitedByPolkadot,
		/// The supplied validation function has compiled into a blob larger than Polkadot is
		/// willing to run
		TooBig,
		/// The inherent which supplies the validation data did not run this block
		ValidationDataNotAvailable,
		/// The inherent which supplies the host configuration did not run this block
		HostConfigurationNotAvailable,
		/// No validation function upgrade is currently scheduled.
		NotScheduled,
		/// No code upgrade has been authorized.
		NothingAuthorized,
		/// The given code upgrade has not been authorized.
		Unauthorized,
	}

	/// In case of a scheduled upgrade, this storage field contains the validation code to be applied.
	///
	/// As soon as the relay chain gives us the go-ahead signal, we will overwrite the [`:code`][well_known_keys::CODE]
	/// which will result the next block process with the new validation code. This concludes the upgrade process.
	///
	/// [well_known_keys::CODE]: sp_core::storage::well_known_keys::CODE
	#[pallet::storage]
	#[pallet::getter(fn new_validation_function)]
	pub(super) type PendingValidationCode<T: Config> = StorageValue<_, Vec<u8>, ValueQuery>;

	/// Validation code that is set by the parachain and is to be communicated to collator and
	/// consequently the relay-chain.
	///
	/// This will be cleared in `on_initialize` of each new block if no other pallet already set
	/// the value.
	#[pallet::storage]
	pub(super) type NewValidationCode<T: Config> = StorageValue<_, Vec<u8>, OptionQuery>;

	/// The [`PersistedValidationData`] set for this block.
	/// This value is expected to be set only once per block and it's never stored
	/// in the trie.
	#[pallet::storage]
	#[pallet::getter(fn validation_data)]
	pub(super) type ValidationData<T: Config> = StorageValue<_, PersistedValidationData>;

	/// Were the validation data set to notify the relay chain?
	#[pallet::storage]
	pub(super) type DidSetValidationCode<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// An option which indicates if the relay-chain restricts signalling a validation code upgrade.
	/// In other words, if this is `Some` and [`NewValidationCode`] is `Some` then the produced
	/// candidate will be invalid.
	///
	/// This storage item is a mirror of the corresponding value for the current parachain from the
	/// relay-chain. This value is ephemeral which means it doesn't hit the storage. This value is
	/// set after the inherent.
	#[pallet::storage]
	pub(super) type UpgradeRestrictionSignal<T: Config> =
		StorageValue<_, Option<relay_chain::v1::UpgradeRestriction>, ValueQuery>;

	/// The snapshot of some state related to messaging relevant to the current parachain as per
	/// the relay parent.
	///
	/// This field is meant to be updated each block with the validation data inherent. Therefore,
	/// before processing of the inherent, e.g. in `on_initialize` this data may be stale.
	///
	/// This data is also absent from the genesis.
	#[pallet::storage]
	#[pallet::getter(fn relevant_messaging_state)]
	pub(super) type RelevantMessagingState<T: Config> = StorageValue<_, MessagingStateSnapshot>;

	/// The parachain host configuration that was obtained from the relay parent.
	///
	/// This field is meant to be updated each block with the validation data inherent. Therefore,
	/// before processing of the inherent, e.g. in `on_initialize` this data may be stale.
	///
	/// This data is also absent from the genesis.
	#[pallet::storage]
	#[pallet::getter(fn host_configuration)]
	pub(super) type HostConfiguration<T: Config> = StorageValue<_, AbridgedHostConfiguration>;

	/// The last downward message queue chain head we have observed.
	///
	/// This value is loaded before and saved after processing inbound downward messages carried
	/// by the system inherent.
	#[pallet::storage]
	pub(super) type LastDmqMqcHead<T: Config> = StorageValue<_, MessageQueueChain, ValueQuery>;

	/// The message queue chain heads we have observed per each channel incoming channel.
	///
	/// This value is loaded before and saved after processing inbound downward messages carried
	/// by the system inherent.
	#[pallet::storage]
	pub(super) type LastHrmpMqcHeads<T: Config> =
		StorageValue<_, BTreeMap<ParaId, MessageQueueChain>, ValueQuery>;

	/// Number of downward messages processed in a block.
	///
	/// This will be cleared in `on_initialize` of each new block.
	#[pallet::storage]
	pub(super) type ProcessedDownwardMessages<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// HRMP watermark that was set in a block.
	///
	/// This will be cleared in `on_initialize` of each new block.
	#[pallet::storage]
	pub(super) type HrmpWatermark<T: Config> =
		StorageValue<_, relay_chain::v1::BlockNumber, ValueQuery>;

	/// HRMP messages that were sent in a block.
	///
	/// This will be cleared in `on_initialize` of each new block.
	#[pallet::storage]
	pub(super) type HrmpOutboundMessages<T: Config> =
		StorageValue<_, Vec<OutboundHrmpMessage>, ValueQuery>;

	/// Upward messages that were sent in a block.
	///
	/// This will be cleared in `on_initialize` of each new block.
	#[pallet::storage]
	pub(super) type UpwardMessages<T: Config> = StorageValue<_, Vec<UpwardMessage>, ValueQuery>;

	/// Upward messages that are still pending and not yet send to the relay chain.
	#[pallet::storage]
	pub(super) type PendingUpwardMessages<T: Config> =
		StorageValue<_, Vec<UpwardMessage>, ValueQuery>;

	/// The number of HRMP messages we observed in `on_initialize` and thus used that number for
	/// announcing the weight of `on_initialize` and `on_finalize`.
	#[pallet::storage]
	pub(super) type AnnouncedHrmpMessagesPerCandidate<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The weight we reserve at the beginning of the block for processing XCMP messages. This
	/// overrides the amount set in the Config trait.
	#[pallet::storage]
	pub(super) type ReservedXcmpWeightOverride<T: Config> = StorageValue<_, Weight>;

	/// The weight we reserve at the beginning of the block for processing DMP messages. This
	/// overrides the amount set in the Config trait.
	#[pallet::storage]
	pub(super) type ReservedDmpWeightOverride<T: Config> = StorageValue<_, Weight>;

	/// The next authorized upgrade, if there is one.
	#[pallet::storage]
	pub(super) type AuthorizedUpgrade<T: Config> = StorageValue<_, T::Hash>;

	/// A custom head data that should be returned as result of `validate_block`.
	///
	/// See [`Pallet::set_custom_validation_head_data`] for more information.
	#[pallet::storage]
	pub(super) type CustomValidationHeadData<T: Config> = StorageValue<_, Vec<u8>, OptionQuery>;

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = sp_inherents::MakeFatalError<()>;
		const INHERENT_IDENTIFIER: InherentIdentifier =
			cumulus_primitives_parachain_inherent::INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let data: ParachainInherentData =
				data.get_data(&Self::INHERENT_IDENTIFIER).ok().flatten().expect(
					"validation function params are always injected into inherent data; qed",
				);

			Some(Call::set_validation_data { data })
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::set_validation_data { .. })
		}
	}

	#[pallet::genesis_config]
	#[derive(Default)]
	pub struct GenesisConfig;

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			// TODO: Remove after https://github.com/paritytech/cumulus/issues/479
			sp_io::storage::set(b":c", &[]);
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> sp_runtime::traits::ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::enact_authorized_upgrade { ref code } = call {
				if let Ok(hash) = Self::validate_authorized_upgrade(code) {
					return Ok(ValidTransaction {
						priority: 100,
						requires: Vec::new(),
						provides: vec![hash.as_ref().to_vec()],
						longevity: TransactionLongevity::max_value(),
						propagate: true,
					})
				}
			}
			if let Call::set_validation_data { .. } = call {
				return Ok(Default::default())
			}
			Err(InvalidTransaction::Call.into())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn validate_authorized_upgrade(code: &[u8]) -> Result<T::Hash, DispatchError> {
		let required_hash = AuthorizedUpgrade::<T>::get().ok_or(Error::<T>::NothingAuthorized)?;
		let actual_hash = T::Hashing::hash(&code[..]);
		ensure!(actual_hash == required_hash, Error::<T>::Unauthorized);
		Ok(actual_hash)
	}
}

impl<T: Config> GetChannelInfo for Pallet<T> {
	fn get_channel_status(id: ParaId) -> ChannelStatus {
		// Note, that we are using `relevant_messaging_state` which may be from the previous
		// block, in case this is called from `on_initialize`, i.e. before the inherent with
		// fresh data is submitted.
		//
		// That shouldn't be a problem though because this is anticipated and already can
		// happen. This is because sending implies that a message is buffered until there is
		// space to send a message in the candidate. After a while waiting in a buffer, it may
		// be discovered that the channel to which a message were addressed is now closed.
		// Another possibility, is that the maximum message size was decreased so that a
		// message in the buffer doesn't fit. Should any of that happen the sender should be
		// notified about the message was discarded.
		//
		// Here it a similar case, with the difference that the realization that the channel is
		// closed came the same block.
		let channels = match Self::relevant_messaging_state() {
			None => {
				log::warn!("calling `get_channel_status` with no RelevantMessagingState?!");
				return ChannelStatus::Closed
			},
			Some(d) => d.egress_channels,
		};
		// ^^^ NOTE: This storage field should carry over from the previous block. So if it's
		// None then it must be that this is an edge-case where a message is attempted to be
		// sent at the first block. It should be safe to assume that there are no channels
		// opened at all so early. At least, relying on this assumption seems to be a better
		// tradeoff, compared to introducing an error variant that the clients should be
		// prepared to handle.
		let index = match channels.binary_search_by_key(&id, |item| item.0) {
			Err(_) => return ChannelStatus::Closed,
			Ok(i) => i,
		};
		let meta = &channels[index].1;
		if meta.msg_count + 1 > meta.max_capacity {
			// The channel is at its capacity. Skip it for now.
			return ChannelStatus::Full
		}
		let max_size_now = meta.max_total_size - meta.total_size;
		let max_size_ever = meta.max_message_size;
		ChannelStatus::Ready(max_size_now as usize, max_size_ever as usize)
	}

	fn get_channel_max(id: ParaId) -> Option<usize> {
		let channels = Self::relevant_messaging_state()?.egress_channels;
		let index = channels.binary_search_by_key(&id, |item| item.0).ok()?;
		Some(channels[index].1.max_message_size as usize)
	}
}

impl<T: Config> Pallet<T> {
	/// Validate the given [`PersistedValidationData`] against the
	/// [`ValidationParams`](polkadot_parachain::primitives::ValidationParams).
	///
	/// This check will only be executed when the block is currently being executed in the context
	/// of [`validate_block`]. If this is being executed in the context of block building or block
	/// import, this is a no-op.
	///
	/// # Panics
	///
	/// Panics while validating the `PoV` on the relay chain if the [`PersistedValidationData`]
	/// passed by the block author was incorrect.
	fn validate_validation_data(validation_data: &PersistedValidationData) {
		validate_block::with_validation_params(|params| {
			assert_eq!(
				params.parent_head, validation_data.parent_head,
				"Parent head doesn't match"
			);
			assert_eq!(
				params.relay_parent_number, validation_data.relay_parent_number,
				"Relay parent number doesn't match",
			);
			assert_eq!(
				params.relay_parent_storage_root, validation_data.relay_parent_storage_root,
				"Relay parent storage root doesn't match",
			);
		});
	}

	/// Process all inbound downward messages relayed by the collator.
	///
	/// Checks if the sequence of the messages is valid, dispatches them and communicates the
	/// number of processed messages to the collator via a storage update.
	///
	/// # Panics
	///
	/// If it turns out that after processing all messages the Message Queue Chain
	/// hash doesn't match the expected.
	fn process_inbound_downward_messages(
		expected_dmq_mqc_head: relay_chain::Hash,
		downward_messages: Vec<InboundDownwardMessage>,
	) -> Weight {
		let dm_count = downward_messages.len() as u32;
		let mut dmq_head = <LastDmqMqcHead<T>>::get();

		let mut weight_used = 0;
		if dm_count != 0 {
			Self::deposit_event(Event::DownwardMessagesReceived(dm_count));
			let max_weight =
				<ReservedDmpWeightOverride<T>>::get().unwrap_or_else(T::ReservedDmpWeight::get);

			let message_iter = downward_messages
				.into_iter()
				.inspect(|m| {
					dmq_head.extend_downward(m);
				})
				.map(|m| (m.sent_at, m.msg));
			weight_used += T::DmpMessageHandler::handle_dmp_messages(message_iter, max_weight);
			<LastDmqMqcHead<T>>::put(&dmq_head);

			Self::deposit_event(Event::DownwardMessagesProcessed(weight_used, dmq_head.head()));
		}

		// After hashing each message in the message queue chain submitted by the collator, we
		// should arrive to the MQC head provided by the relay chain.
		//
		// A mismatch means that at least some of the submitted messages were altered, omitted or
		// added improperly.
		assert_eq!(dmq_head.head(), expected_dmq_mqc_head);

		ProcessedDownwardMessages::<T>::put(dm_count);

		weight_used
	}

	/// Process all inbound horizontal messages relayed by the collator.
	///
	/// This is similar to [`process_inbound_downward_messages`], but works on multiple inbound
	/// channels.
	///
	/// **Panics** if either any of horizontal messages submitted by the collator was sent from
	///            a para which has no open channel to this parachain or if after processing
	///            messages across all inbound channels MQCs were obtained which do not
	///            correspond to the ones found on the relay-chain.
	fn process_inbound_horizontal_messages(
		ingress_channels: &[(ParaId, cumulus_primitives_core::AbridgedHrmpChannel)],
		horizontal_messages: BTreeMap<ParaId, Vec<InboundHrmpMessage>>,
		relay_parent_number: relay_chain::v1::BlockNumber,
	) -> Weight {
		// First, check that all submitted messages are sent from channels that exist. The
		// channel exists if its MQC head is present in `vfp.hrmp_mqc_heads`.
		for sender in horizontal_messages.keys() {
			// A violation of the assertion below indicates that one of the messages submitted
			// by the collator was sent from a sender that doesn't have a channel opened to
			// this parachain, according to the relay-parent state.
			assert!(ingress_channels.binary_search_by_key(sender, |&(s, _)| s).is_ok(),);
		}

		// Second, prepare horizontal messages for a more convenient processing:
		//
		// instead of a mapping from a para to a list of inbound HRMP messages, we will have a
		// list of tuples `(sender, message)` first ordered by `sent_at` (the relay chain block
		// number in which the message hit the relay-chain) and second ordered by para id
		// ascending.
		//
		// The messages will be dispatched in this order.
		let mut horizontal_messages = horizontal_messages
			.into_iter()
			.flat_map(|(sender, channel_contents)| {
				channel_contents.into_iter().map(move |message| (sender, message))
			})
			.collect::<Vec<_>>();
		horizontal_messages.sort_by(|a, b| {
			// first sort by sent-at and then by the para id
			match a.1.sent_at.cmp(&b.1.sent_at) {
				cmp::Ordering::Equal => a.0.cmp(&b.0),
				ord => ord,
			}
		});

		let last_mqc_heads = <LastHrmpMqcHeads<T>>::get();
		let mut running_mqc_heads = BTreeMap::new();
		let mut hrmp_watermark = None;

		{
			for (sender, ref horizontal_message) in &horizontal_messages {
				if hrmp_watermark.map(|w| w < horizontal_message.sent_at).unwrap_or(true) {
					hrmp_watermark = Some(horizontal_message.sent_at);
				}

				running_mqc_heads
					.entry(sender)
					.or_insert_with(|| last_mqc_heads.get(&sender).cloned().unwrap_or_default())
					.extend_hrmp(horizontal_message);
			}
		}
		let message_iter = horizontal_messages
			.iter()
			.map(|&(sender, ref message)| (sender, message.sent_at, &message.data[..]));

		let max_weight =
			<ReservedXcmpWeightOverride<T>>::get().unwrap_or_else(T::ReservedXcmpWeight::get);
		let weight_used = T::XcmpMessageHandler::handle_xcmp_messages(message_iter, max_weight);

		// Check that the MQC heads for each channel provided by the relay chain match the MQC
		// heads we have after processing all incoming messages.
		//
		// Along the way we also carry over the relevant entries from the `last_mqc_heads` to
		// `running_mqc_heads`. Otherwise, in a block where no messages were sent in a channel
		// it won't get into next block's `last_mqc_heads` and thus will be all zeros, which
		// would corrupt the message queue chain.
		for &(ref sender, ref channel) in ingress_channels {
			let cur_head = running_mqc_heads
				.entry(sender)
				.or_insert_with(|| last_mqc_heads.get(&sender).cloned().unwrap_or_default())
				.head();
			let target_head = channel.mqc_head.unwrap_or_default();

			assert!(cur_head == target_head);
		}

		<LastHrmpMqcHeads<T>>::put(running_mqc_heads);

		// If we processed at least one message, then advance watermark to that location or if there
		// were no messages, set it to the block number of the relay parent.
		HrmpWatermark::<T>::put(hrmp_watermark.unwrap_or(relay_parent_number));

		weight_used
	}

	/// Put a new validation function into a particular location where polkadot
	/// monitors for updates. Calling this function notifies polkadot that a new
	/// upgrade has been scheduled.
	fn notify_polkadot_of_pending_upgrade(code: &[u8]) {
		NewValidationCode::<T>::put(code);
		<DidSetValidationCode<T>>::put(true);
	}

	/// Put a new validation function into a particular location where this
	/// parachain will execute it on subsequent blocks.
	fn put_parachain_code(code: &[u8]) {
		storage::unhashed::put_raw(sp_core::storage::well_known_keys::CODE, code);
	}

	/// The maximum code size permitted, in bytes.
	///
	/// Returns `None` if the relay chain parachain host configuration hasn't been submitted yet.
	pub fn max_code_size() -> Option<u32> {
		<HostConfiguration<T>>::get().map(|cfg| cfg.max_code_size)
	}

	/// The implementation of the runtime upgrade functionality for parachains.
	pub fn schedule_code_upgrade(validation_function: Vec<u8>) -> DispatchResult {
		// Ensure that `ValidationData` exists. We do not care about the validation data per se,
		// but we do care about the [`UpgradeRestrictionSignal`] which arrives with the same inherent.
		ensure!(<ValidationData<T>>::exists(), Error::<T>::ValidationDataNotAvailable,);
		ensure!(<UpgradeRestrictionSignal<T>>::get().is_none(), Error::<T>::ProhibitedByPolkadot);

		ensure!(!<PendingValidationCode<T>>::exists(), Error::<T>::OverlappingUpgrades);
		let cfg = Self::host_configuration().ok_or(Error::<T>::HostConfigurationNotAvailable)?;
		ensure!(validation_function.len() <= cfg.max_code_size as usize, Error::<T>::TooBig);

		// When a code upgrade is scheduled, it has to be applied in two
		// places, synchronized: both polkadot and the individual parachain
		// have to upgrade on the same relay chain block.
		//
		// `notify_polkadot_of_pending_upgrade` notifies polkadot; the `PendingValidationCode`
		// storage keeps track locally for the parachain upgrade, which will
		// be applied later: when the relay-chain communicates go-ahead signal to us.
		Self::notify_polkadot_of_pending_upgrade(&validation_function);
		<PendingValidationCode<T>>::put(validation_function);
		Self::deposit_event(Event::ValidationFunctionStored);

		Ok(())
	}

	/// Returns the [`CollationInfo`] of the current active block.
	///
	/// The given `header` is the header of the built block we are collecting the collation info for.
	///
	/// This is expected to be used by the
	/// [`CollectCollationInfo`](cumulus_primitives_core::CollectCollationInfo) runtime api.
	pub fn collect_collation_info(header: &T::Header) -> CollationInfo {
		CollationInfo {
			hrmp_watermark: HrmpWatermark::<T>::get(),
			horizontal_messages: HrmpOutboundMessages::<T>::get(),
			upward_messages: UpwardMessages::<T>::get(),
			processed_downward_messages: ProcessedDownwardMessages::<T>::get(),
			new_validation_code: NewValidationCode::<T>::get().map(Into::into),
			// Check if there is a custom header that will also be returned by the validation phase.
			// If so, we need to also return it here.
			head_data: CustomValidationHeadData::<T>::get()
				.map_or_else(|| header.encode(), |v| v)
				.into(),
		}
	}

	/// Set a custom head data that should be returned as result of `validate_block`.
	///
	/// This will overwrite the head data that is returned as result of `validate_block` while
	/// validating a `PoV` on the relay chain. Normally the head data that is being returned
	/// by `validate_block` is the header of the block that is validated, thus it can be
	/// enacted as the new best block. However, for features like forking it can be useful
	/// to overwrite the head data with a custom header.
	///
	/// # Attention
	///
	/// This should only be used when you are sure what you are doing as this can brick
	/// your Parachain.
	pub fn set_custom_validation_head_data(head_data: Vec<u8>) {
		CustomValidationHeadData::<T>::put(head_data);
	}
}

pub struct ParachainSetCode<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> frame_system::SetCode<T> for ParachainSetCode<T> {
	fn set_code(code: Vec<u8>) -> DispatchResult {
		Pallet::<T>::schedule_code_upgrade(code)
	}
}

impl<T: Config> Pallet<T> {
	pub fn send_upward_message(message: UpwardMessage) -> Result<u32, MessageSendError> {
		// Check if the message fits into the relay-chain constraints.
		//
		// Note, that we are using `host_configuration` here which may be from the previous
		// block, in case this is called from `on_initialize`, i.e. before the inherent with fresh
		// data is submitted.
		//
		// That shouldn't be a problem since this is a preliminary check and the actual check would
		// be performed just before submitting the message from the candidate, and it already can
		// happen that during the time the message is buffered for sending the relay-chain setting
		// may change so that the message is no longer valid.
		//
		// However, changing this setting is expected to be rare.
		match Self::host_configuration() {
			Some(cfg) =>
				if message.len() > cfg.max_upward_message_size as usize {
					return Err(MessageSendError::TooBig)
				},
			None => {
				// This storage field should carry over from the previous block. So if it's None
				// then it must be that this is an edge-case where a message is attempted to be
				// sent at the first block.
				//
				// Let's pass this message through. I think it's not unreasonable to expect that
				// the message is not huge and it comes through, but if it doesn't it can be
				// returned back to the sender.
				//
				// Thus fall through here.
			},
		};
		<PendingUpwardMessages<T>>::append(message);
		Ok(0)
	}
}

impl<T: Config> UpwardMessageSender for Pallet<T> {
	fn send_upward_message(message: UpwardMessage) -> Result<u32, MessageSendError> {
		Self::send_upward_message(message)
	}
}

/// Something that can check the inherents of a block.
pub trait CheckInherents<Block: BlockT> {
	/// Check all inherents of the block.
	///
	/// This function gets passed all the extrinsics of the block, so it is up to the callee to
	/// identify the inherents. The `validation_data` can be used to access the
	fn check_inherents(
		block: &Block,
		validation_data: &RelayChainStateProof,
	) -> frame_support::inherent::CheckInherentsResult;
}

/// Something that should be informed about system related events.
///
/// This includes events like [`on_validation_data`](Self::on_validation_data) that is being
/// called when the parachain inherent is executed that contains the validation data.
/// Or like [`on_validation_code_applied`](Self::on_validation_code_applied) that is called
/// when the new validation is written to the state. This means that
/// from the next block the runtime is being using this new code.
#[impl_trait_for_tuples::impl_for_tuples(30)]
pub trait OnSystemEvent {
	/// Called in each blocks once when the validation data is set by the inherent.
	fn on_validation_data(data: &PersistedValidationData);
	/// Called when the validation code is being applied, aka from the next block on this is the new runtime.
	fn on_validation_code_applied();
}

/// Implements [`BlockNumberProvider`] that returns relay chain block number fetched from
/// validation data.
/// NTOE: When validation data is not available (e.g. within on_initialize), 0 will be returned.
pub struct RelaychainBlockNumberProvider<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> BlockNumberProvider for RelaychainBlockNumberProvider<T> {
	type BlockNumber = relay_chain::BlockNumber;

	fn current_block_number() -> relay_chain::BlockNumber {
		Pallet::<T>::validation_data()
			.map(|d| d.relay_parent_number)
			.unwrap_or_default()
	}
}
