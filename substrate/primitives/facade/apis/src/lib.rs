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

//! This crate contains the definitions of the Polkadot Facade Runtime APIs.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use sp_facade_apis_macro::define_facade_apis;

/// Some custom type.
pub type MyCustomType = bool;
/// Another custom type.
pub type CustomThing = bool;
/// String type.
pub type String = alloc::string::String;

define_facade_apis! { 
    /// An example facade API. Traits are defined the same way
    /// as with `decl_runtime_apis` with some restrictions.
    pub trait FacadeExample {
        /// Method docs are required.
        fn foo(bar: u32, other: Option<String>) -> MyCustomType;

        /// api_version is supported on methods, but not on the
        /// top level trait (because all versions should be defined).
        #[api_version(2)]
        fn bar(something: String, more: CustomThing);

        /// We'll get a compile error if we see a version number N
        /// where N-1 isn't an existing version of another method.
        #[api_version(3)]
        fn wibble(something: String, more: CustomThing);
    }
}
