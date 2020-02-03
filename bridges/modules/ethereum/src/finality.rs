// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Parity-Bridge.

// Parity-Bridge is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity-Bridge is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity-Bridge.  If not, see <http://www.gnu.org/licenses/>.

use sp_std::prelude::*;
use sp_std::collections::{
	btree_map::{BTreeMap, Entry},
	btree_set::BTreeSet,
	vec_deque::VecDeque,
};
use sp_io::crypto::secp256k1_ecdsa_recover;
use primitives::{Address, H256, Header, SealedEmptyStep, public_to_address};
use crate::{Storage, ancestry};
use crate::error::Error;

/// Tries to finalize blocks when given block is imported.
///
/// Returns numbers and hashes of finalized blocks in ascending order.
pub fn finalize_blocks<S: Storage>(
	storage: &S,
	best_finalized_hash: &H256,
	header_validators: (&H256, &[Address]),
	hash: &H256,
	header: &Header,
	two_thirds_majority_transition: u64,
) -> Result<Vec<(u64, H256)>, Error> {
	// compute count of voters for every unfinalized block in ancestry
	let validators = header_validators.1.iter().collect();
	let (mut votes, mut headers) = prepare_votes(
		storage,
		best_finalized_hash,
		&header_validators.0,
		&validators,
		hash,
		header,
		two_thirds_majority_transition,
	)?;

	// now let's iterate in reverse order && find just finalized blocks
	let mut newly_finalized = Vec::new();
	while let Some((oldest_hash, oldest_number, signers)) = headers.pop_front() {
		if !is_finalized(&validators, &votes, oldest_number >= two_thirds_majority_transition) {
			break;
		}

		remove_signers_votes(&signers, &mut votes);
		newly_finalized.push((oldest_number, oldest_hash));
	}

	Ok(newly_finalized)
}

/// Returns true if there are enough votes to treat this header as finalized.
fn is_finalized(
	validators: &BTreeSet<&Address>,
	votes: &BTreeMap<Address, u64>,
	requires_two_thirds_majority: bool,
) -> bool {
	(!requires_two_thirds_majority && votes.len() * 2 > validators.len()) ||
		(requires_two_thirds_majority && votes.len() * 3 > validators.len() * 2)
}

/// Prepare 'votes' of header and its ancestors' signers.
fn prepare_votes<S: Storage>(
	storage: &S,
	best_finalized_hash: &H256,
	validators_begin: &H256,
	validators: &BTreeSet<&Address>,
	hash: &H256,
	header: &Header,
	two_thirds_majority_transition: u64,
) -> Result<(BTreeMap<Address, u64>, VecDeque<(H256, u64, BTreeSet<Address>)>), Error> {
	// this fn can only work with single validators set
	if !validators.contains(&header.author) {
		return Err(Error::NotValidator);
	}

	// prepare iterator of signers of all ancestors of the header
	// we only take ancestors that are not yet pruned and those signed by
	// the same set of validators
	let mut parent_empty_step_signers = empty_steps_signers(header);
	let ancestry = ancestry(storage, header)
		.map(|(hash, header)| {
			let mut signers = BTreeSet::new();
			sp_std::mem::swap(&mut signers, &mut parent_empty_step_signers);
			signers.insert(header.author);

			let empty_step_signers = empty_steps_signers(&header);
			let res = (hash, header.number, signers);
			parent_empty_step_signers = empty_step_signers;
			res
		})
		.take_while(|&(hash, _, _)| hash != *validators_begin && hash != *best_finalized_hash);

	// now let's iterate built iterator and compute number of validators
	// 'voted' for each header
	// we stop when finalized block is met (because we only interested in
	// just finalized blocks)
	let mut votes = BTreeMap::new();
	let mut headers = VecDeque::new();
	for (hash, number, signers) in ancestry {
		add_signers_votes(validators, &signers, &mut votes)?;
		if is_finalized(validators, &votes, number >= two_thirds_majority_transition) {
			remove_signers_votes(&signers, &mut votes);
			break;
		}

		headers.push_front((hash, number, signers));
	}

	// update votes with last header vote
	let mut header_signers = BTreeSet::new();
	header_signers.insert(header.author);
	*votes.entry(header.author).or_insert(0) += 1;
	headers.push_back((*hash, header.number, header_signers));

	Ok((votes, headers))
}

