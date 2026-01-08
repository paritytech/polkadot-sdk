// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Schema for BEEFY state persisted in the aux-db.

use crate::{error::Error, round::VoteWeight, worker::PersistedState, LOG_TARGET};
use codec::{Decode, Encode};
use log::{debug, trace, warn};
use sc_client_api::{backend::AuxStore, Backend};
use sp_application_crypto::RuntimeAppPublic;
use sp_blockchain::{Error as ClientError, Result as ClientResult};
use sp_consensus_beefy::{AuthorityIdBound, Commitment, ValidatorSet, VoteMessage};
use sp_runtime::traits::{Block as BlockT, NumberFor};
use std::{
	collections::{BTreeMap, VecDeque},
	marker::PhantomData,
};

const VERSION_KEY: &[u8] = b"beefy_auxschema_version";
const WORKER_STATE_KEY: &[u8] = b"beefy_voter_state";

const CURRENT_VERSION: u32 = 5;

mod v4 {
	use super::*;

	#[derive(Debug, Decode, Encode, PartialEq)]
	pub(crate) struct PersistedState<B: BlockT, AuthorityId: AuthorityIdBound> {
		pub(crate) best_voted: NumberFor<B>,
		pub(crate) voting_oracle: VoterOracle<B, AuthorityId>,
		pub(crate) pallet_genesis: NumberFor<B>,
	}

	#[derive(Debug, Decode, Encode, PartialEq)]
	pub(crate) struct VoterOracle<B: BlockT, AuthorityId: AuthorityIdBound> {
		pub(crate) sessions: VecDeque<Rounds<B, AuthorityId>>,
		pub(crate) min_block_delta: u32,
		pub(crate) best_grandpa_block_header: <B as BlockT>::Header,
		pub(crate) best_beefy_block: NumberFor<B>,
		pub(crate) _phantom: PhantomData<fn() -> AuthorityId>,
	}

	#[derive(Debug, Decode, Encode, PartialEq)]
	pub(crate) struct Rounds<B: BlockT, AuthorityId: AuthorityIdBound> {
		pub(crate) rounds: BTreeMap<Commitment<NumberFor<B>>, RoundTracker<AuthorityId>>,
		pub(crate) previous_votes: BTreeMap<
			(AuthorityId, NumberFor<B>),
			VoteMessage<NumberFor<B>, AuthorityId, <AuthorityId as RuntimeAppPublic>::Signature>,
		>,
		pub(crate) session_start: NumberFor<B>,
		pub(crate) validator_set: ValidatorSet<AuthorityId>,
		pub(crate) mandatory_done: bool,
		pub(crate) best_done: Option<NumberFor<B>>,
	}

	#[derive(Debug, Decode, Encode, PartialEq)]
	pub(crate) struct RoundTracker<AuthorityId: AuthorityIdBound> {
		pub(crate) votes: BTreeMap<AuthorityId, <AuthorityId as RuntimeAppPublic>::Signature>,
	}
}

mod v5 {
	use super::*;

	#[derive(Debug, Decode, Encode, PartialEq)]
	pub(crate) struct PersistedState<B: BlockT, AuthorityId: AuthorityIdBound> {
		pub(crate) best_voted: NumberFor<B>,
		pub(crate) voting_oracle: VoterOracle<B, AuthorityId>,
		pub(crate) pallet_genesis: NumberFor<B>,
	}

	#[derive(Debug, Decode, Encode, PartialEq)]
	pub(crate) struct VoterOracle<B: BlockT, AuthorityId: AuthorityIdBound> {
		pub(crate) sessions: VecDeque<Rounds<B, AuthorityId>>,
		pub(crate) min_block_delta: u32,
		pub(crate) best_grandpa_block_header: <B as BlockT>::Header,
		pub(crate) best_beefy_block: NumberFor<B>,
		pub(crate) _phantom: PhantomData<fn() -> AuthorityId>,
	}

