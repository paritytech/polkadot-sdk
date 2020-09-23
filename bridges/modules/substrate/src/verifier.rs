// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! The verifier's role is to check the validity of headers being imported, and also determine if
//! they can be finalized.
//!
//! When importing headers it performs checks to ensure that no invariants are broken (like
//! importing the same header twice). When it imports finality proofs it will ensure that the proof
//! has been signed off by the correct Grandpa authorities, and also enact any authority set changes
//! if required.

use crate::BridgeStorage;
use bp_substrate::{check_finality_proof, AuthoritySet, ImportedHeader, ScheduledChange};
use sp_finality_grandpa::{ConsensusLog, GRANDPA_ENGINE_ID};
use sp_runtime::generic::OpaqueDigestItemId;
use sp_runtime::traits::{CheckedAdd, Header as HeaderT, One};
use sp_std::{prelude::Vec, vec};

/// The finality proof used by the pallet.
///
/// For a Substrate based chain using Grandpa this will
/// be an encoded Grandpa Justification.
pub struct FinalityProof(Vec<u8>);

impl From<&[u8]> for FinalityProof {
	fn from(proof: &[u8]) -> Self {
		Self(proof.to_vec())
	}
}

impl From<Vec<u8>> for FinalityProof {
	fn from(proof: Vec<u8>) -> Self {
		Self(proof)
	}
}

/// Errors which can happen while importing a header.
#[derive(Debug, PartialEq)]
pub enum ImportError {
	/// This header is older than our latest finalized block, thus not useful.
	OldHeader,
	/// This header has already been imported by the pallet.
	HeaderAlreadyExists,
	/// We're missing a parent for this header.
	MissingParent,
	/// The number of the header does not follow its parent's number.
	InvalidChildNumber,
	/// The height of the next authority set change overflowed.
	ScheduledHeightOverflow,
}

/// Errors which can happen while verifying a headers finality.
#[derive(Debug, PartialEq)]
pub enum FinalizationError {
	/// This header has never been imported by the pallet.
	UnknownHeader,
	/// We were unable to prove finality for this header.
	UnfinalizedHeader,
	/// Trying to prematurely import a justification
	PrematureJustification,
	/// We failed to verify this header's ancestry.
	AncestryCheckFailed,
	/// This header is older than our latest finalized block, thus not useful.
	OldHeader,
}

/// Used to verify imported headers and their finality status.
#[derive(Debug)]
pub struct Verifier<S> {
	pub storage: S,
}

