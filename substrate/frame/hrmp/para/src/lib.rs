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

//! # HRMP control-plane pallet (parachain side)
//!
//! The user-facing half of HRMP channel management. It holds the channel deposits and drives the
//! relay chain, which still owns the channels themselves and routes the messages through them.
//!
//! Both directions of that coordination are abstract: requests go out through [`SendToRelay`],
//! verdicts come back in through [`Pallet::receive`], gated by [`Config::RelayOrigin`]. Nothing
//! here depends on XCM or on the relay chain's extrinsics.
//!
//! ## Who may ask
//!
//! Either the para itself, arriving as an origin resolved by [`Config::ParachainOrigin`], or the
//! para's registrar manager as a signed account, or root. The para-origin path is what preserves
//! the trust model HRMP has today: a parachain still speaks for itself, it just retargets its
//! message from the relay chain to this one. The manager path is the recovery route for a para
//! that cannot build the message at all.
//!
//! ## Whose money
//!
//! Deposits are held on the para's **sovereign account on this chain**, resolved through
//! [`Config::SovereignAccountOf`] — not on the caller's own account. That is what the migration
//! produces: relay-chain deposits sit on `para…` sovereign accounts and land on `sibl…` accounts
//! here, so a migrated channel and a freshly opened one are indistinguishable and nothing has to
//! reconcile two shapes.
//!
//! ## Releasing
//!
//! A deposit is released only when the relay chain confirms, never on the request alone. Closing
//! is not atomic once it spans two chains, and handing the money back early would free a para's
//! deposit while its channel still carries messages.
//!
//! ## When a verdict never arrives
//!
//! The transport is assumed to deliver. A message that is genuinely lost leaves a channel parked
//! in one of the in-flight states, and [`Pallet::force_remove_channel`] is the way out — a blunt
//! root call rather than a per-state chase-up protocol, on the grounds that the failure is rare,
//! governance is already the backstop, and a self-healing protocol is far more code than the
//! problem is worth.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{
	defensive, ensure,
	traits::{Consideration, EnsureOrigin, Footprint, Get},
};
use hrmp_primitives::{
	ChannelId, FailureReason, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1,
	OnParaRegistered, Outcome, ParaId, ParaManager,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{BlockNumberProvider, Convert, Saturating},
	DispatchResult,
};

pub use pallet::*;
pub use weights::WeightInfo;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// Block number used for in-flight deadlines.
///
/// On a parachain, configure [`Config::BlockNumberProvider`] to
/// `cumulus_pallet_parachain_system::RelaychainDataProvider`, so deadlines are expressed in
/// relay-chain blocks and keep their meaning through a stall in this chain's own block production.
pub type ProvidedBlockNumberOf<T> =
	<<T as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

/// Used to send an XCM `Transact` to the HRMP pallet on the remote relay chain.
pub trait SendToRelay {
	/// Send `message` to the relay chain.
	///
	/// `Err(())` means the message could not be handed to the transport at all. Callers are
	/// expected to fail the whole extrinsic, so nothing is left half-done.
	#[allow(clippy::result_unit_err)]
	fn send(message: MessageToRelay) -> Result<(), ()>;
}

#[cfg(feature = "std")]
impl SendToRelay for () {
	fn send(_message: MessageToRelay) -> Result<(), ()> {
		Ok(())
	}
}

/// Where a channel sits between the two chains.
///
/// Four of the six states are "a message is in flight": this chain has committed, taken or is
/// holding a deposit, and is waiting for the relay chain to say what happened. Only the relay
/// chain's answer moves a channel out of one.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum ChannelState<BlockNumber> {
	/// The sender has asked the relay chain to record a request. Sender deposit held.
	Opening {
		/// The block from which this may be given up on.
		cancellable_at: BlockNumber,
	},
	/// The relay chain is holding the request, waiting for the recipient. Sender deposit held.
	Pending,
	/// The recipient has asked the relay chain to confirm. Both deposits held.
	Accepting {
		/// The block from which this may be given up on.
		cancellable_at: BlockNumber,
	},
	/// The relay chain has the channel. Both deposits held.
	Open,
	/// One end has asked the relay chain to close. Both deposits still held.
	Closing {
		/// The block from which this may be given up on.
		cancellable_at: BlockNumber,
	},
	/// The sender has asked the relay chain to drop an unconfirmed request. Deposit still held.
	Cancelling {
		/// The block from which this may be given up on.
		cancellable_at: BlockNumber,
	},
}

