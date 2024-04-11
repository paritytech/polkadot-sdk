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
pub struct AssetPairFactory<Target, SelfParaId, PalletId, L = Location>(
	PhantomData<(Target, SelfParaId, PalletId, L)>,
);
impl<Target: Get<L>, SelfParaId: Get<ParaId>, PalletId: Get<u32>, L: TryFrom<Location>>
	pallet_asset_conversion::BenchmarkHelper<L> for AssetPairFactory<Target, SelfParaId, PalletId, L>
{
	fn create_pair(seed1: u32, seed2: u32) -> (L, L) {
		let with_id = Location::new(
			1,
			[
				Parachain(SelfParaId::get().into()),
				PalletInstance(PalletId::get() as u8),
				GeneralIndex(seed2.into()),
			],
		);
		if seed1 % 2 == 0 {
			(with_id.try_into().map_err(|_| "Something went wrong").unwrap(), Target::get())
		} else {
			(Target::get(), with_id.try_into().map_err(|_| "Something went wrong").unwrap())
		}
	}
}
