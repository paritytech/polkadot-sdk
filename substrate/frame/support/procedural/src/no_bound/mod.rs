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

//! Derive macros to derive traits without bounding generic parameters.

use syn::spanned::Spanned;

pub mod clone;
pub mod debug;
pub mod default;
pub mod ord;
pub mod partial_eq;
pub mod partial_ord;

/// Create a tuple-field index that points to the original field type.
fn field_index(index: usize, field: &syn::Field) -> syn::Index {
	let mut index = syn::Index::from(index);
	index.span = field.ty.span();
	index
}
