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

//! Import-side check of a block's JAM block authorization seal (issue #62).
//!
//! The same check the runtime runs in `validate_block`: the seal's authorizer has to be in an
//! auth pool at the block's anchor, and running it has to authorize the seal-less header hash.
//! JAM state is read from the proof the block carries in its additional data. Like the import
//! path's re-execution, this trusts the anchor state root carried next to that proof; the PVF
//! checks it against JAM.

use codec::{Decode, DecodeAll};
use cumulus_jam_state_reader::JAM_PROOF_KEY;
use cumulus_primitives_core::CumulusDigestItem;
use parachain_service_core::{
	authorize_block::authorize_block,
	block_authorization::{BlockAuthorization, BLOCK_AUTHORIZATION_ENGINE_ID},
	types::ParaId,
	verify as verify_state_proof, StateProof,
};
use sc_consensus::{BlockImportParams, ForkChoiceStrategy};
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT},
	DigestItem,
};

/// Whether `header` is sealed with a JAM block authorization.
pub fn has_jam_seal<Header: HeaderT>(header: &Header) -> bool {
	matches!(
		header.digest().logs().last(),
		Some(DigestItem::Seal(id, _)) if *id == BLOCK_AUTHORIZATION_ENGINE_ID
	)
}

/// Verify the JAM block authorization seal of a block of `para_id` and move it to the post
/// digests, as the Aura verifier does with an Aura seal.
pub fn verify<Block: BlockT>(
	mut params: BlockImportParams<Block>,
	para_id: u32,
) -> Result<BlockImportParams<Block>, String> {
	if params.state_action.skip_execution_checks() || params.with_state() {
		params.fork_choice = Some(ForkChoiceStrategy::Custom(params.with_state()));
		return Ok(params);
	}

	let post_hash = params.header.hash();
	let seal = params.header.digest_mut().pop().ok_or("the block is not sealed")?;
	let DigestItem::Seal(BLOCK_AUTHORIZATION_ENGINE_ID, data) = &seal else {
		return Err("the block is not sealed with a JAM block authorization".into());
	};
	let authorization = BlockAuthorization::decode_all(&mut &data[..])
		.map_err(|error| format!("undecodable JAM block authorization: {error}"))?;

	let slot = CumulusDigestItem::find_jam_parent_info(params.header.digest())
		.ok_or("the block carries no JamParent digest")?
		.lookup_anchor_slot;
	let entry = params
		.additional_data
		.as_ref()
		.and_then(|data| data.get(JAM_PROOF_KEY))
		.ok_or("the block carries no JAM state proof")?;
	let (anchor_state_root, proof) = <([u8; 32], StateProof)>::decode(&mut &entry[..])
		.map_err(|error| format!("undecodable JAM state proof: {error}"))?;
	let pre_hash: [u8; 32] = params
		.header
		.hash()
		.as_ref()
		.try_into()
		.map_err(|_| "a parachain block hash is 32 bytes".to_string())?;

	authorize_block(
		|key| verify_state_proof(&proof, &anchor_state_root, key).ok().flatten(),
		ParaId::from(para_id),
		&authorization,
		pre_hash,
		slot,
	)
	.map_err(|error| format!("invalid JAM block authorization: {error:?}"))?;

	params.post_digests.push(seal);
	params.fork_choice = Some(ForkChoiceStrategy::LongestChain);
	params.post_hash = Some(post_hash);
	Ok(params)
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use sp_consensus::BlockOrigin;
	use sp_runtime::{generic, traits::BlakeTwo256, OpaqueExtrinsic};

	type TestHeader = generic::Header<u32, BlakeTwo256>;
	type TestBlock = generic::Block<TestHeader, OpaqueExtrinsic>;

	fn sealed(seal: DigestItem) -> BlockImportParams<TestBlock> {
		let jam_parent = CumulusDigestItem::JamParent {
			anchor: Default::default(),
			anchor_slot: 1,
			lookup_anchor: Default::default(),
			lookup_anchor_slot: 1,
		}
		.to_digest_item();
		let mut header = TestHeader::new(
			1,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);
		header.digest_mut().push(jam_parent);
		header.digest_mut().push(seal);
		BlockImportParams::new(BlockOrigin::NetworkBroadcast, header)
	}

	fn authorization() -> BlockAuthorization {
		BlockAuthorization { auth_code_host: 1, code_hash: [0; 32], config: vec![], token: vec![] }
	}

	#[test]
	fn has_jam_seal_works() {
		let jam = sealed(DigestItem::Seal(BLOCK_AUTHORIZATION_ENGINE_ID, vec![]));
		assert!(has_jam_seal(&jam.header));

		let aura = sealed(DigestItem::Seal(*b"aura", vec![]));
		assert!(!has_jam_seal(&aura.header));
	}

	#[test]
	fn malformed_seal_errors() {
		let params = sealed(DigestItem::Seal(BLOCK_AUTHORIZATION_ENGINE_ID, vec![1, 2, 3]));
		let error = verify(params, 100).map(drop).expect_err("the seal does not decode");
		assert!(error.contains("undecodable JAM block authorization"), "{error}");
	}

	#[test]
	fn missing_state_proof_errors() {
		let params =
			sealed(DigestItem::Seal(BLOCK_AUTHORIZATION_ENGINE_ID, authorization().encode()));
		let error = verify(params, 100).map(drop).expect_err("there is nothing to read JAM from");
		assert!(error.contains("no JAM state proof"), "{error}");
	}
}
