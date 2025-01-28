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

use crate::conformance::setup::{connect_backends, create_network_backend, NetworkBackendClient};

use sc_network::{
	request_responses::OutgoingResponse, IfDisconnected, Litep2pNetworkBackend, NetworkWorker,
};

#[tokio::test]
async fn check_connectivity() {
	let _ = sp_tracing::tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init();

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
	let _ = sp_tracing::tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init();

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
