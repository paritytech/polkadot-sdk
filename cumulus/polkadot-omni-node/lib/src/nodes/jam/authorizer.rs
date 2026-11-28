// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The para's AURA authorizers, resolved per anchor slot from the authority set the runtime
//! names there.

use crate::{
	common::{
		aura::{AuraIdT, AuraRuntimeApi},
		types::JamClient,
		ConstructNodeRuntimeApi, NodeBlock,
	},
	nodes::jam::LOG_TARGET,
};
use codec::{Decode, Encode};
use cumulus_client_jam_authorizer::authorizer::AuraAuthorizer;
use cumulus_jam_aura_primitives::JamAuraApi;
use jam_interface::{ServiceId, Slot as JamSlot};
use sp_api::{ApiExt, CallApiAt, CallApiAtParams, CallContext, ProvideRuntimeApi};
use sp_consensus_aura::AuraApi;
use sp_core::Pair;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::HashMap,
	marker::PhantomData,
	path::PathBuf,
	sync::{Arc, Mutex},
};

/// The public key of an aura authority, as `AuraApi::authorities()` hands it out.
pub(crate) type AuraPublic<AuraId> = <<AuraId as AuraIdT>::BoundedPair as Pair>::Public;

/// Where the collator gets the para's AURA authorizer for an anchor slot.
///
/// JAM authorizes packages with the set it uses at the package's *anchor* slot, and a set change
/// is announced a session ahead. The runtime knows the schedule, so every decision that depends
/// on who may author — signing, the round-robin turn, the pool scan — asks this source for the
/// anchor's set rather than caching one set for the node's life.
pub(crate) trait AuthoritySource<Block: BlockT>: Send + Sync {
	/// The authorizer JAM uses at `anchor_slot`, read from the runtime state at `at`.
	///
	/// `Ok(None)` means this node holds no aura key of that set: it may not author there. `Err`
	/// means the set could not be read or derived, which is a configuration or runtime problem
	/// rather than a turn that simply is not ours.
	fn for_anchor(
		&self,
		at: Block::Hash,
		anchor_slot: JamSlot,
	) -> Result<Option<Arc<AuraAuthorizer>>, String>;
}

/// The authority sets a runtime names, by anchor slot.
///
/// Split out from [`Authorizers`] so the slot-bound selection can be tested without a client.
pub(crate) trait RuntimeAuthorities<Block: BlockT, AuraId: AuraIdT>: Send + Sync {
	/// The set the runtime names for a package anchored at `anchor_slot`, read at `at`.
	fn authorities_at(
		&self,
		at: Block::Hash,
		anchor_slot: JamSlot,
	) -> Result<Vec<AuraPublic<AuraId>>, String>;
}

/// [`RuntimeAuthorities`] backed by the parachain client's runtime API.
pub(crate) struct RuntimeApiAuthorities<Block: BlockT, RuntimeApi, AuraId> {
	client: Arc<JamClient<Block, RuntimeApi>>,
	_phantom: PhantomData<AuraId>,
}

impl<Block, RuntimeApi, AuraId> RuntimeApiAuthorities<Block, RuntimeApi, AuraId>
where
	Block: NodeBlock,
{
	pub(crate) fn new(client: Arc<JamClient<Block, RuntimeApi>>) -> Self {
		Self { client, _phantom: PhantomData }
	}
}

