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

#![allow(unused)]

// See reasoning in Cargo.toml why this temporary useless import is needed.
use tokio as _;

mod common;
mod error;
mod metrics;
mod peer_manager;
mod state;

use std::collections::VecDeque;

use common::MAX_STORED_SCORES_PER_PARA;
use error::{log_error, FatalError, FatalResult, Result};
use fatality::Split;
use peer_manager::{Db, PeerManager};
use polkadot_node_subsystem::{
	overseer, ActivatedLeaf, CollatorProtocolSenderTrait, FromOrchestra, OverseerSignal,
};
use polkadot_node_subsystem_util::{
	find_validator_group, request_claim_queue, request_validator_groups, request_validators,
	runtime::recv_runtime, signing_key_and_index,
};
use polkadot_primitives::{Hash, Id as ParaId};
use sp_keystore::KeystorePtr;
use state::State;

pub use metrics::Metrics;

use crate::LOG_TARGET;

/// The main run loop.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
pub(crate) async fn run<Context>(
	mut ctx: Context,
	keystore: KeystorePtr,
	metrics: Metrics,
) -> FatalResult<()> {
	if let Some(_state) = initialize(&mut ctx, keystore, metrics).await? {
		// run_inner(state);
	}

	Ok(())
}

#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn initialize<Context>(
	ctx: &mut Context,
	keystore: KeystorePtr,
	metrics: Metrics,
) -> FatalResult<Option<State<Db>>> {
	loop {
		let first_leaf = match wait_for_first_leaf(ctx).await? {
			Some(activated_leaf) => activated_leaf,
			None => return Ok(None),
		};

		let scheduled_paras = match scheduled_paras(ctx.sender(), first_leaf.hash, &keystore).await
		{
			Ok(paras) => paras,
			Err(err) => {
				log_error(Err(err))?;
				continue
			},
		};

		let backend = Db::new(MAX_STORED_SCORES_PER_PARA).await;

		match PeerManager::startup(backend, ctx.sender(), scheduled_paras.into_iter().collect())
			.await
		{
			Ok(peer_manager) => return Ok(Some(State::new(peer_manager, keystore, metrics))),
			Err(err) => {
				log_error(Err(err))?;
				continue
			},
		}
	}
}

/// Wait for `ActiveLeavesUpdate`, returns `None` if `Conclude` signal came first.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn wait_for_first_leaf<Context>(ctx: &mut Context) -> FatalResult<Option<ActivatedLeaf>> {
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
				// TODO: we should actually disconnect peers connected on collation protocol while
				// we're still bootstrapping. OR buffer these messages until we've bootstrapped.
				gum::warn!(
					target: LOG_TARGET,
					?msg,
					"Received msg before first active leaves update. This is not expected - message will be dropped."
				)
			},
		}
	}
}

async fn scheduled_paras<Sender: CollatorProtocolSenderTrait>(
	sender: &mut Sender,
	hash: Hash,
	keystore: &KeystorePtr,
) -> Result<VecDeque<ParaId>> {
	let validators = recv_runtime(request_validators(hash, sender).await).await?;

	let (groups, rotation_info) =
		recv_runtime(request_validator_groups(hash, sender).await).await?;

	let core_now = if let Some(group) = signing_key_and_index(&validators, keystore)
		.and_then(|(_, index)| find_validator_group(&groups, index))
	{
		rotation_info.core_for_group(group, groups.len())
	} else {
		gum::trace!(target: LOG_TARGET, ?hash, "Not a validator");
		return Ok(VecDeque::new())
	};

	let mut claim_queue = recv_runtime(request_claim_queue(hash, sender).await).await?;
	Ok(claim_queue.remove(&core_now).unwrap_or_else(|| VecDeque::new()))
}
