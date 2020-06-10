// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

use crate::error::Error;
use crate::finality::finalize_blocks;
use crate::validators::{Validators, ValidatorsConfiguration};
use crate::verification::{is_importable_header, verify_aura_header};
use crate::{AuraConfiguration, ChangeToEnact, Storage};
use primitives::{Header, HeaderId, Receipt};
use sp_std::{collections::btree_map::BTreeMap, prelude::*};

/// Maximal number of headers behind best blocks that we are aiming to store. When there
/// are too many unfinalized headers, it slows down finalization tracking significantly.
/// That's why we won't consider imports/reorganizations to blocks of PRUNE_DEPTH age.
/// If there's more headers than that, we prune the oldest. The only exception is
/// when unfinalized header schedules validators set change. We can't compute finality
/// for pruned headers => we won't know when to enact validators set change. That's
/// why we never prune headers with scheduled changes.
pub(crate) const PRUNE_DEPTH: u64 = 4096;

/// Imports bunch of headers and updates blocks finality.
///
/// Transactions receipts must be provided if `header_import_requires_receipts()`
/// has returned true.
/// If successful, returns tuple where first element is the number of useful headers
/// we have imported and the second element is the number of useless headers (duplicate)
/// we have NOT imported.
/// Returns error if fatal error has occured during import. Some valid headers may be
/// imported in this case.
pub fn import_headers<S: Storage>(
	storage: &mut S,
	aura_config: &AuraConfiguration,
	validators_config: &ValidatorsConfiguration,
	prune_depth: u64,
	submitter: Option<S::Submitter>,
	headers: Vec<(Header, Option<Vec<Receipt>>)>,
	finalized_headers: &mut BTreeMap<S::Submitter, u64>,
) -> Result<(u64, u64), Error> {
	let mut useful = 0;
	let mut useless = 0;
	for (header, receipts) in headers {
		let import_result = import_header(
			storage,
			aura_config,
			validators_config,
			prune_depth,
			submitter.clone(),
			header,
			receipts,
		);

		match import_result {
			Ok((_, finalized)) => {
				for (_, submitter) in finalized {
					if let Some(submitter) = submitter {
						*finalized_headers.entry(submitter).or_default() += 1;
					}
				}
				useful += 1;
			}
			Err(Error::AncientHeader) | Err(Error::KnownHeader) => useless += 1,
			Err(error) => return Err(error),
		}
	}

	Ok((useful, useless))
}

/// Imports given header and updates blocks finality (if required).
///
/// Transactions receipts must be provided if `header_import_requires_receipts()`
/// has returned true.
///
/// Returns imported block id and list of all finalized headers.
pub fn import_header<S: Storage>(
	storage: &mut S,
	aura_config: &AuraConfiguration,
	validators_config: &ValidatorsConfiguration,
	prune_depth: u64,
	submitter: Option<S::Submitter>,
	header: Header,
	receipts: Option<Vec<Receipt>>,
) -> Result<(HeaderId, Vec<(HeaderId, Option<S::Submitter>)>), Error> {
	// first check that we are able to import this header at all
	let (header_id, finalized_id) = is_importable_header(storage, &header)?;

	// verify header
	let import_context = verify_aura_header(storage, aura_config, submitter, &header)?;

	// check if block schedules new validators
	let validators = Validators::new(validators_config);
	let (scheduled_change, enacted_change) = validators.extract_validators_change(&header, receipts)?;

	// check if block finalizes some other blocks and corresponding scheduled validators
	let validators_set = import_context.validators_set();
	let finalized_blocks = finalize_blocks(
		storage,
		finalized_id,
		(validators_set.enact_block, &validators_set.validators),
		header_id,
		import_context.submitter(),
		&header,
		aura_config.two_thirds_majority_transition,
	)?;
	let enacted_change = enacted_change
		.map(|validators| ChangeToEnact {
			signal_block: None,
			validators,
		})
		.or_else(|| validators.finalize_validators_change(storage, &finalized_blocks.finalized_headers));

	// NOTE: we can't return Err() from anywhere below this line
	// (because otherwise we'll have inconsistent storage if transaction will fail)

	// and finally insert the block
	let (_, best_total_difficulty) = storage.best_block();
	let total_difficulty = import_context.total_difficulty() + header.difficulty;
	let is_best = total_difficulty > best_total_difficulty;
	let header_number = header.number;
	storage.insert_header(import_context.into_import_header(
		is_best,
		header_id,
		header,
		total_difficulty,
		enacted_change,
		scheduled_change,
		finalized_blocks.votes,
	));

	// now mark finalized headers && prune old headers
	storage.finalize_headers(
		finalized_blocks.finalized_headers.last().map(|(id, _)| *id),
		match is_best {
			true => header_number.checked_sub(prune_depth),
			false => None,
		},
	);

	Ok((header_id, finalized_blocks.finalized_headers))
}

