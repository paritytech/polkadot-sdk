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

//! A generic mock availability recovery suitable to be used in benchmarks.

use std::sync::Arc;

use futures::FutureExt;
use polkadot_node_primitives::{AvailableData, BlockData, PoV};
use polkadot_node_subsystem::{
	messages::AvailabilityRecoveryMessage, overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::OverseerSignal;
use polkadot_primitives::{Hash, HeadData, PersistedValidationData};

pub struct MockAvailabilityRecovery {}

impl MockAvailabilityRecovery {
	pub fn new() -> Self {
		Self {}
	}
}

#[overseer::subsystem(AvailabilityRecovery, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockAvailabilityRecovery {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "test-environment", future }
	}
}

#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
impl MockAvailabilityRecovery {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			let msg = ctx.recv().await.expect("Overseer never fails us");
			match msg {
				orchestra::FromOrchestra::Signal(signal) =>
					if signal == OverseerSignal::Conclude {
						return
					},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					AvailabilityRecoveryMessage::RecoverAvailableData(_, _, _, _, tx) => {
						let available_data = AvailableData {
							pov: Arc::new(PoV { block_data: BlockData(Vec::new()) }),
							validation_data: PersistedValidationData {
								parent_head: HeadData(Vec::new()),
								relay_parent_number: 0,
								relay_parent_storage_root: Hash::default(),
								max_pov_size: 2,
							},
						};
						tx.send(Ok(available_data)).unwrap();
					},
				},
			}
		}
	}
}
