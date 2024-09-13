// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Shim for `litep2p::NotificationHandle` to combine `Peerset`-like behavior
//! with `NotificationService`.

use crate::{
	error::Error,
	litep2p::shim::notification::peerset::{OpenResult, Peerset, PeersetNotificationCommand},
	service::{
		metrics::NotificationMetrics,
		traits::{NotificationEvent as SubstrateNotificationEvent, ValidationResult},
	},
	MessageSink, NotificationService, ProtocolName,
};

use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use litep2p::protocol::notification::{
	NotificationEvent, NotificationHandle, NotificationSink,
	ValidationResult as Litep2pValidationResult,
};
use tokio::sync::oneshot;

use sc_network_types::PeerId;

use std::{collections::HashSet, fmt};

pub mod config;
pub mod peerset;

#[cfg(test)]
mod tests;

/// Logging target for the file.
const LOG_TARGET: &str = "sub-libp2p::notification";

/// Wrapper over `litep2p`'s notification sink.
pub struct Litep2pMessageSink {
	/// Protocol.
	protocol: ProtocolName,

	/// Remote peer ID.
	peer: PeerId,

	/// Notification sink.
	sink: NotificationSink,

	/// Notification metrics.
	metrics: NotificationMetrics,
}

impl Litep2pMessageSink {
	/// Create new [`Litep2pMessageSink`].
	fn new(
		peer: PeerId,
		protocol: ProtocolName,
		sink: NotificationSink,
		metrics: NotificationMetrics,
	) -> Self {
		Self { protocol, peer, sink, metrics }
	}
}

#[async_trait::async_trait]
impl MessageSink for Litep2pMessageSink {
	/// Send synchronous `notification` to the peer associated with this [`MessageSink`].
	fn send_sync_notification(&self, notification: Vec<u8>) {
		let size = notification.len();

		match self.sink.send_sync_notification(notification) {
			Ok(_) => self.metrics.register_notification_sent(&self.protocol, size),
			Err(error) => log::trace!(
				target: LOG_TARGET,
				"{}: failed to send sync notification to {:?}: {error:?}",
				self.protocol,
				self.peer,
			),
		}
	}

	/// Send an asynchronous `notification` to to the peer associated with this [`MessageSink`],
	/// allowing sender to exercise backpressure.
	///
	/// Returns an error if the peer does not exist.
	async fn send_async_notification(&self, notification: Vec<u8>) -> Result<(), Error> {
		let size = notification.len();

		match self.sink.send_async_notification(notification).await {
			Ok(_) => {
				self.metrics.register_notification_sent(&self.protocol, size);
				Ok(())
			},
			Err(error) => {
				log::trace!(
					target: LOG_TARGET,
					"{}: failed to send async notification to {:?}: {error:?}",
					self.protocol,
					self.peer,
				);

				Err(Error::Litep2p(error))
			},
		}
	}
}

/// Notification protocol implementation.
pub struct NotificationProtocol {
	/// Protocol name.
	protocol: ProtocolName,

	/// `litep2p` notification handle.
	handle: NotificationHandle,

	/// Peerset for the notification protocol.
	///
	/// Listens to peering-related events and either opens or closes substreams to remote peers.
	peerset: Peerset,

	/// Pending validations for inbound substreams.
	pending_validations: FuturesUnordered<
		BoxFuture<'static, (PeerId, Result<ValidationResult, oneshot::error::RecvError>)>,
	>,

	/// Pending cancels.
	pending_cancels: HashSet<litep2p::PeerId>,

	/// Notification metrics.
	metrics: NotificationMetrics,
}

impl fmt::Debug for NotificationProtocol {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("NotificationProtocol")
			.field("protocol", &self.protocol)
			.field("handle", &self.handle)
			.finish()
	}
}

impl NotificationProtocol {
	/// Create new [`NotificationProtocol`].
	pub fn new(
		protocol: ProtocolName,
		handle: NotificationHandle,
		peerset: Peerset,
		metrics: NotificationMetrics,
	) -> Self {
		Self {
			protocol,
			handle,
			peerset,
			metrics,
			pending_cancels: HashSet::new(),
			pending_validations: FuturesUnordered::new(),
		}
	}

	/// Handle `Peerset` command.
	async fn on_peerset_command(&mut self, command: PeersetNotificationCommand) {
		match command {
			PeersetNotificationCommand::OpenSubstream { peers } => {
				log::debug!(target: LOG_TARGET, "{}: open substreams to {peers:?}", self.protocol);

				let _ = self.handle.open_substream_batch(peers.into_iter().map(From::from)).await;
			},
			PeersetNotificationCommand::CloseSubstream { peers } => {
				log::debug!(target: LOG_TARGET, "{}: close substreams to {peers:?}", self.protocol);

				self.handle.close_substream_batch(peers.into_iter().map(From::from)).await;
			},
		}
	}
}

#[async_trait::async_trait]
impl NotificationService for NotificationProtocol {
	async fn open_substream(&mut self, _peer: PeerId) -> Result<(), ()> {
		unimplemented!();
	}

	async fn close_substream(&mut self, _peer: PeerId) -> Result<(), ()> {
		unimplemented!();
	}

	fn send_sync_notification(&mut self, peer: &PeerId, notification: Vec<u8>) {
		let size = notification.len();

		if let Ok(_) = self.handle.send_sync_notification(peer.into(), notification) {
			self.metrics.register_notification_sent(&self.protocol, size);
		}
	}

