// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Helpers to deal with configuring the message queue in the runtime.

use core::marker::PhantomData;
use cumulus_primitives_core::{AggregateMessageOrigin, ParaId};
use frame_support::traits::{QueueFootprint, QueuePausedQuery};
use pallet_message_queue::OnQueueChanged;

/// Narrow the scope of the `Inner` query from `AggregateMessageOrigin` to `ParaId`.
///
/// All non-`Sibling` variants will be ignored.
pub struct NarrowOriginToSibling<Inner>(PhantomData<Inner>);
impl<Inner: QueuePausedQuery<ParaId>> QueuePausedQuery<AggregateMessageOrigin>
	for NarrowOriginToSibling<Inner>
{
	fn is_paused(origin: &AggregateMessageOrigin) -> bool {
		match origin {
			AggregateMessageOrigin::Sibling(id) => Inner::is_paused(id),
			_ => false,
		}
	}
}

impl<Inner: OnQueueChanged<ParaId>> OnQueueChanged<AggregateMessageOrigin>
	for NarrowOriginToSibling<Inner>
{
	fn on_queue_changed(origin: AggregateMessageOrigin, fp: QueueFootprint) {
		if let AggregateMessageOrigin::Sibling(id) = origin {
			Inner::on_queue_changed(id, fp)
		}
	}
}

/// Convert a sibling `ParaId` to an `AggregateMessageOrigin`.
pub struct ParaIdToSibling;
impl sp_runtime::traits::Convert<ParaId, AggregateMessageOrigin> for ParaIdToSibling {
	fn convert(para_id: ParaId) -> AggregateMessageOrigin {
		AggregateMessageOrigin::Sibling(para_id)
	}
}
