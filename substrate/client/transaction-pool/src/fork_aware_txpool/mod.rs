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

//! Substrate fork aware transaction pool implementation.
//!
//! # Top level overview.
//! ## Structures.
//! ### View.
//! #### Purpose.
//! The main responsibility of the [`View`] is to provide the valid set of ready transactions at
//! the given block. ForkAwareTxPool keeps the number of recent views for all the blocks notified
//! since recently finalized block.
//!
//! The views associated with blocks at the tips of the forks are actively updated with all newly
//! incoming transactions, while intermediate views are not updated (they still provide transactions
//! ready to be included at that block). Building upon the older blocks happens relatively rare so
//! this does not affect blocks filling.
//!
//! The view is wrapper around [`Pool`] and exposes its functionality, including the ability
//! of [tracking][`crate::graph::watcher::Watcher`] the progress of every transaction.
//!
//! #### Views: active, inactive.
//! All the views are stored in [`ViewStore`] structure. Views at the tips of the forks are referred
//! as [`active_views`], while intermediate views as [`inactive_views`].
//!
//!
//! #### The lifecycle of the [`View`].
//! Views are created when the new [`ChainEvent`] are notified to the pool. The view that is
//! [closest][find_best_view] to the newly notified block is choosen to clone from. The created
//! view is placed in [`active_views`]. When the view is no longer at the tip, it is moved to the
//! [`inactive_views`]. When the block number of the view is lower then the finalized block, the
//! view is permanently removed.
//!
//!
//! *Example*:
//! The following chain
//! ```text
//!    C2 - C3 - C4
//!   /
//! B1
//!   \
//!    B2 - B3 - B4
//! ```
//! and the following set of events:
//! ```text
//! New best block: B1, C3, C4, B4
//! ```
//! will result in the following set of views:
//! ```text
//!   active: C4, B4
//! inactive: B1, C3
//! ```
//! Please note that views are only created for the notified blocks.
//!
//!
//! ### View store.
//! The helper structure that provides means to perform some actions like [submit] or
//! [submit_and_watch] on every view. It keeps track of both active and inactive views.
//!
//! It also keeps tracks of the `most_recent_view` which is used to implement some methods of
//! [TransactionPool API], see `api_considerations_section`.
//!
//!
//! ### Intermediate buffer: mempool.
//! The main purpose of an internal mempool [`TxMemPool`] is to prevent a transaction from being
//! lost, e.g. due to race condition when the new transaction submission occures just before the new
//! view is created. This could also happen when a transaction is invalid on one fork and could be
//! valid on another which is not yet fully processed by the maintain procedure. Additionally, it
//! allows the pool to accept transactions when no blocks have been reported yet.
//!
//! After the view is created, all transactions from mempool are submitted to and validated at this
//! view.
//!
//! The mempool removes its transactions when they get finalized. Mempool's transactions are
//! also periodically verified at every finalized block and removed from the mempool if no longer
//! valid. This is process is called mempool revalidation.
//!
//! ## Flows
//!
//! This section presents the most important flows within the fork aware transaction pool.
//!
//! ### Transaction route: [submit]
//! This flow is simple. Transaction is submitted to every view in [`active_views`], and if it is
//! not rejected by all views (e.g. due to pool limits) it is also included into [`TxMemPool`].
//!
//! When the newly created view does not contain this transaction yet, it is
//! [re-submitted][ForkAwareTxPool::update_view] from [`TxMemPool`] into this view.
//!
//! ### Transaction route: [submit_and_watch]
//! - what events do we have,
//! - how Pool provides a watched,
//! - we need to listen to every view and unify the stream (some events easy, some not - Invalid)
//! - multi view listener
//!
//! ### Handling the new (best) block
//!
//! The transaction pool exposes the [task][crate::common::notification_future] that listens to the
//! finalized and best block streams and executes the
//! `maintain` procedure. If the new block actaully needs to be handled, the following steps are
//! executed:
//! - [find][find_best_view] the best view and clone it to [create a new
//! view][crate::ForkAwareTxPool::build_new_view],
//! - [update the view][ForkAwareTxPool::update_view] with the transactions from the mempool
//! 	- all transactions from the mempool (with some obvious filtering applied) are submitted to the
//!    view,
//! 	- for all watched transactions from the mempool the watcher is registered in the new view,
//! 	and it is connected to the multi-view-listener,
//! - [update the view][ForkAwareTxPool::update_view_with_fork] with the transactions from the
//!   tree_route:
//! 	- resubmit the transcations from retracted blocks,
//! 	- prune extrinsics from the imported blocks, and trigger [`InBlock`] events,
//! - insert newly created and update view into the view store.
//!
//! ### Handling the finalized block
//! The following actions are taken on every finalized block:
//! - send [`Finalized`] events for every transactions on the finalized tree route,
//! - remove all the views (bot active and inactive) that are lower then finalized block from the
//! view store,
//! - removal of finalized transaction from the mempool,
//! - trigger mempool background revalidation
//!
//! ### Background tasks
//! The maintain process shall be as quick as possible, so heavy revalidation job is
//! delegated to the background worker. These includes view and mempool revalidation.
//!
//! ####  View revalidation
//! View revalidation is performed in the background thread. Revalidation is executed for every
//! view. All the transaction from the view are revalidated.
//!
//! The fork-aware pool utilizes two threads to execute maintain and reavalidation procesees
//! exclusively, ensuring maintain performance without overlapping with revalidation.
//!
//! The view revalidation process is triggered at the very end of the maintain process, and stopped
//! and the very beginning of the next maintenance execution (upon the next [`ChainEvent`]
//! repception). The results from the revalidation are immediately applied once the revalidation is
//! [terminated][crate::fork_aware_txpool::view::View::finish_revalidation].
//! ```text
//!  maintenance thread: M----M----M--M-M---
//! revalidation thread: -RRRR-RRRR-RR-R-RRR
//! ```
//!
//! ####  Mempool pruning/revalidation
//!   - time window
//!   - on finalized
//!   - 1k limit
//!   - initial validation
//!
//! ### Networking
//! - import sink notification
//!
//!
//! ### Handling invalid transactions
//!
//! ## Pool limits
//! - dropping
//!
//!
//! ## API Considerations
//! - at (aka internal most_recent_view)
//! - PendingExtrinsics (dedicated call)
//!
//! [`View`]: crate::fork_aware_txpool::view::View
//! [`ViewStore`]: crate::fork_aware_txpool::view_store::ViewStore
//! [find_best_view]: crate::fork_aware_txpool::view_store::ViewStore::find_best_view
//! [`active_views`]: crate::fork_aware_txpool::view_store::ViewStore::views
//! [`inactive_views`]: crate::fork_aware_txpool::view_store::ViewStore::retracted_views
//! [`ChainEvent`]: sc_transaction_pool_api::ChainEvent
//! [`TxMemPool`]: crate::fork_aware_txpool::tx_mem_pool::TxMemPool
//! [`Pool`]: crate::graph::Pool
//! [submit]: sc_transaction_pool_api::TransactionPool::submit_at
//! [submit_and_watch]: sc_transaction_pool_api::TransactionPool::submit_and_watch
//! [TransactionPool API]: sc_transaction_pool_api::TransactionPool
//! [`InBlock`]:sc_transaction_pool_api::TransactionStatus::InBlock
//! [`Finalized`]:sc_transaction_pool_api::TransactionStatus::Finalized
mod dropped_watcher;
pub(crate) mod fork_aware_txpool;
mod import_notification_sink;
mod metrics;
mod multi_view_listener;
mod tx_mem_pool;
mod view;
mod view_revalidation;
mod view_store;

pub(crate) use fork_aware_txpool::FullPool;
pub use fork_aware_txpool::{ForkAwareTxPool, ForkAwareTxPoolTask};