	async fn send_async_notification(
		&mut self,
		peer: &PeerId,
		notification: Vec<u8>,
	) -> Result<(), Error> {
		let size = notification.len();

		match self.handle.send_async_notification(peer.into(), notification).await {
			Ok(_) => {
				self.metrics.register_notification_sent(&self.protocol, size);
				Ok(())
			},
			Err(_) => Err(Error::ChannelClosed),
		}
	}

	/// Set handshake for the notification protocol replacing the old handshake.
	async fn set_handshake(&mut self, handshake: Vec<u8>) -> Result<(), ()> {
		self.handle.set_handshake(handshake);

		Ok(())
	}

	/// Set handshake for the notification protocol replacing the old handshake.
	///
	/// For `litep2p` this is identical to `NotificationService::set_handshake()` since `litep2p`
	/// allows updating the handshake synchronously.
	fn try_set_handshake(&mut self, handshake: Vec<u8>) -> Result<(), ()> {
		self.handle.set_handshake(handshake);

		Ok(())
	}

	/// Make a copy of the object so it can be shared between protocol components
	/// who wish to have access to the same underlying notification protocol.
	fn clone(&mut self) -> Result<Box<dyn NotificationService>, ()> {
		unimplemented!("clonable `NotificationService` not supported by `litep2p`");
	}

	/// Get protocol name of the `NotificationService`.
	fn protocol(&self) -> &ProtocolName {
		&self.protocol
	}

	/// Get message sink of the peer.
	fn message_sink(&self, peer: &PeerId) -> Option<Box<dyn MessageSink>> {
		self.handle.notification_sink(peer.into()).map(|sink| {
			let sink: Box<dyn MessageSink> = Box::new(Litep2pMessageSink::new(
				*peer,
				self.protocol.clone(),
				sink,
				self.metrics.clone(),
			));
			sink
		})
	}

	/// Get next event from the `Notifications` event stream.
	async fn next_event(&mut self) -> Option<SubstrateNotificationEvent> {
		loop {
			tokio::select! {
				biased;

				event = self.handle.next() => match event? {
					NotificationEvent::ValidateSubstream { peer, handshake, .. } => {
						if let ValidationResult::Reject = self.peerset.report_inbound_substream(peer.into()) {
							self.handle.send_validation_result(peer, Litep2pValidationResult::Reject);
							continue;
						}

						let (tx, rx) = oneshot::channel();
						self.pending_validations.push(Box::pin(async move { (peer.into(), rx.await) }));

						log::trace!(target: LOG_TARGET, "{}: validate substream for {peer:?}", self.protocol);

						return Some(SubstrateNotificationEvent::ValidateInboundSubstream {
							peer: peer.into(),
							handshake,
							result_tx: tx,
						});
					}
					NotificationEvent::NotificationStreamOpened {
						peer,
						fallback,
						handshake,
						direction,
						..
					} => {
						self.metrics.register_substream_opened(&self.protocol);

						match self.peerset.report_substream_opened(peer.into(), direction.into()) {
							OpenResult::Reject => {
								let _ = self.handle.close_substream_batch(vec![peer].into_iter().map(From::from)).await;
								self.pending_cancels.insert(peer);

								continue
							}
							OpenResult::Accept { direction } => {
								log::trace!(target: LOG_TARGET, "{}: substream opened for {peer:?}", self.protocol);

								return Some(SubstrateNotificationEvent::NotificationStreamOpened {
									peer: peer.into(),
									handshake,
									direction,
									negotiated_fallback: fallback.map(From::from),
								});
							}
						}
					}
					NotificationEvent::NotificationStreamClosed {
						peer,
					} => {
						log::trace!(target: LOG_TARGET, "{}: substream closed for {peer:?}", self.protocol);

						self.metrics.register_substream_closed(&self.protocol);
						self.peerset.report_substream_closed(peer.into());

						if self.pending_cancels.remove(&peer) {
							log::debug!(
								target: LOG_TARGET,
								"{}: substream closed to canceled peer ({peer:?})",
								self.protocol
							);
							continue
						}

						return Some(SubstrateNotificationEvent::NotificationStreamClosed { peer: peer.into() })
					}
					NotificationEvent::NotificationStreamOpenFailure {
						peer,
						error,
					} => {
						log::trace!(target: LOG_TARGET, "{}: open failure for {peer:?}", self.protocol);
						self.peerset.report_substream_open_failure(peer.into(), error);
					}
					NotificationEvent::NotificationReceived {
						peer,
						notification,
					} => {
						self.metrics.register_notification_received(&self.protocol, notification.len());

						if !self.pending_cancels.contains(&peer) {
							return Some(SubstrateNotificationEvent::NotificationReceived {
								peer: peer.into(),
								notification: notification.to_vec(),
							});
						}
					}
				},
				result = self.pending_validations.next(), if !self.pending_validations.is_empty() => {
					let (peer, result) = result?;
					let validation_result = match result {
						Ok(ValidationResult::Accept) => Litep2pValidationResult::Accept,
						_ => {
							self.peerset.report_substream_rejected(peer);
							Litep2pValidationResult::Reject
						}
					};

					self.handle.send_validation_result(peer.into(), validation_result);
				}
				command = self.peerset.next() => self.on_peerset_command(command?).await,
			}
		}
	}
}
