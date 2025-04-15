// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.
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

//! Various runtime APIs to support XCM processing and manipulation.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

/// Runtime APIs for querying XCM authorized aliases.
pub mod authorized_aliases;
/// Exposes runtime APIs for various XCM-related conversions.
pub mod conversions;
/// Dry-run API.
/// Given an extrinsic or an XCM program, it returns the outcome of its execution.
pub mod dry_run;
/// Fee estimation API.
/// Given an XCM program, it will return the fees needed to execute it properly or send it.
pub mod fees;
/// Exposes runtime API for querying whether a Location is trusted as a reserve or teleporter for a
/// given Asset.
pub mod trusted_query;
