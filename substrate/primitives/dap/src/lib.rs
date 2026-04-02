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

//! Primitives for the Dynamic Allocation Pool (DAP).
//!
//! Shared between `pallet-dap` (on AssetHub) and `pallet-dap-satellite` (on satellite chains)
//! to ensure both pallets agree on the DAP buffer account derivation.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::PalletId;

/// The [`PalletId`] used to derive the DAP buffer account on AssetHub.
///
/// Both `pallet-dap` and `pallet-dap-satellite` use this to derive the same account address,
/// ensuring satellite chains can correctly target the buffer when sending via XCM teleport.
pub const DAP_BUFFER_PALLET_ID: PalletId = PalletId(*b"dap/buff");
