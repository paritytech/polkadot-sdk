// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::{
	H160, ReceiptGasInfo, SimulationError, U256,
	evm::{
		GenericTransaction, SimulationPayload, StateOverrides, Withdrawal,
		block_hash::EthereumBlockBuilderMockHandler,
	},
	mock::MockHandler,
};
use alloc::{fmt::Debug, vec::Vec};
use alloy_core::primitives::BLOOM_SIZE_BYTES;
use sp_core::{ConstU32, H256};
use sp_runtime::BoundedBTreeMap;

/// A resolved model for the block overrides for the simulation runtime function.
///
/// This model contains the resolved block number and the timestamp which are required in the
/// simulation method to be monotonically increasing. In the case of blocks, they're required to be
/// without gaps and in the case of timestamps they're required to be monotonically increasing but
/// with no restriction on gaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSimulationBlockOverrides {
	/// Block number
	pub block_number: U256,

	/// Block timestamp
	pub block_timestamp: U256,

	/// The previous value of randomness beacon
	pub prev_randao: Option<U256>,

	/// Gas limit
	pub gas_limit: Option<U256>,

	/// Fee recipient (also known as coinbase).
	pub fee_recipient: Option<H160>,

	/// Withdrawals made by validators.
	pub withdrawals: Option<Vec<Withdrawal>>,

	/// Base fee per unit of gas (see EIP-1559).
	pub base_fee_per_gas: Option<U256>,

	/// Base fee per unit of blob gas (see EIP-4844).
	pub blob_base_fee: Option<U256>,

	/// A boolean which defines if the block is a filler block or not.
	pub is_filler_gap_block: bool,
}

impl ResolvedSimulationBlockOverrides {
	pub fn new_filler_gap_block(block_number: U256, block_timestamp: U256) -> Self {
		Self {
			block_number,
			block_timestamp,
			prev_randao: Default::default(),
			gas_limit: Default::default(),
			fee_recipient: Default::default(),
			withdrawals: Default::default(),
			base_fee_per_gas: Default::default(),
			blob_base_fee: Default::default(),
			is_filler_gap_block: true,
		}
	}
}

/// The set of blocks to use in the simulation.
///
/// This struct is used to collect the simulation blocks into a coherent structure that's easier to
/// simulate and also easier to validate.
///
/// The Geth spec requires a number of things from the blocks:
///
/// 1. The block number of the first block in the simulation must be strictly larger than the block
///    number that was passed to the method.
/// 2. That the block numbers must be monotonically increasing.
/// 3. That there must exist filler empty blocks in the place of gaps in the blocks.
/// 4. That there can only exist a maximum of 256 blocks in the simulation blocks.
///
/// For the timestamps, the only requirement is that they're equal or monotonically increasing. This
/// struct enforces all of these requirements by allowing the user to add the blocks one by one and
/// doing the above checks when each block is added.
pub struct SimulationBlocks<const BOUND: u32 = 0x100> {
	/// The block number of the last finalized block in the chain.
	block_number_of_last_finalized_block: U256,
	/// The block timestamp of the last finalized block in the chain.
	block_timestamp_of_last_finalized_block: U256,
	/// The map which contains all of the blocks and their associated calls. This is a mapping of
	/// block number to the overrides and the calls that need to be performed in that block with
	/// that set of overrides.
	simulation_blocks: BoundedBTreeMap<
		U256,
		(ResolvedSimulationBlockOverrides, StateOverrides, Vec<GenericTransaction>),
		ConstU32<BOUND>,
	>,
}

impl<const BOUND: u32> SimulationBlocks<BOUND> {
	/// Creates a new [`SimulationBlocks`] with the given upper bound.
	pub fn new(
		block_number_of_last_finalized_block: U256,
		block_timestamp_of_last_finalized_block: U256,
	) -> Self {
		Self {
			simulation_blocks: Default::default(),
			block_number_of_last_finalized_block,
			block_timestamp_of_last_finalized_block,
		}
	}

