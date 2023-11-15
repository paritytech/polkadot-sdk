// Copyright (C) Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Data structures to operate on contract memory during contract execution.
//!
//! These definitions are useful since we are operating in a `no_std` environment
//! and should be used by all ink! crates instead of directly using `std` or `alloc`
//! crates. If needed we shall instead enhance the exposed types here.

cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        pub use std::{
            format,
        };
    } else {
		extern crate alloc;
        pub use alloc::{
            format,
        };
    }
}

