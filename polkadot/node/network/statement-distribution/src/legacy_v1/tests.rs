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

#![allow(clippy::clone_on_copy)]

use super::*;
use crate::{metrics::Metrics, *};

use assert_matches::assert_matches;
use futures::executor;
use polkadot_node_network_protocol::{
	peer_set::ValidationVersion, view, VersionedValidationProtocol,
};
use polkadot_node_primitives::{
	SignedFullStatementWithPVD, Statement, UncheckedSignedFullStatement,
};
use polkadot_node_subsystem::messages::AllMessages;
use polkadot_primitives::{GroupIndex, Hash, HeadData, Id as ParaId, IndexedVec, SessionInfo};
use polkadot_primitives_test_helpers::{
	dummy_committed_candidate_receipt, dummy_hash, AlwaysZeroRng,
};
use sc_keystore::LocalKeystore;
use sp_application_crypto::{sr25519::Pair, AppCrypto, Pair as TraitPair};
use sp_keyring::Sr25519Keyring;
use sp_keystore::{Keystore, KeystorePtr};
use std::sync::Arc;

// Some deterministic genesis hash for protocol names
const GENESIS_HASH: Hash = Hash::repeat_byte(0xff);

fn dummy_pvd() -> PersistedValidationData {
	PersistedValidationData {
		parent_head: HeadData(vec![7, 8, 9]),
		relay_parent_number: 5,
		max_pov_size: 1024,
		relay_parent_storage_root: Default::default(),
	}
}

fn extend_statement_with_pvd(
	statement: SignedFullStatement,
	pvd: PersistedValidationData,
) -> SignedFullStatementWithPVD {
	statement
		.convert_to_superpayload_with(|statement| match statement {
			Statement::Seconded(receipt) => StatementWithPVD::Seconded(receipt, pvd),
			Statement::Valid(candidate_hash) => StatementWithPVD::Valid(candidate_hash),
		})
		.unwrap()
}

#[test]
fn active_head_accepts_only_2_seconded_per_validator() {
	let validators = vec![
		Sr25519Keyring::Alice.public().into(),
		Sr25519Keyring::Bob.public().into(),
		Sr25519Keyring::Charlie.public().into(),
	];
	let parent_hash: Hash = [1; 32].into();

	let session_index = 1;
	let signing_context = SigningContext { parent_hash, session_index };

	let candidate_a = {
		let mut c = dummy_committed_candidate_receipt(dummy_hash());
		c.descriptor.relay_parent = parent_hash;
		c.descriptor.para_id = 1.into();
		c
	};

	let candidate_b = {
		let mut c = dummy_committed_candidate_receipt(dummy_hash());
		c.descriptor.relay_parent = parent_hash;
		c.descriptor.para_id = 2.into();
		c
	};

	let candidate_c = {
		let mut c = dummy_committed_candidate_receipt(dummy_hash());
		c.descriptor.relay_parent = parent_hash;
		c.descriptor.para_id = 3.into();
		c
	};

	let mut head_data = ActiveHeadData::new(
		IndexedVec::<ValidatorIndex, ValidatorId>::from(validators),
		session_index,
	);

	let keystore: KeystorePtr = Arc::new(LocalKeystore::in_memory());
	let alice_public = Keystore::sr25519_generate_new(
		&*keystore,
		ValidatorId::ID,
		Some(&Sr25519Keyring::Alice.to_seed()),
	)
	.unwrap();
	let bob_public = Keystore::sr25519_generate_new(
		&*keystore,
		ValidatorId::ID,
		Some(&Sr25519Keyring::Bob.to_seed()),
	)
	.unwrap();

	// note A
	let a_seconded_val_0 = SignedFullStatement::sign(
		&keystore,
		Statement::Seconded(candidate_a.into()),
		&signing_context,
		ValidatorIndex(0),
		&alice_public.into(),
	)
	.ok()
	.flatten()
	.expect("should be signed");
	assert!(head_data
		.check_useful_or_unknown(&a_seconded_val_0.clone().convert_payload().into())
		.is_ok());
	let noted = head_data.note_statement(a_seconded_val_0.clone());

	assert_matches!(noted, NotedStatement::Fresh(_));

	// note A (duplicate)
	assert_eq!(
		head_data.check_useful_or_unknown(&a_seconded_val_0.clone().convert_payload().into()),
		Err(DeniedStatement::UsefulButKnown),
	);
	let noted = head_data.note_statement(a_seconded_val_0);

	assert_matches!(noted, NotedStatement::UsefulButKnown);

	// note B
	let statement = SignedFullStatement::sign(
		&keystore,
		Statement::Seconded(candidate_b.clone().into()),
		&signing_context,
		ValidatorIndex(0),
		&alice_public.into(),
	)
	.ok()
	.flatten()
	.expect("should be signed");
	assert!(head_data
		.check_useful_or_unknown(&statement.clone().convert_payload().into())
		.is_ok());
	let noted = head_data.note_statement(statement);
	assert_matches!(noted, NotedStatement::Fresh(_));

	// note C (beyond 2 - ignored)
	let statement = SignedFullStatement::sign(
		&keystore,
		Statement::Seconded(candidate_c.clone().into()),
		&signing_context,
		ValidatorIndex(0),
		&alice_public.into(),
	)
	.ok()
	.flatten()
	.expect("should be signed");
	assert_eq!(
		head_data.check_useful_or_unknown(&statement.clone().convert_payload().into()),
		Err(DeniedStatement::NotUseful),
	);
	let noted = head_data.note_statement(statement);
	assert_matches!(noted, NotedStatement::NotUseful);

	// note B (new validator)
	let statement = SignedFullStatement::sign(
		&keystore,
		Statement::Seconded(candidate_b.into()),
		&signing_context,
		ValidatorIndex(1),
		&bob_public.into(),
	)
	.ok()
	.flatten()
	.expect("should be signed");
	assert!(head_data
		.check_useful_or_unknown(&statement.clone().convert_payload().into())
		.is_ok());
	let noted = head_data.note_statement(statement);
	assert_matches!(noted, NotedStatement::Fresh(_));

	// note C (new validator)
	let statement = SignedFullStatement::sign(
		&keystore,
		Statement::Seconded(candidate_c.into()),
		&signing_context,
		ValidatorIndex(1),
		&bob_public.into(),
	)
	.ok()
	.flatten()
	.expect("should be signed");
	assert!(head_data
		.check_useful_or_unknown(&statement.clone().convert_payload().into())
		.is_ok());
	let noted = head_data.note_statement(statement);
	assert_matches!(noted, NotedStatement::Fresh(_));
}

