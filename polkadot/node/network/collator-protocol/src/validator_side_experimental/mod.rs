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

mod common;
mod error;
mod metrics;
mod peer_manager;
mod state;

use futures::select;

use error::{FatalError, FatalResult, Result};
use peer_manager::{Backend, Db, PeerManager};
use state::State;

use polkadot_node_subsystem::{overseer, ActivatedLeaf, FromOrchestra, OverseerSignal};
use sp_keystore::KeystorePtr;

pub use metrics::Metrics;

use crate::LOG_TARGET;

/// The main run loop.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
pub(crate) async fn run<Context>(
	ctx: Context,
	keystore: KeystorePtr,
	metrics: Metrics,
) -> std::result::Result<(), std::convert::Infallible> {
	let state = initialize(&mut ctx, keystore, metrics).await;

	run_inner(state, ctx).await;

	Ok(())
}

#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn initialize<Context>(
	ctx: &mut Context,
	keystore: KeystorePtr,
	metrics: Metrics,
) -> State<Db> {
	loop {
		let first_leaf = match wait_for_first_leaf(ctx).await {
			Ok(Some(activated_leaf)) => activated_leaf,
			Ok(None) => continue,
			Err(e) => {
				// e.split()?.log();
				continue
			},
		};

		if let Some(peer_manager) = PeerManager::try_init(ctx, first_leaf) {
			return State::new(peer_manager, keystore, metrics)
		}
	}
}

/// Wait for `ActiveLeavesUpdate`, returns `None` if `Conclude` signal came first.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn wait_for_first_leaf<Context>(ctx: &mut Context) -> Result<Option<ActivatedLeaf>> {
	loop {
		match ctx.recv().await.map_err(FatalError::SubsystemReceive)? {
			FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(None),
			FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)) => {
				if let Some(activated) = update.activated {
					return Ok(Some(activated))
				}
			},
			FromOrchestra::Signal(OverseerSignal::BlockFinalized(_, _)) => {},
			FromOrchestra::Communication { msg } => {
				gum::warn!(
					target: LOG_TARGET,
					?msg,
					"Received msg before first active leaves update. This is not expected - message will be dropped."
				)
			},
		}
	}
}

#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn run_inner<Context>(state: State<Db>, mut ctx: Context) {
	loop {
		select! {
			res = ctx.recv().fuse() => {
				match res {
					Ok(FromOrchestra::Communication { msg }) => {
						gum::trace!(target: LOG_TARGET, msg = ?msg, "received a message");
						unimplemented!();
					}
					Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) | Err(_) => break,
					Ok(FromOrchestra::Signal(_)) => continue,
				}
			},
		}
	}

	Ok(())
}
