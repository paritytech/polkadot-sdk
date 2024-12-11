// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use color_eyre::eyre;

/// Show if the app is running under Prometheus monitoring
pub(crate) fn is_prometheus_running() -> bool {
	use std::net::TcpStream;

	let prometheus_address = "127.0.0.1:9999"; // Replace with your Prometheus endpoint
	TcpStream::connect(prometheus_address).is_ok()
}

/// Relaunch the app in Prometheus monitoring mode
pub(crate) fn relaunch_in_prometheus_mode() -> eyre::Result<()> {
	use tokio::runtime::Runtime;

	if is_prometheus_running() {
		println!("Prometheus mode is already active.");
		return Ok(());
	}

	// Initialize Tokio runtime for blocking the future
	let runtime = Runtime::new()?;
	runtime.block_on(async {
		let registry = prometheus::Registry::new();
		let addr =
			std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 9999);

		prometheus_endpoint::init_prometheus(addr, registry).await.map_err(|e| {
			eyre::eyre!("Failed to initialize Prometheus endpoint at {}: {:?}", addr, e)
		})
	})?;

	println!("App relaunched in Prometheus monitoring mode.");
	Ok(())
}