impl<Block, RuntimeApi, AuraId> RuntimeAuthorities<Block, AuraId>
	for RuntimeApiAuthorities<Block, RuntimeApi, AuraId>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, JamClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	fn authorities_at(
		&self,
		at: Block::Hash,
		anchor_slot: JamSlot,
	) -> Result<Vec<AuraPublic<AuraId>>, String> {
		let has_jam_aura_api = self
			.client
			.runtime_api()
			.has_api::<dyn JamAuraApi<Block, AuraPublic<AuraId>>>(at)
			.unwrap_or(false);
		if !has_jam_aura_api {
			// A runtime without the slot-bound API has one set for all slots; that is the test
			// runtime, which composes no session pallet.
			return self
				.client
				.runtime_api()
				.authorities(at)
				.map_err(|error| format!("AuraApi::authorities at {at:?}: {error}"));
		}

		// A direct call with the method name the runtime dispatch table uses; the arguments are
		// the tuple `(anchor_slot,)` encoded exactly as the generated client method encodes them.
		// Calling through `CallApiAt` rather than a trait bound on the node's `RuntimeApi` keeps
		// every runtime the omni-node serves compiling without implementing the new API.
		let raw = self
			.client
			.call_api_at(CallApiAtParams {
				at,
				function: "JamAuraApi_authorities_at",
				arguments: anchor_slot.encode(),
				overlayed_changes: &Default::default(),
				call_context: CallContext::Offchain,
				recorder: &None,
				extensions: &Default::default(),
			})
			.map_err(|error| {
				format!("JamAuraApi::authorities_at({anchor_slot}) at {at:?}: {error}")
			})?;
		Vec::<AuraPublic<AuraId>>::decode(&mut &raw[..]).map_err(|error| {
			format!(
				"JamAuraApi::authorities_at({anchor_slot}) at {at:?} returned undecodable data: {error}"
			)
		})
	}
}

/// Derives and caches the para's authorizers, one per authority set.
///
/// The set comes from the runtime's `JamAuraApi::authorities_at` when it exposes it, and from
/// `AuraApi::authorities` otherwise — a runtime without the slot-bound API only has one set, and
/// falling back keeps it authoring exactly as before. A derived `AuraAuthorizer` is cached by the
/// exact authority list it commits to, so a set change costs one derivation and a node whose key
/// is absent from a set keeps `None` for that set instead of failing every tick.
pub(crate) struct Authorizers<Block: BlockT, AuraId: AuraIdT> {
	source: Arc<dyn RuntimeAuthorities<Block, AuraId>>,
	blob_path: PathBuf,
	para_id: u32,
	service_id: ServiceId,
	slot_duration: u32,
	keystore: KeystorePtr,
	/// Keyed by the SCALE-encoded authority list, which is exactly what the authorizer commits to.
	cache: Mutex<HashMap<Vec<u8>, Option<Arc<AuraAuthorizer>>>>,
}

impl<Block: BlockT, AuraId: AuraIdT> Authorizers<Block, AuraId> {
	pub(crate) fn new(
		source: Arc<dyn RuntimeAuthorities<Block, AuraId>>,
		blob_path: PathBuf,
		para_id: u32,
		service_id: ServiceId,
		slot_duration: u32,
		keystore: KeystorePtr,
	) -> Self {
		Self {
			source,
			blob_path,
			para_id,
			service_id,
			slot_duration,
			keystore,
			cache: Mutex::new(HashMap::new()),
		}
	}
}

impl<Block: BlockT, AuraId: AuraIdT> AuthoritySource<Block> for Authorizers<Block, AuraId> {
	fn for_anchor(
		&self,
		at: Block::Hash,
		anchor_slot: JamSlot,
	) -> Result<Option<Arc<AuraAuthorizer>>, String> {
		let authorities = self.source.authorities_at(at, anchor_slot)?;
		let key = authorities.encode();
		if let Some(cached) =
			self.cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&key)
		{
			return Ok(cached.clone());
		}

		let authorizer = AuraAuthorizer::try_new::<AuraId>(
			&authorities,
			&self.blob_path,
			self.para_id,
			self.service_id,
			self.slot_duration,
			self.keystore.clone(),
		)?
		.map(Arc::new);
		tracing::info!(
			target: LOG_TARGET,
			?at,
			anchor_slot,
			collators = authorities.len(),
			ours = authorizer.as_ref().map(|authorizer| authorizer.own_index()),
			"Resolved the aura authorizer for an anchor slot.",
		);
		self.cache
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
			.insert(key, authorizer.clone());
		Ok(authorizer)
	}
}