	/// Given a [`SimulationPayload`] this method performs the validations described on
	/// [`SimulationBlocks`], unpacks the information, and stores it internally.
	pub fn insert_simulation_payload(
		&mut self,
		SimulationPayload { block_overrides, state_overrides, calls }: SimulationPayload,
	) -> Result<(), SimulationError> {
		let (last_block_number, ..) = self.last_block_number_and_timestamp();
		let block_overrides = block_overrides.unwrap_or_default();
		let state_overrides = state_overrides.unwrap_or_default();

		let block_number_override = block_overrides
			.number
			.or_else(|| last_block_number.checked_add(U256::one()))
			.ok_or(SimulationError::OverflowError)?;
		if block_number_override <= last_block_number {
			return Err(SimulationError::BlockNumberOverrideMustBeMonotonicallyIncreasing);
		}

		let gap_blocks_to_add = block_number_override
			.checked_sub(U256::one())
			.and_then(|value| value.checked_sub(last_block_number))
			.ok_or(SimulationError::OverflowError)?;
		self.insert_gap_blocks_by_quantity(gap_blocks_to_add)?;

		let (_, last_block_timestamp) = self.last_block_number_and_timestamp();
		let block_timestamp_override = block_overrides
			.time
			.or_else(|| last_block_timestamp.checked_add(U256::one()))
			.ok_or(SimulationError::OverflowError)?;
		if block_timestamp_override < last_block_timestamp {
			return Err(
				SimulationError::BlockTimestampOverrideMustBeEqualOrMonotonicallyIncreasing,
			);
		}

		let resolved_block_override = ResolvedSimulationBlockOverrides {
			block_number: block_number_override,
			block_timestamp: block_timestamp_override,
			prev_randao: block_overrides.prev_randao,
			gas_limit: block_overrides.gas_limit,
			fee_recipient: block_overrides.fee_recipient,
			withdrawals: block_overrides.withdrawals,
			base_fee_per_gas: block_overrides.base_fee_per_gas,
			blob_base_fee: block_overrides.blob_base_fee,
			is_filler_gap_block: false,
		};
		self.simulation_blocks
			.try_insert(block_number_override, (resolved_block_override, state_overrides, calls))
			.map_err(|_| SimulationError::BlockCapacityExceeded)?;

		Ok(())
	}

	pub fn into_inner(
		self,
	) -> impl Iterator<Item = (ResolvedSimulationBlockOverrides, StateOverrides, Vec<GenericTransaction>)>
	{
		self.simulation_blocks.into_inner().into_values()
	}

	/// Inserts a specific quantity of gap blocks into the simulation blocks.
	fn insert_gap_blocks_by_quantity(&mut self, quantity: U256) -> Result<(), SimulationError> {
		// Converting the quantity to a u32 is safe here since the bounds are a u32.
		let quantity = u32::try_from(quantity).unwrap_or(u32::MAX);
		for _ in 0..quantity {
			self.insert_gap_block()?
		}
		Ok(())
	}

	/// Inserts a single gap block with an incremented block number and timestamp
	fn insert_gap_block(&mut self) -> Result<(), SimulationError> {
		let (last_block_number, last_block_timestamp) = self.last_block_number_and_timestamp();
		let gap_block_number = last_block_number
			.checked_add(U256::one())
			.ok_or(SimulationError::OverflowError)?;
		let gap_timestamp = last_block_timestamp
			.checked_add(U256::one())
			.ok_or(SimulationError::OverflowError)?;

		let resolved_block_override =
			ResolvedSimulationBlockOverrides::new_filler_gap_block(gap_block_number, gap_timestamp);
		self.simulation_blocks
			.try_insert(
				gap_block_number,
				(resolved_block_override, Default::default(), Default::default()),
			)
			.map_err(|_| SimulationError::BlockCapacityExceeded)?;
		Ok(())
	}

	/// Returns the block number and timestamp of the last block. Defaults to the last finalized
	/// block if no other entries are found.
	fn last_block_number_and_timestamp(&self) -> (U256, U256) {
		match self.simulation_blocks.last_key_value() {
			Some((_, (last_block, ..))) => (last_block.block_number, last_block.block_timestamp),
			None => (
				self.block_number_of_last_finalized_block,
				self.block_timestamp_of_last_finalized_block,
			),
		}
	}
}

/// A mock handler used for simulations execution.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct SimulationExecutorMockHandler {
	/// The overridden block number to return in the interpreter.
	block_number: Option<U256>,

	/// The overridden block difficulty to return to the interpreter.
	block_difficulty: Option<u64>,

	/// The overridden block gas limit to return to the interpreter.
	block_gas_limit: Option<u64>,

	/// The overridden block coinbase to return to the interpreter.
	block_coinbase: Option<H160>,

	/// The overridden block base fee per gas to return in the interpreter.
	block_base_fee_per_gas: Option<U256>,
}

#[allow(dead_code)]
impl SimulationExecutorMockHandler {
	/// Creates a new [`SimulationMockHandler`] with no overrides or mocks
	pub fn new() -> Self {
		Default::default()
	}

	/// A builder pattern style method for setting the block number.
	pub fn with_block_number(self, block_number: impl Into<Option<U256>>) -> Self {
		self.with_mutator(|this| this.block_number = block_number.into())
	}

