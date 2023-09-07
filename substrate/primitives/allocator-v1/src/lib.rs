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

//! # V1 spec about runtime abi
//!
//! Export the `alloc`/`dealloc`/`realloc` primitive functions inside the runtime to host.
//!
//! In runtime crate, must use the following sp-io features:
//! - disable_allocator
//! - disable_panic_handler
//! - disable_oom
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(enable_alloc_error_handler, feature(alloc_error_handler))]

#[cfg(all(feature = "allocator-v1", not(feature = "std")))]
pub mod config;