#[test]
fn note_local_works() {
	let hash_a = CandidateHash([1; 32].into());
	let hash_b = CandidateHash([2; 32].into());

	let mut per_peer_tracker = VcPerPeerTracker::default();
	per_peer_tracker.note_local(hash_a);
	per_peer_tracker.note_local(hash_b);

	assert!(per_peer_tracker.local_observed.contains(&hash_a));
	assert!(per_peer_tracker.local_observed.contains(&hash_b));

	assert!(!per_peer_tracker.remote_observed.contains(&hash_a));
	assert!(!per_peer_tracker.remote_observed.contains(&hash_b));
}

#[test]
fn note_remote_works() {
	let hash_a = CandidateHash([1; 32].into());
	let hash_b = CandidateHash([2; 32].into());
	let hash_c = CandidateHash([3; 32].into());

	let mut per_peer_tracker = VcPerPeerTracker::default();
	assert!(per_peer_tracker.note_remote(hash_a));
	assert!(per_peer_tracker.note_remote(hash_b));
	assert!(!per_peer_tracker.note_remote(hash_c));

	assert!(per_peer_tracker.remote_observed.contains(&hash_a));
	assert!(per_peer_tracker.remote_observed.contains(&hash_b));
	assert!(!per_peer_tracker.remote_observed.contains(&hash_c));

	assert!(!per_peer_tracker.local_observed.contains(&hash_a));
	assert!(!per_peer_tracker.local_observed.contains(&hash_b));
	assert!(!per_peer_tracker.local_observed.contains(&hash_c));
}

