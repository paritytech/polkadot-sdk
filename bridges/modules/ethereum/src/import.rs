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
use primitives::{H256, Header, Receipt};
use crate::{AuraConfiguration, Storage};
use crate::error::Error;
use crate::finality::finalize_blocks;
use crate::validators::{Validators, ValidatorsConfiguration};
use crate::verification::verify_aura_header;

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
	headers: Vec<(Header, Option<Vec<Receipt>>)>,
) -> Result<(u64, u64), Error> {
	let mut useful = 0;
	let mut useless = 0;
	for (header, receipts) in headers {
		let import_result = import_header(
			storage,
			aura_config,
			validators_config,
			prune_depth,
			header,
			receipts,
		);

		match import_result {
			Ok(_) => useful += 1,
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
pub fn import_header<S: Storage>(
	storage: &mut S,
	aura_config: &AuraConfiguration,
	validators_config: &ValidatorsConfiguration,
	prune_depth: u64,
	header: Header,
	receipts: Option<Vec<Receipt>>,
) -> Result<H256, Error> {
	// first check that we are able to import this header at all
	let (hash, prev_finalized_hash) = is_importable_header(storage, &header)?;

	// verify header
	let import_context = verify_aura_header(
		storage,
		aura_config,
		&header,
	)?;

	// check if block schedules new validators
	let validators = Validators::new(validators_config);
	let (scheduled_change, enacted_change) =
		validators.extract_validators_change(&header, receipts)?;

	// check if block finalizes some other blocks and corresponding scheduled validators
	let finalized_blocks = finalize_blocks(
		storage,
		&prev_finalized_hash,
		(import_context.validators_start(), import_context.validators()),
		&hash,
		&header,
		aura_config.two_thirds_majority_transition,
	)?;
	let enacted_change = enacted_change
		.or_else(|| validators.finalize_validators_change(storage, &finalized_blocks));

	// NOTE: we can't return Err() from anywhere below this line
	// (because otherwise we'll have inconsistent storage if transaction will fail)

	// and finally insert the block
	let (_, _, best_total_difficulty) = storage.best_block();
	let total_difficulty = import_context.total_difficulty() + header.difficulty;
	let is_best = total_difficulty > best_total_difficulty;
	let header_number = header.number;
	storage.insert_header(import_context.into_import_header(
		is_best,
		hash,
		header,
		total_difficulty,
		enacted_change,
		scheduled_change,
	));

	// now mark finalized headers && prune old headers
	storage.finalize_headers(
		finalized_blocks.last().cloned(),
		match is_best {
			true => header_number.checked_sub(prune_depth),
			false => None,
		},
	);

	Ok(hash)
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

/// Checks that we are able to ***try to** import this header.
/// Returns error if we should not try to import this block.
/// Returns hash of the header and number of the last finalized block.
fn is_importable_header<S: Storage>(storage: &S, header: &Header) -> Result<(H256, H256), Error> {
	// we never import any header that competes with finalized header
	let (finalized_block_number, finalized_block_hash) = storage.finalized_block();
	if header.number <= finalized_block_number {
		return Err(Error::AncientHeader);
	}
	// we never import any header with known hash
	let hash = header.hash();
	if storage.header(&hash).is_some() {
		return Err(Error::KnownHeader);
	}

	Ok((hash, finalized_block_hash))
}

#[cfg(test)]
mod tests {
	use crate::{kovan_aura_config, kovan_validators_config};
	use crate::tests::{
		InMemoryStorage,
		block_i, custom_block_i, signed_header, genesis,
		validator, validators_addresses,
	};
	use crate::validators::ValidatorsSource;
	use super::*;

	#[test]
	fn rejects_finalized_block_competitors() {
		let mut storage = InMemoryStorage::new(genesis(), validators_addresses(3));
		storage.finalize_headers(Some((100, Default::default())), None);
		assert_eq!(
			import_header(
				&mut storage,
				&kovan_aura_config(),
				&kovan_validators_config(),
				PRUNE_DEPTH,
				Default::default(),
				None,
			),
			Err(Error::AncientHeader),
		);
	}

	#[test]
	fn rejects_known_header() {
		let validators = (0..3).map(|i| validator(i as u8)).collect::<Vec<_>>();
		let mut storage = InMemoryStorage::new(genesis(), validators_addresses(3));
		let block = block_i(&storage, 1, &validators);
		assert_eq!(
			import_header(
				&mut storage,
				&kovan_aura_config(),
				&kovan_validators_config(),
				PRUNE_DEPTH,
				block.clone(),
				None,
			).map(|_| ()),
			Ok(()),
		);
		assert_eq!(
			import_header(
				&mut storage,
				&kovan_aura_config(),
				&kovan_validators_config(),
				PRUNE_DEPTH,
				block,
				None,
			).map(|_| ()),
			Err(Error::KnownHeader),
		);
	}

	#[test]
	fn import_header_works() {
		let validators_config = ValidatorsConfiguration::Multi(vec![
			(0, ValidatorsSource::List(validators_addresses(3))),
			(1, ValidatorsSource::List(validators_addresses(2))),
		]);
		let validators = (0..3).map(|i| validator(i as u8)).collect::<Vec<_>>();
		let mut storage = InMemoryStorage::new(genesis(), validators_addresses(3));
		let header = block_i(&storage, 1, &validators);
		let hash = header.hash();
		assert_eq!(
			import_header(&mut storage, &kovan_aura_config(), &validators_config, PRUNE_DEPTH, header, None)
				.map(|_| ()),
			Ok(()),
		);

		// check that new validators will be used for next header
		let imported_header = storage.stored_header(&hash).unwrap();
		assert_eq!(
			imported_header.next_validators_set_id,
			1, // new set is enacted from config
		);
	}

	#[test]
	fn headers_are_pruned() {
		let validators_config = ValidatorsConfiguration::Single(
			ValidatorsSource::Contract([3; 20].into(), validators_addresses(3)),
		);
		let validators = vec![validator(0), validator(1), validator(2)];
		let mut storage = InMemoryStorage::new(genesis(), validators_addresses(3));

		// header [0..11] are finalizing blocks [0; 9]
		// => since we want to keep 10 finalized blocks, we aren't pruning anything
		let mut last_block_hash = Default::default();
		for i in 1..11 {
			let header = block_i(&storage, i, &validators);
			last_block_hash = import_header(
				&mut storage,
				&kovan_aura_config(),
				&validators_config,
				10,
				header,
				None,
			).unwrap();
		}
		assert!(storage.header(&genesis().hash()).is_some());

		// header 11 finalizes headers [10] AND schedules change
		// => we prune header#0
		let header = custom_block_i(&storage, 11, &validators, |header| {
			header.log_bloom = (&[0xff; 256]).into();
			header.receipts_root = "2e60346495092587026484e868a5b3063749032b2ea3843844509a6320d7f951".parse().unwrap();
		});
		last_block_hash = import_header(
			&mut storage,
			&kovan_aura_config(),
			&validators_config,
			10,
			header,
			Some(vec![crate::validators::tests::validators_change_recept(last_block_hash)]),
		).unwrap();
		assert!(storage.header(&genesis().hash()).is_none());

		// and now let's say validators 1 && 2 went offline
		// => in the range 12-25 no blocks are finalized, but we still continue to prune old headers
		// until header#11 is met. we can't prune #11, because it schedules change
		let mut step = 56;
		for i in 12..25 {
			let header = Header {
				number: i as _,
				parent_hash: last_block_hash,
				gas_limit: 0x2000.into(),
				author: validator(2).address().to_fixed_bytes().into(),
				seal: vec![
					vec![step].into(),
					vec![].into(),
				],
				difficulty: i.into(),
				..Default::default()
			};
			let header = signed_header(&validators, header, step as _);
			last_block_hash = import_header(
				&mut storage,
				&kovan_aura_config(),
				&validators_config,
				10,
				header,
				None,
			).unwrap();
			step += 3;
		}
		assert_eq!(storage.oldest_unpruned_block(), 11);

		// now let's insert block signed by validator 1
		// => blocks 11..24 are finalized and blocks 11..14 are pruned
		step -= 2;
		let header = Header {
			number: 25,
			parent_hash: last_block_hash,
			gas_limit: 0x2000.into(),
			author: validator(0).address().to_fixed_bytes().into(),
			seal: vec![
				vec![step].into(),
				vec![].into(),
			],
			difficulty: 25.into(),
			..Default::default()
		};
		let header = signed_header(&validators, header, step as _);
		import_header(
			&mut storage,
			&kovan_aura_config(),
			&validators_config,
			10,
			header,
			None,
		).unwrap();
		assert_eq!(storage.oldest_unpruned_block(), 15);
	}
}
