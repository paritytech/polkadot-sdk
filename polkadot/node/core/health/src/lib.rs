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
//! The components of the node report observations through a [`HealthHandle`], a fire-and-forget
//! call that never waits. The subsystem consumes them, judges them against the reference values
//! and produces the output. A producer reports a measurement and does not decide whether it is
//! good or bad.

mod checks;
mod observation;

pub use observation::Observation;

use checks::{Checks, Event};
use futures::{FutureExt, StreamExt};
use polkadot_node_subsystem::{
	overseer, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};
use std::time::Duration;

const LOG_TARGET: &str = "parachain::health";

/// The capacity of the report channel.
const CHANNEL_CAPACITY: usize = 4096;

/// How often the checks judged on time, not on a chain event, are judged.
const TICK_INTERVAL: Duration = Duration::from_secs(10 * 60);

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

	let mut checks = Checks::default();
	let mut tick = futures_timer::Delay::new(TICK_INTERVAL).fuse();

	loop {
		futures::select! {
			from_overseer = ctx.recv().fuse() => match from_overseer {
				Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) | Err(_) => return,
				Ok(_) => {},
			},
			observation = reports.next() => match observation {
				Some(observation) => checks.on_observation(observation),
				// Every producer dropped its handle: nothing more will arrive.
				None => return,
			},
			_ = tick => {
				checks.on_event(Event::Tick);
				tick = futures_timer::Delay::new(TICK_INTERVAL).fuse();
			},
		}
	}
}