#[test]
fn per_peer_relay_parent_knowledge_send() {
	let mut knowledge = PeerRelayParentKnowledge::default();

	let hash_a = CandidateHash([1; 32].into());

	// Sending an un-pinned statement should not work and should have no effect.
	assert!(!knowledge.can_send(&(CompactStatement::Valid(hash_a), ValidatorIndex(0))));
	assert!(!knowledge.is_known_candidate(&hash_a));
	assert!(knowledge.sent_statements.is_empty());
	assert!(knowledge.received_statements.is_empty());
	assert!(knowledge.seconded_counts.is_empty());
	assert!(knowledge.received_message_count.is_empty());

	// Make the peer aware of the candidate.
	assert_eq!(knowledge.send(&(CompactStatement::Seconded(hash_a), ValidatorIndex(0))), true);
	assert_eq!(knowledge.send(&(CompactStatement::Seconded(hash_a), ValidatorIndex(1))), false);
	assert!(knowledge.is_known_candidate(&hash_a));
	assert_eq!(knowledge.sent_statements.len(), 2);
	assert!(knowledge.received_statements.is_empty());
	assert_eq!(knowledge.seconded_counts.len(), 2);
	assert!(knowledge.received_message_count.get(&hash_a).is_none());

	// And now it should accept the dependent message.
	assert_eq!(knowledge.send(&(CompactStatement::Valid(hash_a), ValidatorIndex(0))), false);
	assert!(knowledge.is_known_candidate(&hash_a));
	assert_eq!(knowledge.sent_statements.len(), 3);
	assert!(knowledge.received_statements.is_empty());
	assert_eq!(knowledge.seconded_counts.len(), 2);
	assert!(knowledge.received_message_count.get(&hash_a).is_none());
}

#[test]
fn cant_send_after_receiving() {
	let mut knowledge = PeerRelayParentKnowledge::default();

	let hash_a = CandidateHash([1; 32].into());
	assert!(knowledge
		.check_can_receive(&(CompactStatement::Seconded(hash_a), ValidatorIndex(0)), 3)
		.is_ok());
	assert!(knowledge
		.receive(&(CompactStatement::Seconded(hash_a), ValidatorIndex(0)), 3)
		.unwrap());
	assert!(!knowledge.can_send(&(CompactStatement::Seconded(hash_a), ValidatorIndex(0))));
}

#[test]
fn per_peer_relay_parent_knowledge_receive() {
	let mut knowledge = PeerRelayParentKnowledge::default();

	let hash_a = CandidateHash([1; 32].into());

	assert_eq!(
		knowledge.check_can_receive(&(CompactStatement::Valid(hash_a), ValidatorIndex(0)), 3),
		Err(COST_UNEXPECTED_STATEMENT_UNKNOWN_CANDIDATE),
	);
	assert_eq!(
		knowledge.receive(&(CompactStatement::Valid(hash_a), ValidatorIndex(0)), 3),
		Err(COST_UNEXPECTED_STATEMENT_UNKNOWN_CANDIDATE),
	);

	assert!(knowledge
		.check_can_receive(&(CompactStatement::Seconded(hash_a), ValidatorIndex(0)), 3)
		.is_ok());
	assert_eq!(
		knowledge.receive(&(CompactStatement::Seconded(hash_a), ValidatorIndex(0)), 3),
		Ok(true),
	);

	// Push statements up to the flood limit.
	assert!(knowledge
		.check_can_receive(&(CompactStatement::Valid(hash_a), ValidatorIndex(1)), 3)
		.is_ok());
	assert_eq!(
		knowledge.receive(&(CompactStatement::Valid(hash_a), ValidatorIndex(1)), 3),
		Ok(false),
	);

	assert!(knowledge.is_known_candidate(&hash_a));
	assert_eq!(*knowledge.received_message_count.get(&hash_a).unwrap(), 2);

	assert!(knowledge
		.check_can_receive(&(CompactStatement::Valid(hash_a), ValidatorIndex(2)), 3)
		.is_ok());
	assert_eq!(
		knowledge.receive(&(CompactStatement::Valid(hash_a), ValidatorIndex(2)), 3),
		Ok(false),
	);

	assert_eq!(*knowledge.received_message_count.get(&hash_a).unwrap(), 3);

	assert_eq!(
		knowledge.check_can_receive(&(CompactStatement::Valid(hash_a), ValidatorIndex(7)), 3),
		Err(COST_APPARENT_FLOOD),
	);
	assert_eq!(
		knowledge.receive(&(CompactStatement::Valid(hash_a), ValidatorIndex(7)), 3),
		Err(COST_APPARENT_FLOOD),
	);

	assert_eq!(*knowledge.received_message_count.get(&hash_a).unwrap(), 3);
	assert_eq!(knowledge.received_statements.len(), 3); // number of prior `Ok`s.

	// Now make sure that the seconding limit is respected.
	let hash_b = CandidateHash([2; 32].into());
	let hash_c = CandidateHash([3; 32].into());

	assert!(knowledge
		.check_can_receive(&(CompactStatement::Seconded(hash_b), ValidatorIndex(0)), 3)
		.is_ok());
	assert_eq!(
		knowledge.receive(&(CompactStatement::Seconded(hash_b), ValidatorIndex(0)), 3),
		Ok(true),
	);

	assert_eq!(
		knowledge.check_can_receive(&(CompactStatement::Seconded(hash_c), ValidatorIndex(0)), 3),
		Err(COST_UNEXPECTED_STATEMENT_REMOTE),
	);
	assert_eq!(
		knowledge.receive(&(CompactStatement::Seconded(hash_c), ValidatorIndex(0)), 3),
		Err(COST_UNEXPECTED_STATEMENT_REMOTE),
	);

	// Last, make sure that already-known statements are disregarded.
	assert_eq!(
		knowledge.check_can_receive(&(CompactStatement::Valid(hash_a), ValidatorIndex(2)), 3),
		Err(COST_DUPLICATE_STATEMENT),
	);
	assert_eq!(
		knowledge.receive(&(CompactStatement::Valid(hash_a), ValidatorIndex(2)), 3),
		Err(COST_DUPLICATE_STATEMENT),
	);

	assert_eq!(
		knowledge.check_can_receive(&(CompactStatement::Seconded(hash_b), ValidatorIndex(0)), 3),
		Err(COST_DUPLICATE_STATEMENT),
	);
	assert_eq!(
		knowledge.receive(&(CompactStatement::Seconded(hash_b), ValidatorIndex(0)), 3),
		Err(COST_DUPLICATE_STATEMENT),
	);
}

