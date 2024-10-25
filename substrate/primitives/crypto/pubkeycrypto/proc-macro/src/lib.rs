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

//! Macros to implement internals of public key crypto modules
//!
use proc_macro::{TokenStream};
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

/// Implement Proof of Possession  Generation and Verificition for Crypto Pair type
#[proc_macro_derive(ProofOfPossession)]
pub fn derive_proof_of_possession(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);

    // Get the name of the struct or enum to implement the trait for
    let name = input.ident;

    // Generate the implementation for the trait
    let expanded = quote! {
	#[cfg(feature = "full_crypto")]
        impl ProofOfPossessionGenerator for #name {}
        impl ProofOfPossessionVerifier for #name {}	
    };

    // Convert the quote output to a TokenStream
    TokenStream::from(expanded)
}
