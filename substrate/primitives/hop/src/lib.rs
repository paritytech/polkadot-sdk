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

//! HOP (Hand-Off Protocol) primitives.
//!
//! Contains the runtime API trait for HOP promotion — promoting ephemeral pool
//! data to permanent on-chain storage via `pallet-transaction-storage`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

sp_api::decl_runtime_apis! {
	/// Runtime API for HOP promotion.
	///
	/// Runtimes that include `pallet-hop-promotion` implement this API so the
	/// node's background maintenance task can automatically promote near-expiry
	/// HOP pool entries to permanent chain storage.
	pub trait HopPromotionApi {
		/// Construct a general transaction extrinsic for promoting HOP data.
		fn create_promotion_extrinsic(data: alloc::vec::Vec<u8>) -> Block::Extrinsic;
		/// Maximum data size per promotion extrinsic.
		fn max_promotion_size() -> u32;
	}
}
