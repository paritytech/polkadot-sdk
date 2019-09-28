// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Substrate Client and associated logic.
//!
//! The [`Client`] is one of the most important components of Substrate. It mainly comprises two
//! parts:
//!
//! - A database containing the blocks and chain state, generally referred to as
//! the [`Backend`](backend::Backend).
//! - A runtime environment, generally referred to as the [`Executor`](CallExecutor).
//!
//! # Initialization
//!
//! Creating a [`Client`] is done by calling the `new` method and passing to it a
//! [`Backend`](backend::Backend) and an [`Executor`](CallExecutor).
//!
//! The former is typically provided by the `substrate-client-db` crate.
//!
//! The latter typically requires passing one of:
//!
//! - A [`LocalCallExecutor`] running the runtime locally.
//! - A [`RemoteCallExecutor`](light::call_executor::RemoteCallExecutor) that will ask a
//! third-party to perform the executions.
//! - A [`RemoteOrLocalCallExecutor`](light::call_executor::RemoteOrLocalCallExecutor), combination
//! of the two.
//!
//! Additionally, the fourth generic parameter of the `Client` is a marker type representing
//! the ways in which the runtime can interface with the outside. Any code that builds a `Client`
//! is responsible for putting the right marker.
//!
//! ## Example
//!
//! ```
//! use std::sync::Arc;
//! use substrate_client::{Client, in_mem::Backend, LocalCallExecutor};
//! use primitives::Blake2Hasher;
//! use sr_primitives::{StorageOverlay, ChildrenStorageOverlay};
//! use executor::NativeExecutor;
//!
//! // In this example, we're using the `Block` and `RuntimeApi` types from the
//! // `substrate-test-runtime-client` crate. These types are automatically generated when
//! // compiling a runtime. In a typical use-case, these types would have been to be generated
//! // from your runtime.
//! use test_client::{LocalExecutor, runtime::Block, runtime::RuntimeApi};
//!
//! let backend = Arc::new(Backend::<Block, Blake2Hasher>::new());
//! let client = Client::<_, _, _, RuntimeApi>::new(
//! 	backend.clone(),
//! 	LocalCallExecutor::new(
//! 		backend.clone(),
//! 		NativeExecutor::<LocalExecutor>::new(None),
//! 		None,
//!		),
//! 	// This parameter provides the storage for the chain genesis.
//! 	<(StorageOverlay, ChildrenStorageOverlay)>::default(),
//! 	Default::default(),
//! 	Default::default(),
//! );
//! ```
//!

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![recursion_limit="128"]

#[macro_use]
pub mod runtime_api;
#[cfg(feature = "std")]
pub mod error;
#[cfg(feature = "std")]
pub mod blockchain;
#[cfg(feature = "std")]
pub mod backend;
#[cfg(feature = "std")]
pub mod cht;
#[cfg(feature = "std")]
pub mod in_mem;
#[cfg(feature = "std")]
pub mod genesis;
pub mod block_builder;
#[cfg(feature = "std")]
pub mod light;
#[cfg(feature = "std")]
pub mod leaves;
#[cfg(feature = "std")]
pub mod children;
#[cfg(feature = "std")]
mod call_executor;
#[cfg(feature = "std")]
mod client;
#[cfg(feature = "std")]
mod notifications;


#[cfg(feature = "std")]
pub use crate::blockchain::Info as ChainInfo;
#[cfg(feature = "std")]
pub use crate::call_executor::{CallExecutor, LocalCallExecutor};
#[cfg(feature = "std")]
pub use crate::client::{
	new_with_backend,
	new_in_mem,
	BlockBody, ImportNotifications, FinalityNotifications, BlockchainEvents,
	BlockImportNotification, Client, ClientInfo, ExecutionStrategies, FinalityNotification,
	LongestChain, BlockOf, ProvideUncles, ForkBlocks,
	utils, apply_aux,
};
#[cfg(feature = "std")]
pub use crate::notifications::{StorageEventStream, StorageChangeSet};
#[cfg(feature = "std")]
pub use state_machine::ExecutionStrategy;
#[cfg(feature = "std")]
pub use crate::leaves::LeafSet;
#[cfg(feature = "std")]
pub use crate::blockchain::well_known_cache_keys;

#[doc(inline)]
pub use sr_api_macros::{decl_runtime_apis, impl_runtime_apis};
