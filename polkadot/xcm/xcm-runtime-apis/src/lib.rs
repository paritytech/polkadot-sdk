// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Various runtime APIs to support XCM processing and manipulation.

#![cfg_attr(not(feature = "std"), no_std)]

/// Exposes runtime APIs for various XCM-related conversions.
pub mod conversions;

/// Dry-run API.
/// Given an extrinsic or an XCM program, it returns the outcome of its execution.
pub mod dry_run;

/// Fee estimation API.
/// Given an XCM program, it will return the fees needed to execute it properly or send it.
pub mod fees;