	/// A builder pattern style method for setting the block difficulty.
	pub fn with_block_difficulty(self, block_difficulty: impl Into<Option<u64>>) -> Self {
		self.with_mutator(|this| this.block_difficulty = block_difficulty.into())
	}

	/// A builder pattern style method for setting the block gas limit.
	pub fn with_block_gas_limit(self, block_gas_limit: impl Into<Option<u64>>) -> Self {
		self.with_mutator(|this| this.block_gas_limit = block_gas_limit.into())
	}

	/// A builder pattern style method for setting the block coinbase.
	pub fn with_block_coinbase(self, block_coinbase: impl Into<Option<H160>>) -> Self {
		self.with_mutator(|this| this.block_coinbase = block_coinbase.into())
	}

	/// A builder pattern style method for setting the block base fee per gas.
	pub fn with_block_base_fee_per_gas(
		self,
		block_base_fee_per_gas: impl Into<Option<U256>>,
	) -> Self {
		self.with_mutator(|this| this.block_base_fee_per_gas = block_base_fee_per_gas.into())
	}

	fn with_mutator(mut self, mutator: impl FnOnce(&mut Self)) -> Self {
		mutator(&mut self);
		self
	}
}

impl<T: crate::Config> MockHandler<T> for SimulationExecutorMockHandler {
	fn mock_block_number(&self) -> Option<&U256> {
		self.block_number.as_ref()
	}

	fn mock_block_difficulty(&self) -> Option<&u64> {
		self.block_difficulty.as_ref()
	}

	fn mock_block_gas_limit(&self) -> Option<&u64> {
		self.block_gas_limit.as_ref()
	}

	fn mock_block_coinbase(&self) -> Option<&H160> {
		self.block_coinbase.as_ref()
	}

	fn mock_block_base_fee_per_gas(&self) -> Option<&U256> {
		self.block_base_fee_per_gas.as_ref()
	}
}

/// A mock handler used for simulations when building blocks.
#[derive(Clone, Debug, Default)]
pub struct SimulationBlockBuilderMockHandler {
	mock_block_number: Option<U256>,
	mock_timestamp: Option<U256>,
	mock_gas_used: Option<U256>,
	mock_tx_hashes: Option<Vec<H256>>,
	mock_gas_info: Option<Vec<ReceiptGasInfo>>,
	mock_base_fee_per_gas: Option<U256>,
	mock_gas_limit: Option<U256>,
	mock_logs_bloom: Option<[u8; BLOOM_SIZE_BYTES]>,
	mock_difficulty: Option<U256>,
	mock_coinbase: Option<H160>,
	mock_withdrawals: Option<Vec<Withdrawal>>,
}

#[allow(dead_code)]
impl SimulationBlockBuilderMockHandler {
	/// Creates a new [`SimulationBlockBuilderMockHandler`] with no mocked values.
	pub fn new() -> Self {
		Self::default()
	}

	/// Sets a mocked value for the block_number
	pub fn with_mock_block_number(self, value: impl Into<Option<U256>>) -> Self {
		self.with_mutator(|this| &mut this.mock_block_number, value.into())
	}

	/// Sets a mocked value for the timestamp
	pub fn with_mock_timestamp(self, value: impl Into<Option<U256>>) -> Self {
		self.with_mutator(|this| &mut this.mock_timestamp, value.into())
	}

	/// Sets a mocked value for the gas_used
	pub fn with_mock_gas_used(self, value: impl Into<Option<U256>>) -> Self {
		self.with_mutator(|this| &mut this.mock_gas_used, value.into())
	}

	/// Sets a mocked value for the tx_hashes
	pub fn with_mock_tx_hashes(self, value: impl Into<Option<Vec<H256>>>) -> Self {
		self.with_mutator(|this| &mut this.mock_tx_hashes, value.into())
	}

	/// Sets a mocked value for the gas_info
	pub fn with_mock_gas_info(self, value: impl Into<Option<Vec<ReceiptGasInfo>>>) -> Self {
		self.with_mutator(|this| &mut this.mock_gas_info, value.into())
	}

	/// Sets a mocked value for the base_fee_per_gas
	pub fn with_mock_base_fee_per_gas(self, value: impl Into<Option<U256>>) -> Self {
		self.with_mutator(|this| &mut this.mock_base_fee_per_gas, value.into())
	}

	/// Sets a mocked value for the gas_limit
	pub fn with_mock_gas_limit(self, value: impl Into<Option<U256>>) -> Self {
		self.with_mutator(|this| &mut this.mock_gas_limit, value.into())
	}

	/// Sets a mocked value for the logs_bloom
	pub fn with_mock_logs_bloom(self, value: impl Into<Option<[u8; BLOOM_SIZE_BYTES]>>) -> Self {
		self.with_mutator(|this| &mut this.mock_logs_bloom, value.into())
	}

