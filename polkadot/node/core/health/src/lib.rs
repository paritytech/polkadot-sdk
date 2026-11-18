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
	overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_util::{
	request_session_index_for_child, request_session_info,
	runtime::{self, recv_runtime},
};
use polkadot_primitives::{AuthorityDiscoveryId, Hash, SessionIndex};
use sp_application_crypto::{AppCrypto, ByteArray};
use sp_keystore::{Keystore, KeystorePtr};
use std::time::Duration;

const LOG_TARGET: &str = "parachain::health";

/// The capacity of the report channel.
const CHANNEL_CAPACITY: usize = 4096;

/// How often the checks judged on time, not on a chain event, are judged.
#[cfg(not(feature = "short-health-tick"))]
const TICK_INTERVAL: Duration = Duration::from_secs(10 * 60);
#[cfg(feature = "short-health-tick")]
const TICK_INTERVAL: Duration = Duration::from_secs(60);

/// The producers' end: cloneable, cheap, and never waits.
/// `None` is a no-op handle. Used for tests, mocks, etc.
#[derive(Clone)]
pub struct HealthHandle(Option<async_channel::Sender<Observation>>);

impl HealthHandle {
	pub fn create_dummy_handle() -> Self {
		Self(None)
	}

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
	keystore: KeystorePtr,
	receiver: HealthReceiver,
}

impl HealthSubsystem {
	pub fn new(keystore: KeystorePtr, receiver: HealthReceiver) -> Self {
		Self { keystore, receiver }
	}
}

#[overseer::subsystem(Health, error = SubsystemError, prefix = self::overseer)]
impl<Context> HealthSubsystem {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = run(ctx, self.keystore, self.receiver).map(|()| Ok(())).boxed();
		SpawnedSubsystem { name: "health-subsystem", future }
	}
}

/// The chain context the subsystem tracks for the checks.
struct State {
	keystore: KeystorePtr,
	/// The last session handed to the checks.
	session: Option<SessionIndex>,
}

#[overseer::contextbounds(Health, prefix = self::overseer)]
async fn run<Context>(mut ctx: Context, keystore: KeystorePtr, mut reports: HealthReceiver) {
	gum::info!(target: LOG_TARGET, capacity = reports.capacity(), "Health subsystem running");

	let mut state = State { keystore, session: None };
	let mut checks = Checks::default();
	let mut tick = futures_timer::Delay::new(TICK_INTERVAL).fuse();

	loop {
		futures::select! {
			from_overseer = ctx.recv().fuse() => match from_overseer {
				Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) | Err(_) => return,
				Ok(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate {
					activated: Some(leaf),
					..
				}))) => if let Err(err) =
					handle_active_leaf(&mut ctx, &mut state, &mut checks, leaf.hash).await
				{
					gum::warn!(target: LOG_TARGET, leaf = ?leaf.hash, ?err, "Failed to process the leaf");
				},
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

/// Track the session from the new leaf, and hand the validators of a new session to the checks.
#[overseer::contextbounds(Health, prefix = self::overseer)]
async fn handle_active_leaf<Context>(
	ctx: &mut Context,
	state: &mut State,
	checks: &mut Checks,
	leaf: Hash,
) -> Result<(), runtime::Error> {
	let session = recv_runtime(request_session_index_for_child(leaf, ctx.sender()).await).await?;

	if state.session.is_some_and(|current| session <= current) {
		return Ok(());
	}

	let Some(info) = recv_runtime(request_session_info(leaf, session, ctx.sender()).await).await?
	else {
		// The session is not remembered: the next leaf retries.
		gum::debug!(target: LOG_TARGET, session, ?leaf, "No session info for the new session");
		return Ok(());
	};
	state.session = Some(session);

	let (ours, others): (Vec<_>, Vec<_>) = info.discovery_keys.into_iter().partition(|key| {
		Keystore::has_keys(&*state.keystore, &[(key.to_raw_vec(), AuthorityDiscoveryId::ID)])
	});
	// Not judged when we are not a validator of the session.
	let validators = (!ours.is_empty()).then(|| others.into_iter().collect());

	checks.on_event(Event::NewSession { validators });

	Ok(())
}
