// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Helpers to deal with configuring the message queue in the runtime.

use cumulus_primitives_core::{AggregateMessageOrigin, MessageOrigin, ParaId};
use frame_support::traits::{QueueFootprint, QueuePausedQuery};
use pallet_message_queue::OnQueueChanged;
use sp_std::marker::PhantomData;

/// Narrow the scope of the `Inner` query from `AggregateMessageOrigin` to `ParaId`.
///
/// All non-`Sibling` variants will be ignored.
pub struct NarrowOriginToXcmSibling<Inner>(PhantomData<Inner>);
impl<Inner: QueuePausedQuery<ParaId>> QueuePausedQuery<AggregateMessageOrigin>
	for NarrowOriginToXcmSibling<Inner>
{
	fn is_paused(origin: &AggregateMessageOrigin) -> bool {
		use AggregateMessageOrigin::*;
		use MessageOrigin::*;
		match origin {
			Xcm(Sibling(id)) => Inner::is_paused(id),
			_ => false,
		}
	}
}

impl<Inner: OnQueueChanged<ParaId>> OnQueueChanged<AggregateMessageOrigin>
	for NarrowOriginToXcmSibling<Inner>
{
	fn on_queue_changed(origin: AggregateMessageOrigin, fp: QueueFootprint) {
		use AggregateMessageOrigin::*;
		use MessageOrigin::*;
		if let Xcm(Sibling(id)) = origin {
			Inner::on_queue_changed(id, fp)
		}
	}
}

/// Convert a sibling `ParaId` to an `AggregateMessageOrigin`.
pub struct ParaIdToXcmSibling;
impl sp_runtime::traits::Convert<ParaId, AggregateMessageOrigin> for ParaIdToXcmSibling {
	fn convert(para_id: ParaId) -> AggregateMessageOrigin {
		use AggregateMessageOrigin::*;
		use MessageOrigin::*;
		Xcm(Sibling(para_id))
	}
}

