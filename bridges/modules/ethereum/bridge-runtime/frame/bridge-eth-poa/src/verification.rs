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

use sp_io::crypto::secp256k1_ecdsa_recover;
use primitives::{Address, Header, H256, H520, SealedEmptyStep, U128, U256, public_to_address};
use crate::{AuraConfiguration, ImportContext, Storage};
use crate::error::Error;
use crate::validators::step_validator;

/// Verify header by Aura rules.
pub fn verify_aura_header<S: Storage>(
	storage: &S,
	params: &AuraConfiguration,
	header: &Header,
) -> Result<ImportContext, Error> {
	// let's do the lightest check first
	contextless_checks(params, header)?;

	// the rest of checks requires parent
	let context = storage.import_context(&header.parent_hash).ok_or(Error::MissingParentBlock)?;
	let validators = context.validators();
	let header_step = header.step().ok_or(Error::MissingStep)?;
	let parent_step = context.parent_header().step().ok_or(Error::MissingStep)?;

	// Ensure header is from the step after context.
	if header_step == parent_step
		|| (header.number >= params.validate_step_transition && header_step <= parent_step) {
		return Err(Error::DoubleVote);
	}

	// If empty step messages are enabled we will validate the messages in the seal, missing messages are not
	// reported as there's no way to tell whether the empty step message was never sent or simply not included.
	let empty_steps_len = match header.number >= params.empty_steps_transition {
		true => {
			let strict_empty_steps = header.number >= params.strict_empty_steps_transition;
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
		},
		false => 0,
	};

	// Validate chain score.
	if header.number >= params.validate_score_transition {
		let expected_difficulty = calculate_score(parent_step, header_step, empty_steps_len as _);
		if header.difficulty != expected_difficulty {
			return Err(Error::InvalidDifficulty);
		}
	}

	let expected_validator = step_validator(validators, header_step);
	if header.author != expected_validator {
		return Err(Error::NotValidator);
	}

	let validator_signature = header.signature().ok_or(Error::MissingSignature)?;
	let header_seal_hash = header
		.seal_hash(header.number >= params.empty_steps_transition)
		.ok_or(Error::MissingEmptySteps)?;
	let is_invalid_proposer = !verify_signature(&expected_validator, &validator_signature, &header_seal_hash);
	if is_invalid_proposer {
		return Err(Error::NotValidator);
	}

	Ok(context)
}

/// Perform basic checks that only require header iteself.
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

#[cfg(test)]
mod tests {
	use parity_crypto::publickey::{KeyPair, sign};
	use primitives::{H520, rlp_encode};
	use crate::kovan_aura_config;
	use crate::tests::{InMemoryStorage, genesis, signed_header, validator, validators_addresses};
	use super::*;

	fn sealed_empty_step(validators: &[KeyPair], parent_hash: &H256, step: u64) -> SealedEmptyStep {
		let mut empty_step = SealedEmptyStep { step, signature: Default::default() };
		let message = empty_step.message(parent_hash);
		let validator_index = (step % validators.len() as u64) as usize;
		let signature: [u8; 65] = sign(
			validators[validator_index].secret(),
			&message.as_fixed_bytes().into(),
		).unwrap().into();
		empty_step.signature = signature.into();
		empty_step
	}

	fn verify_with_config(config: &AuraConfiguration, header: &Header) -> Result<ImportContext, Error> {
		let storage = InMemoryStorage::new(genesis(), validators_addresses(3));
		verify_aura_header(&storage, &config, header)
	}

	fn default_verify(header: &Header) -> Result<ImportContext, Error> {
		verify_with_config(&kovan_aura_config(), header)
	}

	#[test]
	fn verifies_seal_count() {
		// when there are no seals at all
		let mut header = Header::default();
		assert_eq!(default_verify(&header), Err(Error::InvalidSealArity));

		// when there's single seal (we expect 2 or 3 seals)
		header.seal = vec![vec![].into()];
		assert_eq!(default_verify(&header), Err(Error::InvalidSealArity));

		// when there's 3 seals (we expect 2 on Kovan)
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
		let mut config = kovan_aura_config();
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
			gas_limit: kovan_aura_config().min_gas_limit,
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
			gas_limit: kovan_aura_config().min_gas_limit,
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
			gas_limit: kovan_aura_config().min_gas_limit,
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
			gas_limit: kovan_aura_config().min_gas_limit,
			parent_hash: genesis().hash(),
			..Default::default()
		};
		assert_eq!(default_verify(&header), Err(Error::MissingStep));

		// when step is the same as for the parent block
		header.seal = vec![
			vec![42].into(),
			vec![].into(),
		];
		assert_eq!(default_verify(&header), Err(Error::DoubleVote));

		// when step is OK
		header.seal = vec![
			vec![43].into(),
			vec![].into(),
		];
		assert_ne!(default_verify(&header), Err(Error::DoubleVote));

		// now check with validate_step check enabled
		let mut config = kovan_aura_config();
		config.validate_step_transition = 0;

		// when step is lesser that for the parent block
		header.seal = vec![
			vec![40].into(),
			vec![].into(),
		];
		assert_eq!(verify_with_config(&config, &header), Err(Error::DoubleVote));

		// when step is OK
		header.seal = vec![
			vec![44].into(),
			vec![].into(),
		];
		assert_ne!(verify_with_config(&config, &header), Err(Error::DoubleVote));
	}

	#[test]
	fn verifies_empty_step() {
		let validators = vec![validator(0), validator(1), validator(2)];
		let mut config = kovan_aura_config();
		config.empty_steps_transition = 0;

		// when empty step duplicates parent step
		let mut header = Header {
			seal: vec![
				vec![45].into(),
				vec![142].into(),
				SealedEmptyStep::rlp_of(&[
					sealed_empty_step(&validators, &genesis().hash(), 42),
				]),
			],
			gas_limit: kovan_aura_config().min_gas_limit,
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
		let mut config = kovan_aura_config();
		config.validate_score_transition = 0;

		// when chain score is invalid
		let mut header = Header {
			seal: vec![
				vec![43].into(),
				vec![].into(),
			],
			gas_limit: kovan_aura_config().min_gas_limit,
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
		let good_header = signed_header(&validators, Header {
			author: validators[1].address().as_fixed_bytes().into(),
			seal: vec![
				vec![43].into(),
				vec![].into(),
			],
			gas_limit: kovan_aura_config().min_gas_limit,
			parent_hash: genesis().hash(),
			..Default::default()
		}, 43);

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
}
