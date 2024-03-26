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

use codec::{Decode, Encode, MaxEncodedLen};

/// # Multi-Block Migrations Module
///
/// This module showcases a simple use of the multi-block migrations framework.
pub mod v1;

/// Migration identifier.
///
/// Used to identify a migration across all pallets. This identifier is essential because
/// the [`SteppedMigration::Identifier`](`frame_support::migrations::SteppedMigration::Identifier`)
/// needs to be globally unique.
#[derive(MaxEncodedLen, Encode, Decode)]
pub struct MigrationIdentifier {
	pallet_identifier: [u8; 16],
	version_from: u8,
	version_to: u8,
}

/// A unique identifier across all pallets.
///
/// This constant represents a unique identifier for the migrations of this pallet.
/// It helps differentiate migrations for this pallet from those of others.
const PALLET_MIGRATIONS_ID: &[u8; 26] = b"pallet-examples-pallet-mbm";
