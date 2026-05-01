// This file is part of Substrate.

// Copyright (C) 2020-2025 Acala Foundation.
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

//! Runtime API definition for the oracle pallet.
//!
//! This crate provides runtime APIs that allow external clients to query oracle data
//! from the blockchain. The APIs are designed to be efficient and provide access to
//! both individual oracle values and complete datasets.
//!
//! ## Overview
//!
//! The oracle runtime API enables off-chain applications, wallets, and other blockchain
//! clients to retrieve oracle data without needing to parse storage directly. This
//! abstraction provides a clean interface for accessing oracle information and ensures
//! compatibility across different runtime versions.
//!
//! The API supports querying data from specific oracle providers and retrieving all
//! available oracle data, making it suitable for various use cases such as price
//! feeds, data monitoring, and external integrations.

#![cfg_attr(not(feature = "std"), no_std)]
// The `too_many_arguments` warning originates from `decl_runtime_apis` macro.
#![allow(clippy::too_many_arguments)]
// The `unnecessary_mut_passed` warning originates from `decl_runtime_apis` macro.
#![allow(clippy::unnecessary_mut_passed)]

use codec::Codec;
use sp_std::prelude::Vec;

sp_api::decl_runtime_apis! {
	/// Runtime API for querying oracle data from the blockchain.
	///
	/// This trait provides methods to retrieve oracle data without requiring direct
	/// storage access. It's designed to be called from external clients, RPC nodes,
	/// and other blockchain infrastructure components that need access to oracle
	/// information.
	///
	/// The API is generic over three type parameters:
	/// - `ProviderId`: Identifies the oracle provider or data source
	/// - `Key`: The oracle key identifying the specific data feed
	/// - `Value`: The oracle data value type
	pub trait OracleApi<ProviderId, Key, Value> where
		ProviderId: Codec,
		Key: Codec,
		Value: Codec,
	{
		/// Retrieves a specific oracle value for a given provider and key.
		///
		/// Returns the current oracle value if available, or `None` if no data exists
		/// for the specified provider and key combination.
		///
		/// # Parameters
		///
		/// * `provider_id`: The oracle provider identifier
		/// * `key`: The oracle key identifying the data feed
		///
		/// # Returns
		///
		/// Returns `Some(value)` if oracle data exists, `None` otherwise.
		fn get_value(provider_id: ProviderId, key: Key) -> Option<Value>;

		/// Retrieves all oracle values for a specific provider.
		///
		/// Returns a vector of key-value pairs containing all available oracle data
		/// from the specified provider. Each pair contains the oracle key and its
		/// corresponding value (if available).
		///
		/// # Parameters
		///
		/// * `provider_id`: The oracle provider identifier
		///
		/// # Returns
		///
		/// Returns a vector of `(Key, Option<Value>)` pairs representing all oracle
		/// data available from the specified provider.
		fn get_all_values(provider_id: ProviderId) -> Vec<(Key, Option<Value>)>;
	}
}