	#[derive(Debug, Decode, Encode, PartialEq)]
	pub(crate) struct Rounds<B: BlockT, AuthorityId: AuthorityIdBound> {
		pub(crate) rounds: BTreeMap<Commitment<NumberFor<B>>, RoundTracker<AuthorityId>>,
		pub(crate) previous_votes: BTreeMap<
			(AuthorityId, NumberFor<B>),
			VoteMessage<NumberFor<B>, AuthorityId, <AuthorityId as RuntimeAppPublic>::Signature>,
		>,
		pub(crate) session_start: NumberFor<B>,
		pub(crate) validator_set: ValidatorSet<AuthorityId>,
		pub(crate) voting_weights: BTreeMap<AuthorityId, VoteWeight>,
		pub(crate) mandatory_done: bool,
		pub(crate) best_done: Option<NumberFor<B>>,
	}

	#[derive(Debug, Decode, Encode, PartialEq)]
	pub(crate) struct RoundTracker<AuthorityId: AuthorityIdBound> {
		pub(crate) votes: BTreeMap<AuthorityId, <AuthorityId as RuntimeAppPublic>::Signature>,
		pub(crate) accumulated_votes_weight: VoteWeight,
	}
}

pub(crate) fn write_current_version<BE: AuxStore>(backend: &BE) -> Result<(), Error> {
	debug!(target: LOG_TARGET, "🥩 write aux schema version {:?}", CURRENT_VERSION);
	AuxStore::insert_aux(backend, &[(VERSION_KEY, CURRENT_VERSION.encode().as_slice())], &[])
		.map_err(|e| Error::Backend(e.to_string()))
}

/// Write voter state.
pub(crate) fn write_voter_state<B: BlockT, BE: AuxStore, AuthorityId: AuthorityIdBound>(
	backend: &BE,
	state: &PersistedState<B, AuthorityId>,
) -> ClientResult<()> {
	trace!(target: LOG_TARGET, "🥩 persisting {:?}", state);
	AuxStore::insert_aux(backend, &[(WORKER_STATE_KEY, state.encode().as_slice())], &[])
}

fn load_decode<BE: AuxStore, T: Decode>(backend: &BE, key: &[u8]) -> ClientResult<Option<T>> {
	match backend.get_aux(key)? {
		None => Ok(None),
		Some(t) => T::decode(&mut &t[..])
			.map_err(|e| ClientError::Backend(format!("BEEFY DB is corrupted: {}", e)))
			.map(Some),
	}
}

fn migrate_v4_to_v5<B, BE, AuthorityId>(
	backend: &BE,
) -> ClientResult<Option<PersistedState<B, AuthorityId>>>
where
	B: BlockT,
	BE: AuxStore,
	AuthorityId: AuthorityIdBound,
{
	let version_key = CURRENT_VERSION.encode();

	let Some(old) =
		load_decode::<_, v4::PersistedState<B, AuthorityId>>(backend, WORKER_STATE_KEY)?
	else {
		// v4 marker present, but no state.
		return Ok(None);
	};

	let compute_voting_weights = |validator_set: &ValidatorSet<AuthorityId>| {
		validator_set.validators().iter().fold(
			BTreeMap::<AuthorityId, VoteWeight>::new(),
			|mut acc, authority| {
				*acc.entry(authority.to_owned()).or_insert(0) += 1;
				acc
			},
		)
	};

	let sessions = old
		.voting_oracle
		.sessions
		.into_iter()
		.map(|rounds| {
			let voting_weights = compute_voting_weights(&rounds.validator_set);
			let rounds_map = rounds
				.rounds
				.into_iter()
				.map(|(commitment, tracker)| {
					let accumulated_votes_weight =
						tracker.votes.keys().try_fold(0u32, |acc, authority| {
							let weight =
								voting_weights.get(authority).copied().ok_or_else(|| {
									ClientError::Backend(
									"BEEFY DB is corrupted: authority not found in voting weights"
										.into(),
								)
								})?;
							acc.checked_add(weight).ok_or_else(|| {
								ClientError::Backend(
									"BEEFY DB is corrupted: accumulated vote weight overflow"
										.into(),
								)
							})
						})?;
					Ok((
						commitment,
						v5::RoundTracker { votes: tracker.votes, accumulated_votes_weight },
					))
				})
				.collect::<ClientResult<_>>()?;

			Ok(v5::Rounds {
				rounds: rounds_map,
				previous_votes: rounds.previous_votes,
				session_start: rounds.session_start,
				validator_set: rounds.validator_set,
				voting_weights,
				mandatory_done: rounds.mandatory_done,
				best_done: rounds.best_done,
			})
		})
		.collect::<ClientResult<VecDeque<_>>>()?;

	let new_state = v5::PersistedState::<B, AuthorityId> {
		best_voted: old.best_voted,
		voting_oracle: v5::VoterOracle::<B, AuthorityId> {
			sessions,
			min_block_delta: old.voting_oracle.min_block_delta,
			best_grandpa_block_header: old.voting_oracle.best_grandpa_block_header,
			best_beefy_block: old.voting_oracle.best_beefy_block,
			_phantom: PhantomData,
		},
		pallet_genesis: old.pallet_genesis,
	};

	debug!(
		target: LOG_TARGET,
		"🥩 Migrating BEEFY aux-db schema v4 -> v5",
	);

	backend.insert_aux(
		&[(VERSION_KEY, version_key.as_slice()), (WORKER_STATE_KEY, new_state.encode().as_slice())],
		&[],
	)?;

	load_decode::<_, PersistedState<B, AuthorityId>>(backend, WORKER_STATE_KEY)
}