/// Returns true if transactions receipts are required to import given header.
pub fn header_import_requires_receipts<S: Storage>(
	storage: &S,
	validators_config: &ValidatorsConfiguration,
	header: &Header,
) -> bool {
	is_importable_header(storage, header)
		.map(|_| Validators::new(validators_config))
		.map(|validators| validators.maybe_signals_validators_change(header))
		.unwrap_or(false)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{
		block_i, custom_block_i, custom_test_ext, genesis, signed_header, test_aura_config, test_validators_config,
		validator, validators, validators_addresses, TestRuntime,
	};
	use crate::validators::ValidatorsSource;
	use crate::{BlocksToPrune, BridgeStorage, Headers, PruningRange};
	use frame_support::{StorageMap, StorageValue};

	#[test]
	fn rejects_finalized_block_competitors() {
		custom_test_ext(genesis(), validators_addresses(3)).execute_with(|| {
			let mut storage = BridgeStorage::<TestRuntime>::new();
			storage.finalize_headers(
				Some(HeaderId {
					number: 100,
					..Default::default()
				}),
				None,
			);
			assert_eq!(
				import_header(
					&mut storage,
					&test_aura_config(),
					&test_validators_config(),
					PRUNE_DEPTH,
					None,
					Default::default(),
					None,
				),
				Err(Error::AncientHeader),
			);
		});
	}

	#[test]
	fn rejects_known_header() {
		custom_test_ext(genesis(), validators_addresses(3)).execute_with(|| {
			let validators = validators(3);
			let mut storage = BridgeStorage::<TestRuntime>::new();
			let block = block_i(1, &validators);
			assert_eq!(
				import_header(
					&mut storage,
					&test_aura_config(),
					&test_validators_config(),
					PRUNE_DEPTH,
					None,
					block.clone(),
					None,
				)
				.map(|_| ()),
				Ok(()),
			);
			assert_eq!(
				import_header(
					&mut storage,
					&test_aura_config(),
					&test_validators_config(),
					PRUNE_DEPTH,
					None,
					block,
					None,
				)
				.map(|_| ()),
				Err(Error::KnownHeader),
			);
		});
	}

	#[test]
	fn import_header_works() {
		custom_test_ext(genesis(), validators_addresses(3)).execute_with(|| {
			let validators_config = ValidatorsConfiguration::Multi(vec![
				(0, ValidatorsSource::List(validators_addresses(3))),
				(1, ValidatorsSource::List(validators_addresses(2))),
			]);
			let validators = validators(3);
			let mut storage = BridgeStorage::<TestRuntime>::new();
			let header = block_i(1, &validators);
			let hash = header.compute_hash();
			assert_eq!(
				import_header(
					&mut storage,
					&test_aura_config(),
					&validators_config,
					PRUNE_DEPTH,
					None,
					header,
					None
				)
				.map(|_| ()),
				Ok(()),
			);

			// check that new validators will be used for next header
			let imported_header = Headers::<TestRuntime>::get(&hash).unwrap();
			assert_eq!(
				imported_header.next_validators_set_id,
				1, // new set is enacted from config
			);
		});
	}

	#[test]
	fn headers_are_pruned_during_import() {
		custom_test_ext(genesis(), validators_addresses(3)).execute_with(|| {
			let validators_config =
				ValidatorsConfiguration::Single(ValidatorsSource::Contract([3; 20].into(), validators_addresses(3)));
			let validators = vec![validator(0), validator(1), validator(2)];
			let mut storage = BridgeStorage::<TestRuntime>::new();

			// header [0..11] are finalizing blocks [0; 9]
			// => since we want to keep 10 finalized blocks, we aren't pruning anything
			let mut latest_block_id = Default::default();
			for i in 1..11 {
				let header = block_i(i, &validators);
				let (rolling_last_block_id, finalized_blocks) = import_header(
					&mut storage,
					&test_aura_config(),
					&validators_config,
					10,
					Some(100),
					header,
					None,
				)
				.unwrap();
				match i {
					2..=10 => assert_eq!(
						finalized_blocks,
						vec![(block_i(i - 1, &validators).compute_id(), Some(100))],
						"At {}",
						i,
					),
					_ => assert_eq!(finalized_blocks, vec![], "At {}", i),
				}
				latest_block_id = rolling_last_block_id;
			}
			assert!(storage.header(&genesis().compute_hash()).is_some());

			// header 11 finalizes headers [10] AND schedules change
			// => we prune header#0
			let header11 = custom_block_i(11, &validators, |header| {
				header.log_bloom = (&[0xff; 256]).into();
				header.receipts_root = "2e60346495092587026484e868a5b3063749032b2ea3843844509a6320d7f951"
					.parse()
					.unwrap();
			});
			let (rolling_last_block_id, finalized_blocks) = import_header(
				&mut storage,
				&test_aura_config(),
				&validators_config,
				10,
				Some(101),
				header11.clone(),
				Some(vec![crate::validators::tests::validators_change_recept(
					latest_block_id.hash,
				)]),
			)
			.unwrap();
			assert_eq!(
				finalized_blocks,
				vec![(block_i(10, &validators).compute_id(), Some(100))],
			);
			assert!(storage.header(&genesis().compute_hash()).is_none());
			latest_block_id = rolling_last_block_id;

			// and now let's say validators 1 && 2 went offline
			// => in the range 12-25 no blocks are finalized, but we still continue to prune old headers
			// until header#11 is met. we can't prune #11, because it schedules change
			let mut step = 56;
			let mut expected_blocks = vec![(header11.compute_id(), Some(101))];
			for i in 12..25 {
				let header = Header {
					number: i as _,
					parent_hash: latest_block_id.hash,
					gas_limit: 0x2000.into(),
					author: validator(2).address(),
					seal: vec![vec![step].into(), vec![].into()],
					difficulty: i.into(),
					..Default::default()
				};
				let header = signed_header(&validators, header, step as _);
				expected_blocks.push((header.compute_id(), Some(102)));
				let (rolling_last_block_id, finalized_blocks) = import_header(
					&mut storage,
					&test_aura_config(),
					&validators_config,
					10,
					Some(102),
					header,
					None,
				)
				.unwrap();
				assert_eq!(finalized_blocks, vec![],);
				latest_block_id = rolling_last_block_id;
				step += 3;
			}
			assert_eq!(
				BlocksToPrune::get(),
				PruningRange {
					oldest_unpruned_block: 11,
					oldest_block_to_keep: 14,
				},
			);

			// now let's insert block signed by validator 1
			// => blocks 11..24 are finalized and blocks 11..14 are pruned
			step -= 2;
			let header = Header {
				number: 25,
				parent_hash: latest_block_id.hash,
				gas_limit: 0x2000.into(),
				author: validator(0).address(),
				seal: vec![vec![step].into(), vec![].into()],
				difficulty: 25.into(),
				..Default::default()
			};
			let header = signed_header(&validators, header, step as _);
			let (_, finalized_blocks) = import_header(
				&mut storage,
				&test_aura_config(),
				&validators_config,
				10,
				Some(103),
				header,
				None,
			)
			.unwrap();
			assert_eq!(finalized_blocks, expected_blocks);
			assert_eq!(
				BlocksToPrune::get(),
				PruningRange {
					oldest_unpruned_block: 15,
					oldest_block_to_keep: 15,
				},
			);
		});
	}
}
