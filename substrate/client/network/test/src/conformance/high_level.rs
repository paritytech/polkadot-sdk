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

use crate::conformance::setup::{
	connect_backends, connect_notifications, create_network_backend, NetworkBackendClient,
};

use sc_network::{
	request_responses::OutgoingResponse, service::traits::NotificationEvent, IfDisconnected,
	Litep2pNetworkBackend, NetworkWorker,
};

#[tokio::test]
async fn check_connectivity() {
	// Libp2p dials litep2p.
	connect_backends(
		&create_network_backend::<NetworkWorker<_, _>>(),
		&create_network_backend::<Litep2pNetworkBackend>(),
	)
	.await;

	// Litep2p dials libp2p.
	connect_backends(
		&create_network_backend::<Litep2pNetworkBackend>(),
		&create_network_backend::<NetworkWorker<_, _>>(),
	)
	.await;
}

#[tokio::test]
async fn check_request_response() {
	async fn inner_check_request_response(left: NetworkBackendClient, right: NetworkBackendClient) {
		connect_backends(&left, &right).await;

		let rx = right.receiver.clone();
		tokio::spawn(async move {
			while let Ok(request) = rx.recv().await {
				request
					.pending_response
					.send(OutgoingResponse {
						result: Ok(request.payload),
						reputation_changes: vec![],
						sent_feedback: None,
					})
					.expect("Valid response; qed");
			}
		});

		let channels = (0..32)
			.map(|i| {
				let (tx, rx) = futures::channel::oneshot::channel();
				left.network_service.start_request(
					right.network_service.local_peer_id().into(),
					"/request-response/1".into(),
					vec![1, 2, 3, i],
					None,
					tx,
					IfDisconnected::ImmediateError,
				);

				(i, rx)
			})
			.collect::<Vec<_>>();

		for (id, channel) in channels {
			let response = channel
				.await
				.expect("Channel should not be closed")
				.expect(format!("Channel {} should have a response", id).as_str());
			assert_eq!(response.0, vec![1, 2, 3, id]);
		}
	}

	inner_check_request_response(
		create_network_backend::<NetworkWorker<_, _>>(),
		create_network_backend::<Litep2pNetworkBackend>(),
	)
	.await;

	inner_check_request_response(
		create_network_backend::<Litep2pNetworkBackend>(),
		create_network_backend::<NetworkWorker<_, _>>(),
	)
	.await;
}

#[tokio::test]
async fn check_notifications() {
	async fn inner_check_notifications(left: NetworkBackendClient, right: NetworkBackendClient) {
		const MAX_NOTIFICATIONS: usize = 128;
		connect_notifications(&left, &right).await;

		let right_peer = right.network_service.local_peer_id();
		let (tx, rx) = async_channel::bounded(1);

		tokio::spawn(async move {
			let mut notifications_left = left.notification_service.lock().await;
			for _ in 0..MAX_NOTIFICATIONS {
				notifications_left
					.send_async_notification(&right_peer, vec![1, 2, 3])
					.await
					.expect("qed; cannot fail");
			}
			let _ = rx.recv().await;
		});

		let mut notifications_right = right.notification_service.lock().await;
		let mut notification_index = 0;
		while let Some(event) = notifications_right.next_event().await {
			match event {
				NotificationEvent::NotificationReceived { notification, .. } => {
					notification_index += 1;

					if notification_index >= MAX_NOTIFICATIONS {
						let _ = tx.send(()).await;
						break;
					}

					assert_eq!(notification, vec![1, 2, 3]);
				},
				_ => {},
			}
		}
	}

	// Check libp2p -> litep2p.
	inner_check_notifications(
		create_network_backend::<NetworkWorker<_, _>>(),
		create_network_backend::<Litep2pNetworkBackend>(),
	)
	.await;

	// Check litep2p -> libp2p.
	inner_check_notifications(
		create_network_backend::<Litep2pNetworkBackend>(),
		create_network_backend::<NetworkWorker<_, _>>(),
	)
	.await;
}

#[tokio::test]
async fn check_notifications_ping_pong() {
	async fn inner_check_notifications_ping_pong(
		left: NetworkBackendClient,
		right: NetworkBackendClient,
	) {
		const MAX_NOTIFICATIONS: usize = 128;
		connect_notifications(&left, &right).await;

		let left_peer = left.network_service.local_peer_id();
		let right_peer = right.network_service.local_peer_id();

		let mut notification_index = 0;
		tokio::spawn(async move {
			let mut notifications_left = left.notification_service.lock().await;

			notifications_left
				.send_async_notification(&right_peer, vec![1, 2, 3])
				.await
				.expect("qed; cannot fail");

			while let Some(event) = notifications_left.next_event().await {
				match event {
					NotificationEvent::NotificationReceived { notification, .. } => {
						assert_eq!(notification, vec![1, 2, 3, 4, 5]);

						notification_index += 1;

						if notification_index >= MAX_NOTIFICATIONS {
							break;
						}

						notifications_left
							.send_async_notification(&right_peer, vec![1, 2, 3])
							.await
							.expect("qed; cannot fail");
					},
					_ => {},
				}
			}

			for _ in 0..MAX_NOTIFICATIONS {}
		});

		let mut notifications_right = right.notification_service.lock().await;
		let mut notification_index = 0;
		while let Some(event) = notifications_right.next_event().await {
			match event {
				NotificationEvent::NotificationReceived { notification, .. } => {
					assert_eq!(notification, vec![1, 2, 3]);

					notification_index += 1;

					if notification_index >= MAX_NOTIFICATIONS {
						break;
					}

					notifications_right
						.send_async_notification(&left_peer, vec![1, 2, 3, 4, 5])
						.await
						.expect("qed; cannot fail");
				},
				_ => {},
			}
		}
	}

	// Check libp2p -> litep2p.
	inner_check_notifications_ping_pong(
		create_network_backend::<NetworkWorker<_, _>>(),
		create_network_backend::<Litep2pNetworkBackend>(),
	)
	.await;

	// Check litep2p -> libp2p.
	inner_check_notifications_ping_pong(
		create_network_backend::<Litep2pNetworkBackend>(),
		create_network_backend::<NetworkWorker<_, _>>(),
	)
	.await;
}
