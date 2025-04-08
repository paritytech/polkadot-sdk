pub struct FeeHistoryCacheItem {
	pub base_fee: u64,
	pub gas_used_ratio: f64,
	pub rewards: Vec<u64>,
}

pub struct FeeHistoryProvider {
	pub client: Arc<dyn Client>,
	pub fee_history_cache: RwLock<HashMap<SubstrateBlockNumbe, FeeHistoryCacheItem>>,
}
