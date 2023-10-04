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

//! Helpers for std and no-std testing. Can be re-used by other crates and benchmarking.

use super::*;

use codec::Encode;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

/// An opaque cursor of a migration.
pub type MockedCursor = BoundedVec<u8, ConstU32<1024>>;
/// An opaque identifier of a migration.
pub type MockedIdentifier = BoundedVec<u8, ConstU32<256>>;

/// How a [`MockedMigration`] should behave.
#[derive(Debug, Clone, Copy, Encode)]
#[allow(dead_code)]
pub enum MockedMigrationKind {
	/// Succeed after its number of steps elapsed.
	SucceedAfter,
	/// Fail after its number of steps elapsed.
	FailAfter,
	/// Never terminate.
	TimeoutAfter,
	/// Cause an [`InsufficientWeight`] error after its number of steps elapsed.
	HightWeightAfter(Weight),
}
use MockedMigrationKind::*; // C style

impl From<u8> for MockedMigrationKind {
	fn from(v: u8) -> Self {
		match v {
			0 => SucceedAfter,
			1 => FailAfter,
			2 => TimeoutAfter,
			3 => HightWeightAfter(Weight::MAX),
			_ => unreachable!(),
		}
	}
}

impl Into<u8> for MockedMigrationKind {
	fn into(self) -> u8 {
		match self {
			SucceedAfter => 0,
			FailAfter => 1,
			TimeoutAfter => 2,
			HightWeightAfter(_) => 3,
		}
	}
}

/// Calculate the identifier of a mocked migration.
pub fn mocked_id(kind: MockedMigrationKind, steps: u32) -> MockedIdentifier {
	raw_mocked_id(kind.into(), steps)
}

/// FAIL-CI
pub fn raw_mocked_id(kind: u8, steps: u32) -> MockedIdentifier {
	(b"MockedMigration", kind, steps).encode().try_into().unwrap()
}
