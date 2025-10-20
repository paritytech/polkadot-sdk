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
//!Types, and traits to integrate pallet-revive with EVM.
#![warn(missing_docs)]

mod api;
pub use api::*;
mod call;
pub(crate) use call::*;
mod tracing;
pub use tracing::*;
pub mod fees;
pub mod runtime;
pub mod tx_extension;
pub use alloy_core::sol_types::decode_revert_reason;

/// Ethereum block hash builder related types.
pub(crate) mod block_hash;
pub use block_hash::ReceiptGasInfo;

/// Ethereum block storage module.
pub(crate) mod block_storage;

type OnChargeTransactionBalanceOf<T> = <<T as pallet_transaction_payment::Config>::OnChargeTransaction as pallet_transaction_payment::OnChargeTransaction<T>>::Balance;
