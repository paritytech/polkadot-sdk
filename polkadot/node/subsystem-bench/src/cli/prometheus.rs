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

use std::sync::atomic::{AtomicBool, Ordering};
use color_eyre::eyre;

static PROMETHEUS_RUNNING: AtomicBool = AtomicBool::new(false);

/// Show if the app is running under Prometheus monitoring
pub(crate) fn is_prometheus_running() -> bool {
    PROMETHEUS_RUNNING.load(Ordering::SeqCst)
}

/// Relaunch the app in Prometheus monitoring mode
pub(crate) fn relaunch_in_prometheus_mode() -> eyre::Result<()> {
    if is_prometheus_running() {
        println!("Prometheus mode is already active.");
        return Ok(());
    }

    // Mark Prometheus as running
    PROMETHEUS_RUNNING.store(true, Ordering::SeqCst);

    // Start Prometheus monitoring on a predefined address
    let registry = prometheus::Registry::new();
    tokio::spawn(async move {
        if let Err(e) = prometheus_endpoint::init_prometheus(
            std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 9999),
            registry,
        )
        .await
        {
            eprintln!("Failed to initialize Prometheus endpoint: {:?}", e);
        }
    });

    println!("App relaunched in Prometheus monitoring mode.");
    Ok(())
}