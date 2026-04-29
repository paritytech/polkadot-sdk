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

//! Versioned client-facing types that appear inside one or more runtime API payloads but are not
//! themselves a top-level input or output payload. Top-level payloads live in
//! [`crate::runtime_api::payloads`].
//!
//! Each type in this module is declared with the [`define_versioned_type!`] macro from
//! `pallet-revive-proc-macro`, so adding a new version is a matter of writing the delta from the
//! previous version. See the crate-level documentation for the rules that govern when a new version
//! is allowed and what stays frozen once a version is released.
//!
//! [`define_versioned_type!`]: pallet_revive_proc_macro::define_versioned_type

mod block;
mod bytes;
mod code;
mod contract;
mod errors;
mod receipt;
mod state_overrides;
mod storage;
mod tracing_common;
mod tracing_config;
mod tracing_result;
mod transaction;
mod transaction_signed;
mod transaction_unsigned;

pub use block::*;
pub use bytes::*;
pub use code::*;
pub use contract::*;
pub use errors::*;
pub use receipt::*;
pub use state_overrides::*;
pub use storage::*;
pub use tracing_common::*;
pub use tracing_config::*;
pub use tracing_result::*;
pub use transaction::*;
pub use transaction_signed::*;
pub use transaction_unsigned::*;