/// Load or initialize persistent data from backend.
pub(crate) fn load_persistent<B, BE, AuthorityId: AuthorityIdBound>(
	backend: &BE,
) -> ClientResult<Option<PersistedState<B, AuthorityId>>>
where
	B: BlockT,
	BE: Backend<B>,
{
	let version: Option<u32> = load_decode(backend, VERSION_KEY)?;

	match version {
		None => (),

		Some(v) if 1 <= v && v <= 3 =>
		// versions 1, 2 & 3 are obsolete and should be ignored
			warn!(
				target: LOG_TARGET,
				"🥩 backend contains a BEEFY state of an obsolete version {v}. ignoring..."
			),
		Some(4) => return migrate_v4_to_v5::<B, _, AuthorityId>(backend),
		Some(5) =>
			return load_decode::<_, PersistedState<B, AuthorityId>>(backend, WORKER_STATE_KEY),
		other =>
			return Err(ClientError::Backend(format!("Unsupported BEEFY DB version: {:?}", other))),
	}

	// No persistent state found in DB.
	Ok(None)
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;
	use crate::tests::BeefyTestNet;
	use sc_network_test::TestNetFactory;
	use sp_consensus_beefy::{
		ecdsa_crypto, known_payloads, test_utils::Keyring, Payload, ValidatorSet,
	};
	use sp_core::H256;
	use sp_runtime::{
		generic::Digest,
		traits::{Header as HeaderT, Zero},
	};
	use substrate_test_runtime_client as test_client;

	// also used in tests.rs
	pub fn verify_persisted_version<B: BlockT, BE: Backend<B>>(backend: &BE) -> bool {
		let version: u32 = load_decode(backend, VERSION_KEY).unwrap().unwrap();
		version == CURRENT_VERSION
	}

	#[tokio::test]
	async fn should_load_persistent_sanity_checks() {
		let mut net = BeefyTestNet::new(1);
		let backend = net.peer(0).client().as_backend();

		// version not available in db -> None
		assert_eq!(
			load_persistent::<test_client::runtime::Block, _, ecdsa_crypto::AuthorityId>(&*backend)
				.unwrap(),
			None
		);

		// populate version in db
		write_current_version(&*backend).unwrap();
		// verify correct version is retrieved
		assert_eq!(load_decode(&*backend, VERSION_KEY).unwrap(), Some(CURRENT_VERSION));

		// version is available in db but state isn't -> None
		assert_eq!(
			load_persistent::<test_client::runtime::Block, _, ecdsa_crypto::AuthorityId>(&*backend)
				.unwrap(),
			None
		);

		// full `PersistedState` load is tested in `tests.rs`.
	}

	#[tokio::test]
	async fn should_migrate_v4_to_v5() {
		let mut net = BeefyTestNet::new(1);
		let backend = net.peer(0).client().as_backend();
		let beefy_genesis: NumberFor<test_client::runtime::Block> = 1;

		let validators = vec![
			Keyring::<ecdsa_crypto::AuthorityId>::Alice.public(),
			Keyring::<ecdsa_crypto::AuthorityId>::Alice.public(),
			Keyring::<ecdsa_crypto::AuthorityId>::Bob.public(),
		];
		let validator_set = ValidatorSet::new(validators, 0).unwrap();

		let best_grandpa = <test_client::runtime::Header as HeaderT>::new(
			beefy_genesis,
			Default::default(),
			Default::default(),
			H256::random(),
			Digest::default(),
		);
		let commitment = Commitment {
			payload: Payload::from_single_entry(known_payloads::MMR_ROOT_ID, vec![]),
			block_number: beefy_genesis,
			validator_set_id: validator_set.id(),
		};
		let mut votes = BTreeMap::new();
		votes.insert(
			Keyring::<ecdsa_crypto::AuthorityId>::Alice.public(),
			Keyring::<ecdsa_crypto::AuthorityId>::Alice.sign(b"vote"),
		);
		let tracker = v4::RoundTracker::<ecdsa_crypto::AuthorityId> { votes };
		let rounds_map = BTreeMap::from([(commitment, tracker)]);

		let voting_oracle =
			v4::VoterOracle::<test_client::runtime::Block, ecdsa_crypto::AuthorityId> {
				sessions: VecDeque::from([v4::Rounds::<
					test_client::runtime::Block,
					ecdsa_crypto::AuthorityId,
				> {
					rounds: rounds_map,
					previous_votes: BTreeMap::new(),
					session_start: beefy_genesis,
					validator_set,
					mandatory_done: false,
					best_done: None,
				}]),
				min_block_delta: 1,
				best_grandpa_block_header: best_grandpa,
				best_beefy_block: Zero::zero(),
				_phantom: PhantomData,
			};

		let state_v4 = v4::PersistedState::<test_client::runtime::Block, ecdsa_crypto::AuthorityId> {
			best_voted: Zero::zero(),
			voting_oracle,
			pallet_genesis: beefy_genesis,
		};
		let encoded_state_v4 = state_v4.encode();
		let encoded_version_v4 = 4u32.encode();

		AuxStore::insert_aux(
			&*backend,
			&[
				(VERSION_KEY, encoded_version_v4.as_slice()),
				(WORKER_STATE_KEY, encoded_state_v4.as_slice()),
			],
			&[],
		)
		.unwrap();

		assert_eq!(load_decode::<_, u32>(&*backend, VERSION_KEY).unwrap(), Some(4));

		let migrated =
			load_persistent::<test_client::runtime::Block, _, ecdsa_crypto::AuthorityId>(&*backend)
				.unwrap()
				.expect("migration should produce a state; qed.");

		assert_eq!(migrated.pallet_genesis(), beefy_genesis);
		assert_eq!(migrated.voting_oracle().voting_target(), Some(beefy_genesis));

		// Between v4 and v5 we changed the way votes are accumulated.
		// We should check if vote weights are migrated correctly.
		{
			assert_eq!(migrated.voting_oracle().sessions().len(), 1);
			let rounds = migrated.voting_oracle().sessions().front().unwrap();
			let rounds_v5: v5::Rounds<test_client::runtime::Block, ecdsa_crypto::AuthorityId> =
				Decode::decode(&mut &*rounds.encode()).expect("decode as v5 Rounds; qed");

			assert_eq!(
				rounds_v5.voting_weights,
				[
					(Keyring::<ecdsa_crypto::AuthorityId>::Alice.public(), 2),
					(Keyring::<ecdsa_crypto::AuthorityId>::Bob.public(), 1),
				]
				.into_iter()
				.collect()
			);
			assert_eq!(rounds_v5.rounds.len(), 1);
			let (_commitment, tracker) = rounds_v5.rounds.iter().next().unwrap();
			// Alice has 2 votes, Bob has 1 vote. Alice already voted before migration, Bob did not,
			// so we should have accumulated 2 votes.
			assert_eq!(tracker.accumulated_votes_weight, 2);
		}

		assert!(verify_persisted_version(&*backend));
	}
}
