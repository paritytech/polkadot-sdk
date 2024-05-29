// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Substrate transaction pool implementation.

#![recursion_limit = "256"]
#![warn(missing_docs)]
#![warn(unused_extern_crates)]

mod builder;
mod common;
mod fork_aware_txpool;
mod graph;
mod single_state_txpool;

use common::{api, enactment_state};
use std::{future::Future, pin::Pin, sync::Arc};

pub(crate) const LOG_TARGET: &str = "txpool";

pub use graph::{base_pool::Limit as PoolLimit, ChainApi, Options};
pub use single_state_txpool::{notification_future, RevalidationType};

//benches:
pub use graph::Pool;

//testing:
pub use api::FullChainApi;
pub use fork_aware_txpool::{ForkAwareTxPool, ImportNotificationTask};
pub use single_state_txpool::BasicPool;

use single_state_txpool::prune_known_txs_for_block;

// shared types
type BoxedReadyIterator<Hash, Data> = Box<
	dyn sc_transaction_pool_api::ReadyTransactions<
			Item = Arc<graph::base_pool::Transaction<Hash, Data>>,
		> + Send,
>;

type ReadyIteratorFor<PoolApi> =
	BoxedReadyIterator<graph::ExtrinsicHash<PoolApi>, graph::ExtrinsicFor<PoolApi>>;

type PolledIterator<PoolApi> = Pin<Box<dyn Future<Output = ReadyIteratorFor<PoolApi>> + Send>>;

pub use builder::{Builder, TransactionPoolImpl, TransactionPoolOptions, TransactionPoolType};
