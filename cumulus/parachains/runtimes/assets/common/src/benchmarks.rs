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

use cumulus_primitives_core::ParaId;
use sp_runtime::traits::Get;
use sp_std::marker::PhantomData;
use xcm::latest::prelude::*;

/// Creates asset pairs for liquidity pools with `Target` always being the first asset.
pub struct AssetPairFactory<Target, SelfParaId, PalletId>(
	PhantomData<(Target, SelfParaId, PalletId)>,
);
impl<Target: Get<MultiLocation>, SelfParaId: Get<ParaId>, PalletId: Get<u32>>
	pallet_asset_conversion::BenchmarkHelper<MultiLocation>
	for AssetPairFactory<Target, SelfParaId, PalletId>
{
	fn create_pair(seed1: u32, seed2: u32) -> (MultiLocation, MultiLocation) {
		let with_id = MultiLocation::new(
			1,
			X3(
				Parachain(SelfParaId::get().into()),
				PalletInstance(PalletId::get() as u8),
				GeneralIndex(seed2.into()),
			),
		);
		if seed1 % 2 == 0 {
			(with_id, Target::get())
		} else {
			(Target::get(), with_id)
		}
	}
}
