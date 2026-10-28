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

//! The health subsystem.
//!
//! The components of the node report observations through a [`HealthHandle`]: a fire-and-forget
//! call that never waits. The subsystem consumes them, judges them against the reference values
//! and produces the output. A producer reports a measurement and does not decide whether it is
//! good or bad.
//!
//! Substrate components stay generic and never report: what they expose is read on the polkadot
//! side.
//!
//! This is the skeleton: the channel, the loop and a placeholder observation.

use futures::{FutureExt, StreamExt};
use polkadot_node_subsystem::{
	overseer, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};

const LOG_TARGET: &str = "parachain::health";

/// The capacity of the report channel. Sized for bursts, not for the average rate.
const CHANNEL_CAPACITY: usize = 4096;

/// One measurement = one variant. Fields are the concrete values.
#[derive(Debug, Clone)]
pub enum Observation {
	/// A placeholder, until the first real producer is wired.
	Dummy { value: u64 },
}

/// The producers' end: cloneable, cheap, and never waits.
#[derive(Clone)]
pub struct HealthHandle(Option<async_channel::Sender<Observation>>);

impl HealthHandle {
	/// Report an observation. On a full or closed channel it is dropped.
	pub fn report(&self, observation: Observation) {
		// TODO: count the drops per observation kind, so that the subsystem can say whose data
		// was lost and mark the checks that consume that kind as unknown for the period.

		if let Some(tx) = &self.0 {
			let _ = tx.try_send(observation);
		}
	}
}

/// The consumer's end
pub type HealthReceiver = async_channel::Receiver<Observation>;

/// Creates a pair of `HealthHandle` and `HealthReceiver`
pub fn health_channel() -> (HealthHandle, HealthReceiver) {
	let (tx, rx) = async_channel::bounded(CHANNEL_CAPACITY);
	(HealthHandle(Some(tx)), rx)
}

pub struct HealthSubsystem {
	receiver: HealthReceiver,
}

impl HealthSubsystem {
	pub fn new(receiver: HealthReceiver) -> Self {
		Self { receiver }
	}
}

#[overseer::subsystem(Health, error = SubsystemError, prefix = self::overseer)]
impl<Context> HealthSubsystem {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = run(ctx, self.receiver).map(|()| Ok(())).boxed();
		SpawnedSubsystem { name: "health-subsystem", future }
	}
}

#[overseer::contextbounds(Health, prefix = self::overseer)]
async fn run<Context>(mut ctx: Context, mut reports: HealthReceiver) {
	gum::info!(target: LOG_TARGET, capacity = reports.capacity(), "Health subsystem running");

	loop {
		futures::select! {
			from_overseer = ctx.recv().fuse() => match from_overseer {
				Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) | Err(_) => return,
				// The chain signals and the messages are of no use until a check needs them.
				Ok(_) => {},
			},
			observation = reports.next() => {
				// The checks that consume the observation will update their state here.
				gum::trace!(target: LOG_TARGET, ?observation, "Applying report");
			},
		}
	}
}