/// Everything this chain knows about one channel.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct ChannelInfo<Ticket, BlockNumber> {
	/// The sender's deposit, held on its sovereign account here.
	///
	/// `None` for a system channel, which is deposit-free at both ends.
	pub sender_ticket: Option<Ticket>,
	/// The recipient's deposit, held on its sovereign account here. `None` until accepted, and
	/// for a system channel.
	pub recipient_ticket: Option<Ticket>,
	/// Where this channel sits between the two chains.
	pub state: ChannelState<BlockNumber>,
}

/// The [`ChannelInfo`] type as configured.
pub type ChannelInfoOf<T> = ChannelInfo<
	<T as Config>::ChannelConsideration,
	ProvidedBlockNumberOf<T>,
>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::{DispatchResult, *};
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The cost of one end of a channel.
		///
		/// Taken twice per channel, once from each para's sovereign account. The footprint is a
		/// single zero-sized item, so a flat price fits.
		type ChannelConsideration: Consideration<Self::AccountId, Footprint>;

		/// Sends messages to the relay chain.
		type SendToRelay: SendToRelay;

		/// An origin that is sure to be the relay chain's HRMP pallet.
		type RelayOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// An origin a parachain uses to act as itself, resolved to its para id.
		type ParachainOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = ParaId>;

		/// Where to look up who manages a para on this chain.
		///
		/// Normally `pallet-registrar-para`. A para with no manager here can still act through
		/// [`Config::ParachainOrigin`].
		type ParaManager: ParaManager<AccountId = Self::AccountId>;

		/// The sovereign account of a para on this chain.
		///
		/// Deposits are held here rather than on the caller, so a migrated channel and a fresh
		/// one look the same.
		type SovereignAccountOf: Convert<ParaId, Self::AccountId>;

		/// This chain's own para id.
		///
		/// Used for the channel opened with every para this chain registers. A constant rather
		/// than something read from the transport, so the pallet stays testable without XCM.
		#[pallet::constant]
		type SelfParaId: Get<ParaId>;

		/// The largest para id treated as a system chain.
		///
		/// A local mirror of the relay chain's `LOWEST_PUBLIC_ID`, used to decide when a channel
		/// is deposit-free. Ids strictly below this are system chains.
		#[pallet::constant]
		type FirstPublicParaId: Get<ParaId>;

		/// The largest channel capacity the relay chain will accept.
		///
		/// A local mirror of `hrmp_channel_max_capacity`, used to fail early. The relay chain
		/// checks the real thing against its own live configuration.
		#[pallet::constant]
		type MaxCapacity: Get<u32>;

		/// The largest message size the relay chain will accept.
		///
		/// A local mirror of `hrmp_channel_max_message_size`. See [`Config::MaxCapacity`].
		#[pallet::constant]
		type MaxMessageSize: Get<u32>;

		/// How long to wait for the relay chain before a channel counts as stuck.
		///
		/// Measured in [`Config::BlockNumberProvider`] blocks. Nothing acts on it automatically;
		/// it is what [`Pallet::force_remove_channel`] checks, so governance cannot tear down a
		/// channel whose verdict is merely slow.
		#[pallet::constant]
		type PendingDeadline: Get<ProvidedBlockNumberOf<Self>>;

		/// Source of block numbers for in-flight deadlines.
		type BlockNumberProvider: BlockNumberProvider;

		/// Weight information for the extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Hold reasons for runtimes that pay the considerations out of held funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Held for one end of an HRMP channel.
		#[codec(index = 0)]
		Channel,
	}

	/// The id the next message to the relay chain will carry.
	#[pallet::storage]
	pub type NextMessageId<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Every channel this chain knows about, and what is happening with it.
	#[pallet::storage]
	pub type Channels<T: Config> =
		StorageMap<_, Blake2_128Concat, ChannelId, ChannelInfoOf<T>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A channel was requested and the relay chain has been asked to record it.
		OpenRequested { channel: ChannelId, message_id: u64 },
		/// The relay chain is holding the request, waiting for the recipient.
		OpenPending { channel: ChannelId, message_id: u64 },
		/// The relay chain refused the request. The sender's deposit was returned.
		OpenFailed { channel: ChannelId, message_id: u64, reason: FailureReason },
		/// The recipient accepted, and the relay chain has been asked to confirm.
		AcceptRequested { channel: ChannelId, message_id: u64 },
		/// The channel is open on the relay chain. Both deposits are held.
		Opened { channel: ChannelId, message_id: u64 },
		/// The relay chain refused the acceptance. The recipient's deposit was returned.
		AcceptFailed { channel: ChannelId, message_id: u64, reason: FailureReason },
		/// A close was requested. Both deposits stay held until the relay chain answers.
		CloseRequested { channel: ChannelId, message_id: u64, initiator: ParaId },
		/// The channel is gone and both deposits were returned.
		Closed { channel: ChannelId, message_id: u64 },
		/// The relay chain refused the close. The channel is open again.
		CloseFailed { channel: ChannelId, message_id: u64, reason: FailureReason },
		/// The sender asked to withdraw an unconfirmed request.
		CancelRequested { channel: ChannelId, message_id: u64 },
		/// The request is gone and the sender's deposit was returned.
		Cancelled { channel: ChannelId, message_id: u64 },
		/// The relay chain refused the cancellation. The request stands.
		CancelFailed { channel: ChannelId, message_id: u64, reason: FailureReason },
		/// A deposit-free channel was opened in both directions.
		SystemChannelOpened { channel: ChannelId, message_id: u64 },
		/// A channel with a newly registered para could not be opened.
		///
		/// Registration itself succeeded. `establish_system_channel` retries.
		SystemChannelFailed { channel: ChannelId },
		/// Governance removed this chain's record of a channel.
		ChannelForceRemoved { channel: ChannelId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The caller may not act for this para.
		NotOwner,
		/// A channel may not have the same para at both ends.
		ToSelf,
		/// This chain has no record of the channel.
		NoSuchChannel,
		/// A record for this channel already exists.
		AlreadyExists,
		/// The channel is not in a state this call can act on.
		WrongState,
		/// A request for this channel is already in flight with the relay chain.
		RequestInFlight,
		/// The capacity or message size is outside what the relay chain will accept.
		InvalidParameters,
		/// The message could not be handed to the transport.
		SendFailed,
		/// The channel's verdict is not overdue yet.
		NotOverdue,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			// Otherwise no channel could ever be opened.
			assert!(T::MaxCapacity::get() > 0, "MaxCapacity must be positive");
			assert!(T::MaxMessageSize::get() > 0, "MaxMessageSize must be positive");
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Ask the relay chain to record an open-channel request.
		///
		/// Callable by the sending para itself, its registrar manager, or root.
		///
		/// ## Costs
		///
		/// Takes [`Config::ChannelConsideration`] from the **sender's sovereign account on this
		/// chain**, unless either end is a system chain. It is returned if the relay chain refuses
		/// the request, or later when the channel is closed or the request cancelled.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::open_channel())]
		pub fn open_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			recipient: ParaId,
			max_capacity: u32,
			max_message_size: u32,
		) -> DispatchResult {
			Self::ensure_root_para_or_manager(origin, sender)?;
			let channel = ChannelId { sender, recipient };
			ensure!(sender != recipient, Error::<T>::ToSelf);
			ensure!(!Channels::<T>::contains_key(channel), Error::<T>::AlreadyExists);
			ensure!(
				max_capacity > 0 && max_capacity <= T::MaxCapacity::get(),
				Error::<T>::InvalidParameters
			);
			ensure!(
				max_message_size > 0 && max_message_size <= T::MaxMessageSize::get(),
				Error::<T>::InvalidParameters
			);

			let sender_ticket = Self::take_deposit(channel, sender)?;
			let cancellable_at = Self::deadline();
			Channels::<T>::insert(
				channel,
				ChannelInfo {
					sender_ticket,
					recipient_ticket: None,
					state: ChannelState::Opening { cancellable_at },
				},
			);

			// A transport failure returns `Err` and unwinds everything above, ticket included.
			let message_id = Self::next_message_id();
			T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::InitOpenChannel {
				channel,
				message_id,
				max_capacity,
				max_message_size,
			}))
			.map_err(|()| Error::<T>::SendFailed)?;

			Self::deposit_event(Event::OpenRequested { channel, message_id });
			Ok(())
		}

		/// Ask the relay chain to confirm an open-channel request.
		///
		/// Callable by the receiving para itself, its registrar manager, or root. Takes the
		/// recipient's half of the deposit, on the same terms as [`Pallet::open_channel`].
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::accept_open_channel())]
		pub fn accept_open_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			recipient: ParaId,
		) -> DispatchResult {
			Self::ensure_root_para_or_manager(origin, recipient)?;
			let channel = ChannelId { sender, recipient };
			let mut info = Channels::<T>::get(channel).ok_or(Error::<T>::NoSuchChannel)?;
			ensure!(info.state == ChannelState::Pending, Error::<T>::WrongState);

			info.recipient_ticket = Self::take_deposit(channel, recipient)?;
			info.state = ChannelState::Accepting { cancellable_at: Self::deadline() };
			Channels::<T>::insert(channel, info);

			// A transport failure returns `Err` and unwinds everything above, ticket included.
			let message_id = Self::next_message_id();
			T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::AcceptOpenChannel {
				channel,
				message_id,
			}))
			.map_err(|()| Error::<T>::SendFailed)?;

			Self::deposit_event(Event::AcceptRequested { channel, message_id });
			Ok(())
		}

		/// Ask the relay chain to close an open channel.
		///
		/// Callable by **either** end, its registrar manager, or root — the same set the relay
		/// chain accepts today. Nothing is released here: only the relay chain's confirmation
		/// releases the deposits, because a close that is merely requested must not hand the money
		/// back while the channel still carries messages.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::close_channel())]
		pub fn close_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			recipient: ParaId,
		) -> DispatchResult {
			let channel = ChannelId { sender, recipient };
			let initiator = Self::ensure_root_para_or_either_manager(origin, channel)?;
			let mut info = Channels::<T>::get(channel).ok_or(Error::<T>::NoSuchChannel)?;
			ensure!(info.state == ChannelState::Open, Error::<T>::WrongState);

			info.state = ChannelState::Closing { cancellable_at: Self::deadline() };
			Channels::<T>::insert(channel, info);

			// A transport failure returns `Err` and unwinds the state change with it.
			let message_id = Self::next_message_id();
			T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::CloseChannel {
				channel,
				message_id,
				initiator,
			}))
			.map_err(|()| Error::<T>::SendFailed)?;

			Self::deposit_event(Event::CloseRequested { channel, message_id, initiator });
			Ok(())
		}

		/// Ask the relay chain to drop a request the recipient never confirmed.
		///
		/// Callable by the sending para, its registrar manager, or root. As with
		/// [`Pallet::close_channel`], the deposit comes back only on the relay chain's answer.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::cancel_open_request())]
		pub fn cancel_open_request(
			origin: OriginFor<T>,
			sender: ParaId,
			recipient: ParaId,
		) -> DispatchResult {
			Self::ensure_root_para_or_manager(origin, sender)?;
			let channel = ChannelId { sender, recipient };
			let mut info = Channels::<T>::get(channel).ok_or(Error::<T>::NoSuchChannel)?;
			ensure!(info.state == ChannelState::Pending, Error::<T>::WrongState);

			info.state = ChannelState::Cancelling { cancellable_at: Self::deadline() };
			Channels::<T>::insert(channel, info);

			// A transport failure returns `Err` and unwinds the state change with it.
			let message_id = Self::next_message_id();
			T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::CancelOpenRequest {
				channel,
				message_id,
			}))
			.map_err(|()| Error::<T>::SendFailed)?;

			Self::deposit_event(Event::CancelRequested { channel, message_id });
			Ok(())
		}

		/// Accept a report from the relay chain's HRMP pallet.
		///
		/// Not callable by users: the origin must be the relay chain.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::receive())]
		pub fn receive(origin: OriginFor<T>, message: MessageToPara) -> DispatchResult {
			T::RelayOrigin::ensure_origin_or_root(origin)?;

			match message {
				MessageToPara::V1(MessageToParaV1::OpenResponse {
					channel,
					message_id,
					outcome,
				}) => Self::on_open_response(channel, message_id, outcome),
				MessageToPara::V1(MessageToParaV1::AcceptResponse {
					channel,
					message_id,
					outcome,
				}) => Self::on_accept_response(channel, message_id, outcome),
				MessageToPara::V1(MessageToParaV1::CloseResponse {
					channel,
					message_id,
					outcome,
				}) => Self::on_close_response(channel, message_id, outcome),
				MessageToPara::V1(MessageToParaV1::CancelResponse {
					channel,
					message_id,
					outcome,
				}) => Self::on_cancel_response(channel, message_id, outcome),
			}
		}

		/// Open a deposit-free channel in both directions.
		///
		/// Root only. Replaces the relay chain's `establish_system_channel`, which was a signed
		/// call and is filtered once this chain is the control plane — system channels still have
		/// to be openable, and new system chains still have to be onboardable.
		///
		/// Also the retry for a channel this chain failed to open with a newly registered para.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::establish_system_channel())]
		pub fn establish_system_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			recipient: ParaId,
		) -> DispatchResult {
			frame_system::ensure_root(origin)?;
			ensure!(sender != recipient, Error::<T>::ToSelf);

			let message_id = Self::do_establish_system_channel(sender, recipient)?;

			Self::deposit_event(Event::SystemChannelOpened {
				channel: ChannelId { sender, recipient },
				message_id,
			});
			Ok(())
		}

		/// Drop this chain's record of a channel whose verdict never arrived.
		///
		/// Root only, and only once [`Config::PendingDeadline`] has passed, so a verdict that is
		/// merely slow cannot be torn down from under the relay chain.
		///
		/// Deliberately blunt: it releases whatever deposits are held and forgets the channel,
		/// without telling the relay chain anything. The two chains can therefore disagree
		/// afterwards, which is why this is governance's tool and not a user's.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::force_remove_channel())]
		pub fn force_remove_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			recipient: ParaId,
		) -> DispatchResult {
			frame_system::ensure_root(origin)?;
			let channel = ChannelId { sender, recipient };
			let info = Channels::<T>::get(channel).ok_or(Error::<T>::NoSuchChannel)?;

			let overdue = match info.state {
				ChannelState::Opening { cancellable_at } |
				ChannelState::Accepting { cancellable_at } |
				ChannelState::Closing { cancellable_at } |
				ChannelState::Cancelling { cancellable_at } =>
					T::BlockNumberProvider::current_block_number() >= cancellable_at,
				// Nothing is in flight for these, so there is no verdict to be overdue.
				ChannelState::Pending | ChannelState::Open => true,
			};
			ensure!(overdue, Error::<T>::NotOverdue);

			Self::release(info.sender_ticket, sender)?;
			Self::release(info.recipient_ticket, recipient)?;
			Channels::<T>::remove(channel);

			Self::deposit_event(Event::ChannelForceRemoved { channel });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// The footprint one end of a channel is charged for.
	fn channel_footprint() -> Footprint {
		Footprint::from_parts(1, 0)
	}

	/// Whether a channel between these two is deposit-free.
	///
	/// Mirrors the relay chain's rule: a channel with or amongst system chains costs nothing.
	fn is_system(channel: ChannelId) -> bool {
		channel.sender < T::FirstPublicParaId::get() ||
			channel.recipient < T::FirstPublicParaId::get()
	}

	/// Take one end's deposit from that para's sovereign account here.
	fn take_deposit(
		channel: ChannelId,
		para_id: ParaId,
	) -> Result<Option<T::ChannelConsideration>, sp_runtime::DispatchError> {
		if Self::is_system(channel) {
			return Ok(None);
		}
		let who = T::SovereignAccountOf::convert(para_id);
		Ok(Some(T::ChannelConsideration::new(&who, Self::channel_footprint())?))
	}

	/// Give one end's deposit back to that para's sovereign account.
	fn release(ticket: Option<T::ChannelConsideration>, para_id: ParaId) -> DispatchResult {
		if let Some(ticket) = ticket {
			ticket.drop(&T::SovereignAccountOf::convert(para_id))?;
		}
		Ok(())
	}

	/// The block from which an in-flight request counts as overdue.
	fn deadline() -> ProvidedBlockNumberOf<T> {
		T::BlockNumberProvider::current_block_number()
			.saturating_add(T::PendingDeadline::get())
	}

	/// Take the id for the next message to the relay chain.
	fn next_message_id() -> u64 {
		NextMessageId::<T>::mutate(|next| {
			let id = *next;
			*next = next.wrapping_add(1);
			id
		})
	}

	/// Record both directions of a deposit-free channel and ask the relay chain to open them.
	fn do_establish_system_channel(
		sender: ParaId,
		recipient: ParaId,
	) -> Result<u64, sp_runtime::DispatchError> {
		let channel = ChannelId { sender, recipient };
		let back = ChannelId { sender: recipient, recipient: sender };

		// Re-establishing is allowed: a retry after a failed open must be able to make progress,
		// and a system channel holds no deposit to lose by being overwritten.
		for id in [channel, back] {
			Channels::<T>::insert(
				id,
				ChannelInfo {
					sender_ticket: None,
					recipient_ticket: None,
					state: ChannelState::Open,
				},
			);
		}

		// A transport failure returns `Err` and unwinds both records with it.
		let message_id = Self::next_message_id();
		T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::EstablishSystemChannel {
			channel,
			message_id,
		}))
		.map_err(|()| Error::<T>::SendFailed)?;

		Ok(message_id)
	}

	/// Ensure `origin` may act for `para_id`: the para itself, its manager, or root.
	fn ensure_root_para_or_manager(
		origin: frame_system::pallet_prelude::OriginFor<T>,
		para_id: ParaId,
	) -> DispatchResult {
		// The para is tried first: a runtime may deliver a para origin that also reads as a
		// signed one, and the manager branch must not swallow it.
		if let Ok(id) = T::ParachainOrigin::ensure_origin(origin.clone()) {
			ensure!(id == para_id, Error::<T>::NotOwner);
			return Ok(());
		}
		if let Ok(who) = frame_system::ensure_signed(origin.clone()) {
			ensure!(T::ParaManager::manager_of(para_id) == Some(who), Error::<T>::NotOwner);
			return Ok(());
		}
		frame_system::ensure_root(origin)?;
		Ok(())
	}

	/// Ensure `origin` may act for *either* end of `channel`, and say which end it was.
	///
	/// Closing is the one operation both ends may drive, so the relay chain has to be told which
	/// of them asked.
	fn ensure_root_para_or_either_manager(
		origin: frame_system::pallet_prelude::OriginFor<T>,
		channel: ChannelId,
	) -> Result<ParaId, sp_runtime::DispatchError> {
		if let Ok(id) = T::ParachainOrigin::ensure_origin(origin.clone()) {
			ensure!(channel.is_participant(id), Error::<T>::NotOwner);
			return Ok(id);
		}
		if let Ok(who) = frame_system::ensure_signed(origin.clone()) {
			for end in [channel.sender, channel.recipient] {
				if T::ParaManager::manager_of(end) == Some(who.clone()) {
					return Ok(end);
				}
			}
			return Err(Error::<T>::NotOwner.into());
		}
		frame_system::ensure_root(origin)?;
		// Root acts on the channel rather than for an end; name the sender, which is the end the
		// relay chain will accept for either kind of close.
		Ok(channel.sender)
	}

	/// Read a channel that must be in the state a response expects.
	///
	/// A response for a channel this chain is not expecting one for is dropped rather than
	/// treated as a dispatch error: erroring would unwind the whole incoming message for
	/// something that cannot be fixed from here. Unexpected responses still trip a defensive
	/// failure so they are loud in logs, and panic under `debug_assertions`.
	fn expect(
		channel: ChannelId,
		matches: fn(&ChannelState<ProvidedBlockNumberOf<T>>) -> bool,
	) -> Option<ChannelInfoOf<T>> {
		let Some(info) = Channels::<T>::get(channel) else {
			defensive!("hrmp response for unknown channel, dropping");
			return None;
		};
		if !matches(&info.state) {
			defensive!("hrmp response for a channel in the wrong state, dropping");
			return None;
		}
		Some(info)
	}

	/// Apply the relay chain's verdict on an open request.
	fn on_open_response(channel: ChannelId, message_id: u64, outcome: Outcome) -> DispatchResult {
		let Some(mut info) = Self::expect(channel, |s| matches!(s, ChannelState::Opening { .. })) else {
			return Ok(());
		};

		match outcome {
			Ok(()) => {
				info.state = ChannelState::Pending;
				Channels::<T>::insert(channel, info);
				Self::deposit_event(Event::OpenPending { channel, message_id });
			},
			Err(reason) => {
				Self::release(info.sender_ticket, channel.sender)?;
				Channels::<T>::remove(channel);
				Self::deposit_event(Event::OpenFailed { channel, message_id, reason });
			},
		}
		Ok(())
	}

	/// Apply the relay chain's verdict on an acceptance.
	fn on_accept_response(channel: ChannelId, message_id: u64, outcome: Outcome) -> DispatchResult {
		let Some(mut info) = Self::expect(channel, |s| matches!(s, ChannelState::Accepting { .. })) else {
			return Ok(());
		};

		match outcome {
			Ok(()) => {
				info.state = ChannelState::Open;
				Channels::<T>::insert(channel, info);
				Self::deposit_event(Event::Opened { channel, message_id });
			},
			Err(reason) => {
				// Only the recipient's half is returned: the request itself still stands on the
				// relay chain, so the sender's deposit is still owed.
				Self::release(info.recipient_ticket.take(), channel.recipient)?;
				info.state = ChannelState::Pending;
				Channels::<T>::insert(channel, info);
				Self::deposit_event(Event::AcceptFailed { channel, message_id, reason });
			},
		}
		Ok(())
	}

	/// Apply the relay chain's verdict on a close.
	fn on_close_response(channel: ChannelId, message_id: u64, outcome: Outcome) -> DispatchResult {
		let Some(mut info) = Self::expect(channel, |s| matches!(s, ChannelState::Closing { .. })) else {
			return Ok(());
		};

		match outcome {
			Ok(()) => {
				Self::release(info.sender_ticket, channel.sender)?;
				Self::release(info.recipient_ticket, channel.recipient)?;
				Channels::<T>::remove(channel);
				Self::deposit_event(Event::Closed { channel, message_id });
			},
			Err(reason) => {
				info.state = ChannelState::Open;
				Channels::<T>::insert(channel, info);
				Self::deposit_event(Event::CloseFailed { channel, message_id, reason });
			},
		}
		Ok(())
	}

	/// Apply the relay chain's verdict on a cancellation.
	fn on_cancel_response(channel: ChannelId, message_id: u64, outcome: Outcome) -> DispatchResult {
		let Some(mut info) = Self::expect(channel, |s| matches!(s, ChannelState::Cancelling { .. })) else {
			return Ok(());
		};

		match outcome {
			Ok(()) => {
				Self::release(info.sender_ticket, channel.sender)?;
				Channels::<T>::remove(channel);
				Self::deposit_event(Event::Cancelled { channel, message_id });
			},
			Err(reason) => {
				info.state = ChannelState::Pending;
				Channels::<T>::insert(channel, info);
				Self::deposit_event(Event::CancelFailed { channel, message_id, reason });
			},
		}
		Ok(())
	}
}

/// Open a channel with every para this chain registers.
///
/// This chain is the control plane for those paras, so it needs a route to each one — and a para
/// can only `Transact` into this chain if a channel already exists, which is what makes the
/// para-origin path in [`Pallet::open_channel`] reachable at all.
///
/// A failure here must never unwind: the registration has already succeeded by the time this
/// runs, and a channel that could fail it would make onboarding depend on HRMP capacity. Failures
/// are reported as [`Event::SystemChannelFailed`] and retried with
/// [`Pallet::establish_system_channel`].
impl<T: Config> OnParaRegistered for Pallet<T> {
	fn on_registered(para_id: ParaId) {
		let here = T::SelfParaId::get();
		let channel = ChannelId { sender: here, recipient: para_id };

		let opened = frame_support::storage::with_storage_layer(|| {
			Self::do_establish_system_channel(here, para_id)
		});

		match opened {
			Ok(message_id) =>
				Self::deposit_event(Event::SystemChannelOpened { channel, message_id }),
			Err(_) => Self::deposit_event(Event::SystemChannelFailed { channel }),
		}
	}
}
