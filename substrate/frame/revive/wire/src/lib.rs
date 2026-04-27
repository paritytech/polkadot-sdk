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

//! Wire types and versioned runtime API interfaces shared by `pallet-revive` runtime code and
//! clients.
//!
//! This crate owns the stable boundary types that cross the runtime API. The types are kept
//! separate from the pallet so clients can depend on the wire format without depending on the
//! pallet's execution internals.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

pub use pallet_revive_proc_macro::{define_versioned_interface, define_versioned_type};