	/// Sets a mocked value for the difficulty
	pub fn with_mock_difficulty(self, value: impl Into<Option<U256>>) -> Self {
		self.with_mutator(|this| &mut this.mock_difficulty, value.into())
	}

	/// Sets a mocked value for the coinbase
	pub fn with_mock_coinbase(self, value: impl Into<Option<H160>>) -> Self {
		self.with_mutator(|this| &mut this.mock_coinbase, value.into())
	}

	/// Sets a mocked value for the withdrawals
	pub fn with_mock_withdrawals(self, value: impl Into<Option<Vec<Withdrawal>>>) -> Self {
		self.with_mutator(|this| &mut this.mock_withdrawals, value.into())
	}

	/// A mutator method which is useful in builder pattern style value setting
	fn with_mutator<V>(mut self, selector: impl FnOnce(&mut Self) -> &mut V, value: V) -> Self {
		let field = selector(&mut self);
		*field = value;
		self
	}
}

impl<T: crate::Config> EthereumBlockBuilderMockHandler<T> for SimulationBlockBuilderMockHandler {
	fn mock_block_number(&self) -> Option<&U256> {
		self.mock_block_number.as_ref()
	}

	fn mock_timestamp(&self) -> Option<&U256> {
		self.mock_timestamp.as_ref()
	}

	fn mock_gas_used(&self) -> Option<&U256> {
		self.mock_gas_used.as_ref()
	}

	fn mock_tx_hashes(&self) -> Option<&[H256]> {
		self.mock_tx_hashes.as_deref()
	}

	fn mock_gas_info(&self) -> Option<&[ReceiptGasInfo]> {
		self.mock_gas_info.as_deref()
	}

	fn mock_base_fee_per_gas(&self) -> Option<&U256> {
		self.mock_base_fee_per_gas.as_ref()
	}

	fn mock_gas_limit(&self) -> Option<&U256> {
		self.mock_gas_limit.as_ref()
	}

	fn mock_logs_bloom(&self) -> Option<&[u8; BLOOM_SIZE_BYTES]> {
		self.mock_logs_bloom.as_ref()
	}

	fn mock_difficulty(&self) -> Option<&U256> {
		self.mock_difficulty.as_ref()
	}

	fn mock_coinbase(&self) -> Option<&H160> {
		self.mock_coinbase.as_ref()
	}