#[test]
fn peer_view_update_sends_messages() {
	let hash_a = Hash::repeat_byte(1);
	let hash_b = Hash::repeat_byte(2);
	let hash_c = Hash::repeat_byte(3);

	let candidate = {
		let mut c = dummy_committed_candidate_receipt(dummy_hash());
		c.descriptor.relay_parent = hash_c;
		c.descriptor.para_id = ParaId::from(1_u32);
		c
	};
	let candidate_hash = candidate.hash();

	let old_view = view![hash_a, hash_b];
	let new_view = view![hash_b, hash_c];

	let mut active_heads = HashMap::new();
	let validators = vec![
		Sr25519Keyring::Alice.public().into(),
		Sr25519Keyring::Bob.public().into(),
		Sr25519Keyring::Charlie.public().into(),
	];

	let session_index = 1;
	let signing_context = SigningContext { parent_hash: hash_c, session_index };

	let keystore: KeystorePtr = Arc::new(LocalKeystore::in_memory());

	let alice_public = Keystore::sr25519_generate_new(
		&*keystore,
		ValidatorId::ID,
		Some(&Sr25519Keyring::Alice.to_seed()),
	)
	.unwrap();
	let bob_public = Keystore::sr25519_generate_new(
		&*keystore,
		ValidatorId::ID,
		Some(&Sr25519Keyring::Bob.to_seed()),
	)
	.unwrap();
	let charlie_public = Keystore::sr25519_generate_new(
		&*keystore,
		ValidatorId::ID,
		Some(&Sr25519Keyring::Charlie.to_seed()),
	)
	.unwrap();

	let new_head_data = {
		let mut data = ActiveHeadData::new(
			IndexedVec::<ValidatorIndex, ValidatorId>::from(validators),
			session_index,
		);

		let statement = SignedFullStatement::sign(
			&keystore,
			Statement::Seconded(candidate.clone().into()),
			&signing_context,
			ValidatorIndex(0),
			&alice_public.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");
		assert!(data
			.check_useful_or_unknown(&statement.clone().convert_payload().into())
			.is_ok());
		let noted = data.note_statement(statement);

		assert_matches!(noted, NotedStatement::Fresh(_));

		let statement = SignedFullStatement::sign(
			&keystore,
			Statement::Valid(candidate_hash),
			&signing_context,
			ValidatorIndex(1),
			&bob_public.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");
		assert!(data
			.check_useful_or_unknown(&statement.clone().convert_payload().into())
			.is_ok());
		let noted = data.note_statement(statement);

		assert_matches!(noted, NotedStatement::Fresh(_));

		let statement = SignedFullStatement::sign(
			&keystore,
			Statement::Valid(candidate_hash),
			&signing_context,
			ValidatorIndex(2),
			&charlie_public.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");
		assert!(data
			.check_useful_or_unknown(&statement.clone().convert_payload().into())
			.is_ok());
		let noted = data.note_statement(statement);
		assert_matches!(noted, NotedStatement::Fresh(_));

		data
	};

	active_heads.insert(hash_c, new_head_data);

	let mut peer_data = PeerData {
		view: old_view,
		protocol_version: ValidationVersion::V1,
		view_knowledge: {
			let mut k = HashMap::new();

			k.insert(hash_a, Default::default());
			k.insert(hash_b, Default::default());

			k
		},
		maybe_authority: None,
	};

	let pool = sp_core::testing::TaskExecutor::new();
	let (mut ctx, mut handle) = polkadot_node_subsystem_test_helpers::make_subsystem_context::<
		StatementDistributionMessage,
		_,
	>(pool);
	let peer = PeerId::random();

	executor::block_on(async move {
		let mut topology = GridNeighbors::empty();
		topology.peers_x = HashSet::from_iter(vec![peer].into_iter());
		update_peer_view_and_maybe_send_unlocked(
			peer,
			&topology,
			&mut peer_data,
			&mut ctx,
			&active_heads,
			new_view.clone(),
			&Default::default(),
			&mut AlwaysZeroRng,
		)
		.await;

		assert_eq!(peer_data.view, new_view);
		assert!(!peer_data.view_knowledge.contains_key(&hash_a));
		assert!(peer_data.view_knowledge.contains_key(&hash_b));

		let c_knowledge = peer_data.view_knowledge.get(&hash_c).unwrap();

		assert!(c_knowledge.is_known_candidate(&candidate_hash));
		assert!(c_knowledge
			.sent_statements
			.contains(&(CompactStatement::Seconded(candidate_hash), ValidatorIndex(0))));
		assert!(c_knowledge
			.sent_statements
			.contains(&(CompactStatement::Valid(candidate_hash), ValidatorIndex(1))));
		assert!(c_knowledge
			.sent_statements
			.contains(&(CompactStatement::Valid(candidate_hash), ValidatorIndex(2))));

		// now see if we got the 3 messages from the active head data.
		let active_head = active_heads.get(&hash_c).unwrap();

		// semi-fragile because hashmap iterator ordering is undefined, but in practice
		// it will not change between runs of the program.
		for statement in active_head.statements_about(candidate_hash) {
			let message = handle.recv().await;
			let expected_to = vec![peer];
			let expected_payload = VersionedValidationProtocol::from(Versioned::V1(
				v1_statement_message(hash_c, statement.statement.clone(), &Metrics::default()),
			));

			assert_matches!(
				message,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendValidationMessage(
					to,
					payload,
				)) => {
					assert_eq!(to, expected_to);
					assert_eq!(payload, expected_payload)
				}
			)
		}
	});
}

#[test]
fn circulated_statement_goes_to_all_peers_with_view() {
	let hash_a = Hash::repeat_byte(1);
	let hash_b = Hash::repeat_byte(2);
	let hash_c = Hash::repeat_byte(3);

	let candidate = {
		let mut c = dummy_committed_candidate_receipt(dummy_hash());
		c.descriptor.relay_parent = hash_b;
		c.descriptor.para_id = ParaId::from(1_u32);
		c.into()
	};

	let peer_a = PeerId::random();
	let peer_b = PeerId::random();
	let peer_c = PeerId::random();

	let peer_a_view = view![hash_a];
	let peer_b_view = view![hash_a, hash_b];
	let peer_c_view = view![hash_b, hash_c];

	let session_index = 1;

	let peer_data_from_view = |view: View| PeerData {
		view: view.clone(),
		protocol_version: ValidationVersion::V1,
		view_knowledge: view.iter().map(|v| (*v, Default::default())).collect(),
		maybe_authority: None,
	};

	let mut peer_data: HashMap<_, _> = vec![
		(peer_a, peer_data_from_view(peer_a_view)),
		(peer_b, peer_data_from_view(peer_b_view)),
		(peer_c, peer_data_from_view(peer_c_view)),
	]
	.into_iter()
	.collect();

	let pool = sp_core::testing::TaskExecutor::new();
	let (mut ctx, mut handle) = polkadot_node_subsystem_test_helpers::make_subsystem_context::<
		StatementDistributionMessage,
		_,
	>(pool);

	executor::block_on(async move {
		let signing_context = SigningContext { parent_hash: hash_b, session_index };

		let keystore: KeystorePtr = Arc::new(LocalKeystore::in_memory());
		let alice_public = Keystore::sr25519_generate_new(
			&*keystore,
			ValidatorId::ID,
			Some(&Sr25519Keyring::Alice.to_seed()),
		)
		.unwrap();

		let statement = SignedFullStatement::sign(
			&keystore,
			Statement::Seconded(candidate),
			&signing_context,
			ValidatorIndex(0),
			&alice_public.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let comparator = StoredStatementComparator {
			compact: statement.payload().to_compact(),
			validator_index: ValidatorIndex(0),
			signature: statement.signature().clone(),
		};
		let statement = StoredStatement { comparator: &comparator, statement: &statement };

		let mut topology = GridNeighbors::empty();
		topology.peers_x = HashSet::from_iter(vec![peer_a, peer_b, peer_c].into_iter());
		let needs_dependents = circulate_statement(
			RequiredRouting::GridXY,
			&topology,
			&mut peer_data,
			&mut ctx,
			hash_b,
			statement,
			Vec::new(),
			&Metrics::default(),
			&mut AlwaysZeroRng,
		)
		.await;

		{
			assert_eq!(needs_dependents.len(), 2);
			assert!(needs_dependents.contains(&peer_b));
			assert!(needs_dependents.contains(&peer_c));
		}

		let fingerprint = (statement.compact().clone(), ValidatorIndex(0));

		assert!(peer_data
			.get(&peer_b)
			.unwrap()
			.view_knowledge
			.get(&hash_b)
			.unwrap()
			.sent_statements
			.contains(&fingerprint));

		assert!(peer_data
			.get(&peer_c)
			.unwrap()
			.view_knowledge
			.get(&hash_b)
			.unwrap()
			.sent_statements
			.contains(&fingerprint));

		let message = handle.recv().await;
		assert_matches!(
			message,
			AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendValidationMessage(
				to,
				payload,
			)) => {
				assert_eq!(to.len(), 2);
				assert!(to.contains(&peer_b));
				assert!(to.contains(&peer_c));

				assert_eq!(
					payload,
					VersionedValidationProtocol::from(Versioned::V1(v1_statement_message(hash_b, statement.statement.clone(), &Metrics::default()))),
				);
			}
		)
	});
}

fn make_session_info(validators: Vec<Pair>, groups: Vec<Vec<u32>>) -> SessionInfo {
	let validator_groups: IndexedVec<GroupIndex, Vec<ValidatorIndex>> = groups
		.iter()
		.map(|g| g.into_iter().map(|v| ValidatorIndex(*v)).collect())
		.collect();

	SessionInfo {
		discovery_keys: validators.iter().map(|k| k.public().into()).collect(),
		// Not used:
		n_cores: validator_groups.len() as u32,
		validator_groups,
		validators: validators.iter().map(|k| k.public().into()).collect(),
		// Not used values:
		assignment_keys: Vec::new(),
		zeroth_delay_tranche_width: 0,
		relay_vrf_modulo_samples: 0,
		n_delay_tranches: 0,
		no_show_slots: 0,
		needed_approvals: 0,
		active_validator_indices: Vec::new(),
		dispute_period: 6,
		random_seed: [0u8; 32],
	}
}

fn derive_metadata_assuming_seconded(
	hash: Hash,
	statement: UncheckedSignedFullStatement,
) -> protocol_v1::StatementMetadata {
	protocol_v1::StatementMetadata {
		relay_parent: hash,
		candidate_hash: statement.unchecked_payload().candidate_hash(),
		signed_by: statement.unchecked_validator_index(),
		signature: statement.unchecked_signature().clone(),
	}
}

// TODO [now]: adapt most tests to v2 messages.
// TODO [now]: test that v2 peers send v1 messages to v1 peers
// TODO [now]: test that v2 peers handle v1 messages from v1 peers.
// TODO [now]: test that v2 peers send v2 messages to v2 peers.
