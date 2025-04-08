#![allow(unused, dead_code)]

use crate::{
	client::{SubstrateBlock, SubstrateBlockNumber},
	BlockInfo, ClientError,
};
use pallet_revive::evm::{Block, FeeHistoryResult, ReceiptInfo};
use sp_core::U256;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::RwLock;

const MAX_BLOCK_COUNT: u32 = 1024;

#[derive(Default, Clone)]
pub struct FeeHistoryCacheItem {
	pub base_fee: U256,
	pub gas_used_ratio: f64,
	pub rewards: Vec<U256>,
}

#[derive(Default, Clone)]
pub struct FeeHistoryProvider {
	pub fee_history_cache: Arc<RwLock<BTreeMap<SubstrateBlockNumber, FeeHistoryCacheItem>>>,
}

impl FeeHistoryProvider {
	pub async fn update_fee_history(&self, block: &Block, receipts: &[ReceiptInfo]) {
		let block_number: SubstrateBlockNumber =
			block.number.try_into().expect("Block number is always valid");
		let base_fee = block.base_fee_per_gas.unwrap_or_default();
		let gas_used_ratio = (block.gas_used.as_u128() as f64) / (block.gas_limit.as_u128() as f64);

		let reward_percentiles: Vec<f64> = {
			let mut percentile: f64 = 0.0;
			(0..201)
				.map(|_| {
					let val = percentile;
					percentile += 0.5;
					val
				})
				.collect()
		};

		receipts.iter().map(|receipt| {
			let gas_used = receipt.gas_used;
			let reward = receipt.gas_price.saturating_sub(base_fee);

			return (gas_used, reward);
		});

		let mut result = FeeHistoryCacheItem::default();
		let gas_used = block.gas_used;

		self.fee_history_cache.write().await.insert(block_number, result);
	}

	pub async fn fee_history(
		&self,
		block_count: u32,
		highest: SubstrateBlockNumber,
		reward_percentiles: Option<Vec<f64>>,
	) -> Result<FeeHistoryResult, ClientError> {
		let block_count = block_count.min(MAX_BLOCK_COUNT);

		let cache = self.fee_history_cache.read().await;
		let Some(lowest_in_cache) = cache.first_key_value().map(|(k, _)| *k) else {
			return Ok(FeeHistoryResult {
				oldest_block: U256::zero(),
				base_fee_per_gas: vec![],
				gas_used_ratio: vec![],
				reward: vec![],
			})
		};

		let lowest = highest.saturating_sub(block_count.saturating_sub(1)).max(lowest_in_cache);

		let mut response = FeeHistoryResult {
			oldest_block: U256::from(lowest),
			base_fee_per_gas: Vec::new(),
			gas_used_ratio: Vec::new(),
			reward: Default::default(),
		};

		let mut rewards = &mut response.reward;
		// Iterate over the requested block range.
		for n in lowest..highest + 1 {
			if let Some(block) = cache.get(&n) {
				response.base_fee_per_gas.push(U256::from(block.base_fee));
				response.gas_used_ratio.push(block.gas_used_ratio);
				// If the request includes reward percentiles, get them from the cache.
				if let Some(ref requested_percentiles) = reward_percentiles {
					let mut block_rewards = Vec::new();
					// Resolution is half a point. I.e. 1.0,1.5
					let resolution_per_percentile: f64 = 2.0;
					// Get cached reward for each provided percentile.
					for p in requested_percentiles {
						// Find the cache index from the user percentile.
						let p = p.clamp(0.0, 100.0);
						let index = ((p.round() / 2f64) * 2f64) * resolution_per_percentile;
						// Get and push the reward.
						let reward = if let Some(r) = block.rewards.get(index as usize) {
							U256::from(*r)
						} else {
							U256::zero()
						};
						block_rewards.push(reward);
					}
					// Push block rewards.
					if !block_rewards.is_empty() {
						// Push block rewards.
						rewards.push(block_rewards);
					}
				}
			}
		}

		Ok(response)
	}
}
