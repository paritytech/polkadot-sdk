// Copyright (C) Parity Technologies (UK) Ltd.
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

//! `cumulus-pallet-parachain-system` is a base pallet for Cumulus-based parachains.
//!
//! This pallet handles low-level details of being a parachain. Its responsibilities include:
//!
//! - ingestion of the parachain validation data;
//! - ingestion and dispatch of incoming downward and lateral messages;
//! - coordinating upgrades with the Relay Chain; and
//! - communication of parachain outputs, such as sent messages, signaling an upgrade, etc.
//!
//! Users must ensure that they register this pallet as an inherent provider.

use codec::{Decode, Encode, MaxEncodedLen};
use cumulus_primitives_core::{
	relay_chain, AbridgedHostConfiguration, ChannelStatus, CollationInfo, DmpMessageHandler,
	GetChannelInfo, InboundDownwardMessage, InboundHrmpMessage, MessageSendError,
	OutboundHrmpMessage, ParaId, PersistedValidationData, UpwardMessage, UpwardMessageSender,
	XcmpMessageHandler, XcmpMessageSource,
};
use cumulus_primitives_parachain_inherent::{MessageQueueChain, ParachainInherentData};
use frame_support::{
	dispatch::{DispatchResult, Pays, PostDispatchInfo},
	ensure,
	inherent::{InherentData, InherentIdentifier, ProvideInherent},
	storage,
	traits::Get,
	weights::Weight,
};
use frame_system::{ensure_none, ensure_root, pallet_prelude::HeaderFor};
use polkadot_parachain_primitives::primitives::RelayChainBlockNumber;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Block as BlockT, BlockNumberProvider, Hash},
	transaction_validity::{
		InvalidTransaction, TransactionLongevity, TransactionSource, TransactionValidity,
		ValidTransaction,
	},
	DispatchError, RuntimeDebug,
};
use sp_std::{cmp, collections::btree_map::BTreeMap, prelude::*};
use xcm::latest::XcmHash;

pub mod migration;

#[cfg(test)]
mod tests;
mod unincluded_segment;

pub mod consensus_hook;
pub mod relay_state_snapshot;
#[macro_use]
pub mod validate_block;

use unincluded_segment::{
	Ancestor, HrmpChannelUpdate, HrmpWatermarkUpdate, OutboundBandwidthLimits, SegmentTracker,
	UsedBandwidth,
};

pub use consensus_hook::{ConsensusHook, ExpectParentIncluded};
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
pub use relay_state_snapshot::{MessagingStateSnapshot, RelayChainStateProof};

pub use pallet::*;

/// Something that can check the associated relay block number.
///
/// Each Parachain block is built in the context of a relay chain block, this trait allows us
/// to validate the given relay chain block number. With async backing it is legal to build
/// multiple Parachain blocks per relay chain parent. With this trait it is possible for the
/// Parachain to ensure that still only one Parachain block is build per relay chain parent.
///
/// By default [`RelayNumberStrictlyIncreases`] and [`AnyRelayNumber`] are provided.
pub trait CheckAssociatedRelayNumber {
	/// Check the current relay number versus the previous relay number.
	///
	/// The implementation should panic when there is something wrong.
	fn check_associated_relay_number(
		current: RelayChainBlockNumber,
		previous: RelayChainBlockNumber,
	);
}

/// Provides an implementation of [`CheckAssociatedRelayNumber`].
///
/// It will ensure that the associated relay block number strictly increases between Parachain
/// blocks. This should be used by production Parachains when in doubt.
pub struct RelayNumberStrictlyIncreases;

impl CheckAssociatedRelayNumber for RelayNumberStrictlyIncreases {
	fn check_associated_relay_number(
		current: RelayChainBlockNumber,
		previous: RelayChainBlockNumber,
	) {
		if current <= previous {
			panic!("Relay chain block number needs to strictly increase between Parachain blocks!")
		}
	}
}

/// Provides an implementation of [`CheckAssociatedRelayNumber`].
///
/// This will accept any relay chain block number combination. This is mainly useful for
/// test parachains.
pub struct AnyRelayNumber;

impl CheckAssociatedRelayNumber for AnyRelayNumber {
	fn check_associated_relay_number(_: RelayChainBlockNumber, _: RelayChainBlockNumber) {}
}

/// Provides an implementation of [`CheckAssociatedRelayNumber`].
///
/// It will ensure that the associated relay block number monotonically increases between Parachain
/// blocks. This should be used when asynchronous backing is enabled.
pub struct RelayNumberMonotonicallyIncreases;

impl CheckAssociatedRelayNumber for RelayNumberMonotonicallyIncreases {
	fn check_associated_relay_number(
		current: RelayChainBlockNumber,
		previous: RelayChainBlockNumber,
	) {
		if current < previous {
			panic!("Relay chain block number needs to monotonically increase between Parachain blocks!")
		}
	}
}

