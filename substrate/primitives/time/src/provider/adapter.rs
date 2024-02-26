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

use crate::{Duration, Instant, InstantProvider};
use sp_runtime::traits::{BlockNumberProvider, Get};

/// Adapter for converting a `Get<I>` into an `InstantProvider<I>`.
pub struct StaticInstantProvider<I, N>(core::marker::PhantomData<(I, N)>);

impl<I: Instant, N: Get<I>> InstantProvider<I> for StaticInstantProvider<I, N> {
	fn now() -> I {
		N::get()
	}
}

/// Uses a `BlockNumberProvider` to calculate the current time with respect to offset and slope.
///
/// This can be used by solo/relay chains to convert the `System` pallet into an `InstantProvider`
/// or by a parachain by using the `cumulus_pallet_parachain_system` pallet.
pub struct BlockNumberInstantProvider<I, B, O, S>(core::marker::PhantomData<(I, B, O, S)>);

impl<I, B, O, S> InstantProvider<I> for BlockNumberInstantProvider<I, B, O, S>
where
	I: Instant,
	B: BlockNumberProvider,
	<B as BlockNumberProvider>::BlockNumber: Into<u128>,
	O: Get<I>,
	S: Get<I::Duration>,
{
	fn now() -> I {
		let block: u128 = B::current_block_number().into();
		let slope = S::get().saturating_mul_int(block);
		let offset = O::get();

		offset.saturating_add(&slope)
	}
}
