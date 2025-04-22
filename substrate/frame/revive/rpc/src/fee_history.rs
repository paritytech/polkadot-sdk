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

pub struct FeeHistoryCacheItem {
	pub base_fee: u64,
	pub gas_used_ratio: f64,
	pub rewards: Vec<u64>,
}

pub struct FeeHistoryProvider {
	pub client: Arc<dyn Client>,
	pub fee_history_cache: RwLock<HashMap<SubstrateBlockNumbe, FeeHistoryCacheItem>>,
}