	fn mock_withdrawals(&self) -> Option<&[Withdrawal]> {
		self.mock_withdrawals.as_deref()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::evm::{BlockOverrides, SimulationPayload};

	fn payload_with_number(number: u64) -> SimulationPayload {
		SimulationPayload {
			block_overrides: Some(BlockOverrides {
				number: Some(U256::from(number)),
				..Default::default()
			}),
			state_overrides: None,
			calls: vec![],
		}
	}

	fn payload_with_number_and_time(number: u64, time: u64) -> SimulationPayload {
		SimulationPayload {
			block_overrides: Some(BlockOverrides {
				number: Some(U256::from(number)),
				time: Some(U256::from(time)),
				..Default::default()
			}),
			state_overrides: None,
			calls: vec![],
		}
	}

	fn payload_with_no_overrides() -> SimulationPayload {
		SimulationPayload { block_overrides: None, state_overrides: None, calls: vec![] }
	}

	fn payload_with_time(time: u64) -> SimulationPayload {
		SimulationPayload {
			block_overrides: Some(BlockOverrides {
				time: Some(U256::from(time)),
				..Default::default()
			}),
			state_overrides: None,
			calls: vec![],
		}
	}

	fn collect_numbers_and_timestamps(blocks: SimulationBlocks<0x100>) -> Vec<(U256, U256)> {
		blocks
			.into_inner()
			.map(|(overrides, _, _)| (overrides.block_number, overrides.block_timestamp))
			.collect()
	}

	/// Geth `sanitizeChain`: if no block number override is provided, the block
	/// number defaults to `prevNumber + 1`.
	#[test]
	fn block_number_defaults_to_parent_plus_one() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks.insert_simulation_payload(payload_with_no_overrides()).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result.len(), 1);
		assert_eq!(result[0].0, U256::from(11));
	}

	/// Geth `sanitizeChain`: consecutive payloads with no block number override
	/// produce sequential block numbers.
	#[test]
	fn consecutive_defaults_produce_sequential_block_numbers() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks.insert_simulation_payload(payload_with_no_overrides()).unwrap();
		blocks.insert_simulation_payload(payload_with_no_overrides()).unwrap();
		blocks.insert_simulation_payload(payload_with_no_overrides()).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result.len(), 3);
		assert_eq!(result[0].0, U256::from(11));
		assert_eq!(result[1].0, U256::from(12));
		assert_eq!(result[2].0, U256::from(13));
	}

	/// Geth `sanitizeChain`: `diff.Cmp(common.Big0) <= 0` rejects block numbers
	/// equal to the parent (error code -38020).
	#[test]
	fn block_number_equal_to_parent_is_rejected() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number(10));

		// Assert
		assert_eq!(result, Err(SimulationError::BlockNumberOverrideMustBeMonotonicallyIncreasing));
	}

	/// Geth `sanitizeChain`: `diff.Cmp(common.Big0) <= 0` rejects block numbers
	/// less than the parent (error code -38020).
	#[test]
	fn block_number_less_than_parent_is_rejected() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number(5));

		// Assert
		assert_eq!(result, Err(SimulationError::BlockNumberOverrideMustBeMonotonicallyIncreasing));
	}

	/// Geth `sanitizeChain`: block numbers must be strictly increasing relative to
	/// the previous block in the sequence, not just the base.
	#[test]
	fn block_number_equal_to_previous_inserted_block_is_rejected() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));
		blocks.insert_simulation_payload(payload_with_number(12)).unwrap();

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number(12));

		// Assert
		assert_eq!(result, Err(SimulationError::BlockNumberOverrideMustBeMonotonicallyIncreasing));
	}

	/// Geth `sanitizeChain`: block numbers must be strictly increasing relative to
	/// the previous block in the sequence.
	#[test]
	fn block_number_less_than_previous_inserted_block_is_rejected() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));
		blocks.insert_simulation_payload(payload_with_number(15)).unwrap();

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number(13));

		// Assert
		assert_eq!(result, Err(SimulationError::BlockNumberOverrideMustBeMonotonicallyIncreasing));
	}

	/// Geth `sanitizeChain`: block number 0 override when parent is also 0 must be
	/// rejected since block numbers must be strictly greater.
	#[test]
	fn block_number_zero_when_parent_is_zero_is_rejected() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(0), U256::from(0));

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number(0));

		// Assert
		assert_eq!(result, Err(SimulationError::BlockNumberOverrideMustBeMonotonicallyIncreasing));
	}

	/// Geth `sanitizeChain`: gaps in block numbers are filled with empty blocks.
	/// If parent is N and override is N+3, blocks N+1 and N+2 are inserted.
	#[test]
	fn gap_blocks_are_filled_between_parent_and_override() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks.insert_simulation_payload(payload_with_number(13)).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result.len(), 3);
		assert_eq!(result[0].0, U256::from(11));
		assert_eq!(result[1].0, U256::from(12));
		assert_eq!(result[2].0, U256::from(13));
	}

	/// Geth `sanitizeChain`: gap filling also applies between two user-provided
	/// payloads, not just between the base and the first payload.
	#[test]
	fn gap_blocks_are_filled_between_consecutive_overrides() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks.insert_simulation_payload(payload_with_number(11)).unwrap();
		blocks.insert_simulation_payload(payload_with_number(14)).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result.len(), 4);
		assert_eq!(result[0].0, U256::from(11));
		assert_eq!(result[1].0, U256::from(12));
		assert_eq!(result[2].0, U256::from(13));
		assert_eq!(result[3].0, U256::from(14));
	}

	/// Geth `sanitizeChain`: no gap blocks are inserted when the override is
	/// exactly `prevNumber + 1` (diff == 1, loop runs 0 times).
	#[test]
	fn no_gap_blocks_when_override_is_parent_plus_one() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks.insert_simulation_payload(payload_with_number(11)).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result.len(), 1);
		assert_eq!(result[0].0, U256::from(11));
	}

	/// Geth `sanitizeChain`: each gap block advances the timestamp by
	/// `timestampIncrement` (12s in geth, 1s in this implementation).
	#[test]
	fn gap_blocks_have_incrementing_timestamps() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks
			.insert_simulation_payload(payload_with_number_and_time(14, 1010))
			.unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result.len(), 4);
		assert_eq!(result[0], (U256::from(11), U256::from(1001)));
		assert_eq!(result[1], (U256::from(12), U256::from(1002)));
		assert_eq!(result[2], (U256::from(13), U256::from(1003)));
		assert_eq!(result[3], (U256::from(14), U256::from(1010)));
	}

	/// Geth `sanitizeChain`: gap blocks have no transactions or state overrides.
	#[test]
	fn gap_blocks_have_no_calls_or_state_overrides() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks.insert_simulation_payload(payload_with_number(13)).unwrap();

		// Assert
		let all: Vec<_> = blocks.into_inner().collect();
		assert_eq!(all.len(), 3);
		assert!(all[0].1.0.is_empty());
		assert!(all[0].2.is_empty());
		assert!(all[1].1.0.is_empty());
		assert!(all[1].2.is_empty());
	}

	/// Geth `sanitizeChain`: gap blocks have no optional block overrides set.
	#[test]
	fn gap_blocks_have_no_optional_overrides() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks.insert_simulation_payload(payload_with_number(13)).unwrap();

		// Assert
		let all: Vec<_> = blocks.into_inner().collect();
		for gap_block in &all[..2] {
			assert_eq!(gap_block.0.prev_randao, None);
			assert_eq!(gap_block.0.gas_limit, None);
			assert_eq!(gap_block.0.fee_recipient, None);
			assert_eq!(gap_block.0.withdrawals, None);
			assert_eq!(gap_block.0.base_fee_per_gas, None);
			assert_eq!(gap_block.0.blob_base_fee, None);
		}
	}

	/// Geth `sanitizeChain`: if no timestamp override is provided, the timestamp
	/// defaults to `prevTimestamp + timestampIncrement`.
	#[test]
	fn timestamp_defaults_to_parent_plus_one() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks.insert_simulation_payload(payload_with_no_overrides()).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result[0].1, U256::from(1001));
	}

	/// Geth `sanitizeChain`: consecutive default timestamps each advance by
	/// `timestampIncrement`.
	#[test]
	fn consecutive_default_timestamps_increment_by_one() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks.insert_simulation_payload(payload_with_no_overrides()).unwrap();
		blocks.insert_simulation_payload(payload_with_no_overrides()).unwrap();
		blocks.insert_simulation_payload(payload_with_no_overrides()).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result[0].1, U256::from(1001));
		assert_eq!(result[1].1, U256::from(1002));
		assert_eq!(result[2].1, U256::from(1003));
	}

	/// Spec `execute.yaml`: timestamps may "remain constant relative to the
	/// previous block". A timestamp equal to the base block's timestamp is
	/// accepted when there are no gap blocks.
	#[test]
	fn timestamp_equal_to_base_block_is_accepted() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number_and_time(11, 1000));

		// Assert
		assert!(result.is_ok());
	}

	/// Spec `execute.yaml`: timestamps may "remain constant relative to the
	/// previous block". A timestamp equal to the last gap block's timestamp is
	/// accepted.
	#[test]
	fn timestamp_equal_to_gap_block_is_accepted() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number_and_time(12, 1001));

		// Assert
		assert!(result.is_ok());
	}

	/// Spec `execute.yaml`: multiple payloads with the same explicit timestamp are
	/// accepted since equal timestamps are allowed.
	#[test]
	fn multiple_payloads_with_same_timestamp_are_accepted() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));
		blocks
			.insert_simulation_payload(payload_with_number_and_time(11, 2000))
			.unwrap();

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number_and_time(12, 2000));

		// Assert
		assert!(result.is_ok());
	}

	/// Geth `sanitizeChain`: `t <= prevTimestamp` rejects timestamps strictly less
	/// than the parent's timestamp (error code -38021).
	#[test]
	fn timestamp_less_than_base_is_rejected() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number_and_time(11, 999));

		// Assert
		assert_eq!(
			result,
			Err(SimulationError::BlockTimestampOverrideMustBeEqualOrMonotonicallyIncreasing)
		);
	}

	/// Geth `sanitizeChain`: a user-provided timestamp that is less than the last
	/// gap block's timestamp is rejected, even if it is greater than the base
	/// block's timestamp.
	#[test]
	fn timestamp_less_than_gap_block_timestamp_is_rejected() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number_and_time(13, 1001));

		// Assert
		assert_eq!(
			result,
			Err(SimulationError::BlockTimestampOverrideMustBeEqualOrMonotonicallyIncreasing)
		);
	}

	/// Geth `sanitizeChain`: timestamps must be non-decreasing across payloads.
	#[test]
	fn timestamp_decreasing_across_payloads_is_rejected() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));
		blocks
			.insert_simulation_payload(payload_with_number_and_time(11, 2000))
			.unwrap();

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number_and_time(12, 1500));

		// Assert
		assert_eq!(
			result,
			Err(SimulationError::BlockTimestampOverrideMustBeEqualOrMonotonicallyIncreasing)
		);
	}

	/// Geth `sanitizeChain`: strictly increasing timestamps across payloads are
	/// accepted.
	#[test]
	fn timestamp_increasing_across_payloads_is_accepted() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));
		blocks
			.insert_simulation_payload(payload_with_number_and_time(11, 2000))
			.unwrap();

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number_and_time(12, 2001));

		// Assert
		assert!(result.is_ok());
	}

	/// Geth `sanitizeChain`: when no timestamp is provided for a block with gap
	/// blocks, the default timestamp builds on the last gap block's timestamp.
	#[test]
	fn default_timestamp_builds_on_gap_blocks() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks.insert_simulation_payload(payload_with_number(13)).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result[2], (U256::from(13), U256::from(1003)));
	}

	/// Geth `sanitizeChain`: when the second payload has no timestamp and gap
	/// blocks exist between the two payloads, the default timestamp builds on
	/// the last gap block between them.
	#[test]
	fn default_timestamp_builds_on_gap_blocks_between_payloads() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));
		blocks
			.insert_simulation_payload(payload_with_number_and_time(11, 5000))
			.unwrap();

		// Act
		blocks.insert_simulation_payload(payload_with_number(14)).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result[1], (U256::from(12), U256::from(5001)));
		assert_eq!(result[2], (U256::from(13), U256::from(5002)));
		assert_eq!(result[3], (U256::from(14), U256::from(5003)));
	}

	/// Geth `simulate.go`: `maxSimulateBlocks = 256`. Exceeding the block limit
	/// (including gap blocks) must be rejected.
	#[test]
	fn exceeding_block_limit_is_rejected() {
		// Arrange
		let mut blocks = SimulationBlocks::<5>::new(U256::from(10), U256::from(1000));
		for i in 11..=15 {
			blocks.insert_simulation_payload(payload_with_number(i)).unwrap();
		}

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number(16));

		// Assert
		assert_eq!(result, Err(SimulationError::BlockCapacityExceeded));
	}

	/// Geth `simulate.go`: gap blocks count toward `maxSimulateBlocks`.
	#[test]
	fn gap_blocks_count_toward_block_limit() {
		// Arrange
		let mut blocks = SimulationBlocks::<3>::new(U256::from(10), U256::from(1000));

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number(14));

		// Assert
		assert_eq!(result, Err(SimulationError::BlockCapacityExceeded));
	}

	/// Geth `simulate.go`: exactly `maxSimulateBlocks` blocks (including gaps)
	/// should succeed.
	#[test]
	fn exactly_filling_capacity_succeeds() {
		// Arrange
		let mut blocks = SimulationBlocks::<3>::new(U256::from(10), U256::from(1000));

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number(13));

		// Assert
		assert!(result.is_ok());
		let all: Vec<_> = blocks.into_inner().collect();
		assert_eq!(all.len(), 3);
	}

	/// Geth `simulate.go`: a second payload that would cause gap blocks to exceed
	/// the remaining capacity must be rejected.
	#[test]
	fn gap_blocks_from_second_payload_exceed_remaining_capacity() {
		// Arrange
		let mut blocks = SimulationBlocks::<4>::new(U256::from(10), U256::from(1000));
		blocks.insert_simulation_payload(payload_with_number(11)).unwrap();
		blocks.insert_simulation_payload(payload_with_number(12)).unwrap();

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number(15));

		// Assert
		assert_eq!(result, Err(SimulationError::BlockCapacityExceeded));
	}

	/// Geth `sanitizeChain`: non-timing block overrides are preserved in the
	/// resolved output.
	#[test]
	fn block_overrides_are_preserved_in_output() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));
		let fee_recipient = H160::from_low_u64_be(0xCAFE);

		// Act
		blocks
			.insert_simulation_payload(SimulationPayload {
				block_overrides: Some(BlockOverrides {
					number: Some(U256::from(11)),
					time: Some(U256::from(2000)),
					fee_recipient: Some(fee_recipient),
					gas_limit: Some(U256::from(30_000_000)),
					prev_randao: Some(U256::from(42)),
					base_fee_per_gas: Some(U256::from(1_000_000_000)),
					blob_base_fee: Some(U256::from(100)),
					..Default::default()
				}),
				state_overrides: None,
				calls: vec![],
			})
			.unwrap();

		// Assert
		let all: Vec<_> = blocks.into_inner().collect();
		assert_eq!(all.len(), 1);
		let overrides = &all[0].0;
		assert_eq!(overrides.fee_recipient, Some(fee_recipient));
		assert_eq!(overrides.gas_limit, Some(U256::from(30_000_000)));
		assert_eq!(overrides.prev_randao, Some(U256::from(42)));
		assert_eq!(overrides.base_fee_per_gas, Some(U256::from(1_000_000_000)));
		assert_eq!(overrides.blob_base_fee, Some(U256::from(100)));
	}

	/// Geth `sanitizeChain`: `block_overrides: None` and
	/// `block_overrides: Some(Default)` produce identical results since missing
	/// overrides are replaced with defaults.
	#[test]
	fn none_overrides_and_default_overrides_are_equivalent() {
		// Arrange
		let mut blocks_none = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));
		let mut blocks_default = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks_none.insert_simulation_payload(payload_with_no_overrides()).unwrap();
		blocks_default
			.insert_simulation_payload(SimulationPayload {
				block_overrides: Some(BlockOverrides::default()),
				state_overrides: None,
				calls: vec![],
			})
			.unwrap();

		// Assert
		let result_none = collect_numbers_and_timestamps(blocks_none);
		let result_default = collect_numbers_and_timestamps(blocks_default);
		assert_eq!(result_none, result_default);
	}

	/// Geth `sanitizeChain`: a single payload at parent + 1 with an explicit
	/// timestamp works.
	#[test]
	fn single_block_with_explicit_timestamp() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(0), U256::from(0));

		// Act
		blocks.insert_simulation_payload(payload_with_number_and_time(1, 100)).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result.len(), 1);
		assert_eq!(result[0], (U256::from(1), U256::from(100)));
	}

	/// No payloads produces an empty sequence.
	#[test]
	fn empty_simulation_produces_no_blocks() {
		// Arrange
		let blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		let result: Vec<_> = blocks.into_inner().collect();

		// Assert
		assert!(result.is_empty());
	}

	/// Geth `sanitizeChain`: simulation starting from genesis (block 0) with an
	/// override at block 1 works.
	#[test]
	fn genesis_as_parent_with_block_one_override() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(0), U256::from(0));

		// Act
		blocks.insert_simulation_payload(payload_with_number(1)).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result.len(), 1);
		assert_eq!(result[0].0, U256::from(1));
	}

	/// Geth `sanitizeChain`: genesis parent with gap filling to block 3 produces
	/// gap blocks at 1, 2 and the actual block at 3.
	#[test]
	fn genesis_as_parent_with_gap_filling() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(0), U256::from(0));

		// Act
		blocks.insert_simulation_payload(payload_with_number(3)).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result.len(), 3);
		assert_eq!(result[0].0, U256::from(1));
		assert_eq!(result[1].0, U256::from(2));
		assert_eq!(result[2].0, U256::from(3));
	}

	/// Geth `sanitizeChain`: a timestamp-only override defaults the block number
	/// to parent + 1 while using the provided timestamp.
	#[test]
	fn only_timestamp_override_defaults_block_number() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks.insert_simulation_payload(payload_with_time(5000)).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		assert_eq!(result.len(), 1);
		assert_eq!(result[0], (U256::from(11), U256::from(5000)));
	}

	/// Geth `sanitizeChain`: blocks are returned in ascending block number order
	/// (BTreeMap ordering).
	#[test]
	fn blocks_are_returned_in_ascending_order() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(10), U256::from(1000));

		// Act
		blocks.insert_simulation_payload(payload_with_number(11)).unwrap();
		blocks.insert_simulation_payload(payload_with_number(15)).unwrap();

		// Assert
		let result = collect_numbers_and_timestamps(blocks);
		let numbers: Vec<U256> = result.iter().map(|(n, _)| *n).collect();
		assert_eq!(
			numbers,
			vec![U256::from(11), U256::from(12), U256::from(13), U256::from(14), U256::from(15),]
		);
	}

	/// Geth `sanitizeChain`: a large gap between payloads with an explicit
	/// timestamp on the second payload validates the timestamp against the last
	/// gap block, not the first payload.
	#[test]
	fn large_gap_with_explicit_timestamp_validates_against_last_gap() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(0), U256::from(0));
		blocks.insert_simulation_payload(payload_with_number_and_time(1, 100)).unwrap();

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number_and_time(6, 102));

		// Assert
		assert_eq!(
			result,
			Err(SimulationError::BlockTimestampOverrideMustBeEqualOrMonotonicallyIncreasing)
		);
	}

	/// Geth `sanitizeChain`: a large gap between payloads with a valid timestamp
	/// that exceeds the last gap block's timestamp is accepted.
	#[test]
	fn large_gap_with_valid_timestamp_is_accepted() {
		// Arrange
		let mut blocks = SimulationBlocks::<0x100>::new(U256::from(0), U256::from(0));
		blocks.insert_simulation_payload(payload_with_number_and_time(1, 100)).unwrap();

		// Act
		let result = blocks.insert_simulation_payload(payload_with_number_and_time(6, 200));

		// Assert
		assert!(result.is_ok());
		let all = collect_numbers_and_timestamps(blocks);
		assert_eq!(all.len(), 6);
		assert_eq!(all[5], (U256::from(6), U256::from(200)));
	}
}