/// Increase count of 'votes' for every passed signer.
/// Fails if at least one of signers is not in the `validators` set.
fn add_signers_votes(
	validators: &BTreeSet<&Address>,
	signers_to_add: &BTreeSet<Address>,
	votes: &mut BTreeMap<Address, u64>,
) -> Result<(), Error> {
	for signer in signers_to_add {
		if !validators.contains(signer) {
			return Err(Error::NotValidator);
		}

		*votes.entry(*signer).or_insert(0) += 1;
	}

	Ok(())
}

/// Decrease 'votes' count for every passed signer.
fn remove_signers_votes(signers_to_remove: &BTreeSet<Address>, votes: &mut BTreeMap<Address, u64>) {
	for signer in signers_to_remove {
		match votes.entry(*signer) {
			Entry::Occupied(mut entry) => {
				if *entry.get() <= 1 {
					entry.remove();
				} else {
					*entry.get_mut() -= 1;
				}
			},
			Entry::Vacant(_) => unreachable!("we only remove signers that have been added; qed"),
		}
	}
}

/// Returns unique set of empty steps signers.
fn empty_steps_signers(header: &Header) -> BTreeSet<Address> {
	header.empty_steps()
		.into_iter()
		.flat_map(|steps| steps)
		.filter_map(|step| empty_step_signer(&step, &header.parent_hash))
		.collect::<BTreeSet<_>>()
}

/// Returns author of empty step signature.
fn empty_step_signer(empty_step: &SealedEmptyStep, parent_hash: &H256) -> Option<Address> {
	let message = empty_step.message(parent_hash);
	secp256k1_ecdsa_recover(empty_step.signature.as_fixed_bytes(), message.as_fixed_bytes())
		.ok()
		.map(|public| public_to_address(&public))
}

#[cfg(test)]
mod tests {
	use crate::HeaderToImport;
	use crate::tests::{InMemoryStorage, genesis, validator, validators_addresses};
	use super::*;

	#[test]
	fn verifies_header_author() {
		assert_eq!(
			finalize_blocks(
				&InMemoryStorage::new(genesis(), validators_addresses(5)),
				&Default::default(),
				(&Default::default(), &[]),
				&Default::default(),
				&Header::default(),
				0,
			),
			Err(Error::NotValidator),
		);
	}

	#[test]
	fn prepares_votes() {
		// let's say we have 5 validators (we need 'votes' from 3 validators to achieve
		// finality)
		let mut storage = InMemoryStorage::new(genesis(), validators_addresses(5));

		// when header#1 is inserted, nothing is finalized (1 vote)
		let header1 = Header {
			author: validator(0).address().as_fixed_bytes().into(),
			parent_hash: genesis().hash(),
			number: 1,
			..Default::default()
		};
		let hash1 = header1.hash();
		let mut header_to_import = HeaderToImport {
			context: storage.import_context(&genesis().hash()).unwrap(),
			is_best: true,
			hash: hash1,
			header: header1,
			total_difficulty: 0.into(),
			enacted_change: None,
			scheduled_change: None,
		};
		assert_eq!(
			finalize_blocks(
				&storage,
				&Default::default(),
				(&Default::default(), &validators_addresses(5)),
				&hash1,
				&header_to_import.header,
				u64::max_value(),
			),
			Ok(Vec::new()),
		);
		storage.insert_header(header_to_import.clone());

		// when header#2 is inserted, nothing is finalized (2 votes)
		header_to_import.header = Header {
			author: validator(1).address().as_fixed_bytes().into(),
			parent_hash: hash1,
			number: 2,
			..Default::default()
		};
		header_to_import.hash = header_to_import.header.hash();
		let hash2 = header_to_import.header.hash();
		assert_eq!(
			finalize_blocks(
				&storage,
				&Default::default(),
				(&Default::default(), &validators_addresses(5)),
				&hash2,
				&header_to_import.header,
				u64::max_value(),
			),
			Ok(Vec::new()),
		);
		storage.insert_header(header_to_import.clone());

		// when header#3 is inserted, header#1 is finalized (3 votes)
		header_to_import.header = Header {
			author: validator(2).address().as_fixed_bytes().into(),
			parent_hash: hash2,
			number: 3,
			..Default::default()
		};
		header_to_import.hash = header_to_import.header.hash();
		let hash3 = header_to_import.header.hash();
		assert_eq!(
			finalize_blocks(
				&storage,
				&Default::default(),
				(&Default::default(), &validators_addresses(5)),
				&hash3,
				&header_to_import.header,
				u64::max_value(),
			),
			Ok(vec![(1, hash1)]),
		);
		storage.insert_header(header_to_import);
	}
}
