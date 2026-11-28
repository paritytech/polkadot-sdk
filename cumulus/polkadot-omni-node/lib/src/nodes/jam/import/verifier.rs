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

//! The JAM import queue: seals are checked against the set JAM authorizes at the block's anchor.

use crate::{
	common::{
		aura::{AuraIdT, AuraRuntimeApi},
		spec::BuildImportQueue,
		types::{JamClient, JamExecutor, ParachainBlockImport},
		ConstructNodeRuntimeApi, NodeBlock,
	},
	nodes::jam::authorizer::{AuraPublic, RuntimeApiAuthorities, RuntimeAuthorities},
};
use cumulus_client_consensus_aura::equivocation_import_queue::Verifier as EquivocationVerifier;
use cumulus_jam_aura_primitives::JamAuraApi;
use cumulus_primitives_core::CumulusDigestItem;
use sc_consensus::{
	import_queue::{BasicQueue, Verifier as VerifierT},
	BlockImportParams, DefaultImportQueue,
};
use sc_service::{Configuration, TaskManager};
use sc_telemetry::TelemetryHandle;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_consensus_aura::AuraApi;
use sp_inherents::CreateInherentDataProviders;
use sp_runtime::{
	app_crypto::AppCrypto,
	traits::{Block as BlockT, Header as HeaderT},
};
use std::{marker::PhantomData, sync::Arc};

/// Checks a JAM block's seal against the set JAM authorizes at its anchor slot, not the set of
/// its parent: the next set is announced a session ahead, so the first block anchored at a switch
/// slot is already sealed by the new set.
///
/// A block without a `JamParent` digest, or on a runtime without the slot-bound
/// [`JamAuraApi`], goes to `fallback` (the plain Aura verifier).
pub(crate) struct JamSealVerifier<Block: BlockT, Client, AuraId: AuraIdT, CIDP> {
	client: Arc<Client>,
	authorities: Arc<dyn RuntimeAuthorities<Block, AuraId>>,
	fallback: Box<dyn VerifierT<Block>>,
	create_inherent_data_providers: CIDP,
}

impl<Block, Client, AuraId, CIDP> JamSealVerifier<Block, Client, AuraId, CIDP>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + Send + Sync,
	Client::Api: AuraApi<Block, AuraPublic<AuraId>> + sp_block_builder::BlockBuilder<Block>,
	AuraId: AuraIdT + Sync,
	CIDP: CreateInherentDataProviders<Block, ()>,
{
	async fn verify_jam(
		&self,
		mut params: BlockImportParams<Block>,
		anchor_slot: u32,
	) -> Result<BlockImportParams<Block>, String> {
		if params.state_action.skip_execution_checks() || params.with_state() {
			params.fork_choice =
				Some(sc_consensus::ForkChoiceStrategy::Custom(params.with_state()));
			return Ok(params);
		}

		let post_hash = params.header.hash();
		let parent_hash = *params.header.parent_hash();
		let authorities = self.authorities.authorities_at(parent_hash, anchor_slot)?;
		let slot_duration = self
			.client
			.runtime_api()
			.slot_duration(parent_hash)
			.map_err(|e| format!("slot duration at {parent_hash:?}: {e}"))?;
		let slot_now = sp_consensus_aura::Slot::from_timestamp(
			sp_timestamp::Timestamp::current(),
			slot_duration,
		);
		let (pre_header, _slot, seal) =
			sc_consensus_aura::standalone::check_header_slot_and_seal::<
				Block,
				<AuraId as AuraIdT>::BoundedPair,
			>(slot_now, params.header, &authorities)
			.map_err(|e| {
				format!(
					"Rejecting block {post_hash:?} with invalid seal at anchor slot {anchor_slot}: {e:?}"
				)
			})?;
		params.header = pre_header;
		params.post_digests.push(seal);
		params.fork_choice = Some(sc_consensus::ForkChoiceStrategy::LongestChain);
		params.post_hash = Some(post_hash);

		if let Some(body) = params.body.clone() {
			let block = Block::new(params.header.clone(), body);
			let providers = self
				.create_inherent_data_providers
				.create_inherent_data_providers(parent_hash, ())
				.await
				.map_err(|e| format!("Could not create inherent data {e:?}"))?;
			sp_block_builder::check_inherents(self.client.clone(), parent_hash, block, &providers)
				.await
				.map_err(|e| format!("Error checking block inherents {e:?}"))?;
		}
		Ok(params)
	}
}

