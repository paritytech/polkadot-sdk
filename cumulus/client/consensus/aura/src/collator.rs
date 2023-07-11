// Copyright 2023 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! The core collator logic for Aura - slot claiming, block proposing, and collation
//! packaging.
//!
//! The [`Collator`] struct exposed here is meant to be a component of higher-level logic
//! which actually manages the control flow of the collator - which slots to claim, how
//! many collations to build, when to work, etc.
//!
//! This module also exposes some standalone functions for common operations when building
//! aura-based collators.

use codec::{Decode, Encode};
use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::{
	self as consensus_common, ParachainBlockImportMarker, ParachainCandidate,
};
use cumulus_client_consensus_proposer::ProposerInterface;
use cumulus_primitives_core::{
	relay_chain::Hash as PHash, DigestItem, ParachainBlockData, PersistedValidationData,
};
use cumulus_primitives_parachain_inherent::ParachainInherentData;
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_node_primitives::{Collation, MaybeCompressedPoV};
use polkadot_primitives::{Header as PHeader, Id as ParaId};

use futures::prelude::*;
use sc_consensus::{BlockImport, BlockImportParams, ForkChoiceStrategy, StateAction};
use sc_consensus_aura::standalone as aura_internal;
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppPublic;
use sp_consensus::BlockOrigin;
use sp_consensus_aura::{AuraApi, Slot, SlotDuration};
use sp_core::crypto::Pair;
use sp_inherents::{CreateInherentDataProviders, InherentData, InherentDataProvider};
use sp_keystore::KeystorePtr;
use sp_runtime::{
	generic::Digest,
	traits::{Block as BlockT, HashFor, Header as HeaderT, Member},
};
use sp_state_machine::StorageChanges;
use sp_timestamp::Timestamp;
use std::{convert::TryFrom, error::Error, hash::Hash, sync::Arc, time::Duration};

/// Parameters for instantiating a [`Collator`].
pub struct Params<BI, CIDP, RClient, Proposer, CS> {
	/// A builder for inherent data builders.
	pub create_inherent_data_providers: CIDP,
	/// The block import handle.
	pub block_import: BI,
	/// An interface to the relay-chain client.
	pub relay_client: Arc<RClient>,
	/// The keystore handle used for accessing parachain key material.
	pub keystore: KeystorePtr,
	/// The identifier of the parachain within the relay-chain.
	pub para_id: ParaId,
	/// The block proposer used for building blocks.
	pub proposer: Proposer,
	/// The collator service used for bundling proposals into collations and announcing
	/// to the network.
	pub collator_service: CS,
}

/// A utility struct for writing collation logic that makes use of Aura entirely
/// or in part. See module docs for more details.
pub struct Collator<Block, P, BI, CIDP, RClient, Proposer, CS> {
	create_inherent_data_providers: CIDP,
	block_import: BI,
	relay_client: Arc<RClient>,
	keystore: KeystorePtr,
	para_id: ParaId,
	proposer: Proposer,
	collator_service: CS,
	_marker: std::marker::PhantomData<(Block, P)>,
}

