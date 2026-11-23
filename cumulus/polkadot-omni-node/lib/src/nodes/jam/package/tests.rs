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

use crate::nodes::jam::{
	package::{
		build_pov, extrinsic_spec,
		source::{prerequisites_for_parent, record_submitted_hash, Anchored, PackageSource},
		work_package, work_package_hash, PackageParams,
	},
	store::hash_ledger::test_support::ledger,
};
use codec::{Decode, DecodeAll, Encode};
use cumulus_client_jam_authorizer::{authorizer::AuraAuthorizer, testing::authorizer_of};
use cumulus_jam_state_reader::JAM_PROOF_KEY;
use cumulus_primitives_core::{CumulusDigestItem, JamParent, ParachainBlockData};
use cumulus_test_runtime::{Block as TestBlock, Header as TestHeader};
use jam_interface::{HeaderHash, Slot as JamSlot, WorkPackage, WorkPackageHash};
use jam_std_common::build_encoded_bundle;
use jam_types::{CodeHash, RefineContext, VecSet};
use parachain_authorizer::aura::{signable_work_package_hash, AuthToken as GuestToken};
use parachain_authorizer_sr25519::Sr25519;
use parachain_service_core::{candidate::ParachainCandidate, StateProof};
use sp_core::H256;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use sp_trie::CompactProof;

/// The one-collator set this node is in, so every package a test builds can be signed.
fn aura() -> AuraAuthorizer {
	authorizer_of("alice", "Alice", 1)
}

/// A package as the manager builds one: assembled, then signed.
pub(crate) fn signed(source: &PackageSource<TestBlock>, anchored: &Anchored) -> WorkPackage {
	signed_with_pov(source, anchored).0
}

/// A signed package together with the PoV it sends as work-item extrinsic 0.
fn signed_with_pov(
	source: &PackageSource<TestBlock>,
	anchored: &Anchored,
) -> (WorkPackage, Vec<u8>) {
	let (mut package, pov) = source.package(anchored);
	aura()
		.authorize(&mut package)
		.expect("the keystore holds Alice's aura key; qed");
	(package, pov)
}

pub(crate) fn wp_hash(byte: u8) -> WorkPackageHash {
	WorkPackageHash::from([byte; 32])
}

pub(crate) fn header(number: u32, parent: H256) -> TestHeader {
	TestHeader::new(number, H256::repeat_byte(1), H256::repeat_byte(2), parent, <_>::default())
}

pub(crate) fn anchored(anchor_slot: JamSlot) -> Anchored {
	Anchored {
		context: RefineContext {
			anchor: HeaderHash::from([9u8; 32]),
			anchor_slot,
			state_root: [4u8; 32].into(),
			beefy_root: [5u8; 32].into(),
			lookup_anchor: HeaderHash::from([6u8; 32]),
			lookup_anchor_slot: anchor_slot,
			lookup_anchor_state_root: Default::default(),
			prerequisites: Default::default(),
		},
		anchor_slot,
		submit_target: Some(0),
	}
}

/// `anchored(11)` carrying `prerequisites`, the way `on_new_block` hands them to the
/// assembler.
fn anchored_naming(prerequisites: VecSet<WorkPackageHash>) -> Anchored {
	Anchored { context: RefineContext { prerequisites, ..anchored(11).context }, ..anchored(11) }
}

pub(crate) fn package_source() -> PackageSource<TestBlock> {
	PackageSource {
		blocks: vec![TestBlock::new(header(1, H256::repeat_byte(7)), vec![])],
		proof: CompactProof { encoded_nodes: vec![vec![1u8, 2, 3]] },
		parent_header: header(0, H256::repeat_byte(6)),
		additional_data: [(JAM_PROOF_KEY.to_string(), vec![1u8, 2, 3])].into(),
		validation_code_hash: [8u8; 32],
		params: PackageParams {
			para_id: 2000.into(),
			service_id: 42,
			service_code_hash: CodeHash::from([9u8; 32]),
			refine_gas_limit: 1_000,
			accumulate_gas_limit: 1_000,
		},
		authorizer: aura().authorizer(),
	}
}

