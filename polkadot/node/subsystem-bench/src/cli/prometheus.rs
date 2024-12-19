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

pub(crate) fn initialize_prometheus_endpoint(
    port: u16,
    dependencies: &Dependencies, // Assume Dependencies includes `registry` and `task_manager`
) -> Result<()> {
    let registry_clone = dependencies.registry.clone();
    let addr = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), port);

    // Spawn the Prometheus task using the task manager
    dependencies.task_manager.spawn_handle().spawn_blocking(
        "prometheus",
        "test-environment",
        async move {
            if let Err(e) = prometheus_endpoint::init_prometheus(addr, registry_clone).await {
                eprintln!("Failed to initialize Prometheus endpoint at {}: {:?}", addr, e);
            } else {
                println!("Prometheus endpoint initialized at {}.", addr);
            }
        },
    );

    Ok(())
}