#[async_trait::async_trait]
impl<Block, Client, AuraId, CIDP> VerifierT<Block> for JamSealVerifier<Block, Client, AuraId, CIDP>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + Send + Sync,
	Client::Api: AuraApi<Block, AuraPublic<AuraId>> + sp_block_builder::BlockBuilder<Block>,
	AuraId: AuraIdT + Sync,
	CIDP: CreateInherentDataProviders<Block, ()> + Send + Sync,
{
	async fn verify(
		&self,
		block_import: BlockImportParams<Block>,
	) -> Result<BlockImportParams<Block>, String> {
		let parent_hash = *block_import.header.parent_hash();
		if let Some(jam_parent) =
			CumulusDigestItem::find_jam_parent_info(block_import.header.digest())
		{
			let slot_bound = self
				.client
				.runtime_api()
				.has_api::<dyn JamAuraApi<Block, AuraPublic<AuraId>>>(parent_hash)
				.unwrap_or(false);
			if slot_bound {
				return self.verify_jam(block_import, jam_parent.anchor_slot).await;
			}
		}
		self.fallback.verify(block_import).await
	}
}

/// Builds the JAM node's import queue over [`JamSealVerifier`].
pub(crate) struct BuildJamImportQueue<Block, RuntimeApi, AuraId>(
	PhantomData<(Block, RuntimeApi, AuraId)>,
);