/// With no entry for the parent block in the ledger the chain restarts here: the assembled
/// package names nothing. Nothing is imported into it and nothing is exported out of it for
/// a child to import — the whole of what independent packages change on the wire.
#[test]
fn a_package_with_no_known_parent_has_empty_prerequisites() {
	let ledger = ledger();
	let prerequisites = prerequisites_for_parent(&ledger, H256::repeat_byte(0x22).into());
	let (package, _pov) = package_source().package(&anchored_naming(prerequisites));

	assert!(package.context.prerequisites.as_ref().is_empty());
	assert!(package.items[0].import_segments.is_empty());
	assert_eq!(package.items[0].export_count, 0);
	assert_eq!(package.items[0].extrinsics.len(), 1, "the PoV, and only the PoV, travels");
}

/// When the ledger remembers the parent block's package, the assembled package names exactly
/// that one package as its prerequisite — the linear chain.
#[test]
fn a_package_with_known_parent_names_it_as_prerequisite() {
	let parent_hash = H256::repeat_byte(0x11);
	let parent_wp_hash = wp_hash(0xAB);
	let ledger = ledger();
	ledger.insert(&parent_hash.into(), parent_wp_hash).expect("insert ok; qed");

	let prerequisites = prerequisites_for_parent(&ledger, parent_hash.into());
	let (package, _pov) = package_source().package(&anchored_naming(prerequisites));

	assert_eq!(
		package.context.prerequisites.as_ref(),
		&[parent_wp_hash],
		"exactly the parent's package",
	);
}

/// A package chains behind its parent's package when the ledger remembers that package's
/// hash: exactly one prerequisite, the parent — the linear chain.
#[test]
fn package_names_parent_as_prerequisite() {
	let parent_hash = H256::repeat_byte(0x11);
	let parent_wp_hash = wp_hash(0xAB);
	let ledger = ledger();
	ledger.insert(&parent_hash.into(), parent_wp_hash).expect("insert ok; qed");

	let prerequisites = prerequisites_for_parent(&ledger, parent_hash.into());
	assert_eq!(prerequisites.as_ref(), &[parent_wp_hash], "exactly the parent's package");

	let package = signed(&package_source(), &anchored_naming(prerequisites));
	assert_eq!(
		package.context.prerequisites.as_ref(),
		&[parent_wp_hash],
		"the assembled package carries the parent's hash",
	);
}

/// With no entry for the parent block the chain restarts here: the package names nothing.
#[test]
fn package_without_known_parent_has_no_prerequisite() {
	let ledger = ledger();

	let prerequisites = prerequisites_for_parent(&ledger, H256::repeat_byte(0x22).into());
	assert!(prerequisites.is_empty(), "a parent hash the ledger does not hold is no link");

	let package = signed(&package_source(), &anchored_naming(prerequisites));
	assert!(package.context.prerequisites.as_ref().is_empty());
}

/// The recording step of `on_new_block`: the hash JAM keys the submitted package by is the
/// hash the ledger remembers for its block, ready for the block's child to name.
#[test]
fn submitted_hash_is_recorded_in_the_ledger() {
	let ledger = ledger();
	let block_hash = H256::repeat_byte(0x33);
	let package = signed(&package_source(), &anchored(11));

	let recorded = record_submitted_hash(&ledger, block_hash.into(), &package);

	assert_eq!(recorded, work_package_hash(&package));
	assert_eq!(
		ledger.get(&block_hash.into()).expect("read ok; qed"),
		Some(work_package_hash(&package)),
	);
}

/// The prerequisite is part of the bytes the token signs: the token verifies against the
/// package that names the parent and fails against the same package without the link. A
/// prerequisite set after signing would invert both.
#[test]
fn prerequisite_is_covered_by_the_signature() {
	let parent_wp_hash = wp_hash(0xAB);
	let package = signed(&package_source(), &anchored_naming(vec![parent_wp_hash].into()));
	assert_eq!(package.context.prerequisites.as_ref(), &[parent_wp_hash]);

	let token = GuestToken::decode_all(&mut &package.authorization[..])
		.expect("the guest decodes the token this node encoded");
	token
		.check_signature::<Sr25519>(signable_work_package_hash(&package))
		.expect("the signature covers the prerequisite");

	let without = signed(&package_source(), &anchored(11));
	assert!(
		token.check_signature::<Sr25519>(signable_work_package_hash(&without)).is_err(),
		"the same token does not authorize the package without the prerequisite",
	);
}