/// Information needed when a new runtime binary is submitted and needs to be authorized before
/// replacing the current runtime.
#[derive(Decode, Encode, Default, PartialEq, Eq, MaxEncodedLen, TypeInfo)]
#[scale_info(skip_type_params(T))]
struct CodeUpgradeAuthorization<T>
where
	T: Config,
{
	/// Hash of the new runtime binary.
	code_hash: T::Hash,
	/// Whether or not to carry out version checks.
	check_version: bool,
}

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
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

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

		/// Something that can check the associated relay parent block number.
		type CheckAssociatedRelayNumber: CheckAssociatedRelayNumber;

		/// An entry-point for higher-level logic to manage the backlog of unincluded parachain
		/// blocks and authorship rights for those blocks.
		///
		/// Typically, this should be a hook tailored to the collator-selection/consensus mechanism
		/// that is used for this chain.
		///
		/// However, to maintain the same behavior as prior to asynchronous backing, provide the
		/// [`consensus_hook::ExpectParentIncluded`] here. This is only necessary in the case
		/// that collators aren't expected to have node versions that supply the included block
		/// in the relay-chain state proof.
		///
		/// This config type is only available when the `parameterized-consensus-hook` crate feature
		/// is activated.
		#[cfg(feature = "parameterized-consensus-hook")]
		type ConsensusHook: ConsensusHook;
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(_: BlockNumberFor<T>) {
			<DidSetValidationCode<T>>::kill();
			<UpgradeRestrictionSignal<T>>::kill();
			let relay_upgrade_go_ahead = <UpgradeGoAhead<T>>::take();

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

			// Before updating the relevant messaging state, we need to extract
			// the total bandwidth limits for the purpose of updating the unincluded
			// segment.
			let total_bandwidth_out = match Self::relevant_messaging_state() {
				Some(s) => OutboundBandwidthLimits::from_relay_chain_state(&s),
				None => {
					debug_assert!(
						false,
						"relevant messaging state is promised to be set until `on_finalize`; \
							qed",
					);
					return
				},
			};

			// After this point, the `RelevantMessagingState` in storage reflects the
			// unincluded segment.
			Self::adjust_egress_bandwidth_limits();

			let (ump_msg_count, ump_total_bytes) = <PendingUpwardMessages<T>>::mutate(|up| {
				let (available_capacity, available_size) = match Self::relevant_messaging_state() {
					Some(limits) => (
						limits.relay_dispatch_queue_remaining_capacity.remaining_count,
						limits.relay_dispatch_queue_remaining_capacity.remaining_size,
					),
					None => {
						debug_assert!(
							false,
							"relevant messaging state is promised to be set until `on_finalize`; \
								qed",
						);
						return (0, 0)
					},
				};

				let available_capacity =
					cmp::min(available_capacity, host_config.max_upward_message_num_per_candidate);

				// Count the number of messages we can possibly fit in the given constraints, i.e.
				// available_capacity and available_size.
				let (num, total_size) = up
					.iter()
					.scan((0u32, 0u32), |state, msg| {
						let (cap_used, size_used) = *state;
						let new_cap = cap_used.saturating_add(1);
						let new_size = size_used.saturating_add(msg.len() as u32);
						match available_capacity
							.checked_sub(new_cap)
							.and(available_size.checked_sub(new_size))
						{
							Some(_) => {
								*state = (new_cap, new_size);
								Some(*state)
							},
							_ => None,
						}
					})
					.last()
					.unwrap_or_default();

				// TODO: #274 Return back messages that do not longer fit into the queue.

				UpwardMessages::<T>::put(&up[..num as usize]);
				*up = up.split_off(num as usize);

				(num, total_size)
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

			// Note: this internally calls the `GetChannelInfo` implementation for this
			// pallet, which draws on the `RelevantMessagingState`. That in turn has
			// been adjusted above to reflect the correct limits in all channels.
			let outbound_messages =
				T::OutboundXcmpMessageSource::take_outbound_messages(maximum_channels)
					.into_iter()
					.map(|(recipient, data)| OutboundHrmpMessage { recipient, data })
					.collect::<Vec<_>>();

			// Update the unincluded segment length; capacity checks were done previously in
			// `set_validation_data`, so this can be done unconditionally.
			{
				let hrmp_outgoing = outbound_messages
					.iter()
					.map(|msg| {
						(
							msg.recipient,
							HrmpChannelUpdate { msg_count: 1, total_bytes: msg.data.len() as u32 },
						)
					})
					.collect();
				let used_bandwidth =
					UsedBandwidth { ump_msg_count, ump_total_bytes, hrmp_outgoing };

				let mut aggregated_segment =
					AggregatedUnincludedSegment::<T>::get().unwrap_or_default();
				let consumed_go_ahead_signal =
					if aggregated_segment.consumed_go_ahead_signal().is_some() {
						// Some ancestor within the segment already processed this signal --
						// validated during inherent creation.
						None
					} else {
						relay_upgrade_go_ahead
					};
				// The bandwidth constructed was ensured to satisfy relay chain constraints.
				let ancestor = Ancestor::new_unchecked(used_bandwidth, consumed_go_ahead_signal);

				let watermark = HrmpWatermark::<T>::get();
				let watermark_update =
					HrmpWatermarkUpdate::new(watermark, LastRelayChainBlockNumber::<T>::get());

				aggregated_segment
					.append(&ancestor, watermark_update, &total_bandwidth_out)
					.expect("unincluded segment limits exceeded");
				AggregatedUnincludedSegment::<T>::put(aggregated_segment);
				// Check in `on_initialize` guarantees there's space for this block.
				UnincludedSegment::<T>::append(ancestor);
			}
			HrmpOutboundMessages::<T>::put(outbound_messages);
		}

		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			let mut weight = Weight::zero();

			// To prevent removing `NewValidationCode` that was set by another `on_initialize`
			// like for example from scheduler, we only kill the storage entry if it was not yet
			// updated in the current block.
			if !<DidSetValidationCode<T>>::get() {
				NewValidationCode::<T>::kill();
				weight += T::DbWeight::get().writes(1);
			}

			// The parent hash was unknown during block finalization. Update it here.
			{
				<UnincludedSegment<T>>::mutate(|chain| {
					if let Some(ancestor) = chain.last_mut() {
						let parent = frame_system::Pallet::<T>::parent_hash();
						// Ancestor is the latest finalized block, thus current parent is
						// its output head.
						ancestor.replace_para_head_hash(parent);
					}
				});
				weight += T::DbWeight::get().reads_writes(1, 1);

				// Weight used during finalization.
				weight += T::DbWeight::get().reads_writes(3, 2);
			}

			// Remove the validation from the old block.
			ValidationData::<T>::kill();
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

			// Weight for adjusting the unincluded segment in `on_finalize`.
			weight += T::DbWeight::get().reads_writes(6, 3);

			// Always try to read `UpgradeGoAhead` in `on_finalize`.
			weight += T::DbWeight::get().reads(1);

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
		#[pallet::call_index(0)]
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

			// TODO: This is more than zero, but will need benchmarking to figure out what.
			let mut total_weight = Weight::zero();

			// NOTE: the inherent data is expected to be unique, even if this block is built
			// in the context of the same relay parent as the previous one. In particular,
			// the inherent shouldn't contain messages that were already processed by any of the
			// ancestors.
			//
			// This invariant should be upheld by the `ProvideInherent` implementation.
			let ParachainInherentData {
				validation_data: vfp,
				relay_chain_state,
				downward_messages,
				horizontal_messages,
			} = data;

			// Check that the associated relay chain block number is as expected.
			T::CheckAssociatedRelayNumber::check_associated_relay_number(
				vfp.relay_parent_number,
				LastRelayChainBlockNumber::<T>::get(),
			);
			LastRelayChainBlockNumber::<T>::put(vfp.relay_parent_number);

			let relay_state_proof = RelayChainStateProof::new(
				T::SelfParaId::get(),
				vfp.relay_parent_storage_root,
				relay_chain_state.clone(),
			)
			.expect("Invalid relay chain state proof");

			// Update the desired maximum capacity according to the consensus hook.
			#[cfg(feature = "parameterized-consensus-hook")]
			let (consensus_hook_weight, capacity) = T::ConsensusHook::on_state_proof(&relay_state_proof);
			#[cfg(not(feature = "parameterized-consensus-hook"))]
			let (consensus_hook_weight, capacity) = ExpectParentIncluded::on_state_proof(&relay_state_proof);
			total_weight += consensus_hook_weight;
			total_weight += Self::maybe_drop_included_ancestors(&relay_state_proof, capacity);
			// Deposit a log indicating the relay-parent storage root.
			// TODO: remove this in favor of the relay-parent's hash after
			// https://github.com/paritytech/cumulus/issues/303
			frame_system::Pallet::<T>::deposit_log(
				cumulus_primitives_core::rpsr_digest::relay_parent_storage_root_item(
					vfp.relay_parent_storage_root,
					vfp.relay_parent_number,
				),
			);

			// initialization logic: we know that this runs exactly once every block,
			// which means we can put the initialization logic here to remove the
			// sequencing problem.
			let upgrade_go_ahead_signal = relay_state_proof
				.read_upgrade_go_ahead_signal()
				.expect("Invalid upgrade go ahead signal");

			let upgrade_signal_in_segment = AggregatedUnincludedSegment::<T>::get()
				.as_ref()
				.and_then(SegmentTracker::consumed_go_ahead_signal);
			if let Some(signal_in_segment) = upgrade_signal_in_segment.as_ref() {
				// Unincluded ancestor consuming upgrade signal is still within the segment,
				// sanity check that it matches with the signal from relay chain.
				assert_eq!(upgrade_go_ahead_signal, Some(*signal_in_segment));
			}
			match upgrade_go_ahead_signal {
				Some(_signal) if upgrade_signal_in_segment.is_some() => {
					// Do nothing, processing logic was executed by unincluded ancestor.
				},
				Some(relay_chain::UpgradeGoAhead::GoAhead) => {
					assert!(
						<PendingValidationCode<T>>::exists(),
						"No new validation function found in storage, GoAhead signal is not expected",
					);
					let validation_code = <PendingValidationCode<T>>::take();

					Self::put_parachain_code(&validation_code);
					<T::OnSystemEvent as OnSystemEvent>::on_validation_code_applied();
					Self::deposit_event(Event::ValidationFunctionApplied {
						relay_chain_block_num: vfp.relay_parent_number,
					});
				},
				Some(relay_chain::UpgradeGoAhead::Abort) => {
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
			<UpgradeGoAhead<T>>::put(upgrade_go_ahead_signal);

			let host_config = relay_state_proof
				.read_abridged_host_configuration()
				.expect("Invalid host configuration in relay chain state proof");

			let relevant_messaging_state = relay_state_proof
				.read_messaging_state_snapshot(&host_config)
				.expect("Invalid messaging state in relay chain state proof");

			<ValidationData<T>>::put(&vfp);
			<RelayStateProof<T>>::put(relay_chain_state);
			<RelevantMessagingState<T>>::put(relevant_messaging_state.clone());
			<HostConfiguration<T>>::put(host_config);

			<T::OnSystemEvent as OnSystemEvent>::on_validation_data(&vfp);

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

		#[pallet::call_index(1)]
		#[pallet::weight((1_000, DispatchClass::Operational))]
		pub fn sudo_send_upward_message(
			origin: OriginFor<T>,
			message: UpwardMessage,
		) -> DispatchResult {
			ensure_root(origin)?;
			let _ = Self::send_upward_message(message);
			Ok(())
		}

		/// Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied
		/// later.
		///
		/// The `check_version` parameter sets a boolean flag for whether or not the runtime's spec
		/// version and name should be verified on upgrade. Since the authorization only has a hash,
		/// it cannot actually perform the verification.
		///
		/// This call requires Root origin.
		#[pallet::call_index(2)]
		#[pallet::weight((1_000_000, DispatchClass::Operational))]
		pub fn authorize_upgrade(
			origin: OriginFor<T>,
			code_hash: T::Hash,
			check_version: bool,
		) -> DispatchResult {
			ensure_root(origin)?;
			AuthorizedUpgrade::<T>::put(CodeUpgradeAuthorization { code_hash, check_version });

			Self::deposit_event(Event::UpgradeAuthorized { code_hash });
			Ok(())
		}

		/// Provide the preimage (runtime binary) `code` for an upgrade that has been authorized.
		///
		/// If the authorization required a version check, this call will ensure the spec name
		/// remains unchanged and that the spec version has increased.
		///
		/// Note that this function will not apply the new `code`, but only attempt to schedule the
		/// upgrade with the Relay Chain.
		///
		/// All origins are allowed.
		#[pallet::call_index(3)]
		#[pallet::weight({1_000_000})]
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
		ValidationFunctionApplied { relay_chain_block_num: RelayChainBlockNumber },
		/// The relay-chain aborted the upgrade process.
		ValidationFunctionDiscarded,
		/// An upgrade has been authorized.
		UpgradeAuthorized { code_hash: T::Hash },
		/// Some downward messages have been received and will be processed.
		DownwardMessagesReceived { count: u32 },
		/// Downward messages were processed using the given weight.
		DownwardMessagesProcessed { weight_used: Weight, dmq_head: relay_chain::Hash },
		/// An upward message was sent to the relay chain.
		UpwardMessageSent { message_hash: Option<XcmHash> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Attempt to upgrade validation function while existing upgrade pending.
		OverlappingUpgrades,
		/// Polkadot currently prohibits this parachain from upgrading its validation function.
		ProhibitedByPolkadot,
		/// The supplied validation function has compiled into a blob larger than Polkadot is
		/// willing to run.
		TooBig,
		/// The inherent which supplies the validation data did not run this block.
		ValidationDataNotAvailable,
		/// The inherent which supplies the host configuration did not run this block.
		HostConfigurationNotAvailable,
		/// No validation function upgrade is currently scheduled.
		NotScheduled,
		/// No code upgrade has been authorized.
		NothingAuthorized,
		/// The given code upgrade has not been authorized.
		Unauthorized,
	}

	/// Latest included block descendants the runtime accepted. In other words, these are
	/// ancestors of the currently executing block which have not been included in the observed
	/// relay-chain state.
	///
	/// The segment length is limited by the capacity returned from the [`ConsensusHook`] configured
	/// in the pallet.
	#[pallet::storage]
	pub(super) type UnincludedSegment<T: Config> =
		StorageValue<_, Vec<Ancestor<T::Hash>>, ValueQuery>;

	/// Storage field that keeps track of bandwidth used by the unincluded segment along with the
	/// latest the latest HRMP watermark. Used for limiting the acceptance of new blocks with
	/// respect to relay chain constraints.
	#[pallet::storage]
	pub(super) type AggregatedUnincludedSegment<T: Config> =
		StorageValue<_, SegmentTracker<T::Hash>, OptionQuery>;

	/// In case of a scheduled upgrade, this storage field contains the validation code to be
	/// applied.
	///
	/// As soon as the relay chain gives us the go-ahead signal, we will overwrite the
	/// [`:code`][sp_core::storage::well_known_keys::CODE] which will result the next block process
	/// with the new validation code. This concludes the upgrade process.
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

	/// The relay chain block number associated with the last parachain block.
	#[pallet::storage]
	pub(super) type LastRelayChainBlockNumber<T: Config> =
		StorageValue<_, RelayChainBlockNumber, ValueQuery>;

	/// An option which indicates if the relay-chain restricts signalling a validation code upgrade.
	/// In other words, if this is `Some` and [`NewValidationCode`] is `Some` then the produced
	/// candidate will be invalid.
	///
	/// This storage item is a mirror of the corresponding value for the current parachain from the
	/// relay-chain. This value is ephemeral which means it doesn't hit the storage. This value is
	/// set after the inherent.
	#[pallet::storage]
	pub(super) type UpgradeRestrictionSignal<T: Config> =
		StorageValue<_, Option<relay_chain::UpgradeRestriction>, ValueQuery>;

	/// Optional upgrade go-ahead signal from the relay-chain.
	///
	/// This storage item is a mirror of the corresponding value for the current parachain from the
	/// relay-chain. This value is ephemeral which means it doesn't hit the storage. This value is
	/// set after the inherent.
	#[pallet::storage]
	pub(super) type UpgradeGoAhead<T: Config> =
		StorageValue<_, Option<relay_chain::UpgradeGoAhead>, ValueQuery>;

	/// The state proof for the last relay parent block.
	///
	/// This field is meant to be updated each block with the validation data inherent. Therefore,
	/// before processing of the inherent, e.g. in `on_initialize` this data may be stale.
	///
	/// This data is also absent from the genesis.
	#[pallet::storage]
	#[pallet::getter(fn relay_state_proof)]
	pub(super) type RelayStateProof<T: Config> = StorageValue<_, sp_trie::StorageProof>;

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
		StorageValue<_, relay_chain::BlockNumber, ValueQuery>;

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
	pub(super) type AuthorizedUpgrade<T: Config> = StorageValue<_, CodeUpgradeAuthorization<T>>;

	/// A custom head data that should be returned as result of `validate_block`.
	///
	/// See `Pallet::set_custom_validation_head_data` for more information.
	#[pallet::storage]
	pub(super) type CustomValidationHeadData<T: Config> = StorageValue<_, Vec<u8>, OptionQuery>;

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = sp_inherents::MakeFatalError<()>;
		const INHERENT_IDENTIFIER: InherentIdentifier =
			cumulus_primitives_parachain_inherent::INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let mut data: ParachainInherentData =
				data.get_data(&Self::INHERENT_IDENTIFIER).ok().flatten().expect(
					"validation function params are always injected into inherent data; qed",
				);

			Self::drop_processed_messages_from_inherent(&mut data);

			Some(Call::set_validation_data { data })
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::set_validation_data { .. })
		}
	}

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		#[serde(skip)]
		pub _config: sp_std::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
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
		let authorization = AuthorizedUpgrade::<T>::get().ok_or(Error::<T>::NothingAuthorized)?;

		// ensure that the actual hash matches the authorized hash
		let actual_hash = T::Hashing::hash(code);
		ensure!(actual_hash == authorization.code_hash, Error::<T>::Unauthorized);

		// check versions if required as part of the authorization
		if authorization.check_version {
			frame_system::Pallet::<T>::can_set_code(code)?;
		}

		Ok(actual_hash)
	}

	/// Get the unincluded segment size after the given hash.
	///
	/// If the unincluded segment doesn't contain the given hash, this returns the
	/// length of the entire unincluded segment.
	///
	/// This is intended to be used for determining how long the unincluded segment _would be_
	/// in runtime APIs related to authoring.
	pub fn unincluded_segment_size_after(included_hash: T::Hash) -> u32 {
		let segment = UnincludedSegment::<T>::get();
		crate::unincluded_segment::size_after_included(included_hash, &segment)
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
		// trade-off, compared to introducing an error variant that the clients should be
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
	/// Updates inherent data to only contain messages that weren't already processed
	/// by the runtime based on last relay chain block number.
	///
	/// This method doesn't check for mqc heads mismatch.
	fn drop_processed_messages_from_inherent(para_inherent: &mut ParachainInherentData) {
		let ParachainInherentData { downward_messages, horizontal_messages, .. } = para_inherent;

		// Last relay chain block number. Any message with sent-at block number less
		// than or equal to this value is assumed to be processed previously.
		let last_relay_block_number = LastRelayChainBlockNumber::<T>::get();

		// DMQ.
		let dmq_processed_num = downward_messages
			.iter()
			.take_while(|message| message.sent_at <= last_relay_block_number)
			.count();
		downward_messages.drain(..dmq_processed_num);

		// HRMP.
		for horizontal in horizontal_messages.values_mut() {
			let horizontal_processed_num = horizontal
				.iter()
				.take_while(|message| message.sent_at <= last_relay_block_number)
				.count();
			horizontal.drain(..horizontal_processed_num);
		}

		// If MQC doesn't match after dropping messages, the runtime will panic when creating
		// inherent.
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

		let mut weight_used = Weight::zero();
		if dm_count != 0 {
			Self::deposit_event(Event::DownwardMessagesReceived { count: dm_count });
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

			Self::deposit_event(Event::DownwardMessagesProcessed {
				weight_used,
				dmq_head: dmq_head.head(),
			});
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
	/// This is similar to `Pallet::process_inbound_downward_messages`, but works on multiple
	/// inbound channels.
	///
	/// **Panics** if either any of horizontal messages submitted by the collator was sent from
	///            a para which has no open channel to this parachain or if after processing
	///            messages across all inbound channels MQCs were obtained which do not
	///            correspond to the ones found on the relay-chain.
	fn process_inbound_horizontal_messages(
		ingress_channels: &[(ParaId, cumulus_primitives_core::AbridgedHrmpChannel)],
		horizontal_messages: BTreeMap<ParaId, Vec<InboundHrmpMessage>>,
		relay_parent_number: relay_chain::BlockNumber,
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
					.or_insert_with(|| last_mqc_heads.get(sender).cloned().unwrap_or_default())
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
		for (sender, channel) in ingress_channels {
			let cur_head = running_mqc_heads
				.entry(sender)
				.or_insert_with(|| last_mqc_heads.get(sender).cloned().unwrap_or_default())
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

	/// Drop blocks from the unincluded segment with respect to the latest parachain head.
	fn maybe_drop_included_ancestors(
		relay_state_proof: &RelayChainStateProof,
		capacity: consensus_hook::UnincludedSegmentCapacity,
	) -> Weight {
		let mut weight_used = Weight::zero();
		// If the unincluded segment length is nonzero, then the parachain head must be present.
		let para_head =
			relay_state_proof.read_included_para_head().ok().map(|h| T::Hashing::hash(&h.0));

		let unincluded_segment_len = <UnincludedSegment<T>>::decode_len().unwrap_or(0);
		weight_used += T::DbWeight::get().reads(1);

		// Clean up unincluded segment if nonempty.
		let included_head = match (para_head, capacity.is_expecting_included_parent()) {
			(Some(h), true) => {
				assert_eq!(
					h,
					frame_system::Pallet::<T>::parent_hash(),
					"expected parent to be included"
				);

				h
			},
			(Some(h), false) => h,
			(None, true) => {
				// All this logic is essentially a workaround to support collators which
				// might still not provide the included block with the state proof.
				frame_system::Pallet::<T>::parent_hash()
			},
			(None, false) => panic!("included head not present in relay storage proof"),
		};

		let new_len = {
			let para_head_hash = included_head;
			let dropped: Vec<Ancestor<T::Hash>> = <UnincludedSegment<T>>::mutate(|chain| {
				// Drop everything up to (inclusive) the block with an included para head, if
				// present.
				let idx = chain
					.iter()
					.position(|block| {
						let head_hash = block
							.para_head_hash()
							.expect("para head hash is updated during block initialization; qed");
						head_hash == &para_head_hash
					})
					.map_or(0, |idx| idx + 1); // inclusive.

				chain.drain(..idx).collect()
			});
			weight_used += T::DbWeight::get().reads_writes(1, 1);

			let new_len = unincluded_segment_len - dropped.len();
			if !dropped.is_empty() {
				<AggregatedUnincludedSegment<T>>::mutate(|agg| {
					let agg = agg.as_mut().expect(
						"dropped part of the segment wasn't empty, hence value exists; qed",
					);
					for block in dropped {
						agg.subtract(&block);
					}
				});
				weight_used += T::DbWeight::get().reads_writes(1, 1);
			}

			new_len as u32
		};

		// Current block validity check: ensure there is space in the unincluded segment.
		//
		// If this fails, the parachain needs to wait for ancestors to be included before
		// a new block is allowed.
		assert!(new_len < capacity.get(), "no space left for the block in the unincluded segment");
		weight_used
	}

	/// This adjusts the `RelevantMessagingState` according to the bandwidth limits in the
	/// unincluded segment.
	//
	// Reads: 2
	// Writes: 1
	fn adjust_egress_bandwidth_limits() {
		let unincluded_segment = match AggregatedUnincludedSegment::<T>::get() {
			None => return,
			Some(s) => s,
		};

		<RelevantMessagingState<T>>::mutate(|messaging_state| {
			let messaging_state = match messaging_state {
				None => return,
				Some(s) => s,
			};

			let used_bandwidth = unincluded_segment.used_bandwidth();

			let channels = &mut messaging_state.egress_channels;
			for (para_id, used) in used_bandwidth.hrmp_outgoing.iter() {
				let i = match channels.binary_search_by_key(para_id, |item| item.0) {
					Ok(i) => i,
					Err(_) => continue, // indicates channel closed.
				};

				let c = &mut channels[i].1;

				c.total_size = (c.total_size + used.total_bytes).min(c.max_total_size);
				c.msg_count = (c.msg_count + used.msg_count).min(c.max_capacity);
			}

			let upward_capacity = &mut messaging_state.relay_dispatch_queue_remaining_capacity;
			upward_capacity.remaining_count =
				upward_capacity.remaining_count.saturating_sub(used_bandwidth.ump_msg_count);
			upward_capacity.remaining_size =
				upward_capacity.remaining_size.saturating_sub(used_bandwidth.ump_total_bytes);
		});
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
		// but we do care about the [`UpgradeRestrictionSignal`] which arrives with the same
		// inherent.
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
	/// The given `header` is the header of the built block we are collecting the collation info
	/// for.
	///
	/// This is expected to be used by the
	/// [`CollectCollationInfo`](cumulus_primitives_core::CollectCollationInfo) runtime api.
	pub fn collect_collation_info(header: &HeaderFor<T>) -> CollationInfo {
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

	/// Open HRMP channel for using it in benchmarks.
	///
	/// The caller assumes that the pallet will accept regular outbound message to the sibling
	/// `target_parachain` after this call. No other assumptions are made.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn open_outbound_hrmp_channel_for_benchmarks(target_parachain: ParaId) {
		RelevantMessagingState::<T>::put(MessagingStateSnapshot {
			dmq_mqc_head: Default::default(),
			relay_dispatch_queue_remaining_capacity: Default::default(),
			ingress_channels: Default::default(),
			egress_channels: vec![(
				target_parachain,
				cumulus_primitives_core::AbridgedHrmpChannel {
					max_capacity: 10,
					max_total_size: 10_000_000_u32,
					max_message_size: 10_000_000_u32,
					msg_count: 5,
					total_size: 5_000_000_u32,
					mqc_head: None,
				},
			)],
		})
	}

	/// Prepare/insert relevant data for `schedule_code_upgrade` for benchmarks.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn initialize_for_set_code_benchmark(max_code_size: u32) {
		// insert dummy ValidationData
		let vfp = PersistedValidationData {
			parent_head: polkadot_parachain_primitives::primitives::HeadData(Default::default()),
			relay_parent_number: 1,
			relay_parent_storage_root: Default::default(),
			max_pov_size: 1_000,
		};
		<ValidationData<T>>::put(&vfp);

		// insert dummy HostConfiguration with
		let host_config = AbridgedHostConfiguration {
			max_code_size,
			max_head_data_size: 32 * 1024,
			max_upward_queue_count: 8,
			max_upward_queue_size: 1024 * 1024,
			max_upward_message_size: 4 * 1024,
			max_upward_message_num_per_candidate: 2,
			hrmp_max_message_num_per_candidate: 2,
			validation_upgrade_cooldown: 2,
			validation_upgrade_delay: 2,
			async_backing_params: relay_chain::vstaging::AsyncBackingParams {
				allowed_ancestry_len: 0,
				max_candidate_depth: 0,
			},
		};
		<HostConfiguration<T>>::put(host_config);
	}
}

pub struct ParachainSetCode<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> frame_system::SetCode<T> for ParachainSetCode<T> {
	fn set_code(code: Vec<u8>) -> DispatchResult {
		Pallet::<T>::schedule_code_upgrade(code)
	}
}

impl<T: Config> Pallet<T> {
	pub fn send_upward_message(message: UpwardMessage) -> Result<(u32, XcmHash), MessageSendError> {
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
		if let Some(cfg) = Self::host_configuration() {
			if message.len() > cfg.max_upward_message_size as usize {
				return Err(MessageSendError::TooBig)
			}
		} else {
			// This storage field should carry over from the previous block. So if it's None
			// then it must be that this is an edge-case where a message is attempted to be
			// sent at the first block.
			//
			// Let's pass this message through. I think it's not unreasonable to expect that
			// the message is not huge and it comes through, but if it doesn't it can be
			// returned back to the sender.
			//
			// Thus fall through here.
		};
		<PendingUpwardMessages<T>>::append(message.clone());

		// The relay ump does not use using_encoded
		// We apply the same this to use the same hash
		let hash = sp_io::hashing::blake2_256(&message);
		Self::deposit_event(Event::UpwardMessageSent { message_hash: Some(hash) });
		Ok((0, hash))
	}
}

impl<T: Config> UpwardMessageSender for Pallet<T> {
	fn send_upward_message(message: UpwardMessage) -> Result<(u32, XcmHash), MessageSendError> {
		Self::send_upward_message(message)
	}
}

/// Something that can check the inherents of a block.
#[cfg_attr(
	feature = "parameterized-consensus-hook",
	deprecated = "consider switching to `cumulus-pallet-parachain-system::ConsensusHook`"
)]
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

/// Struct that always returns `Ok` on inherents check, needed for backwards-compatibility.
#[doc(hidden)]
pub struct DummyCheckInherents<Block>(sp_std::marker::PhantomData<Block>);

#[allow(deprecated)]
impl<Block: BlockT> CheckInherents<Block> for DummyCheckInherents<Block> {
	fn check_inherents(
		_: &Block,
		_: &RelayChainStateProof,
	) -> frame_support::inherent::CheckInherentsResult {
		sp_inherents::CheckInherentsResult::new()
	}
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
	/// Called when the validation code is being applied, aka from the next block on this is the new
	/// runtime.
	fn on_validation_code_applied();
}

/// Holds the most recent relay-parent state root and block number of the current parachain block.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, Default, RuntimeDebug)]
pub struct RelayChainState {
	/// Current relay chain height.
	pub number: relay_chain::BlockNumber,
	/// State root for current relay chain height.
	pub state_root: relay_chain::Hash,
}

/// This exposes the [`RelayChainState`] to other runtime modules.
///
/// Enables parachains to read relay chain state via state proofs.
pub trait RelaychainStateProvider {
	/// May be called by any runtime module to obtain the current state of the relay chain.
	///
	/// **NOTE**: This is not guaranteed to return monotonically increasing relay parents.
	fn current_relay_chain_state() -> RelayChainState;

	/// Utility function only to be used in benchmarking scenarios, to be implemented optionally,
	/// else a noop.
	///
	/// It allows for setting a custom RelayChainState.
	#[cfg(feature = "runtime-benchmarks")]
	fn set_current_relay_chain_state(_state: RelayChainState) {}
}

/// Implements [`BlockNumberProvider`] that returns relay chain block number fetched from validation
/// data. When validation data is not available (e.g. within on_initialize), 0 will be returned.
///
/// **NOTE**: This has been deprecated, please use [`RelaychainDataProvider`]
#[deprecated = "Use `RelaychainDataProvider` instead"]
pub struct RelaychainBlockNumberProvider<T>(sp_std::marker::PhantomData<T>);

#[allow(deprecated)]
impl<T: Config> BlockNumberProvider for RelaychainBlockNumberProvider<T> {
	type BlockNumber = relay_chain::BlockNumber;

	fn current_block_number() -> relay_chain::BlockNumber {
		Pallet::<T>::validation_data()
			.map(|d| d.relay_parent_number)
			.unwrap_or_default()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_block_number(block: Self::BlockNumber) {
		let mut validation_data = Pallet::<T>::validation_data().unwrap_or_else(||
			// PersistedValidationData does not impl default in non-std
			PersistedValidationData {
				parent_head: vec![].into(),
				relay_parent_number: Default::default(),
				max_pov_size: Default::default(),
				relay_parent_storage_root: Default::default(),
			});
		validation_data.relay_parent_number = block;
		ValidationData::<T>::put(validation_data)
	}
}

impl<T: Config> RelaychainStateProvider for RelaychainDataProvider<T> {
	fn current_relay_chain_state() -> RelayChainState {
		Pallet::<T>::validation_data()
			.map(|d| RelayChainState {
				number: d.relay_parent_number,
				state_root: d.relay_parent_storage_root,
			})
			.unwrap_or_default()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_current_relay_chain_state(state: RelayChainState) {
		let mut validation_data = Pallet::<T>::validation_data().unwrap_or_else(||
			// PersistedValidationData does not impl default in non-std
			PersistedValidationData {
				parent_head: vec![].into(),
				relay_parent_number: Default::default(),
				max_pov_size: Default::default(),
				relay_parent_storage_root: Default::default(),
			});
		validation_data.relay_parent_number = state.number;
		validation_data.relay_parent_storage_root = state.state_root;
		ValidationData::<T>::put(validation_data)
	}
}

/// Implements [`BlockNumberProvider`] and [`RelaychainStateProvider`] that returns relevant relay
/// data fetched from validation data.
/// NOTE: When validation data is not available (e.g. within on_initialize), default values will be
/// returned.
pub struct RelaychainDataProvider<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> BlockNumberProvider for RelaychainDataProvider<T> {
	type BlockNumber = relay_chain::BlockNumber;

	fn current_block_number() -> relay_chain::BlockNumber {
		Pallet::<T>::validation_data()
			.map(|d| d.relay_parent_number)
			.unwrap_or_default()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_block_number(block: Self::BlockNumber) {
		let mut validation_data = Pallet::<T>::validation_data().unwrap_or_else(||
			// PersistedValidationData does not impl default in non-std
			PersistedValidationData {
				parent_head: vec![].into(),
				relay_parent_number: Default::default(),
				max_pov_size: Default::default(),
				relay_parent_storage_root: Default::default(),
			});
		validation_data.relay_parent_number = block;
		ValidationData::<T>::put(validation_data)
	}
}
