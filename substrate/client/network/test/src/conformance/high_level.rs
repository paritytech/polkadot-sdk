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

use crate::conformance::setup::{connect_backends, create_network_backend};

use sc_network::{Litep2pNetworkBackend, NetworkWorker};

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