impl<S, H> Verifier<S>
where
	S: BridgeStorage<Header = H>,
	H: HeaderT,
{
	/// Import a header to the pallet.
	///
	/// Will perform some basic checks to make sure that this header doesn't break any assumptions
	/// such as being on a different finalized fork.
	pub fn import_header(&mut self, header: H) -> Result<(), ImportError> {
		let best_finalized = self.storage.best_finalized_header();

		if header.number() <= best_finalized.number() {
			return Err(ImportError::OldHeader);
		}

		if self.storage.header_exists(header.hash()) {
			return Err(ImportError::HeaderAlreadyExists);
		}

		let parent_header = self
			.storage
			.header_by_hash(*header.parent_hash())
			.ok_or(ImportError::MissingParent)?;

		let parent_number = *parent_header.number();
		if parent_number + One::one() != *header.number() {
			return Err(ImportError::InvalidChildNumber);
		}

		// This header requires a justification since it enacts an authority set change. We don't
		// need to act on it right away (we'll update the set once this header gets finalized), but
		// we need to make a note of it.
		//
		// TODO: This assumes that we can only have one authority set change pending at a time.
		// This is not strictly true as Grandpa may schedule multiple changes on a given chain
		// if the "next next" change is scheduled after the "delay" period of the "next" change
		let requires_justification = if let Some(change) = self.storage.scheduled_set_change() {
			change.height == *header.number()
		} else {
			// Since we don't currently have a pending authority set change let's check if the header
			// contains a log indicating when the next change should be.
			if let Some(change) = find_scheduled_change(&header) {
				let next_set = AuthoritySet {
					authorities: change.next_authorities,
					set_id: self.storage.current_authority_set().set_id + 1,
				};

				let height = (*header.number())
					.checked_add(&change.delay)
					.ok_or(ImportError::ScheduledHeightOverflow)?;
				let scheduled_change = ScheduledChange {
					authority_set: next_set,
					height,
				};

				self.storage.schedule_next_set_change(scheduled_change);

				// If the delay is 0 this header will enact the change it signaled
				height == *header.number()
			} else {
				false
			}
		};

		let is_finalized = false;
		self.storage.write_header(&ImportedHeader {
			header,
			requires_justification,
			is_finalized,
		});

		Ok(())
	}

	/// Verify that a previously imported header can be finalized with the given Grandpa finality
	/// proof. If the header enacts an authority set change the change will be applied once the
	/// header has been finalized.
	pub fn import_finality_proof(&mut self, hash: H::Hash, proof: FinalityProof) -> Result<(), FinalizationError> {
		// Make sure that we've previously imported this header
		let header = self
			.storage
			.header_by_hash(hash)
			.ok_or(FinalizationError::UnknownHeader)?;

		// We don't want to finalize an ancestor of an already finalized
		// header, this would be inconsistent
		let last_finalized = self.storage.best_finalized_header();
		if header.number() <= last_finalized.number() {
			return Err(FinalizationError::OldHeader);
		}

		let current_authority_set = self.storage.current_authority_set();
		let is_finalized = check_finality_proof(&header, &current_authority_set, &proof.0);
		if !is_finalized {
			return Err(FinalizationError::UnfinalizedHeader);
		}

		frame_support::debug::trace!(target: "sub-bridge", "Checking ancestry for headers between {:?} and {:?}", last_finalized, header);
		let mut finalized_headers =
			if let Some(ancestors) = headers_between(&self.storage, last_finalized, header.clone()) {
				// Since we only try and finalize headers with a height strictly greater
				// than `best_finalized` if `headers_between` returns Some we must have
				// at least one element. If we don't something's gone wrong, so best
				// to die before we write to storage.
				assert_eq!(
					ancestors.is_empty(),
					false,
					"Empty ancestry list returned from `headers_between()`",
				);

				// Check if any of our ancestors `requires_justification` a.k.a schedule authority
				// set changes. If they're still waiting to be finalized we must reject this
				// justification. We don't include our current header in this check.
				//
				// We do this because it is important to to import justifications _in order_,
				// otherwise we risk finalizing headers on competing chains.
				let requires_justification = ancestors.iter().skip(1).find(|h| h.requires_justification);
				if requires_justification.is_some() {
					return Err(FinalizationError::PrematureJustification);
				}

				ancestors
			} else {
				return Err(FinalizationError::AncestryCheckFailed);
			};

		// If the current header was marked as `requires_justification` it means that it enacts a
		// new authority set change. When we finalize the header we need to update the current
		// authority set.
		if header.requires_justification {
			// If we are unable to enact an authority set it means our storage entry for scheduled
			// changes is missing. Best to crash since this is likely a bug.
			let _ = self.storage.enact_authority_set().expect(
				"Headers must only be marked as `requires_justification` if there's a scheduled change in storage.",
			);
		}

		for header in finalized_headers.iter_mut() {
			header.is_finalized = true;
			header.requires_justification = false;
			self.storage.write_header(header);
		}

		self.storage.update_best_finalized(hash);

		Ok(())
	}
}

/// Returns the lineage of headers between [child, ancestor)
fn headers_between<S, H>(
	storage: &S,
	ancestor: ImportedHeader<H>,
	child: ImportedHeader<H>,
) -> Option<Vec<ImportedHeader<H>>>
where
	S: BridgeStorage<Header = H>,
	H: HeaderT,
{
	let mut ancestors = vec![];
	let mut current_header = child;

	while ancestor.hash() != current_header.hash() {
		// We've gotten to the same height and we're not related
		if ancestor.number() >= current_header.number() {
			return None;
		}

		let parent = storage.header_by_hash(*current_header.parent_hash());
		ancestors.push(current_header);
		current_header = match parent {
			Some(h) => h,
			None => return None,
		}
	}

	Some(ancestors)
}