impl<Block, P, BI, CIDP, RClient, Proposer, CS> Collator<Block, P, BI, CIDP, RClient, Proposer, CS>
where
	Block: BlockT,
	RClient: RelayChainInterface,
	CIDP: CreateInherentDataProviders<Block, ()> + 'static,
	BI: BlockImport<Block> + ParachainBlockImportMarker + Send + Sync + 'static,
	Proposer: ProposerInterface<Block, Transaction = BI::Transaction>,
	Proposer::Transaction: Sync,
	CS: CollatorServiceInterface<Block>,
	P: Pair + Send + Sync,
	P::Public: AppPublic + Hash + Member + Encode + Decode,
	P::Signature: TryFrom<Vec<u8>> + Hash + Member + Encode + Decode,
{
	/// Instantiate a new instance of the `Aura` manager.
	pub fn new(params: Params<BI, CIDP, RClient, Proposer, CS>) -> Self {
		Collator {
			create_inherent_data_providers: params.create_inherent_data_providers,
			block_import: params.block_import,
			relay_client: params.relay_client,
			keystore: params.keystore,
			para_id: params.para_id,
			proposer: params.proposer,
			collator_service: params.collator_service,
			_marker: std::marker::PhantomData,
		}
	}

	/// Explicitly creates the inherent data for parachain block authoring and overrides
	/// the timestamp inherent data with the one provided, if any.
	pub async fn create_inherent_data(
		&self,
		relay_parent: PHash,
		validation_data: &PersistedValidationData,
		parent_hash: Block::Hash,
		timestamp: impl Into<Option<Timestamp>>,
	) -> Result<(ParachainInherentData, InherentData), Box<dyn Error>> {
		let paras_inherent_data = ParachainInherentData::create_at(
			relay_parent,
			&self.relay_client,
			validation_data,
			self.para_id,
		)
		.await;

		let paras_inherent_data = match paras_inherent_data {
			Some(p) => p,
			None =>
				return Err(
					format!("Could not create paras inherent data at {:?}", relay_parent).into()
				),
		};

		let mut other_inherent_data = self
			.create_inherent_data_providers
			.create_inherent_data_providers(parent_hash, ())
			.map_err(|e| e as Box<dyn Error>)
			.await?
			.create_inherent_data()
			.await
			.map_err(Box::new)?;

		if let Some(timestamp) = timestamp.into() {
			other_inherent_data.replace_data(sp_timestamp::INHERENT_IDENTIFIER, &timestamp);
		}

		Ok((paras_inherent_data, other_inherent_data))
	}

	/// Propose, seal, and import a block, packaging it into a collation.
	///
	/// Provide the slot to build at as well as any other necessary pre-digest logs,
	/// the inherent data, and the proposal duration and PoV size limits.
	///
	/// The Aura pre-digest should not be explicitly provided and is set internally.
	///
	/// This does not announce the collation to the parachain network or the relay chain.
	pub async fn collate(
		&mut self,
		parent_header: &Block::Header,
		slot_claim: &SlotClaim<P::Public>,
		additional_pre_digest: impl Into<Option<Vec<DigestItem>>>,
		inherent_data: (ParachainInherentData, InherentData),
		proposal_duration: Duration,
		max_pov_size: usize,
	) -> Result<(Collation, ParachainBlockData<Block>, Block::Hash), Box<dyn Error>> {
		let mut digest = additional_pre_digest.into().unwrap_or_default();
		digest.push(slot_claim.pre_digest.clone());

		let proposal = self
			.proposer
			.propose(
				&parent_header,
				&inherent_data.0,
				inherent_data.1,
				Digest { logs: digest },
				proposal_duration,
				Some(max_pov_size),
			)
			.await
			.map_err(|e| Box::new(e))?;

		let sealed_importable = seal::<_, _, P>(
			proposal.block,
			proposal.storage_changes,
			&slot_claim.author_pub,
			&self.keystore,
		)?;

		let post_hash = sealed_importable.post_hash();
		let block = Block::new(
			sealed_importable.post_header(),
			sealed_importable
				.body
				.as_ref()
				.expect("body always created with this `propose` fn; qed")
				.clone(),
		);

		self.block_import.import_block(sealed_importable).await?;

		if let Some((collation, block_data)) = self.collator_service.build_collation(
			parent_header,
			post_hash,
			ParachainCandidate { block, proof: proposal.proof },
		) {
			tracing::info!(
				target: crate::LOG_TARGET,
				"PoV size {{ header: {}kb, extrinsics: {}kb, storage_proof: {}kb }}",
				block_data.header().encode().len() as f64 / 1024f64,
				block_data.extrinsics().encode().len() as f64 / 1024f64,
				block_data.storage_proof().encode().len() as f64 / 1024f64,
			);

			if let MaybeCompressedPoV::Compressed(ref pov) = collation.proof_of_validity {
				tracing::info!(
					target: crate::LOG_TARGET,
					"Compressed PoV size: {}kb",
					pov.block_data.0.len() as f64 / 1024f64,
				);
			}

			Ok((collation, block_data, post_hash))
		} else {
			Err("Unable to produce collation".to_string().into())
		}
	}

	/// Get the underlying collator service.
	pub fn collator_service(&self) -> &CS {
		&self.collator_service
	}
}

