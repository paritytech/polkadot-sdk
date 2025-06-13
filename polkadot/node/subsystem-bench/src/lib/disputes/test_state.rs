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

use crate::{
	configuration::{TestAuthorities, TestConfiguration},
	network::{HandleNetworkMessage, NetworkMessage},
};

#[derive(Clone)]
pub struct TestState {
	// Full test config
	pub config: TestConfiguration,
	// Authority keys for the network emulation.
	pub test_authorities: TestAuthorities,
}

impl TestState {
	pub fn new(config: &TestConfiguration) -> Self {
		Self { config: config.clone(), test_authorities: config.generate_authorities() }
	}
}

#[async_trait::async_trait]
impl HandleNetworkMessage for TestState {
	async fn handle(
		&self,
		message: NetworkMessage,
		_node_sender: &mut futures::channel::mpsc::UnboundedSender<NetworkMessage>,
	) -> Option<NetworkMessage> {
		match message {
			_ => Some(message),
		}
	}
}
