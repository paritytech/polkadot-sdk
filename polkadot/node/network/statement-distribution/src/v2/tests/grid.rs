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

use super::*;

use bitvec::order::Lsb0;
use polkadot_node_network_protocol::v2::{BackedCandidateAcknowledgement, BackedCandidateManifest};
use polkadot_node_subsystem::messages::CandidateBackingMessage;
use polkadot_primitives_test_helpers::make_candidate;

// Backed candidate leads to advertisement to relevant validators with relay-parent.
#[test]
fn backed_candidate_leads_to_advertisement() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let peer_a = PeerId::random();
	let peer_b = PeerId::random();
	let peer_c = PeerId::random();
	let peer_d = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let local_validator = state.local.clone().unwrap();
		let local_group_index = local_validator.group_index.unwrap();
		let local_para = ParaId::from(local_group_index.0);

		let other_group = next_group_index(local_group_index, validator_count, group_size);

		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			local_para,
			test_leaf.para_data(local_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let local_group_validators = state.group_validators(local_group_index, true);
		let other_group_validators = state.group_validators(other_group, true);
		let v_a = local_group_validators[0];
		let v_b = local_group_validators[1];
		let v_c = other_group_validators[0];
		let v_d = other_group_validators[1];

		// peer A is in group, has relay parent in view.
		// peer B is in group, has no relay parent in view.
		// peer C is not in group, has relay parent in view.
		// peer D is not in group, has no relay parent in view.
		{
			connect_peer(
				&mut overseer,
				peer_a.clone(),
				Some(vec![state.discovery_id(v_a)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_b.clone(),
				Some(vec![state.discovery_id(v_b)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_c.clone(),
				Some(vec![state.discovery_id(v_c)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_d.clone(),
				Some(vec![state.discovery_id(v_d)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_a.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &test_leaf, &state, true, vec![]).await;

		// Send gossip topology.
		send_new_topology(&mut overseer, state.make_dummy_topology()).await;

		// Confirm the candidate locally so that we don't send out requests.
		{
			let statement = state
				.sign_full_statement(
					local_validator.validator_index,
					Statement::Seconded(candidate.clone()),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
					pvd.clone(),
				)
				.clone();

			overseer
				.send(FromOrchestra::Communication {
					msg: StatementDistributionMessage::Share(relay_parent, statement),
				})
				.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendValidationMessage(peers, _)) if peers == vec![peer_a]
			);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		// Send enough statements to make candidate backable, make sure announcements are sent.

		// Send statement from peer A.
		{
			let statement = state
				.sign_statement(
					v_a,
					CompactStatement::Seconded(candidate_hash),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
				)
				.as_unchecked()
				.clone();

			send_peer_message(
				&mut overseer,
				peer_a.clone(),
				protocol_v2::StatementDistributionMessage::Statement(relay_parent, statement),
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_a && r == BENEFIT_VALID_STATEMENT_FIRST.into() => { }
			);
		}

		// Send statement from peer B.
		{
			let statement = state
				.sign_statement(
					v_b,
					CompactStatement::Seconded(candidate_hash),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
				)
				.as_unchecked()
				.clone();

			send_peer_message(
				&mut overseer,
				peer_b.clone(),
				protocol_v2::StatementDistributionMessage::Statement(relay_parent, statement),
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_b && r == BENEFIT_VALID_STATEMENT_FIRST.into() => { }
			);

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendValidationMessage(peers, _)) if peers == vec![peer_a]
			);
		}

		// Send Backed notification.
		{
			overseer
				.send(FromOrchestra::Communication {
					msg: StatementDistributionMessage::Backed(candidate_hash),
				})
				.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages:: NetworkBridgeTx(
					NetworkBridgeTxMessage::SendValidationMessage(
						peers,
						Versioned::V2(
							protocol_v2::ValidationProtocol::StatementDistribution(
								protocol_v2::StatementDistributionMessage::BackedCandidateManifest(manifest),
							),
						),
					)
				) => {
					assert_eq!(peers, vec![peer_c]);
					assert_eq!(manifest, BackedCandidateManifest {
						relay_parent,
						candidate_hash,
						group_index: local_group_index,
						para_id: local_para,
						parent_head_data_hash: pvd.parent_head.hash(),
						statement_knowledge: StatementFilter {
							seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 1],
							validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
						},
					});
				}
			);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		overseer
	});
}

#[test]
fn received_advertisement_before_confirmation_leads_to_request() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let peer_a = PeerId::random();
	let peer_b = PeerId::random();
	let peer_c = PeerId::random();
	let peer_d = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let local_validator = state.local.clone().unwrap();
		let local_group_index = local_validator.group_index.unwrap();

		let other_group = next_group_index(local_group_index, validator_count, group_size);
		let other_para = ParaId::from(other_group.0);

		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			other_para,
			test_leaf.para_data(other_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let local_group_validators = state.group_validators(local_group_index, true);
		let other_group_validators = state.group_validators(other_group, true);
		let v_a = local_group_validators[0];
		let v_b = local_group_validators[1];
		let v_c = other_group_validators[0];
		let v_d = other_group_validators[1];

		// peer A is in group, has relay parent in view.
		// peer B is in group, has no relay parent in view.
		// peer C is not in group, has relay parent in view.
		// peer D is not in group, has relay parent in view.
		{
			connect_peer(
				&mut overseer,
				peer_a.clone(),
				Some(vec![state.discovery_id(v_a)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_b.clone(),
				Some(vec![state.discovery_id(v_b)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_c.clone(),
				Some(vec![state.discovery_id(v_c)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_d.clone(),
				Some(vec![state.discovery_id(v_d)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_a.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_d.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &test_leaf, &state, true, vec![]).await;

		// Send gossip topology.
		send_new_topology(&mut overseer, state.make_dummy_topology()).await;

		// Receive an advertisement from C on an unconfirmed candidate.
		{
			let manifest = BackedCandidateManifest {
				relay_parent,
				candidate_hash,
				group_index: other_group,
				para_id: other_para,
				parent_head_data_hash: pvd.parent_head.hash(),
				statement_knowledge: StatementFilter {
					seconded_in_group: bitvec::bitvec![u8, Lsb0; 0, 1, 1],
					validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
				},
			};
			send_peer_message(
				&mut overseer,
				peer_c.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(manifest),
			)
			.await;

			let statements = vec![
				state
					.sign_statement(
						v_c,
						CompactStatement::Seconded(candidate_hash),
						&SigningContext { parent_hash: relay_parent, session_index: 1 },
					)
					.as_unchecked()
					.clone(),
				state
					.sign_statement(
						v_d,
						CompactStatement::Seconded(candidate_hash),
						&SigningContext { parent_hash: relay_parent, session_index: 1 },
					)
					.as_unchecked()
					.clone(),
			];
			handle_sent_request(
				&mut overseer,
				peer_c,
				candidate_hash,
				StatementFilter::blank(group_size),
				candidate.clone(),
				pvd.clone(),
				statements,
			)
			.await;

			// C provided two statements we're seeing for the first time.
			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_STATEMENT.into() => { }
			);
			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_STATEMENT.into() => { }
			);

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_RESPONSE.into() => { }
			);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		overseer
	});
}

// 1. We receive manifest from grid peer, request, pass votes to backing, then receive Backed
// message. Only then should we send an acknowledgement to the grid peer.
//
// 2. (starting from end state of (1)) we receive a manifest about the same candidate from another
// grid peer and instantaneously acknowledge.
//
// Bit more context about this design choice: Statement-distribution doesn't fully emulate the
// statement logic of backing and only focuses on the number of statements. That means that we might
// request a manifest and for some reason the backing subsystem would still not consider the
// candidate as backed. So, in particular, we don't want to advertise such an unbacked candidate
// along the grid & increase load on ourselves and our peers for serving & importing such a
// candidate.
#[test]
fn received_advertisement_after_backing_leads_to_acknowledgement() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	test_harness(config, |state, mut overseer| async move {
		let peers_to_connect = [
			TestPeerToConnect { local: true, relay_parent_in_view: false },
			TestPeerToConnect { local: true, relay_parent_in_view: false },
			TestPeerToConnect { local: false, relay_parent_in_view: true },
			TestPeerToConnect { local: false, relay_parent_in_view: true },
			TestPeerToConnect { local: false, relay_parent_in_view: true },
		];

		let TestSetupInfo {
			other_group,
			other_para,
			relay_parent,
			test_leaf,
			peers,
			validators,
			..
		} = setup_test_and_connect_peers(
			&state,
			&mut overseer,
			validator_count,
			group_size,
			&peers_to_connect,
			false,
		)
		.await;
		let [_, _, peer_c, peer_d, _] = peers[..] else { panic!() };
		let [_, _, v_c, v_d, v_e] = validators[..] else { panic!() };

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			other_para,
			test_leaf.para_data(other_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let manifest = BackedCandidateManifest {
			relay_parent,
			candidate_hash,
			group_index: other_group,
			para_id: other_para,
			parent_head_data_hash: pvd.parent_head.hash(),
			statement_knowledge: StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 1],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
			},
		};

		let statement_c = state
			.sign_statement(
				v_c,
				CompactStatement::Seconded(candidate_hash),
				&SigningContext { parent_hash: relay_parent, session_index: 1 },
			)
			.as_unchecked()
			.clone();
		let statement_d = state
			.sign_statement(
				v_d,
				CompactStatement::Seconded(candidate_hash),
				&SigningContext { parent_hash: relay_parent, session_index: 1 },
			)
			.as_unchecked()
			.clone();

		// Receive an advertisement from C.
		{
			send_manifest_from_peer(&mut overseer, peer_c, manifest.clone()).await;

			// Should send a request to C.
			let statements = vec![
				statement_c.clone(),
				statement_d.clone(),
				state
					.sign_statement(
						v_e,
						CompactStatement::Seconded(candidate_hash),
						&SigningContext { parent_hash: relay_parent, session_index: 1 },
					)
					.as_unchecked()
					.clone(),
			];
			handle_sent_request(
				&mut overseer,
				peer_c,
				candidate_hash,
				StatementFilter::blank(group_size),
				candidate.clone(),
				pvd.clone(),
				statements,
			)
			.await;

			assert_peer_reported!(&mut overseer, peer_c, BENEFIT_VALID_STATEMENT);
			assert_peer_reported!(&mut overseer, peer_c, BENEFIT_VALID_STATEMENT);
			assert_peer_reported!(&mut overseer, peer_c, BENEFIT_VALID_STATEMENT);
			assert_peer_reported!(&mut overseer, peer_c, BENEFIT_VALID_RESPONSE);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		// Receive Backed message.
		send_backed_message(&mut overseer, candidate_hash).await;

		// Should send an acknowledgement back to C.
		{
			assert_matches!(
				overseer.recv().await,
				AllMessages:: NetworkBridgeTx(
					NetworkBridgeTxMessage::SendValidationMessage(
						peers,
						Versioned::V2(
							protocol_v2::ValidationProtocol::StatementDistribution(
								protocol_v2::StatementDistributionMessage::BackedCandidateKnown(ack),
							),
						),
					)
				) => {
					assert_eq!(peers, vec![peer_c]);
					assert_eq!(ack, BackedCandidateAcknowledgement {
						candidate_hash,
						statement_knowledge: StatementFilter {
							seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 1],
							validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
						},
					});
				}
			);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		// Receive a manifest about the same candidate from peer D.
		{
			send_manifest_from_peer(&mut overseer, peer_d, manifest.clone()).await;

			let expected_ack = BackedCandidateAcknowledgement {
				candidate_hash,
				statement_knowledge: StatementFilter {
					seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 1],
					validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
				},
			};

			// Instantaneously acknowledge.
			assert_matches!(
				overseer.recv().await,
				AllMessages:: NetworkBridgeTx(
					NetworkBridgeTxMessage::SendValidationMessages(messages)
				) => {
					assert_eq!(messages.len(), 1);
					assert_eq!(messages[0].0, vec![peer_d]);

					assert_matches!(
						&messages[0].1,
						Versioned::V2(protocol_v2::ValidationProtocol::StatementDistribution(
							protocol_v2::StatementDistributionMessage::BackedCandidateKnown(ack)
						)) if *ack == expected_ack
					);
				}
			);
		}

		overseer
	});
}

#[test]
fn receive_ack_for_unconfirmed_candidate() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	test_harness(config, |state, mut overseer| async move {
		let peers_to_connect = [
			TestPeerToConnect { local: true, relay_parent_in_view: true },
			TestPeerToConnect { local: true, relay_parent_in_view: false },
			TestPeerToConnect { local: false, relay_parent_in_view: true },
			TestPeerToConnect { local: false, relay_parent_in_view: false },
		];
		let TestSetupInfo { local_para, relay_parent, test_leaf, peers, .. } =
			setup_test_and_connect_peers(
				&state,
				&mut overseer,
				validator_count,
				group_size,
				&peers_to_connect,
				false,
			)
			.await;
		let [_, _, peer_c, _] = peers[..] else { panic!() };

		let (candidate, _pvd) = make_candidate(
			relay_parent,
			1,
			local_para,
			test_leaf.para_data(local_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let ack = BackedCandidateAcknowledgement {
			candidate_hash,
			statement_knowledge: StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 1],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
			},
		};

		// Receive an acknowledgement from a peer before the candidate is confirmed.
		send_ack_from_peer(&mut overseer, peer_c, ack.clone()).await;
		assert_peer_reported!(
			&mut overseer,
			peer_c,
			COST_UNEXPECTED_ACKNOWLEDGEMENT_UNKNOWN_CANDIDATE,
		);

		overseer
	});
}

// Test receiving unexpected and expected acknowledgements for a locally confirmed candidate.
#[test]
fn received_acknowledgements_for_locally_confirmed() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	test_harness(config, |state, mut overseer| async move {
		let peers_to_connect = [
			TestPeerToConnect { local: true, relay_parent_in_view: true },
			TestPeerToConnect { local: true, relay_parent_in_view: false },
			TestPeerToConnect { local: false, relay_parent_in_view: true },
			TestPeerToConnect { local: false, relay_parent_in_view: false },
		];
		let TestSetupInfo {
			local_validator,
			local_group,
			local_para,
			relay_parent,
			test_leaf,
			peers,
			validators,
			..
		} = setup_test_and_connect_peers(
			&state,
			&mut overseer,
			validator_count,
			group_size,
			&peers_to_connect,
			true,
		)
		.await;
		let [peer_a, peer_b, peer_c, peer_d] = peers[..] else { panic!() };
		let [_, v_b, _, _] = validators[..] else { panic!() };

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			local_para,
			test_leaf.para_data(local_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let ack = BackedCandidateAcknowledgement {
			candidate_hash,
			statement_knowledge: StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 1],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
			},
		};

		// Confirm the candidate locally so that we don't send out requests.
		{
			let statement = state
				.sign_full_statement(
					local_validator.validator_index,
					Statement::Seconded(candidate.clone()),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
					pvd.clone(),
				)
				.clone();

			send_share_message(&mut overseer, relay_parent, statement).await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendValidationMessage(peers, _)) if peers == vec![peer_a]
			);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		// Receive an unexpected acknowledgement from peer D.
		send_ack_from_peer(&mut overseer, peer_d, ack.clone()).await;
		assert_peer_reported!(&mut overseer, peer_d, COST_UNEXPECTED_MANIFEST_DISALLOWED);

		// Send statement from peer B.
		{
			let statement = state
				.sign_statement(
					v_b,
					CompactStatement::Seconded(candidate_hash),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
				)
				.as_unchecked()
				.clone();

			send_peer_message(
				&mut overseer,
				peer_b.clone(),
				protocol_v2::StatementDistributionMessage::Statement(relay_parent, statement),
			)
			.await;

			assert_peer_reported!(&mut overseer, peer_b, BENEFIT_VALID_STATEMENT_FIRST);

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendValidationMessage(peers, _)) if peers == vec![peer_a]
			);
		}

		// Send Backed notification.
		{
			send_backed_message(&mut overseer, candidate_hash).await;

			// We should send out a manifest.
			assert_matches!(
				overseer.recv().await,
				AllMessages:: NetworkBridgeTx(
					NetworkBridgeTxMessage::SendValidationMessage(
						peers,
						Versioned::V2(
							protocol_v2::ValidationProtocol::StatementDistribution(
								protocol_v2::StatementDistributionMessage::BackedCandidateManifest(manifest),
							),
						),
					)
				) => {
					assert_eq!(peers, vec![peer_c]);
					assert_eq!(manifest, BackedCandidateManifest {
						relay_parent,
						candidate_hash,
						group_index: local_group,
						para_id: local_para,
						parent_head_data_hash: pvd.parent_head.hash(),
						statement_knowledge: StatementFilter {
							seconded_in_group: bitvec::bitvec![u8, Lsb0; 0, 1, 1],
							validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
						},
					});
				}
			);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		// Receive an unexpected acknowledgement from peer D.
		//
		// It still shouldn't know this manifest.
		send_ack_from_peer(&mut overseer, peer_d, ack.clone()).await;
		assert_peer_reported!(&mut overseer, peer_d, COST_UNEXPECTED_MANIFEST_DISALLOWED);

		// Receive an acknowledgement from peer C.
		//
		// It's OK, we know they know it because we sent them a manifest.
		send_ack_from_peer(&mut overseer, peer_c, ack.clone()).await;

		// What happens if we get another valid ack?
		send_ack_from_peer(&mut overseer, peer_c, ack.clone()).await;

		overseer
	});
}

// Test receiving unexpected acknowledgements for a candidate confirmed in a different group.
#[test]
fn received_acknowledgements_for_externally_confirmed() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	test_harness(config, |state, mut overseer| async move {
		let peers_to_connect = [
			TestPeerToConnect { local: true, relay_parent_in_view: true },
			TestPeerToConnect { local: true, relay_parent_in_view: false },
			TestPeerToConnect { local: false, relay_parent_in_view: true },
			TestPeerToConnect { local: false, relay_parent_in_view: true },
			TestPeerToConnect { local: false, relay_parent_in_view: true },
		];
		let TestSetupInfo {
			other_group,
			other_para,
			relay_parent,
			test_leaf,
			peers,
			validators,
			..
		} = setup_test_and_connect_peers(
			&state,
			&mut overseer,
			validator_count,
			group_size,
			&peers_to_connect,
			false,
		)
		.await;
		let [peer_a, _, peer_c, peer_d, _] = peers[..] else { panic!() };
		let [_, _, v_c, v_d, v_e] = validators[..] else { panic!() };

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			other_para,
			test_leaf.para_data(other_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let manifest = BackedCandidateManifest {
			relay_parent,
			candidate_hash,
			group_index: other_group,
			para_id: other_para,
			parent_head_data_hash: pvd.parent_head.hash(),
			statement_knowledge: StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 0, 1, 1],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
			},
		};

		let statement_c = state
			.sign_statement(
				v_c,
				CompactStatement::Seconded(candidate_hash),
				&SigningContext { parent_hash: relay_parent, session_index: 1 },
			)
			.as_unchecked()
			.clone();
		let statement_d = state
			.sign_statement(
				v_d,
				CompactStatement::Seconded(candidate_hash),
				&SigningContext { parent_hash: relay_parent, session_index: 1 },
			)
			.as_unchecked()
			.clone();

		// Receive an advertisement from C, confirming the candidate.
		{
			send_manifest_from_peer(&mut overseer, peer_c, manifest.clone()).await;

			// Should send a request to C.
			let statements = vec![
				statement_c.clone(),
				statement_d.clone(),
				state
					.sign_statement(
						v_e,
						CompactStatement::Seconded(candidate_hash),
						&SigningContext { parent_hash: relay_parent, session_index: 1 },
					)
					.as_unchecked()
					.clone(),
			];
			handle_sent_request(
				&mut overseer,
				peer_c,
				candidate_hash,
				StatementFilter::blank(group_size),
				candidate.clone(),
				pvd.clone(),
				statements,
			)
			.await;

			assert_peer_reported!(&mut overseer, peer_c, BENEFIT_VALID_STATEMENT);
			assert_peer_reported!(&mut overseer, peer_c, BENEFIT_VALID_STATEMENT);
			assert_peer_reported!(&mut overseer, peer_c, BENEFIT_VALID_STATEMENT);
			assert_peer_reported!(&mut overseer, peer_c, BENEFIT_VALID_RESPONSE);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		let ack = BackedCandidateAcknowledgement {
			candidate_hash,
			statement_knowledge: StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 1],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
			},
		};

		// Receive an unexpected acknowledgement from peer D.
		send_ack_from_peer(&mut overseer, peer_d, ack.clone()).await;
		assert_peer_reported!(&mut overseer, peer_d, COST_UNEXPECTED_MANIFEST_PEER_UNKNOWN);

		// Receive an unexpected acknowledgement from peer A.
		send_ack_from_peer(&mut overseer, peer_a, ack.clone()).await;
		assert_peer_reported!(&mut overseer, peer_a, COST_UNEXPECTED_MANIFEST_DISALLOWED);

		overseer
	});
}

