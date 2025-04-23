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

//! Various predefined implementations for the pallet.

use crate::{Config, Pallet};
use bp_parachains::OnNewHead;
use bp_polkadot_core::parachains::{ParaHash, ParaHead, ParaId};
use bp_runtime::Parachain;
use frame_support::{traits::Get, weights::Weight};

/// An adapter `OnNewHead` implementation that listens for parachain head updates and schedules them
/// for syncing.
pub struct SyncParaHeadersFor<T, I, C>(core::marker::PhantomData<(T, I, C)>);
impl<T: Config<I, Key = ParaId, Value = ParaHead>, I: 'static, C: Parachain<Hash = ParaHash>>
	OnNewHead for SyncParaHeadersFor<T, I, C>
{
	fn on_new_head(id: ParaId, head: &ParaHead) -> Weight {
		// Filter by para ID.
		if C::PARACHAIN_ID != id.0 {
			return Weight::zero();
		}

		// Schedule for syncing.
		Pallet::<T, I>::schedule_for_sync(id, head.clone());

		// Schedule does one read and one write.
		T::DbWeight::get().reads_writes(1, 1)
	}
}