/// The hash is the key everything else uses — the status subscription, the manager's own
/// lookup — so it has to be the hash the node derives. polkajam's bundle builder is the
/// reference; with no imports the bundle opens with the encoded package, so the two must
/// agree exactly.
/// A package is submitted under the para's own authorizer and carries a token, so a guarantor
/// looks the AURA blob up by the hash the core's pool holds instead of waving the package
/// through. `auth_code_host` is the parachain service, which is where that blob's preimage is
/// hosted — a package naming any other host is one no guarantor can resolve the code for.
#[test]
fn a_package_runs_under_the_paras_own_authorizer() {
	let package = signed(&package_source(), &anchored(11));

	// The literal is `package_source`'s parachain service, so this pins the host to that
	// service rather than to the bootstrap service 0 the blob used to be hosted by.
	assert_eq!(package.auth_code_host, 42);
	assert_eq!(
		parachain_service_core::authorizer::authorizer_hash(&package.authorizer),
		aura().hash(),
		"the package names the authorizer whose hash a core's pool must hold",
	);
	assert!(!package.authorization.is_empty(), "and it carries a token, not an empty one");
}

#[test]
fn the_package_hash_is_the_one_polkajam_derives() {
	let (package, pov) = signed_with_pov(&package_source(), &anchored(11));
	let package_bytes = jam_codec::Encode::encode(&package);

	let (reference, bundle) = build_encoded_bundle(&package, [&pov], &[Vec::new()]);

	assert_eq!(work_package_hash(&package), reference, "the hash covers only the package");
	assert_eq!(
		bundle,
		[&package_bytes[..], &pov[..]].concat(),
		"the bundle is the package followed by the PoV extrinsic",
	);
}

/// The PoV does not ride in the payload: `ParachainCandidate` carries only the para id and
/// the validation-code hash, and the package's one extrinsic spec names the PoV's hash and
/// length.
#[test]
fn the_pov_travels_as_the_only_extrinsic() {
	let source = package_source();
	let (package, pov) = signed_with_pov(&source, &anchored(11));

	let candidate = <ParachainCandidate as Decode>::decode(&mut &package.items[0].payload.0[..])
		.expect("the payload is a ParachainCandidate");
	let expected = ParachainCandidate {
		para_id: source.params.para_id,
		validation_code: source.validation_code_hash.into(),
	};
	assert_eq!(
		candidate.encode(),
		expected.encode(),
		"the payload is the candidate and carries the para id and the validation-code hash",
	);

	assert_eq!(package.items[0].extrinsics.len(), 1, "the PoV is the one extrinsic");
	let spec = &package.items[0].extrinsics[0];
	assert_eq!(spec.len, pov.len() as u32);
	assert_eq!(spec.hash, jam_types::ExtrinsicHash::from(jam_std_common::hash_raw(&pov)));
}

/// The extrinsic bytes a bundle carries decode as the V4 `ParachainBlockData` the runtime and
/// the recovery decoder expect.
#[test]
fn the_pov_extrinsic_decodes_as_parachain_block_data() {
	let (package, pov) = signed_with_pov(&package_source(), &anchored(11));
	let package_bytes = jam_codec::Encode::encode(&package);
	let (_, bundle) = build_encoded_bundle(&package, [&pov], &[Vec::new()]);

	let decoded = ParachainBlockData::<TestBlock>::decode(&mut &bundle[package_bytes.len()..])
		.expect("the trailing extrinsic bytes decode as the PoV");

	assert_eq!(decoded.blocks().len(), 1, "the one block the package proves");
}