// Received advertisement after confirmation but before backing leads to nothing.
#[test]
fn received_advertisement_after_confirmation_before_backing() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let peer_c = PeerId::random();
	let peer_d = PeerId::random();
	let peer_e = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let local_validator = state.local.clone().unwrap();
		let local_group_index = local_validator.group_index.unwrap();

		let other_group = next_group_index(local_group_index, validator_count, group_size);
		let other_para = ParaId::from(other_group.0);

		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			other_para,
			test_leaf.para_data(other_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let other_group_validators = state.group_validators(other_group, true);
		let v_c = other_group_validators[0];
		let v_d = other_group_validators[1];
		let v_e = other_group_validators[2];

		// Connect C, D, E
		{
			connect_peer(
				&mut overseer,
				peer_c.clone(),
				Some(vec![state.discovery_id(v_c)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_d.clone(),
				Some(vec![state.discovery_id(v_d)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_e.clone(),
				Some(vec![state.discovery_id(v_e)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_d.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_e.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &test_leaf, &state, true, vec![]).await;

		// Send gossip topology.
		send_new_topology(&mut overseer, state.make_dummy_topology()).await;

		let manifest = BackedCandidateManifest {
			relay_parent,
			candidate_hash,
			group_index: other_group,
			para_id: other_para,
			parent_head_data_hash: pvd.parent_head.hash(),
			statement_knowledge: StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 0, 1, 1],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
			},
		};

		let statement_c = state
			.sign_statement(
				v_c,
				CompactStatement::Seconded(candidate_hash),
				&SigningContext { parent_hash: relay_parent, session_index: 1 },
			)
			.as_unchecked()
			.clone();
		let statement_d = state
			.sign_statement(
				v_d,
				CompactStatement::Seconded(candidate_hash),
				&SigningContext { parent_hash: relay_parent, session_index: 1 },
			)
			.as_unchecked()
			.clone();

		// Receive an advertisement from C.
		{
			send_peer_message(
				&mut overseer,
				peer_c.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(
					manifest.clone(),
				),
			)
			.await;

			// Should send a request to C.
			let statements = vec![
				statement_c.clone(),
				statement_d.clone(),
				state
					.sign_statement(
						v_e,
						CompactStatement::Seconded(candidate_hash),
						&SigningContext { parent_hash: relay_parent, session_index: 1 },
					)
					.as_unchecked()
					.clone(),
			];
			handle_sent_request(
				&mut overseer,
				peer_c,
				candidate_hash,
				StatementFilter::blank(group_size),
				candidate.clone(),
				pvd.clone(),
				statements,
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_STATEMENT.into()
			);
			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_STATEMENT.into()
			);
			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_STATEMENT.into()
			);

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_RESPONSE.into()
			);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		// Receive advertisement from peer D (after confirmation but before backing).
		{
			send_peer_message(
				&mut overseer,
				peer_d.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(
					manifest.clone(),
				),
			)
			.await;
		}

		overseer
	});
}

#[test]
fn additional_statements_are_shared_after_manifest_exchange() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let peer_c = PeerId::random();
	let peer_d = PeerId::random();
	let peer_e = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let local_validator = state.local.clone().unwrap();
		let local_group_index = local_validator.group_index.unwrap();

		let other_group = next_group_index(local_group_index, validator_count, group_size);
		let other_para = ParaId::from(other_group.0);

		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			other_para,
			test_leaf.para_data(other_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let other_group_validators = state.group_validators(other_group, true);
		let v_c = other_group_validators[0];
		let v_d = other_group_validators[1];
		let v_e = other_group_validators[2];

		// Connect C, D, E
		{
			connect_peer(
				&mut overseer,
				peer_c.clone(),
				Some(vec![state.discovery_id(v_c)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_d.clone(),
				Some(vec![state.discovery_id(v_d)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_e.clone(),
				Some(vec![state.discovery_id(v_e)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_d.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_e.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &test_leaf, &state, true, vec![]).await;

		// Send gossip topology.
		send_new_topology(&mut overseer, state.make_dummy_topology()).await;

		// Receive an advertisement from C.
		{
			let manifest = BackedCandidateManifest {
				relay_parent,
				candidate_hash,
				group_index: other_group,
				para_id: other_para,
				parent_head_data_hash: pvd.parent_head.hash(),
				statement_knowledge: StatementFilter {
					seconded_in_group: bitvec::bitvec![u8, Lsb0; 0, 1, 1],
					validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
				},
			};

			send_peer_message(
				&mut overseer,
				peer_c.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(
					manifest.clone(),
				),
			)
			.await;
		}

		// Should send a request to C.
		{
			let statements = vec![
				state
					.sign_statement(
						v_d,
						CompactStatement::Seconded(candidate_hash),
						&SigningContext { parent_hash: relay_parent, session_index: 1 },
					)
					.as_unchecked()
					.clone(),
				state
					.sign_statement(
						v_e,
						CompactStatement::Seconded(candidate_hash),
						&SigningContext { parent_hash: relay_parent, session_index: 1 },
					)
					.as_unchecked()
					.clone(),
			];

			handle_sent_request(
				&mut overseer,
				peer_c,
				candidate_hash,
				StatementFilter::blank(group_size),
				candidate.clone(),
				pvd.clone(),
				statements,
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_STATEMENT.into()
			);
			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_STATEMENT.into()
			);

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_RESPONSE.into()
			);
		}

		let hypothetical = HypotheticalCandidate::Complete {
			candidate_hash,
			receipt: Arc::new(candidate.clone()),
			persisted_validation_data: pvd.clone(),
		};
		let membership = vec![(relay_parent, vec![0])];
		answer_expected_hypothetical_depth_request(&mut overseer, vec![(hypothetical, membership)])
			.await;

		// Statements are sent to the Backing subsystem.
		{
			assert_matches!(
				overseer.recv().await,
				AllMessages::CandidateBacking(
					CandidateBackingMessage::Statement(hash, statement)
				) => {
					assert_eq!(hash, relay_parent);
					assert_matches!(
						statement.payload(),
						FullStatementWithPVD::Seconded(c, p)
							if c == &candidate && p == &pvd
					);
				}
			);
			assert_matches!(
				overseer.recv().await,
				AllMessages::CandidateBacking(
					CandidateBackingMessage::Statement(hash, statement)
				) => {
					assert_eq!(hash, relay_parent);
					assert_matches!(
						statement.payload(),
						FullStatementWithPVD::Seconded(c, p)
							if c == &candidate && p == &pvd
					);
				}
			);
		}

		// Receive Backed message.
		overseer
			.send(FromOrchestra::Communication {
				msg: StatementDistributionMessage::Backed(candidate_hash),
			})
			.await;

		// Should send an acknowledgement back to C.
		{
			assert_matches!(
				overseer.recv().await,
				AllMessages:: NetworkBridgeTx(
					NetworkBridgeTxMessage::SendValidationMessage(
						peers,
						Versioned::V2(
							protocol_v2::ValidationProtocol::StatementDistribution(
								protocol_v2::StatementDistributionMessage::BackedCandidateKnown(ack),
							),
						),
					)
				) => {
					assert_eq!(peers, vec![peer_c]);
					assert_eq!(ack, BackedCandidateAcknowledgement {
						candidate_hash,
						statement_knowledge: StatementFilter {
							seconded_in_group: bitvec::bitvec![u8, Lsb0; 0, 1, 1],
							validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
						},
					});
				}
			);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		// Receive a manifest about the same candidate from peer D. Contains different statements.
		{
			let manifest = BackedCandidateManifest {
				relay_parent,
				candidate_hash,
				group_index: other_group,
				para_id: other_para,
				parent_head_data_hash: pvd.parent_head.hash(),
				statement_knowledge: StatementFilter {
					seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 0],
					validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
				},
			};

			send_peer_message(
				&mut overseer,
				peer_d.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(
					manifest.clone(),
				),
			)
			.await;

			let expected_ack = BackedCandidateAcknowledgement {
				candidate_hash,
				statement_knowledge: StatementFilter {
					seconded_in_group: bitvec::bitvec![u8, Lsb0; 0, 1, 1],
					validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
				},
			};

			// Instantaneously acknowledge.
			assert_matches!(
				overseer.recv().await,
				AllMessages:: NetworkBridgeTx(
					NetworkBridgeTxMessage::SendValidationMessages(messages)
				) => {
					assert_eq!(messages.len(), 2);
					assert_eq!(messages[0].0, vec![peer_d]);
					assert_eq!(messages[1].0, vec![peer_d]);

					assert_matches!(
						&messages[0].1,
						Versioned::V2(protocol_v2::ValidationProtocol::StatementDistribution(
							protocol_v2::StatementDistributionMessage::BackedCandidateKnown(ack)
						)) if *ack == expected_ack
					);

					assert_matches!(
						&messages[1].1,
						Versioned::V2(protocol_v2::ValidationProtocol::StatementDistribution(
							protocol_v2::StatementDistributionMessage::Statement(r, s)
						)) if *r == relay_parent && s.unchecked_payload() == &CompactStatement::Seconded(candidate_hash) && s.unchecked_validator_index() == v_e
					);
				}
			);
		}

		overseer
	});
}

// Grid-sending validator view entering relay-parent leads to advertisement.
#[test]
fn advertisement_sent_when_peer_enters_relay_parent_view() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let peer_a = PeerId::random();
	let peer_b = PeerId::random();
	let peer_c = PeerId::random();
	let peer_d = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let local_validator = state.local.clone().unwrap();
		let local_group_index = local_validator.group_index.unwrap();
		let local_para = ParaId::from(local_group_index.0);

		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			local_para,
			test_leaf.para_data(local_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let local_group_validators = state.group_validators(local_group_index, true);
		let other_group_validators = state.group_validators((local_group_index.0 + 1).into(), true);
		let v_a = local_group_validators[0];
		let v_b = local_group_validators[1];
		let v_c = other_group_validators[0];
		let v_d = other_group_validators[1];

		// peer A is in group, has relay parent in view.
		// peer B is in group, has no relay parent in view.
		// peer C is not in group, has relay parent in view.
		// peer D is not in group, has no relay parent in view.
		{
			connect_peer(
				&mut overseer,
				peer_a.clone(),
				Some(vec![state.discovery_id(v_a)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_b.clone(),
				Some(vec![state.discovery_id(v_b)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_c.clone(),
				Some(vec![state.discovery_id(v_c)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_d.clone(),
				Some(vec![state.discovery_id(v_d)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_a.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &test_leaf, &state, true, vec![]).await;

		send_new_topology(&mut overseer, state.make_dummy_topology()).await;

		// Confirm the candidate locally so that we don't send out requests.
		{
			let statement = state
				.sign_full_statement(
					local_validator.validator_index,
					Statement::Seconded(candidate.clone()),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
					pvd.clone(),
				)
				.clone();

			overseer
				.send(FromOrchestra::Communication {
					msg: StatementDistributionMessage::Share(relay_parent, statement),
				})
				.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendValidationMessage(peers, _)) if peers == vec![peer_a]
			);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		// Send enough statements to make candidate backable, make sure announcements are sent.

		// Send statement from peer A.
		{
			let statement = state
				.sign_statement(
					v_a,
					CompactStatement::Seconded(candidate_hash),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
				)
				.as_unchecked()
				.clone();

			send_peer_message(
				&mut overseer,
				peer_a.clone(),
				protocol_v2::StatementDistributionMessage::Statement(relay_parent, statement),
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_a && r == BENEFIT_VALID_STATEMENT_FIRST.into() => { }
			);
		}

		// Send statement from peer B.
		{
			let statement = state
				.sign_statement(
					v_b,
					CompactStatement::Seconded(candidate_hash),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
				)
				.as_unchecked()
				.clone();

			send_peer_message(
				&mut overseer,
				peer_b.clone(),
				protocol_v2::StatementDistributionMessage::Statement(relay_parent, statement),
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_b && r == BENEFIT_VALID_STATEMENT_FIRST.into() => { }
			);

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendValidationMessage(peers, _)) if peers == vec![peer_a]
			);
		}

		// Send Backed notification.
		overseer
			.send(FromOrchestra::Communication {
				msg: StatementDistributionMessage::Backed(candidate_hash),
			})
			.await;

		answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;

		// Relay parent enters view of peer C.
		{
			send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;

			let expected_manifest = BackedCandidateManifest {
				relay_parent,
				candidate_hash,
				group_index: local_group_index,
				para_id: local_para,
				parent_head_data_hash: pvd.parent_head.hash(),
				statement_knowledge: StatementFilter {
					seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 1],
					validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
				},
			};

			assert_matches!(
				overseer.recv().await,
				AllMessages:: NetworkBridgeTx(
					NetworkBridgeTxMessage::SendValidationMessages(messages)
				) => {
					assert_eq!(messages.len(), 1);
					assert_eq!(messages[0].0, vec![peer_c]);

					assert_matches!(
						&messages[0].1,
						Versioned::V2(protocol_v2::ValidationProtocol::StatementDistribution(
							protocol_v2::StatementDistributionMessage::BackedCandidateManifest(manifest)
						)) => {
							assert_eq!(*manifest, expected_manifest);
						}
					);
				}
			);
		}

		overseer
	});
}

// Advertisement not re-sent after re-entering relay parent (view oscillation).
#[test]
fn advertisement_not_re_sent_when_peer_re_enters_view() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let peer_a = PeerId::random();
	let peer_b = PeerId::random();
	let peer_c = PeerId::random();
	let peer_d = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let local_validator = state.local.clone().unwrap();
		let local_group_index = local_validator.group_index.unwrap();
		let local_para = ParaId::from(local_group_index.0);

		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			local_para,
			test_leaf.para_data(local_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let local_group_validators = state.group_validators(local_group_index, true);
		let other_group_validators = state.group_validators((local_group_index.0 + 1).into(), true);
		let v_a = local_group_validators[0];
		let v_b = local_group_validators[1];
		let v_c = other_group_validators[0];
		let v_d = other_group_validators[1];

		// peer A is in group, has relay parent in view.
		// peer B is in group, has no relay parent in view.
		// peer C is not in group, has relay parent in view.
		// peer D is not in group, has no relay parent in view.
		{
			connect_peer(
				&mut overseer,
				peer_a.clone(),
				Some(vec![state.discovery_id(v_a)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_b.clone(),
				Some(vec![state.discovery_id(v_b)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_c.clone(),
				Some(vec![state.discovery_id(v_c)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_d.clone(),
				Some(vec![state.discovery_id(v_d)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_a.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &test_leaf, &state, true, vec![]).await;

		send_new_topology(&mut overseer, state.make_dummy_topology()).await;

		// Confirm the candidate locally so that we don't send out requests.
		{
			let statement = state
				.sign_full_statement(
					local_validator.validator_index,
					Statement::Seconded(candidate.clone()),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
					pvd.clone(),
				)
				.clone();

			overseer
				.send(FromOrchestra::Communication {
					msg: StatementDistributionMessage::Share(relay_parent, statement),
				})
				.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendValidationMessage(peers, _)) if peers == vec![peer_a]
			);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		// Send enough statements to make candidate backable, make sure announcements are sent.

		// Send statement from peer A.
		{
			let statement = state
				.sign_statement(
					v_a,
					CompactStatement::Seconded(candidate_hash),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
				)
				.as_unchecked()
				.clone();

			send_peer_message(
				&mut overseer,
				peer_a.clone(),
				protocol_v2::StatementDistributionMessage::Statement(relay_parent, statement),
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_a && r == BENEFIT_VALID_STATEMENT_FIRST.into() => { }
			);
		}

		// Send statement from peer B.
		{
			let statement = state
				.sign_statement(
					v_b,
					CompactStatement::Seconded(candidate_hash),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
				)
				.as_unchecked()
				.clone();

			send_peer_message(
				&mut overseer,
				peer_b.clone(),
				protocol_v2::StatementDistributionMessage::Statement(relay_parent, statement),
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_b && r == BENEFIT_VALID_STATEMENT_FIRST.into() => { }
			);

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendValidationMessage(peers, _)) if peers == vec![peer_a]
			);
		}

		// Send Backed notification.
		{
			overseer
				.send(FromOrchestra::Communication {
					msg: StatementDistributionMessage::Backed(candidate_hash),
				})
				.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages:: NetworkBridgeTx(
					NetworkBridgeTxMessage::SendValidationMessage(
						peers,
						Versioned::V2(
							protocol_v2::ValidationProtocol::StatementDistribution(
								protocol_v2::StatementDistributionMessage::BackedCandidateManifest(manifest),
							),
						),
					)
				) => {
					assert_eq!(peers, vec![peer_c]);
					assert_eq!(manifest, BackedCandidateManifest {
						relay_parent,
						candidate_hash,
						group_index: local_group_index,
						para_id: local_para,
						parent_head_data_hash: pvd.parent_head.hash(),
						statement_knowledge: StatementFilter {
							seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 1],
							validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
						},
					});
				}
			);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		// Peer leaves view.
		send_peer_view_change(&mut overseer, peer_c.clone(), view![]).await;

		// Peer re-enters view.
		send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;

		overseer
	});
}

// Grid statements imported to backing once candidate enters hypothetical frontier.
#[test]
fn grid_statements_imported_to_backing() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let peer_c = PeerId::random();
	let peer_d = PeerId::random();
	let peer_e = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let local_validator = state.local.clone().unwrap();
		let local_group_index = local_validator.group_index.unwrap();

		let other_group = next_group_index(local_group_index, validator_count, group_size);
		let other_para = ParaId::from(other_group.0);

		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			other_para,
			test_leaf.para_data(other_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let other_group_validators = state.group_validators(other_group, true);
		let v_c = other_group_validators[0];
		let v_d = other_group_validators[1];
		let v_e = other_group_validators[2];

		// Connect C, D, E
		{
			connect_peer(
				&mut overseer,
				peer_c.clone(),
				Some(vec![state.discovery_id(v_c)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_d.clone(),
				Some(vec![state.discovery_id(v_d)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_e.clone(),
				Some(vec![state.discovery_id(v_e)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_d.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_e.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &test_leaf, &state, true, vec![]).await;

		send_new_topology(&mut overseer, state.make_dummy_topology()).await;

		// Receive an advertisement from C.
		{
			let manifest = BackedCandidateManifest {
				relay_parent,
				candidate_hash,
				group_index: other_group,
				para_id: other_para,
				parent_head_data_hash: pvd.parent_head.hash(),
				statement_knowledge: StatementFilter {
					seconded_in_group: bitvec::bitvec![u8, Lsb0; 0, 1, 1],
					validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
				},
			};

			send_peer_message(
				&mut overseer,
				peer_c.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(
					manifest.clone(),
				),
			)
			.await;
		}

		// Should send a request to C.
		{
			let statements = vec![
				state
					.sign_statement(
						v_d,
						CompactStatement::Seconded(candidate_hash),
						&SigningContext { parent_hash: relay_parent, session_index: 1 },
					)
					.as_unchecked()
					.clone(),
				state
					.sign_statement(
						v_e,
						CompactStatement::Seconded(candidate_hash),
						&SigningContext { parent_hash: relay_parent, session_index: 1 },
					)
					.as_unchecked()
					.clone(),
			];

			handle_sent_request(
				&mut overseer,
				peer_c,
				candidate_hash,
				StatementFilter::blank(group_size),
				candidate.clone(),
				pvd.clone(),
				statements,
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_STATEMENT.into()
			);
			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_STATEMENT.into()
			);

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_RESPONSE.into()
			);
		}

		let hypothetical = HypotheticalCandidate::Complete {
			candidate_hash,
			receipt: Arc::new(candidate.clone()),
			persisted_validation_data: pvd.clone(),
		};
		let membership = vec![(relay_parent, vec![0])];
		answer_expected_hypothetical_depth_request(&mut overseer, vec![(hypothetical, membership)])
			.await;

		// Receive messages from Backing subsystem.
		{
			assert_matches!(
				overseer.recv().await,
				AllMessages::CandidateBacking(
					CandidateBackingMessage::Statement(hash, statement)
				) => {
					assert_eq!(hash, relay_parent);
					assert_matches!(
						statement.payload(),
						FullStatementWithPVD::Seconded(c, p)
							if c == &candidate && p == &pvd
					);
				}
			);
			assert_matches!(
				overseer.recv().await,
				AllMessages::CandidateBacking(
					CandidateBackingMessage::Statement(hash, statement)
				) => {
					assert_eq!(hash, relay_parent);
					assert_matches!(
						statement.payload(),
						FullStatementWithPVD::Seconded(c, p)
							if c == &candidate && p == &pvd
					);
				}
			);
		}

		overseer
	});
}

#[test]
fn advertisements_rejected_from_incorrect_peers() {
	sp_tracing::try_init_simple();
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let peer_a = PeerId::random();
	let peer_b = PeerId::random();
	let peer_c = PeerId::random();
	let peer_d = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let local_validator = state.local.clone().unwrap();
		let local_group_index = local_validator.group_index.unwrap();

		let other_group = next_group_index(local_group_index, validator_count, group_size);
		let other_para = ParaId::from(other_group.0);

		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			other_para,
			test_leaf.para_data(other_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let local_group_validators = state.group_validators(local_group_index, true);
		let other_group_validators = state.group_validators(other_group, true);
		let v_a = local_group_validators[0];
		let v_b = local_group_validators[1];
		let v_c = other_group_validators[0];
		let v_d = other_group_validators[1];

		// peer A is in group, has relay parent in view.
		// peer B is in group, has no relay parent in view.
		// peer C is not in group, has relay parent in view.
		// peer D is not in group, has no relay parent in view.
		{
			connect_peer(
				&mut overseer,
				peer_a.clone(),
				Some(vec![state.discovery_id(v_a)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_b.clone(),
				Some(vec![state.discovery_id(v_b)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_c.clone(),
				Some(vec![state.discovery_id(v_c)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_d.clone(),
				Some(vec![state.discovery_id(v_d)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_a.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &test_leaf, &state, true, vec![]).await;

		send_new_topology(&mut overseer, state.make_dummy_topology()).await;

		let manifest = BackedCandidateManifest {
			relay_parent,
			candidate_hash,
			group_index: other_group,
			para_id: other_para,
			parent_head_data_hash: pvd.parent_head.hash(),
			statement_knowledge: StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 0, 1, 1],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
			},
		};

		// Receive an advertisement from A (our group).
		{
			send_peer_message(
				&mut overseer,
				peer_a.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(
					manifest.clone(),
				),
			)
			.await;

			// Message not expected from peers of our own group.
			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_a && r == COST_UNEXPECTED_MANIFEST_PEER_UNKNOWN.into() => { }
			);
		}

		// Receive an advertisement from B (our group).
		{
			send_peer_message(
				&mut overseer,
				peer_b.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(manifest),
			)
			.await;

			// Message not expected from peers of our own group.
			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_b && r == COST_UNEXPECTED_MANIFEST_PEER_UNKNOWN.into() => { }
			);
		}

		overseer
	});
}

#[test]
fn manifest_rejected_with_unknown_relay_parent() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let unknown_parent = Hash::repeat_byte(2);
	let peer_c = PeerId::random();
	let peer_d = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let local_validator = state.local.clone().unwrap();
		let local_group_index = local_validator.group_index.unwrap();

		let other_group = next_group_index(local_group_index, validator_count, group_size);
		let other_para = ParaId::from(other_group.0);

		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			unknown_parent,
			1,
			other_para,
			test_leaf.para_data(other_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let other_group_validators = state.group_validators(other_group, true);
		let v_c = other_group_validators[0];
		let v_d = other_group_validators[1];

		// peer C is not in group, has relay parent in view.
		// peer D is not in group, has no relay parent in view.
		{
			connect_peer(
				&mut overseer,
				peer_c.clone(),
				Some(vec![state.discovery_id(v_c)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_d.clone(),
				Some(vec![state.discovery_id(v_d)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &test_leaf, &state, true, vec![]).await;

		send_new_topology(&mut overseer, state.make_dummy_topology()).await;

		let manifest = BackedCandidateManifest {
			relay_parent: unknown_parent,
			candidate_hash,
			group_index: other_group,
			para_id: other_para,
			parent_head_data_hash: pvd.parent_head.hash(),
			statement_knowledge: StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 0, 1, 1],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
			},
		};

		// Receive an advertisement from C.
		{
			send_peer_message(
				&mut overseer,
				peer_c.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(
					manifest.clone(),
				),
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == COST_UNEXPECTED_MANIFEST_MISSING_KNOWLEDGE.into() => { }
			);
		}

		overseer
	});
}

#[test]
fn manifest_rejected_when_not_a_validator() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::None,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let peer_c = PeerId::random();
	let peer_d = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let other_group = GroupIndex::from(0);
		let other_para = ParaId::from(other_group.0);

		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			other_para,
			test_leaf.para_data(other_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let other_group_validators = state.group_validators(other_group, true);
		let v_c = other_group_validators[0];
		let v_d = other_group_validators[1];

		// peer C is not in group, has relay parent in view.
		// peer D is not in group, has no relay parent in view.
		{
			connect_peer(
				&mut overseer,
				peer_c.clone(),
				Some(vec![state.discovery_id(v_c)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_d.clone(),
				Some(vec![state.discovery_id(v_d)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &test_leaf, &state, true, vec![]).await;

		send_new_topology(&mut overseer, state.make_dummy_topology()).await;

		let manifest = BackedCandidateManifest {
			relay_parent,
			candidate_hash,
			group_index: other_group,
			para_id: other_para,
			parent_head_data_hash: pvd.parent_head.hash(),
			statement_knowledge: StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 0, 1, 1],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
			},
		};

		// Receive an advertisement from C.
		{
			send_peer_message(
				&mut overseer,
				peer_c.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(
					manifest.clone(),
				),
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == COST_UNEXPECTED_MANIFEST_MISSING_KNOWLEDGE.into() => { }
			);
		}

		overseer
	});
}

#[test]
fn manifest_rejected_when_group_does_not_match_para() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let peer_c = PeerId::random();
	let peer_d = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let local_validator = state.local.clone().unwrap();
		let local_group_index = local_validator.group_index.unwrap();

		let other_group = next_group_index(local_group_index, validator_count, group_size);
		// Create a mismatch between group and para.
		let other_para = next_group_index(other_group, validator_count, group_size);
		let other_para = ParaId::from(other_para.0);

		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			other_para,
			test_leaf.para_data(other_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let other_group_validators = state.group_validators(other_group, true);
		let v_c = other_group_validators[0];
		let v_d = other_group_validators[1];

		// peer C is not in group, has relay parent in view.
		// peer D is not in group, has no relay parent in view.
		{
			connect_peer(
				&mut overseer,
				peer_c.clone(),
				Some(vec![state.discovery_id(v_c)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_d.clone(),
				Some(vec![state.discovery_id(v_d)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &test_leaf, &state, true, vec![]).await;

		send_new_topology(&mut overseer, state.make_dummy_topology()).await;

		let manifest = BackedCandidateManifest {
			relay_parent,
			candidate_hash,
			group_index: other_group,
			para_id: other_para,
			parent_head_data_hash: pvd.parent_head.hash(),
			statement_knowledge: StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 0, 1, 1],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
			},
		};

		// Receive an advertisement from C.
		{
			send_peer_message(
				&mut overseer,
				peer_c.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(
					manifest.clone(),
				),
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == COST_MALFORMED_MANIFEST.into() => { }
			);
		}

		overseer
	});
}

#[test]
fn peer_reported_for_advertisement_conflicting_with_confirmed_candidate() {
	let validator_count = 6;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::Validator,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let peer_c = PeerId::random();
	let peer_d = PeerId::random();
	let peer_e = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let local_validator = state.local.clone().unwrap();
		let local_group_index = local_validator.group_index.unwrap();

		let other_group = next_group_index(local_group_index, validator_count, group_size);
		let other_para = ParaId::from(other_group.0);

		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			other_para,
			test_leaf.para_data(other_para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let other_group_validators = state.group_validators(other_group, true);
		let v_c = other_group_validators[0];
		let v_d = other_group_validators[1];
		let v_e = other_group_validators[2];

		// Connect C, D, E
		{
			connect_peer(
				&mut overseer,
				peer_c.clone(),
				Some(vec![state.discovery_id(v_c)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_d.clone(),
				Some(vec![state.discovery_id(v_d)].into_iter().collect()),
			)
			.await;

			connect_peer(
				&mut overseer,
				peer_e.clone(),
				Some(vec![state.discovery_id(v_e)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_c.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_d.clone(), view![relay_parent]).await;
			send_peer_view_change(&mut overseer, peer_e.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &test_leaf, &state, true, vec![]).await;

		send_new_topology(&mut overseer, state.make_dummy_topology()).await;

		let manifest = BackedCandidateManifest {
			relay_parent,
			candidate_hash,
			group_index: other_group,
			para_id: other_para,
			parent_head_data_hash: pvd.parent_head.hash(),
			statement_knowledge: StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 1],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 0],
			},
		};

		let statement_c = state
			.sign_statement(
				v_c,
				CompactStatement::Seconded(candidate_hash),
				&SigningContext { parent_hash: relay_parent, session_index: 1 },
			)
			.as_unchecked()
			.clone();
		let statement_d = state
			.sign_statement(
				v_d,
				CompactStatement::Seconded(candidate_hash),
				&SigningContext { parent_hash: relay_parent, session_index: 1 },
			)
			.as_unchecked()
			.clone();

		// Receive an advertisement from C.
		{
			send_peer_message(
				&mut overseer,
				peer_c.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(
					manifest.clone(),
				),
			)
			.await;

			// Should send a request to C.
			let statements = vec![
				statement_c.clone(),
				statement_d.clone(),
				state
					.sign_statement(
						v_e,
						CompactStatement::Seconded(candidate_hash),
						&SigningContext { parent_hash: relay_parent, session_index: 1 },
					)
					.as_unchecked()
					.clone(),
			];

			handle_sent_request(
				&mut overseer,
				peer_c,
				candidate_hash,
				StatementFilter::blank(group_size),
				candidate.clone(),
				pvd.clone(),
				statements,
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_STATEMENT.into()
			);
			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_STATEMENT.into()
			);
			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_STATEMENT.into()
			);

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == BENEFIT_VALID_RESPONSE.into()
			);

			answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;
		}

		// Receive conflicting advertisement from peer C after confirmation.
		//
		// NOTE: This causes a conflict because we track received manifests on a per-validator
		// basis, and this is the second time we're getting a manifest from C.
		{
			let mut manifest = manifest.clone();
			manifest.statement_knowledge = StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 1, 0],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 0, 0, 0],
			};
			send_peer_message(
				&mut overseer,
				peer_c.clone(),
				protocol_v2::StatementDistributionMessage::BackedCandidateManifest(manifest),
			)
			.await;

			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_c && r == COST_CONFLICTING_MANIFEST.into()
			);
		}

		overseer
	});
}

#[test]
fn inactive_local_participates_in_grid() {
	let validator_count = 11;
	let group_size = 3;
	let config = TestConfig {
		validator_count,
		group_size,
		local_validator: LocalRole::InactiveValidator,
		async_backing_params: None,
	};

	let relay_parent = Hash::repeat_byte(1);
	let peer_a = PeerId::random();

	test_harness(config, |state, mut overseer| async move {
		let local_validator = state.local.clone().unwrap();
		assert_eq!(local_validator.validator_index.0, validator_count as u32);

		let group_idx = GroupIndex::from(0);
		let para = ParaId::from(0);

		// Dummy leaf is needed to update topology.
		let dummy_leaf = state.make_dummy_leaf(Hash::repeat_byte(2));
		let test_leaf = state.make_dummy_leaf(relay_parent);

		let (candidate, pvd) = make_candidate(
			relay_parent,
			1,
			para,
			test_leaf.para_data(para).head_data.clone(),
			vec![4, 5, 6].into(),
			Hash::repeat_byte(42).into(),
		);
		let candidate_hash = candidate.hash();

		let first_group = state.group_validators(group_idx, true);
		let v_a = first_group.last().unwrap().clone();
		let v_b = first_group.first().unwrap().clone();

		{
			connect_peer(
				&mut overseer,
				peer_a.clone(),
				Some(vec![state.discovery_id(v_a)].into_iter().collect()),
			)
			.await;

			send_peer_view_change(&mut overseer, peer_a.clone(), view![relay_parent]).await;
		}

		activate_leaf(&mut overseer, &dummy_leaf, &state, true, vec![]).await;
		// Send gossip topology.
		send_new_topology(&mut overseer, state.make_dummy_topology()).await;
		activate_leaf(&mut overseer, &test_leaf, &state, false, vec![]).await;

		// Receive an advertisement from A.
		let manifest = BackedCandidateManifest {
			relay_parent,
			candidate_hash,
			group_index: group_idx,
			para_id: para,
			parent_head_data_hash: pvd.parent_head.hash(),
			statement_knowledge: StatementFilter {
				seconded_in_group: bitvec::bitvec![u8, Lsb0; 1, 0, 1],
				validated_in_group: bitvec::bitvec![u8, Lsb0; 1, 0, 1],
			},
		};
		send_peer_message(
			&mut overseer,
			peer_a.clone(),
			protocol_v3::StatementDistributionMessage::BackedCandidateManifest(manifest),
		)
		.await;

		let statements = vec![
			state
				.sign_statement(
					v_a,
					CompactStatement::Seconded(candidate_hash),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
				)
				.as_unchecked()
				.clone(),
			state
				.sign_statement(
					v_b,
					CompactStatement::Seconded(candidate_hash),
					&SigningContext { parent_hash: relay_parent, session_index: 1 },
				)
				.as_unchecked()
				.clone(),
		];
		// Inactive node requests this candidate.
		handle_sent_request(
			&mut overseer,
			peer_a,
			candidate_hash,
			StatementFilter::blank(group_size),
			candidate.clone(),
			pvd.clone(),
			statements,
		)
		.await;

		for _ in 0..2 {
			assert_matches!(
				overseer.recv().await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
					if p == peer_a && r == BENEFIT_VALID_STATEMENT.into() => { }
			);
		}
		assert_matches!(
			overseer.recv().await,
			AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
				if p == peer_a && r == BENEFIT_VALID_RESPONSE.into() => { }
		);
		answer_expected_hypothetical_depth_request(&mut overseer, vec![]).await;

		overseer
	});
}
