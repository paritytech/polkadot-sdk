// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! Implementation of the [`Pallet::reprioritize`] dispatchable.

use crate::{
	pallet::*, weights::WeightInfo, Outcome, Position, PriorityProvider, SortedListInterface,
};
use frame::prelude::*;

impl<T: Config> Pallet<T> {
	/// Refresh `(list_id, item)`'s stored priority from [`crate::PriorityProvider`]
	/// and reposition it via [`SortedListInterface::re_insert`]. Returns the
	/// actual dispatch weight to report for refunding.
	pub(crate) fn do_reprioritize(
		list_id: T::ListId,
		item: T::ItemId,
		hint: Position<T::ItemId>,
	) -> Result<Weight, Error<T>> {
		let Some(real_priority) = T::PriorityProvider::priority(&list_id, &item) else {
			Self::remove(&list_id, &item)?;
			return Ok(T::WeightInfo::reprioritize_priority_removed());
		};

		let outcome = Self::re_insert(list_id.clone(), item.clone(), real_priority, hint)?;

		Ok(match outcome {
			Outcome::NoOp => T::WeightInfo::reprioritize_no_op(),
			Outcome::InPlace => {
				Self::deposit_event(Event::Reprioritized {
					list_id,
					item,
					new_priority: real_priority,
				});
				T::WeightInfo::reprioritize_in_place()
			},
			Outcome::Relocated { steps } => {
				Self::deposit_event(Event::Reprioritized {
					list_id,
					item,
					new_priority: real_priority,
				});
				T::WeightInfo::reprioritize_relocate(steps)
			},
		})
	}
}
