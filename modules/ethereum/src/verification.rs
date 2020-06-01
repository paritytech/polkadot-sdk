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
use crate::validators::{step_validator, Validators, ValidatorsConfiguration};
use crate::{AuraConfiguration, ImportContext, PoolConfiguration, ScheduledChange, Storage};
use codec::Encode;
use primitives::{public_to_address, Address, Header, Receipt, SealedEmptyStep, H256, H520, U128, U256};
use sp_io::crypto::secp256k1_ecdsa_recover;
use sp_std::{vec, vec::Vec};

/// Pre-check to see if should try and import this header.
/// Returns error if we should not try to import this block.
/// Returns hash of the header and number of the last finalized block otherwise.
pub fn is_importable_header<S: Storage>(storage: &S, header: &Header) -> Result<(H256, H256), Error> {
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

/// Try accept unsigned aura header into transaction pool.
pub fn accept_aura_header_into_pool<S: Storage>(
	storage: &S,
	config: &AuraConfiguration,
	validators_config: &ValidatorsConfiguration,
	pool_config: &PoolConfiguration,
	header: &Header,
	receipts: Option<&Vec<Receipt>>,
) -> Result<(Vec<Vec<u8>>, Vec<Vec<u8>>), Error> {
	// check if we can verify further
	let (hash, _) = is_importable_header(storage, header)?;

	// we can always do contextless checks
	contextless_checks(config, header)?;

	// we want to avoid having same headers twice in the pool
	// => we're strict about receipts here - if we need them, we require receipts to be Some,
	// otherwise we require receipts to be None
	let receipts_required = Validators::new(validators_config).maybe_signals_validators_change(header);
	match (receipts_required, receipts.is_some()) {
		(true, false) => return Err(Error::MissingTransactionsReceipts),
		(false, true) => return Err(Error::RedundantTransactionsReceipts),
		_ => (),
	}

	// we do not want to have all future headers in the pool at once
	// => if we see header with number > maximal ever seen header number + LIMIT,
	// => we consider this transaction invalid, but only at this moment (we do not want to ban it)
	// => let's mark it as Unknown transaction
	let (best_number, best_hash, _) = storage.best_block();
	let difference = header.number.saturating_sub(best_number);
	if difference > pool_config.max_future_number_difference {
		return Err(Error::UnsignedTooFarInTheFuture);
	}

	// TODO: only accept new headers when we're at the tip of PoA chain
	// https://github.com/paritytech/parity-bridges-common/issues/38

	// we want to see at most one header with given number from single authority
	// => every header is providing tag (block_number + authority)
	// => since only one tx in the pool can provide the same tag, they're auto-deduplicated
	let provides_number_and_authority_tag = (header.number, header.author).encode();

	// we want to see several 'future' headers in the pool at once, but we may not have access to
	// previous headers here
	// => we can at least 'verify' that headers comprise a chain by providing and requiring
	// tag (header.number, header.hash)
	let provides_header_number_and_hash_tag = (header.number, hash).encode();

	// depending on whether parent header is available, we either perform full or 'shortened' check
	let context = storage.import_context(None, &header.parent_hash);
	let tags = match context {
		Some(context) => {
			let header_step = contextual_checks(config, &context, None, header)?;
			validator_checks(config, &context.validators_set().validators, header, header_step)?;

			// since our parent is already in the storage, we do not require it
			// to be in the transaction pool
			(
				vec![],
				vec![provides_number_and_authority_tag, provides_header_number_and_hash_tag],
			)
		}
		None => {
			// we know nothing about parent header
			// => the best thing we can do is to believe that there are no forks in
			// PoA chain AND that the header is produced either by previous, or next
			// scheduled validators set change
			let header_step = header.step().ok_or(Error::MissingStep)?;
			let best_context = storage.import_context(None, &best_hash).expect(
				"import context is None only when header is missing from the storage;\
							best header is always in the storage; qed",
			);
			let validators_check_result =
				validator_checks(config, &best_context.validators_set().validators, header, header_step);
			if let Err(error) = validators_check_result {
				find_next_validators_signal(storage, &best_context)
					.ok_or_else(|| error)
					.and_then(|next_validators| validator_checks(config, &next_validators, header, header_step))?;
			}

			// since our parent is missing from the storage, we **DO** require it
			// to be in the transaction pool
			// (- 1 can't underflow because there's always best block in the header)
			let requires_header_number_and_hash_tag = (header.number - 1, header.parent_hash).encode();
			(
				vec![requires_header_number_and_hash_tag],
				vec![provides_number_and_authority_tag, provides_header_number_and_hash_tag],
			)
		}
	};

	// the heaviest, but rare operation - we do not want invalid receipts in the pool
	if let Some(receipts) = receipts {
		if !header.check_transactions_receipts(receipts) {
			return Err(Error::TransactionsReceiptsMismatch);
		}
	}

	Ok(tags)
}

/// Verify header by Aura rules.
pub fn verify_aura_header<S: Storage>(
	storage: &S,
	config: &AuraConfiguration,
	submitter: Option<S::Submitter>,
	header: &Header,
) -> Result<ImportContext<S::Submitter>, Error> {
	// let's do the lightest check first
	contextless_checks(config, header)?;

	// the rest of checks requires access to the parent header
	let context = storage
		.import_context(submitter, &header.parent_hash)
		.ok_or(Error::MissingParentBlock)?;
	let header_step = contextual_checks(config, &context, None, header)?;
	validator_checks(config, &context.validators_set().validators, header, header_step)?;

	Ok(context)
}

/// Perform basic checks that only require header itself.
fn contextless_checks(config: &AuraConfiguration, header: &Header) -> Result<(), Error> {
	let expected_seal_fields = expected_header_seal_fields(config, header);
	if header.seal.len() != expected_seal_fields {
		return Err(Error::InvalidSealArity);
	}
	if header.number >= u64::max_value() {
		return Err(Error::RidiculousNumber);
	}
	if header.gas_used > header.gas_limit {
		return Err(Error::TooMuchGasUsed);
	}
	if header.gas_limit < config.min_gas_limit {
		return Err(Error::InvalidGasLimit);
	}
	if header.gas_limit > config.max_gas_limit {
		return Err(Error::InvalidGasLimit);
	}
	if header.number != 0 && header.extra_data.len() as u64 > config.maximum_extra_data_size {
		return Err(Error::ExtraDataOutOfBounds);
	}

	// we can't detect if block is from future in runtime
	// => let's only do an overflow check
	if header.timestamp > i32::max_value() as u64 {
		return Err(Error::TimestampOverflow);
	}

	Ok(())
}

/// Perform checks that require access to parent header.
fn contextual_checks<Submitter>(
	config: &AuraConfiguration,
	context: &ImportContext<Submitter>,
	validators_override: Option<&[Address]>,
	header: &Header,
) -> Result<u64, Error> {
	let validators = validators_override.unwrap_or_else(|| &context.validators_set().validators);
	let header_step = header.step().ok_or(Error::MissingStep)?;
	let parent_step = context.parent_header().step().ok_or(Error::MissingStep)?;

	// Ensure header is from the step after context.
	if header_step == parent_step || (header.number >= config.validate_step_transition && header_step <= parent_step) {
		return Err(Error::DoubleVote);
	}

	// If empty step messages are enabled we will validate the messages in the seal, missing messages are not
	// reported as there's no way to tell whether the empty step message was never sent or simply not included.
	let empty_steps_len = match header.number >= config.empty_steps_transition {
		true => {
			let strict_empty_steps = header.number >= config.strict_empty_steps_transition;
			let empty_steps = header.empty_steps().ok_or(Error::MissingEmptySteps)?;
			let empty_steps_len = empty_steps.len();
			let mut prev_empty_step = 0;

			for empty_step in empty_steps {
				if empty_step.step <= parent_step || empty_step.step >= header_step {
					return Err(Error::InsufficientProof);
				}

				if !verify_empty_step(&header.parent_hash, &empty_step, validators) {
					return Err(Error::InsufficientProof);
				}

				if strict_empty_steps {
					if empty_step.step <= prev_empty_step {
						return Err(Error::InsufficientProof);
					}

					prev_empty_step = empty_step.step;
				}
			}

			empty_steps_len
		}
		false => 0,
	};

	// Validate chain score.
	if header.number >= config.validate_score_transition {
		let expected_difficulty = calculate_score(parent_step, header_step, empty_steps_len as _);
		if header.difficulty != expected_difficulty {
			return Err(Error::InvalidDifficulty);
		}
	}

	Ok(header_step)
}

/// Check that block is produced by expected validator.
fn validator_checks(
	config: &AuraConfiguration,
	validators: &[Address],
	header: &Header,
	header_step: u64,
) -> Result<(), Error> {
	let expected_validator = step_validator(validators, header_step);
	if header.author != expected_validator {
		return Err(Error::NotValidator);
	}

	let validator_signature = header.signature().ok_or(Error::MissingSignature)?;
	let header_seal_hash = header
		.seal_hash(header.number >= config.empty_steps_transition)
		.ok_or(Error::MissingEmptySteps)?;
	let is_invalid_proposer = !verify_signature(&expected_validator, &validator_signature, &header_seal_hash);
	if is_invalid_proposer {
		return Err(Error::NotValidator);
	}

	Ok(())
}

/// Returns expected number of seal fields in the header.
fn expected_header_seal_fields(config: &AuraConfiguration, header: &Header) -> usize {
	if header.number >= config.empty_steps_transition {
		3
	} else {
		2
	}
}

/// Verify single sealed empty step.
fn verify_empty_step(parent_hash: &H256, step: &SealedEmptyStep, validators: &[Address]) -> bool {
	let expected_validator = step_validator(validators, step.step);
	let message = step.message(parent_hash);
	verify_signature(&expected_validator, &step.signature, &message)
}

/// Chain scoring: total weight is sqrt(U256::max_value())*height - step
fn calculate_score(parent_step: u64, current_step: u64, current_empty_steps: usize) -> U256 {
	U256::from(U128::max_value()) + U256::from(parent_step) - U256::from(current_step) + U256::from(current_empty_steps)
}

/// Verify that the signature over message has been produced by given validator.
fn verify_signature(expected_validator: &Address, signature: &H520, message: &H256) -> bool {
	secp256k1_ecdsa_recover(signature.as_fixed_bytes(), message.as_fixed_bytes())
		.map(|public| public_to_address(&public))
		.map(|address| *expected_validator == address)
		.unwrap_or(false)
}

/// Find next unfinalized validators set change after finalized set.
fn find_next_validators_signal<S: Storage>(storage: &S, context: &ImportContext<S::Submitter>) -> Option<Vec<Address>> {
	// that's the earliest block number we may met in following loop
	// it may be None if that's the first set
	let best_set_signal_block = context.validators_set().signal_block;

	// if parent schedules validators set change, then it may be our set
	// else we'll start with last known change
	let mut current_set_signal_block = context.last_signal_block().cloned();
	let mut next_scheduled_set: Option<ScheduledChange> = None;

	loop {
		// if we have reached block that signals finalized change, then
		// next_current_block_hash points to the block that schedules next
		// change
		let current_scheduled_set = match current_set_signal_block {
			Some(current_set_signal_block) if Some(&current_set_signal_block) == best_set_signal_block.as_ref() => {
				return next_scheduled_set.map(|scheduled_set| scheduled_set.validators)
			}
			None => return next_scheduled_set.map(|scheduled_set| scheduled_set.validators),
			Some(current_set_signal_block) => storage.scheduled_change(&current_set_signal_block).expect(
				"header that is associated with this change is not pruned;\
					scheduled changes are only removed when header is pruned; qed",
			),
		};

		current_set_signal_block = current_scheduled_set.prev_signal_block;
		next_scheduled_set = Some(current_scheduled_set);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{
		block_i, custom_block_i, custom_test_ext, genesis, insert_header, signed_header, test_aura_config, validator,
		validators_addresses, AccountId, TestRuntime,
	};
	use crate::validators::{tests::validators_change_recept, ValidatorsSource};
	use crate::{
		pool_configuration, BridgeStorage, FinalizedBlock, Headers, HeadersByNumber, NextValidatorsSetId,
		ScheduledChanges, ValidatorsSet, ValidatorsSets,
	};
	use frame_support::{StorageMap, StorageValue};
	use parity_crypto::publickey::{sign, KeyPair};
	use primitives::{rlp_encode, TransactionOutcome, H520};

	fn sealed_empty_step(validators: &[KeyPair], parent_hash: &H256, step: u64) -> SealedEmptyStep {
		let mut empty_step = SealedEmptyStep {
			step,
			signature: Default::default(),
		};
		let message = empty_step.message(parent_hash);
		let validator_index = (step % validators.len() as u64) as usize;
		let signature: [u8; 65] = sign(validators[validator_index].secret(), &message.as_fixed_bytes().into())
			.unwrap()
			.into();
		empty_step.signature = signature.into();
		empty_step
	}

	fn verify_with_config(config: &AuraConfiguration, header: &Header) -> Result<ImportContext<AccountId>, Error> {
		custom_test_ext(genesis(), validators_addresses(3)).execute_with(|| {
			let storage = BridgeStorage::<TestRuntime>::new();
			verify_aura_header(&storage, &config, None, header)
		})
	}

	fn default_verify(header: &Header) -> Result<ImportContext<AccountId>, Error> {
		verify_with_config(&test_aura_config(), header)
	}

	fn default_accept_into_pool(
		mut make_header: impl FnMut(&[KeyPair]) -> (Header, Option<Vec<Receipt>>),
	) -> Result<(Vec<Vec<u8>>, Vec<Vec<u8>>), Error> {
		custom_test_ext(genesis(), validators_addresses(3)).execute_with(|| {
			let validators = vec![validator(0), validator(1), validator(2)];
			let mut storage = BridgeStorage::<TestRuntime>::new();
			let block1 = block_i(1, &validators);
			insert_header(&mut storage, block1);
			let block2 = block_i(2, &validators);
			let block2_hash = block2.hash();
			insert_header(&mut storage, block2);
			let block3 = block_i(3, &validators);
			insert_header(&mut storage, block3);

			FinalizedBlock::put((2, block2_hash));

			let validators_config =
				ValidatorsConfiguration::Single(ValidatorsSource::Contract(Default::default(), Vec::new()));
			let (header, receipts) = make_header(&validators);
			accept_aura_header_into_pool(
				&storage,
				&test_aura_config(),
				&validators_config,
				&pool_configuration(),
				&header,
				receipts.as_ref(),
			)
		})
	}

	fn change_validators_set_at(number: u64, finalized_set: Vec<Address>, signalled_set: Option<Vec<Address>>) {
		let set_id = NextValidatorsSetId::get();
		NextValidatorsSetId::put(set_id + 1);
		ValidatorsSets::insert(
			set_id,
			ValidatorsSet {
				validators: finalized_set,
				signal_block: None,
				enact_block: HeadersByNumber::get(&0).unwrap()[0].clone(),
			},
		);

		let header_hash = HeadersByNumber::get(&number).unwrap()[0].clone();
		let mut header = Headers::<TestRuntime>::get(&header_hash).unwrap();
		header.next_validators_set_id = set_id;
		if let Some(signalled_set) = signalled_set {
			header.last_signal_block = Some(header.header.parent_hash);
			ScheduledChanges::insert(
				header.header.parent_hash,
				ScheduledChange {
					validators: signalled_set,
					prev_signal_block: None,
				},
			);
		}

		Headers::<TestRuntime>::insert(header_hash, header);
	}

	#[test]
	fn verifies_seal_count() {
		// when there are no seals at all
		let mut header = Header::default();
		assert_eq!(default_verify(&header), Err(Error::InvalidSealArity));

		// when there's single seal (we expect 2 or 3 seals)
		header.seal = vec![vec![].into()];
		assert_eq!(default_verify(&header), Err(Error::InvalidSealArity));

		// when there's 3 seals (we expect 2 by default)
		header.seal = vec![vec![].into(), vec![].into(), vec![].into()];
		assert_eq!(default_verify(&header), Err(Error::InvalidSealArity));

		// when there's 2 seals
		header.seal = vec![vec![].into(), vec![].into()];
		assert_ne!(default_verify(&header), Err(Error::InvalidSealArity));
	}

	#[test]
	fn verifies_header_number() {
		// when number is u64::max_value()
		let mut header = Header {
			seal: vec![vec![].into(), vec![].into(), vec![].into()],
			number: u64::max_value(),
			..Default::default()
		};
		assert_eq!(default_verify(&header), Err(Error::RidiculousNumber));

		// when header is < u64::max_value()
		header.seal = vec![vec![].into(), vec![].into()];
		header.number -= 1;
		assert_ne!(default_verify(&header), Err(Error::RidiculousNumber));
	}

	#[test]
	fn verifies_gas_used() {
		// when gas used is larger than gas limit
		let mut header = Header {
			seal: vec![vec![].into(), vec![].into()],
			gas_used: 1.into(),
			gas_limit: 0.into(),
			..Default::default()
		};
		assert_eq!(default_verify(&header), Err(Error::TooMuchGasUsed));

		// when gas used is less than gas limit
		header.gas_limit = 1.into();
		assert_ne!(default_verify(&header), Err(Error::TooMuchGasUsed));
	}

	#[test]
	fn verifies_gas_limit() {
		let mut config = test_aura_config();
		config.min_gas_limit = 100.into();
		config.max_gas_limit = 200.into();

		// when limit is lower than expected
		let mut header = Header {
			seal: vec![vec![].into(), vec![].into()],
			gas_limit: 50.into(),
			..Default::default()
		};
		assert_eq!(verify_with_config(&config, &header), Err(Error::InvalidGasLimit));

		// when limit is larger than expected
		header.gas_limit = 250.into();
		assert_eq!(verify_with_config(&config, &header), Err(Error::InvalidGasLimit));

		// when limit is within expected range
		header.gas_limit = 150.into();
		assert_ne!(verify_with_config(&config, &header), Err(Error::InvalidGasLimit));
	}

	#[test]
	fn verifies_extra_data_len() {
		// when extra data is too large
		let mut header = Header {
			seal: vec![vec![].into(), vec![].into()],
			gas_limit: test_aura_config().min_gas_limit,
			extra_data: std::iter::repeat(42).take(1000).collect::<Vec<_>>().into(),
			number: 1,
			..Default::default()
		};
		assert_eq!(default_verify(&header), Err(Error::ExtraDataOutOfBounds));

		// when extra data size is OK
		header.extra_data = std::iter::repeat(42).take(10).collect::<Vec<_>>().into();
		assert_ne!(default_verify(&header), Err(Error::ExtraDataOutOfBounds));
	}

	#[test]
	fn verifies_timestamp() {
		// when timestamp overflows i32
		let mut header = Header {
			seal: vec![vec![].into(), vec![].into()],
			gas_limit: test_aura_config().min_gas_limit,
			timestamp: i32::max_value() as u64 + 1,
			..Default::default()
		};
		assert_eq!(default_verify(&header), Err(Error::TimestampOverflow));

		// when timestamp doesn't overflow i32
		header.timestamp -= 1;
		assert_ne!(default_verify(&header), Err(Error::TimestampOverflow));
	}

	#[test]
	fn verifies_parent_existence() {
		// when there's no parent in the storage
		let mut header = Header {
			seal: vec![vec![].into(), vec![].into()],
			gas_limit: test_aura_config().min_gas_limit,
			..Default::default()
		};
		assert_eq!(default_verify(&header), Err(Error::MissingParentBlock));

		// when parent is in the storage
		header.parent_hash = genesis().hash();
		assert_ne!(default_verify(&header), Err(Error::MissingParentBlock));
	}

	#[test]
	fn verifies_step() {
		// when step is missing from seals
		let mut header = Header {
			seal: vec![vec![].into(), vec![].into()],
			gas_limit: test_aura_config().min_gas_limit,
			parent_hash: genesis().hash(),
			..Default::default()
		};
		assert_eq!(default_verify(&header), Err(Error::MissingStep));

		// when step is the same as for the parent block
		header.seal = vec![vec![42].into(), vec![].into()];
		assert_eq!(default_verify(&header), Err(Error::DoubleVote));

		// when step is OK
		header.seal = vec![vec![43].into(), vec![].into()];
		assert_ne!(default_verify(&header), Err(Error::DoubleVote));

		// now check with validate_step check enabled
		let mut config = test_aura_config();
		config.validate_step_transition = 0;

		// when step is lesser that for the parent block
		header.seal = vec![vec![40].into(), vec![].into()];
		assert_eq!(verify_with_config(&config, &header), Err(Error::DoubleVote));

		// when step is OK
		header.seal = vec![vec![44].into(), vec![].into()];
		assert_ne!(verify_with_config(&config, &header), Err(Error::DoubleVote));
	}

	#[test]
	fn verifies_empty_step() {
		let validators = vec![validator(0), validator(1), validator(2)];
		let mut config = test_aura_config();
		config.empty_steps_transition = 0;

		// when empty step duplicates parent step
		let mut header = Header {
			seal: vec![
				vec![45].into(),
				vec![142].into(),
				SealedEmptyStep::rlp_of(&[sealed_empty_step(&validators, &genesis().hash(), 42)]),
			],
			gas_limit: test_aura_config().min_gas_limit,
			parent_hash: genesis().hash(),
			..Default::default()
		};
		assert_eq!(verify_with_config(&config, &header), Err(Error::InsufficientProof));

		// when empty step signature check fails
		let mut wrong_sealed_empty_step = sealed_empty_step(&validators, &genesis().hash(), 43);
		wrong_sealed_empty_step.signature = Default::default();
		header.seal[2] = SealedEmptyStep::rlp_of(&[wrong_sealed_empty_step]);
		assert_eq!(verify_with_config(&config, &header), Err(Error::InsufficientProof));

		// when we are accepting strict empty steps and they come not in order
		config.strict_empty_steps_transition = 0;
		header.seal[2] = SealedEmptyStep::rlp_of(&[
			sealed_empty_step(&validators, &genesis().hash(), 44),
			sealed_empty_step(&validators, &genesis().hash(), 43),
		]);
		assert_eq!(verify_with_config(&config, &header), Err(Error::InsufficientProof));

		// when empty steps are OK
		header.seal[2] = SealedEmptyStep::rlp_of(&[
			sealed_empty_step(&validators, &genesis().hash(), 43),
			sealed_empty_step(&validators, &genesis().hash(), 44),
		]);
		assert_ne!(verify_with_config(&config, &header), Err(Error::InsufficientProof));
	}

	#[test]
	fn verifies_chain_score() {
		let mut config = test_aura_config();
		config.validate_score_transition = 0;

		// when chain score is invalid
		let mut header = Header {
			seal: vec![vec![43].into(), vec![].into()],
			gas_limit: test_aura_config().min_gas_limit,
			parent_hash: genesis().hash(),
			..Default::default()
		};
		assert_eq!(verify_with_config(&config, &header), Err(Error::InvalidDifficulty));

		// when chain score is accepted
		header.difficulty = calculate_score(42, 43, 0);
		assert_ne!(verify_with_config(&config, &header), Err(Error::InvalidDifficulty));
	}

	#[test]
	fn verifies_validator() {
		let validators = vec![validator(0), validator(1), validator(2)];
		let good_header = signed_header(
			&validators,
			Header {
				author: validators[1].address().as_fixed_bytes().into(),
				seal: vec![vec![43].into(), vec![].into()],
				gas_limit: test_aura_config().min_gas_limit,
				parent_hash: genesis().hash(),
				..Default::default()
			},
			43,
		);

		// when header author is invalid
		let mut header = good_header.clone();
		header.author = Default::default();
		assert_eq!(default_verify(&header), Err(Error::NotValidator));

		// when header signature is invalid
		let mut header = good_header.clone();
		header.seal[1] = rlp_encode(&H520::default());
		assert_eq!(default_verify(&header), Err(Error::NotValidator));

		// when everything is OK
		assert_eq!(default_verify(&good_header).map(|_| ()), Ok(()));
	}

	#[test]
	fn pool_verifies_known_blocks() {
		// when header is known
		assert_eq!(
			default_accept_into_pool(|validators| (block_i(3, validators), None)),
			Err(Error::KnownHeader),
		);
	}

	#[test]
	fn pool_verifies_ancient_blocks() {
		// when header number is less than finalized
		assert_eq!(
			default_accept_into_pool(|validators| (
				custom_block_i(2, validators, |header| header.gas_limit += 1.into()),
				None,
			),),
			Err(Error::AncientHeader),
		);
	}

	#[test]
	fn pool_rejects_headers_without_required_receipts() {
		assert_eq!(
			default_accept_into_pool(|_| (
				Header {
					number: 20_000_000,
					seal: vec![vec![].into(), vec![].into()],
					gas_limit: test_aura_config().min_gas_limit,
					log_bloom: (&[0xff; 256]).into(),
					..Default::default()
				},
				None,
			),),
			Err(Error::MissingTransactionsReceipts),
		);
	}

	#[test]
	fn pool_rejects_headers_with_redundant_receipts() {
		assert_eq!(
			default_accept_into_pool(|validators| (
				block_i(4, validators),
				Some(vec![Receipt {
					gas_used: 1.into(),
					log_bloom: (&[0xff; 256]).into(),
					logs: vec![],
					outcome: TransactionOutcome::Unknown,
				}]),
			),),
			Err(Error::RedundantTransactionsReceipts),
		);
	}

	#[test]
	fn pool_verifies_future_block_number() {
		// when header is too far from the future
		assert_eq!(
			default_accept_into_pool(|validators| (custom_block_i(4, validators, |header| header.number = 100), None,),),
			Err(Error::UnsignedTooFarInTheFuture),
		);
	}

	#[test]
	fn pool_performs_full_verification_when_parent_is_known() {
		// if parent is known, then we'll execute contextual_checks, which
		// checks for DoubleVote
		assert_eq!(
			default_accept_into_pool(|validators| (
				custom_block_i(4, validators, |header| header.seal[0] =
					block_i(3, validators).seal[0].clone()),
				None,
			),),
			Err(Error::DoubleVote),
		);
	}

	#[test]
	fn pool_performs_validators_checks_when_parent_is_unknown() {
		// if parent is unknown, then we still need to check if header has required signature
		// (even if header will be considered invalid/duplicate later, we can use this signature
		// as a proof of malicious action by this validator)
		assert_eq!(
			default_accept_into_pool(|validators| (
				signed_header(
					validators,
					Header {
						author: validators[1].address().as_fixed_bytes().into(),
						seal: vec![vec![8].into(), vec![].into()],
						gas_limit: test_aura_config().min_gas_limit,
						parent_hash: [42; 32].into(),
						number: 8,
						..Default::default()
					},
					43
				),
				None,
			)),
			Err(Error::NotValidator),
		);
	}

	#[test]
	fn pool_verifies_header_with_known_parent() {
		let mut hash = None;
		assert_eq!(
			default_accept_into_pool(|validators| {
				let header = block_i(4, &validators);
				hash = Some(header.hash());
				(header, None)
			}),
			Ok((
				// no tags are required
				vec![],
				// header provides two tags
				vec![
					(4u64, validators_addresses(3)[1]).encode(),
					(4u64, hash.unwrap()).encode(),
				],
			)),
		);
	}

	#[test]
	fn pool_verifies_header_with_unknown_parent() {
		let mut hash = None;
		assert_eq!(
			default_accept_into_pool(|validators| {
				let header = signed_header(
					validators,
					Header {
						author: validators[2].address().as_fixed_bytes().into(),
						seal: vec![vec![47].into(), vec![].into()],
						gas_limit: test_aura_config().min_gas_limit,
						parent_hash: [42; 32].into(),
						number: 5,
						..Default::default()
					},
					47,
				);
				hash = Some(header.hash());
				(header, None)
			}),
			Ok((
				// parent tag required
				vec![(4u64, [42u8; 32]).encode(),],
				// header provides two tags
				vec![
					(5u64, validators_addresses(3)[2]).encode(),
					(5u64, hash.unwrap()).encode(),
				],
			)),
		);
	}

	#[test]
	fn pool_uses_next_validators_set_when_finalized_fails() {
		assert_eq!(
			default_accept_into_pool(|actual_validators| {
				// change finalized set at parent header
				change_validators_set_at(3, validators_addresses(1), None);

				// header is signed using wrong set
				let header = signed_header(
					actual_validators,
					Header {
						author: actual_validators[2].address().as_fixed_bytes().into(),
						seal: vec![vec![47].into(), vec![].into()],
						gas_limit: test_aura_config().min_gas_limit,
						parent_hash: [42; 32].into(),
						number: 5,
						..Default::default()
					},
					47,
				);

				(header, None)
			}),
			Err(Error::NotValidator),
		);

		let mut hash = None;
		assert_eq!(
			default_accept_into_pool(|actual_validators| {
				// change finalized set at parent header + signal valid set at parent block
				change_validators_set_at(3, validators_addresses(10), Some(validators_addresses(3)));

				// header is signed using wrong set
				let header = signed_header(
					actual_validators,
					Header {
						author: actual_validators[2].address().as_fixed_bytes().into(),
						seal: vec![vec![47].into(), vec![].into()],
						gas_limit: test_aura_config().min_gas_limit,
						parent_hash: [42; 32].into(),
						number: 5,
						..Default::default()
					},
					47,
				);
				hash = Some(header.hash());

				(header, None)
			}),
			Ok((
				// parent tag required
				vec![(4u64, [42u8; 32]).encode(),],
				// header provides two tags
				vec![
					(5u64, validators_addresses(3)[2]).encode(),
					(5u64, hash.unwrap()).encode(),
				],
			)),
		);
	}

	#[test]
	fn pool_rejects_headers_with_invalid_receipts() {
		assert_eq!(
			default_accept_into_pool(|validators| {
				let header = custom_block_i(4, &validators, |header| {
					header.log_bloom = (&[0xff; 256]).into();
				});
				(header, Some(vec![validators_change_recept(Default::default())]))
			}),
			Err(Error::TransactionsReceiptsMismatch),
		);
	}

	#[test]
	fn pool_accepts_headers_with_valid_receipts() {
		let mut hash = None;
		assert_eq!(
			default_accept_into_pool(|validators| {
				let header = custom_block_i(4, &validators, |header| {
					header.log_bloom = (&[0xff; 256]).into();
					header.receipts_root = "81ce88dc524403b796222046bf3daf543978329b87ffd50228f1d3987031dc45"
						.parse()
						.unwrap();
				});
				hash = Some(header.hash());
				(header, Some(vec![validators_change_recept(Default::default())]))
			}),
			Ok((
				// no tags are required
				vec![],
				// header provides two tags
				vec![
					(4u64, validators_addresses(3)[1]).encode(),
					(4u64, hash.unwrap()).encode(),
				],
			)),
		);
	}
}