/// `build_pov` must produce a V4 PoV whose `parent_header()` accessor returns the
/// SCALE-encoded header that was passed in. The PVF reads it to establish `state_root`; the
/// parachain service's `accumulate` verifies it against its stored head. The additional-data
/// map assembled at build time travels with it, under `JAM_PROOF_KEY`.
#[test]
fn the_pov_carries_the_parent_header() {
	let parent_header = header(4, H256::repeat_byte(6));
	let block_header = header(5, parent_header.hash());
	// Shaped like the production entry `register_jam_state_reader` stores.
	let entry = ([7u8; 32], [8u8; 32], StateProof { nodes: vec![], values: vec![] }).encode();
	let additional_data = [(JAM_PROOF_KEY.to_string(), entry.clone())].into();

	let pov = build_pov(
		&[TestBlock::new(block_header, vec![])],
		&CompactProof { encoded_nodes: vec![vec![1u8, 2, 3]] },
		&parent_header,
		&additional_data,
		Vec::new(),
	);

	let decoded = ParachainBlockData::<TestBlock>::decode(&mut &pov[..])
		.expect("PoV decodes as ParachainBlockData");

	let expected = parent_header.encode();
	assert_eq!(
		decoded.parent_header(),
		Some(expected.as_slice()),
		"the V4 PoV carries the SCALE-encoded parent header",
	);
	assert_eq!(
		decoded.additional_data(),
		vec![Some(additional_data.clone())],
		"and the additional-data map assembled at build time",
	);
	let carried = decoded.additional_data()[0].as_ref().expect("the V4 PoV carries the map");
	assert_eq!(
		carried.get(JAM_PROOF_KEY).map(|value| value.as_slice()),
		Some(entry.as_slice()),
		"the `JAM_PROOF_KEY` entry round-trips byte-exact",
	);
	assert!(decoded.anchor_chain().is_empty(), "an authoring-time PoV carries no anchor chain");
}

/// The extracted `work_package` has to assemble exactly what `PackageSource::package` did, so
/// every path that builds a package around a block produces the same work item.
#[test]
fn work_package_matches_package_source() {
	let source = package_source();
	let anchored = anchored(11);
	let (package, pov) = source.package(&anchored);

	let expected = work_package(
		vec![extrinsic_spec(&pov)],
		source.validation_code_hash,
		&source.params,
		source.authorizer.clone(),
		anchored.context.clone(),
	);

	assert_eq!(
		jam_codec::Encode::encode(&package),
		jam_codec::Encode::encode(&expected),
		"the extracted assembler and the author path agree byte for byte",
	);
}

/// The PoV store keeps an authored block the way the import path keeps an imported one, and
/// what it keeps rebuilds the very PoV the block's package carries.
#[test]
fn an_authored_block_is_kept_with_what_rebuilds_its_pov() {
	let jam_parent = JamParent {
		anchor: H256::repeat_byte(9),
		anchor_slot: 11,
		lookup_anchor: H256::repeat_byte(6),
		lookup_anchor_slot: 11,
	};
	let mut source = package_source();
	source.blocks[0].header.digest.push(
		CumulusDigestItem::JamParent {
			anchor: jam_parent.anchor,
			anchor_slot: jam_parent.anchor_slot,
			lookup_anchor: jam_parent.lookup_anchor,
			lookup_anchor_slot: jam_parent.lookup_anchor_slot,
		}
		.to_digest_item(),
	);
	let block = source.blocks[0].clone();
	let parent_header = source.parent_header.clone();
	let (package, pov) = source.package(&anchored(11));
	let spec = package.items[0].extrinsics[0].clone();

	let kept = source.into_block_pov(spec.clone()).expect("the block carries `JamParent`");

	assert_eq!(kept.block, block);
	assert_eq!(kept.parent_header, parent_header);
	assert_eq!(kept.jam_parent, jam_parent);
	assert_eq!(kept.parent_hash, *block.header().parent_hash());
	assert_eq!(kept.number, *block.header().number());
	assert_eq!(kept.spec.hash, spec.hash);
	assert_eq!(kept.spec.len, pov.len() as u32);
	assert_eq!(
		build_pov(
			std::slice::from_ref(&kept.block),
			&kept.compact_proof,
			&kept.parent_header,
			&kept.additional_data,
			Vec::new(),
		),
		pov,
		"the kept entry alone rebuilds the submitted PoV byte for byte",
	);
	assert!(
		package_source().into_block_pov(spec).is_none(),
		"a block without a `JamParent` digest is not kept, as on the import path",
	);
}