fn find_scheduled_change<H: HeaderT>(header: &H) -> Option<sp_finality_grandpa::ScheduledChange<H::Number>> {
	let id = OpaqueDigestItemId::Consensus(&GRANDPA_ENGINE_ID);

	let filter_log = |log: ConsensusLog<H::Number>| match log {
		ConsensusLog::ScheduledChange(change) => Some(change),
		_ => None,
	};

	// find the first consensus digest with the right ID which converts to
	// the right kind of consensus log.
	header.digest().convert_first(|l| l.try_to(id).and_then(filter_log))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;
	use crate::{BestFinalized, ImportedHeaders, PalletStorage};
	use frame_support::{assert_err, assert_ok};
	use frame_support::{StorageMap, StorageValue};
	use sp_finality_grandpa::{AuthorityId, AuthorityList};
	use sp_runtime::testing::UintAuthorityId;

	type TestHeader = <TestRuntime as frame_system::Trait>::Header;
	type TestNumber = <TestHeader as HeaderT>::Number;

	fn unfinalized_header(num: u64) -> ImportedHeader<TestHeader> {
		ImportedHeader {
			header: TestHeader::new_from_number(num),
			requires_justification: false,
			is_finalized: false,
		}
	}

	fn get_authorities(authorities: Vec<(u64, u64)>) -> AuthorityList {
		authorities
			.iter()
			.map(|(id, weight)| (UintAuthorityId(*id).to_public_key::<AuthorityId>(), *weight))
			.collect()
	}

	fn schedule_next_change(
		authorities: Vec<(u64, u64)>,
		set_id: u64,
		height: TestNumber,
	) -> ScheduledChange<TestNumber> {
		let authorities = get_authorities(authorities);
		let authority_set = AuthoritySet::new(authorities, set_id);
		ScheduledChange { authority_set, height }
	}

	// Useful for quickly writing a chain of headers to storage
	fn write_headers<S: BridgeStorage<Header = TestHeader>>(
		storage: &mut S,
		headers: Vec<(u64, bool, bool)>,
	) -> Vec<ImportedHeader<TestHeader>> {
		let mut imported_headers = vec![];
		let genesis = ImportedHeader {
			header: TestHeader::new_from_number(0),
			requires_justification: false,
			is_finalized: true,
		};

		<BestFinalized<TestRuntime>>::put(genesis.hash());
		storage.write_header(&genesis);
		imported_headers.push(genesis);

		for (num, requires_justification, is_finalized) in headers {
			let mut h = TestHeader::new_from_number(num);
			h.parent_hash = imported_headers.last().unwrap().hash();

			let header = ImportedHeader {
				header: h,
				requires_justification,
				is_finalized,
			};

			storage.write_header(&header);
			imported_headers.push(header);
		}

		imported_headers
	}

	#[test]
	fn fails_to_import_old_header() {
		run_test(|| {
			let mut storage = PalletStorage::<TestRuntime>::new();
			let parent = unfinalized_header(5);
			storage.write_header(&parent);
			storage.update_best_finalized(parent.hash());

			let header = TestHeader::new_from_number(1);
			let mut verifier = Verifier { storage };
			assert_err!(verifier.import_header(header), ImportError::OldHeader);
		})
	}

	#[test]
	fn fails_to_import_header_without_parent() {
		run_test(|| {
			let mut storage = PalletStorage::<TestRuntime>::new();
			let parent = unfinalized_header(1);
			storage.write_header(&parent);
			storage.update_best_finalized(parent.hash());

			// By default the parent is `0x00`
			let header = TestHeader::new_from_number(2);

			let mut verifier = Verifier { storage };
			assert_err!(verifier.import_header(header), ImportError::MissingParent);
		})
	}

	#[test]
	fn fails_to_import_header_twice() {
		run_test(|| {
			let storage = PalletStorage::<TestRuntime>::new();
			let header = TestHeader::new_from_number(1);
			<BestFinalized<TestRuntime>>::put(header.hash());

			let imported_header = ImportedHeader {
				header: header.clone(),
				requires_justification: false,
				is_finalized: false,
			};
			<ImportedHeaders<TestRuntime>>::insert(header.hash(), &imported_header);

			let mut verifier = Verifier { storage };
			assert_err!(verifier.import_header(header), ImportError::OldHeader);
		})
	}

	#[test]
	fn succesfully_imports_valid_but_unfinalized_header() {
		run_test(|| {
			let storage = PalletStorage::<TestRuntime>::new();
			let parent = TestHeader::new_from_number(1);
			let parent_hash = parent.hash();
			<BestFinalized<TestRuntime>>::put(parent.hash());

			let imported_header = ImportedHeader {
				header: parent,
				requires_justification: false,
				is_finalized: true,
			};
			<ImportedHeaders<TestRuntime>>::insert(parent_hash, &imported_header);

			let mut header = TestHeader::new_from_number(2);
			header.parent_hash = parent_hash;
			let mut verifier = Verifier {
				storage: storage.clone(),
			};
			assert_ok!(verifier.import_header(header.clone()));

			let stored_header = storage.header_by_hash(header.hash());
			assert!(stored_header.is_some());
			assert_eq!(stored_header.unwrap().is_finalized, false);
		})
	}

	#[test]
	fn related_headers_are_ancestors() {
		run_test(|| {
			let mut storage = PalletStorage::<TestRuntime>::new();

			let headers = vec![(1, false, false), (2, false, false), (3, false, false)];
			let mut imported_headers = write_headers(&mut storage, headers);

			for header in imported_headers.iter() {
				assert!(storage.header_exists(header.hash()));
			}

			let ancestor = imported_headers.remove(0);
			let child = imported_headers.pop().unwrap();
			let ancestors = headers_between(&storage, ancestor, child);

			assert!(ancestors.is_some());
			assert_eq!(ancestors.unwrap().len(), 3);
		})
	}

	#[test]
	fn unrelated_headers_are_not_ancestors() {
		run_test(|| {
			let mut storage = PalletStorage::<TestRuntime>::new();

			let headers = vec![(1, false, false), (2, false, false), (3, false, false)];
			let mut imported_headers = write_headers(&mut storage, headers);
			for header in imported_headers.iter() {
				assert!(storage.header_exists(header.hash()));
			}

			// Need to give it a different parent_hash or else it'll be
			// related to our test genesis header
			let mut bad_ancestor = TestHeader::new_from_number(0);
			bad_ancestor.parent_hash = [1u8; 32].into();
			let bad_ancestor = ImportedHeader {
				header: bad_ancestor,
				requires_justification: false,
				is_finalized: false,
			};

			let child = imported_headers.pop().unwrap();
			let ancestors = headers_between(&storage, bad_ancestor, child);
			assert!(ancestors.is_none());
		})
	}

	#[test]
	fn ancestor_newer_than_child_is_not_related() {
		run_test(|| {
			let mut storage = PalletStorage::<TestRuntime>::new();

			let headers = vec![(1, false, false), (2, false, false), (3, false, false)];
			let mut imported_headers = write_headers(&mut storage, headers);
			for header in imported_headers.iter() {
				assert!(storage.header_exists(header.hash()));
			}

			// What if we have an "ancestor" that's newer than child?
			let new_ancestor = TestHeader::new_from_number(5);
			let new_ancestor = ImportedHeader {
				header: new_ancestor,
				requires_justification: false,
				is_finalized: false,
			};

			let child = imported_headers.pop().unwrap();
			let ancestors = headers_between(&storage, new_ancestor, child);
			assert!(ancestors.is_none());
		})
	}

	#[test]
	fn finalizes_header_which_doesnt_enact_or_schedule_a_new_authority_set() {
		run_test(|| {
			let mut storage = PalletStorage::<TestRuntime>::new();
			let headers = vec![(1, false, false)];
			let imported_headers = write_headers(&mut storage, headers);

			// Nothing special about this header, yet Grandpa may have created a justification
			// for it since it does that periodically
			let mut header = TestHeader::new_from_number(2);
			header.parent_hash = imported_headers[1].hash();

			let mut verifier = Verifier {
				storage: storage.clone(),
			};

			assert_ok!(verifier.import_header(header.clone()));
			assert_ok!(verifier.import_finality_proof(header.hash(), vec![4, 2].into()));
			assert_eq!(storage.best_finalized_header().header, header);
		})
	}

	#[test]
	fn correctly_verifies_and_finalizes_chain_of_headers() {
		run_test(|| {
			let mut storage = PalletStorage::<TestRuntime>::new();
			let headers = vec![(1, false, false), (2, false, false)];
			let imported_headers = write_headers(&mut storage, headers);

			let mut header = TestHeader::new_from_number(3);
			header.parent_hash = imported_headers[2].hash();

			let mut verifier = Verifier {
				storage: storage.clone(),
			};
			assert!(verifier.import_header(header.clone()).is_ok());
			assert!(verifier.import_finality_proof(header.hash(), vec![4, 2].into()).is_ok());

			// Make sure we marked the our headers as finalized
			assert!(storage.header_by_hash(imported_headers[1].hash()).unwrap().is_finalized);
			assert!(storage.header_by_hash(imported_headers[2].hash()).unwrap().is_finalized);
			assert!(storage.header_by_hash(header.hash()).unwrap().is_finalized);

			// Make sure the header at the highest height is the best finalized
			assert_eq!(storage.best_finalized_header().header, header);
		});
	}

	#[test]
	fn updates_authority_set_upon_finalizing_header_which_enacts_change() {
		run_test(|| {
			let mut storage = PalletStorage::<TestRuntime>::new();
			let headers = vec![(1, false, false)];
			let imported_headers = write_headers(&mut storage, headers);

			let set_id = 0;
			let authorities = get_authorities(vec![(1, 1)]);
			let initial_authority_set = AuthoritySet::new(authorities, set_id);
			storage.update_current_authority_set(initial_authority_set);

			// This header enacts an authority set change upon finalization
			let mut header = TestHeader::new_from_number(2);
			header.parent_hash = imported_headers[1].hash();

			// Schedule a change at the height of our header
			let set_id = 1;
			let height = *header.number();
			let authorities = vec![(2, 1)];
			let change = schedule_next_change(authorities, set_id, height);
			storage.schedule_next_set_change(change.clone());

			let mut verifier = Verifier {
				storage: storage.clone(),
			};

			assert_ok!(verifier.import_header(header.clone()));
			assert_ok!(verifier.import_finality_proof(header.hash(), vec![4, 2].into()));
			assert_eq!(storage.best_finalized_header().header, header);

			// Make sure that we have updated the set now that we've finalized our header
			assert_eq!(storage.current_authority_set(), change.authority_set);
		})
	}

	#[test]
	fn importing_finality_proof_for_already_finalized_header_doesnt_work() {
		run_test(|| {
			let mut storage = PalletStorage::<TestRuntime>::new();
			let genesis = TestHeader::new_from_number(0);

			let genesis = ImportedHeader {
				header: genesis,
				requires_justification: false,
				is_finalized: true,
			};

			// Make sure that genesis is the best finalized header
			<BestFinalized<TestRuntime>>::put(genesis.hash());
			storage.write_header(&genesis);

			let mut verifier = Verifier { storage };

			// Now we want to try and import it again to see what happens
			assert_eq!(
				verifier
					.import_finality_proof(genesis.hash(), vec![4, 2].into())
					.unwrap_err(),
				FinalizationError::OldHeader
			);
		});
	}

	// We're supposed to enact a set change at header N. This means that when we import it we must
	// remember that it requires a justification. We can continue importing headers past N but must
	// not finalize any childen. At a later point in time we should be able to import the
	// justification for N.
	//
	// Since N enacts a new authority set, when we finalize it we should see this change reflected
	// correctly.
	//
	// [G] <- [N-1] <- [N] <- [N+1] <- [N+2]
	//                  |                |- Import justification for N here
	//                  |- Enacts change, needs justification
	#[test]
	fn allows_importing_justification_at_block_past_scheduled_change() {
		run_test(|| {
			let mut storage = PalletStorage::<TestRuntime>::new();
			let headers = vec![(1, false, false)];
			let imported_headers = write_headers(&mut storage, headers);

			// This is header N
			let mut header = TestHeader::new_from_number(2);
			header.parent_hash = imported_headers[1].hash();

			// Schedule a change at height N
			let set_id = 1;
			let height = *header.number();
			let authorities = vec![(1, 1)];
			let change = schedule_next_change(authorities, set_id, height);
			storage.schedule_next_set_change(change.clone());

			// Import header N
			let mut verifier = Verifier {
				storage: storage.clone(),
			};
			assert!(verifier.import_header(header.clone()).is_ok());

			// Header N should be marked as needing a justification
			assert_eq!(
				storage.header_by_hash(header.hash()).unwrap().requires_justification,
				true
			);

			// Now we want to import some headers which are past N
			let mut child = TestHeader::new_from_number(*header.number() + 1);
			child.parent_hash = header.hash();
			assert!(verifier.import_header(child.clone()).is_ok());

			let mut grandchild = TestHeader::new_from_number(*child.number() + 1);
			grandchild.parent_hash = child.hash();
			assert!(verifier.import_header(grandchild).is_ok());

			// Even though we're a few headers ahead we should still be able to import
			// a justification for header N
			assert!(verifier.import_finality_proof(header.hash(), vec![4, 2].into()).is_ok());

			// Some checks to make sure that our header has been correctly finalized
			let finalized_header = storage.header_by_hash(header.hash()).unwrap();
			assert!(finalized_header.is_finalized);
			assert_eq!(finalized_header.requires_justification, false);
			assert_eq!(storage.best_finalized_header().header, header);

			// Make sure we marked the parent of the header at N as finalized
			assert!(storage.header_by_hash(imported_headers[1].hash()).unwrap().is_finalized);

			// Since our header was supposed to enact a new authority set change when it got
			// finalized let's make sure that the authority set actually changed
			assert_eq!(storage.current_authority_set(), change.authority_set);
		})
	}
}
