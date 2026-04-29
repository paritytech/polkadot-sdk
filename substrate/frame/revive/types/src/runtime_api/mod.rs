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

//! Runtime API client-facing types and versioned payloads shared by `pallet-revive` and its
//! off-chain clients.
//!
//! This module groups the reusable client-facing types nested inside runtime API payloads, the
//! top-level payloads themselves, and the payload-version discovery value clients use to negotiate
//! the newest mutually supported payload version.

pub mod payloads;
pub mod types;
pub mod version_declarations;

pub use payloads::*;
pub use types::*;
pub use version_declarations::*;