/// A claim on an Aura slot.
pub struct SlotClaim<Pub> {
	author_pub: Pub,
	pre_digest: DigestItem,
	timestamp: Timestamp,
}

impl<Pub> SlotClaim<Pub> {
	/// Create a slot-claim from the given author public key, slot, and timestamp.
	///
	/// This does not check whether the author actually owns the slot or the timestamp
	/// falls within the slot.
	pub fn unchecked<P>(author_pub: Pub, slot: Slot, timestamp: Timestamp) -> Self
	where
		P: Pair<Public = Pub>,
		P::Public: Encode + Decode,
		P::Signature: Encode + Decode,
	{
		SlotClaim { author_pub, timestamp, pre_digest: aura_internal::pre_digest::<P>(slot) }
	}

	/// Get the author's public key.
	pub fn author_pub(&self) -> &Pub {
		&self.author_pub
	}

	/// Get the Aura pre-digest for this slot.
	pub fn pre_digest(&self) -> &DigestItem {
		&self.pre_digest
	}

	/// Get the timestamp corresponding to the relay-chain slot this claim was
	/// generated against.
	pub fn timestamp(&self) -> Timestamp {
		self.timestamp
	}
}

/// Attempt to claim a slot derived from the given relay-parent header's slot.
pub async fn claim_slot<B, C, P>(
	client: &C,
	parent_hash: B::Hash,
	relay_parent_header: &PHeader,
	slot_duration: SlotDuration,
	relay_chain_slot_duration: SlotDuration,
	keystore: &KeystorePtr,
) -> Result<Option<SlotClaim<P::Public>>, Box<dyn Error>>
where
	B: BlockT,
	C: ProvideRuntimeApi<B> + Send + Sync + 'static,
	C::Api: AuraApi<B, P::Public>,
	P: Pair,
	P::Public: Encode + Decode,
	P::Signature: Encode + Decode,
{
	// load authorities
	let authorities = client.runtime_api().authorities(parent_hash).map_err(Box::new)?;

	// Determine the current slot and timestamp based on the relay-parent's.
	let (slot_now, timestamp) = match consensus_common::relay_slot_and_timestamp(
		relay_parent_header,
		relay_chain_slot_duration,
	) {
		Some((_, t)) => (Slot::from_timestamp(t, slot_duration), t),
		None => return Ok(None),
	};

	// Try to claim the slot locally.
	let author_pub = {
		let res = aura_internal::claim_slot::<P>(slot_now, &authorities, keystore).await;
		match res {
			Some(p) => p,
			None => return Ok(None),
		}
	};

	Ok(Some(SlotClaim::unchecked::<P>(author_pub, slot_now, timestamp)))
}

/// Seal a block with a signature in the header.
pub fn seal<B: BlockT, T, P>(
	pre_sealed: B,
	storage_changes: StorageChanges<T, HashFor<B>>,
	author_pub: &P::Public,
	keystore: &KeystorePtr,
) -> Result<BlockImportParams<B, T>, Box<dyn Error>>
where
	P: Pair,
	P::Signature: Encode + Decode + TryFrom<Vec<u8>>,
	P::Public: AppPublic,
{
	let (pre_header, body) = pre_sealed.deconstruct();
	let pre_hash = pre_header.hash();
	let block_number = *pre_header.number();

	// seal the block.
	let block_import_params = {
		let seal_digest =
			aura_internal::seal::<_, P>(&pre_hash, &author_pub, keystore).map_err(Box::new)?;
		let mut block_import_params = BlockImportParams::new(BlockOrigin::Own, pre_header);
		block_import_params.post_digests.push(seal_digest);
		block_import_params.body = Some(body.clone());
		block_import_params.state_action =
			StateAction::ApplyChanges(sc_consensus::StorageChanges::Changes(storage_changes));
		block_import_params.fork_choice = Some(ForkChoiceStrategy::LongestChain);
		block_import_params
	};
	let post_hash = block_import_params.post_hash();

	tracing::info!(
		target: crate::LOG_TARGET,
		"🔖 Pre-sealed block for proposal at {}. Hash now {:?}, previously {:?}.",
		block_number,
		post_hash,
		pre_hash,
	);

	Ok(block_import_params)
}
