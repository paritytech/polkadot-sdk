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

use super::{helper, InheritedCallWeightAttr};
use frame_support_procedural_tools::get_doc_literals;
use proc_macro2::Span;
use quote::ToTokens;
use std::collections::HashMap;
use syn::{spanned::Spanned, ExprClosure};

/// Definition of dispatchables typically `impl<T: Config> Pallet<T> { ... }`
pub struct ViewFunctionsDef {
	// /// The where_clause used.
	// pub where_clause: Option<syn::WhereClause>,
	// /// A set of usage of instance, must be check for consistency with trait.
	// pub instances: Vec<helper::InstanceUsage>,
	// /// The index of call item in pallet module.
	// pub index: usize,
	// /// Information on methods (used for expansion).
	// pub methods: Vec<CallVariantDef>,
	// /// The span of the pallet::call attribute.
	// pub attr_span: proc_macro2::Span,
	// /// Docs, specified on the impl Block.
	// pub docs: Vec<syn::Expr>,
	// /// The optional `weight` attribute on the `pallet::call`.
	// pub inherited_call_weight: Option<InheritedCallWeightAttr>,
	view_functions: Vec<ViewFunctionDef>
}

pub struct ViewFunctionDef {
	pub name: syn::Ident,
	pub return_type: syn::Type,
}

impl ViewFunctionsDef {
	pub fn try_from(
		attr_span: proc_macro2::Span,
		item: &mut syn::Item,
	) -> syn::Result<Self> {
		let item_impl = if let syn::Item::Impl(item) = item {
			item
		} else {
			return Err(syn::Error::new(item.span(), "Invalid pallet::view_functions, expected item impl"))
		};
		let mut view_functions = Vec::new();
		for item in &mut item_impl.items {
			if let syn::ImplItem::Fn(method) = item {
				if !matches!(method.vis, syn::Visibility::Public(_)) {
					let msg = "Invalid pallet::view_functions, view function must be public: \
						`pub fn`";

					let span = match method.vis {
						syn::Visibility::Inherited => method.sig.span(),
						_ => method.vis.span(),
					};

					return Err(syn::Error::new(span, msg))
				}

				let syn::ReturnType::Type(_, type_) = &method.sig.output else {
					return Err(syn::Error::new(method.sig.output.span(), "view functions must return a value"))
				};

				view_functions.push(ViewFunctionDef {
					name: method.sig.ident.clone(),
					return_type: *type_.clone(),
				})
			}
		}
		Ok(Self {
			view_functions,
		})
	}
}
