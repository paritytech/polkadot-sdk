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

//! # HOP (Hand-Off Protocol) Service
//!
//! Ephemeral data pool service for Substrate nodes. Provides 24-hour disk-backed
//! storage with an RPC interface for submission and retrieval.
//!
//! ## Overview
//!
//! HOP is a node-level service that enables peer-to-peer data sharing when
//! recipients are offline. Data is stored temporarily in a disk-backed pool
//! before being promoted to permanent chain storage.
//!
//! ## Features
//!
//! - **Disk-backed data pool** with configurable size limits
//! - **24-hour retention** (configurable in blocks)
//! - **RPC interface** for data submission and retrieval
//! - **Content-addressed storage** using Blake2-256 hashes
//!
//! ## Integration Guide
//!
//! ### 1. Add CLI Parameters
//!
//! ```rust,ignore
//! use sc_hop::HopParams;
//!
//! #[derive(Debug, clap::Parser)]
//! pub struct Cli {
//!     #[clap(flatten)]
//!     pub hop: HopParams,
//!     // ... other CLI fields
//! }
//! ```
//!
//! ### 2. Initialize the Service
//!
//! ```rust,ignore
//! use sc_hop::HopDataPool;
//! use std::sync::Arc;
//!
//! // Conditional initialization (SDK pattern)
//! let hop_pool = hop_params.enabled.then(|| {
//!     HopDataPool::new(
//!         hop_params.max_pool_size * 1024 * 1024,  // Convert MiB to bytes
//!         hop_params.retention_blocks,
//!     )
//!     .map(Arc::new)
//!     .map_err(|e| format!("Failed to create HOP pool: {}", e))
//! }).transpose()?;
//! ```
//!
//! ### 3. Register RPC Methods
//!
//! ```rust,ignore
//! use sc_hop::{HopApiServer, HopRpcServer};
//!
//! if let Some(hop_pool) = hop_pool {
//!     module.merge(HopRpcServer::new(hop_pool, client.clone()).into_rpc())?;
//! }
//! ```
//!
//! ## RPC Methods
//!
//! - `hop_submit(data: Bytes, recipients: Vec<Bytes>, signature: Bytes, signer: Bytes) ->
//!   SubmitResult` - Submit data with a signature from an authorized account
//! - `hop_claim(hash: Bytes, signature: Bytes) -> Bytes` - Claim data with SCALE-encoded
//!   MultiSignature
//! - `hop_ack(hash: Bytes, signature: Bytes) -> ()` - Acknowledge receipt. Marks recipient as
//!   claimed, triggers cleanup when all recipients have ack'd. Idempotent.
//! - `hop_poolStatus() -> PoolStatus` - Get pool statistics
//!
//! ## CLI Flags
//!
//! - `--enable-hop` - Enable HOP service
//! - `--hop-max-pool-size <MiB>` - Maximum pool size (default: 10240 MiB)
//! - `--hop-retention-blocks <blocks>` - Retention period (default: 14400)
//! - `--hop-check-interval <seconds>` - Maintenance interval (default: 3600)

pub mod cli;
pub mod pool;
pub mod primitives;
pub mod promotion;
pub mod rpc;
pub mod types;

// Convenience re-exports for common use cases
pub use cli::HopParams;
pub use pool::HopDataPool;
pub use primitives::{HopBlockNumber, HopHash};
pub use promotion::{HopMaintenanceTask, HopPromoter, RuntimeApiPromoter, try_build_promoter};
pub use rpc::{HopApiServer, HopRpcServer};
pub use types::{HopEntryMeta, HopError, PoolStatus, SenderId, SubmitResult};