impl<Block: NodeBlock, RuntimeApi, AuraId, BlockImport>
	BuildImportQueue<Block, RuntimeApi, BlockImport, JamExecutor>
	for BuildJamImportQueue<Block, RuntimeApi, AuraId>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, JamClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
	BlockImport:
		sc_consensus::BlockImport<Block, Error = sp_consensus::Error> + Send + Sync + 'static,
{
	fn build_import_queue(
		client: Arc<JamClient<Block, RuntimeApi>>,
		block_import: ParachainBlockImport<Block, BlockImport>,
		config: &Configuration,
		telemetry_handle: Option<TelemetryHandle>,
		task_manager: &TaskManager,
	) -> sc_service::error::Result<DefaultImportQueue<Block>> {
		let inherent_data_providers =
			move |_, _| async move { Ok(sp_timestamp::InherentDataProvider::from_system_time()) };
		let fallback = EquivocationVerifier::<<AuraId as AppCrypto>::Pair, _, _, _>::new(
			client.clone(),
			inherent_data_providers,
			telemetry_handle,
		);
		let verifier = JamSealVerifier {
			authorities: Arc::new(RuntimeApiAuthorities::<Block, RuntimeApi, AuraId>::new(
				client.clone(),
			)),
			client,
			fallback: Box::new(fallback),
			create_inherent_data_providers: inherent_data_providers,
		};
		Ok(BasicQueue::new(
			verifier,
			Box::new(block_import),
			None,
			&task_manager.spawn_essential_handle(),
			config.prometheus_registry(),
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use jam_interface::Slot as JamSlot;
	use sp_consensus_aura::{
		sr25519::{AuthorityId, AuthorityPair},
		Slot, SlotDuration, AURA_ENGINE_ID,
	};
	use sp_core::{Pair, H256};
	use sp_runtime::{generic, traits::BlakeTwo256, DigestItem, OpaqueExtrinsic};
	use std::sync::atomic::{AtomicBool, Ordering};

	type Header = generic::Header<u32, BlakeTwo256>;
	type Block = generic::Block<Header, OpaqueExtrinsic>;

	const SWITCH: JamSlot = 100;

	fn pair(name: &str) -> AuthorityPair {
		AuthorityPair::from_string(&format!("//{name}"), None).expect("a dev seed; qed")
	}

	fn set(names: &[&str]) -> Vec<AuthorityId> {
		names.iter().map(|name| pair(name).public()).collect()
	}

	/// `alice,bob` before [`SWITCH`], `alice,charlie` from it on.
	struct ScheduledSet;

	impl RuntimeAuthorities<Block, AuthorityId> for ScheduledSet {
		fn authorities_at(
			&self,
			_: H256,
			anchor_slot: JamSlot,
		) -> Result<Vec<AuthorityId>, String> {
			Ok(if anchor_slot < SWITCH {
				set(&["Alice", "Bob"])
			} else {
				set(&["Alice", "Charlie"])
			})
		}
	}

	struct TestClient;

	struct RuntimeApi;

	impl ProvideRuntimeApi<Block> for TestClient {
		type Api = RuntimeApi;

		fn runtime_api(&self) -> sp_api::ApiRef<'_, Self::Api> {
			RuntimeApi.into()
		}
	}

	sp_api::mock_impl_runtime_apis! {
		impl AuraApi<Block, AuthorityId> for RuntimeApi {
			fn slot_duration() -> SlotDuration {
				SlotDuration::from_millis(6000)
			}

			fn authorities() -> Vec<AuthorityId> {
				set(&["Alice", "Bob"])
			}
		}

		impl sp_block_builder::BlockBuilder<Block> for RuntimeApi {}
	}

	/// Records whether it was consulted.
	struct Fallback(Arc<AtomicBool>);

	#[async_trait::async_trait]
	impl VerifierT<Block> for Fallback {
		async fn verify(
			&self,
			params: BlockImportParams<Block>,
		) -> Result<BlockImportParams<Block>, String> {
			self.0.store(true, Ordering::SeqCst);
			Ok(params)
		}
	}

	fn verifier(
		consulted: Arc<AtomicBool>,
	) -> JamSealVerifier<Block, TestClient, AuthorityId, impl CreateInherentDataProviders<Block, ()>>
	{
		JamSealVerifier {
			client: Arc::new(TestClient),
			authorities: Arc::new(ScheduledSet),
			fallback: Box::new(Fallback(consulted)),
			create_inherent_data_providers: |_, _| async {
				Ok(sp_timestamp::InherentDataProvider::from_system_time())
			},
		}
	}

	/// A header-only block at Aura slot 1 (index 1 of a two-member set), sealed by `author`,
	/// carrying a `JamParent` anchored at `anchor_slot` when given.
	fn sealed_block(author: &str, anchor_slot: Option<JamSlot>) -> BlockImportParams<Block> {
		let mut header = Header::new(
			1,
			Default::default(),
			Default::default(),
			H256::zero(),
			Default::default(),
		);
		header
			.digest_mut()
			.push(DigestItem::PreRuntime(AURA_ENGINE_ID, Slot::from(1).encode()));
		if let Some(anchor_slot) = anchor_slot {
			header.digest_mut().push(
				CumulusDigestItem::JamParent {
					anchor: Default::default(),
					anchor_slot,
					lookup_anchor: Default::default(),
					lookup_anchor_slot: anchor_slot,
				}
				.to_digest_item(),
			);
		}
		let signature = pair(author).sign(header.hash().as_ref());
		header.digest_mut().push(DigestItem::Seal(AURA_ENGINE_ID, signature.encode()));
		BlockImportParams::new(sp_consensus::BlockOrigin::NetworkBroadcast, header)
	}

	/// Charlie is only in the set from the switch slot on, and the parent's set (`AuraApi`) never
	/// names him: the JAM verifier accepts his seal by the anchor's set without consulting the
	/// fallback, which would check against the parent's set.
	#[tokio::test]
	async fn a_seal_by_the_anchors_set_is_accepted_without_the_fallback() {
		let consulted = Arc::new(AtomicBool::new(false));
		let verified = verifier(consulted.clone())
			.verify(sealed_block("Charlie", Some(SWITCH)))
			.await
			.expect("Charlie is the author at slot 1 of the anchor's set");
		assert_eq!(verified.post_digests.len(), 1, "the seal moved to the post digests");
		assert!(!consulted.load(Ordering::SeqCst), "the fallback is not consulted");
	}

	/// Before the switch the anchor's set is `alice,bob`, so Charlie's seal is rejected.
	#[tokio::test]
	async fn a_seal_outside_the_anchors_set_is_rejected() {
		let consulted = Arc::new(AtomicBool::new(false));
		let error = verifier(consulted.clone())
			.verify(sealed_block("Charlie", Some(SWITCH - 1)))
			.await
			.err()
			.expect("Bob is the author at slot 1 of the old set");
		assert!(error.contains("invalid seal"), "{error}");
		assert!(!consulted.load(Ordering::SeqCst));
	}

	/// A block without `JamParent` goes to the plain Aura verifier.
	#[tokio::test]
	async fn a_block_without_jam_parent_goes_to_the_fallback() {
		let consulted = Arc::new(AtomicBool::new(false));
		verifier(consulted.clone())
			.verify(sealed_block("Bob", None))
			.await
			.expect("fallback");
		assert!(consulted.load(Ordering::SeqCst));
	}
}
