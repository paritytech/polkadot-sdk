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

use inflector::Inflector;
use syn::spanned::Spanned;
use frame_support_procedural_tools::get_doc_literals;

/// Definition of dispatchables typically `impl<T: Config> Pallet<T> { ... }`
pub struct ViewFunctionsImplDef {
	/// The where_clause used.
	pub where_clause: Option<syn::WhereClause>,
	// /// A set of usage of instance, must be check for consistency with trait.
	// pub instances: Vec<helper::InstanceUsage>,
	// /// The index of call item in pallet module.
	// pub index: usize,
	// /// Information on methods (used for expansion).
	// pub methods: Vec<CallVariantDef>,
	/// The span of the pallet::view_functions attribute.
	pub attr_span: proc_macro2::Span,
	// /// Docs, specified on the impl Block.
	// pub docs: Vec<syn::Expr>,
	// /// The optional `weight` attribute on the `pallet::call`.
	// pub inherited_call_weight: Option<InheritedCallWeightAttr>,
	pub view_functions: Vec<ViewFunctionDef>,
}

impl ViewFunctionsImplDef {
	pub fn try_from(attr_span: proc_macro2::Span, item: &mut syn::Item) -> syn::Result<Self> {
		let item_impl = if let syn::Item::Impl(item) = item {
			item
		} else {
			return Err(syn::Error::new(
				item.span(),
				"Invalid pallet::view_functions, expected item impl",
			))
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

				let view_fn_def = ViewFunctionDef::try_from(method.clone())?;
				view_functions.push(view_fn_def)
			}
		}
		Ok(Self {
			view_functions,
			attr_span,
			where_clause: item_impl.generics.where_clause.clone(),
		})
	}
}

pub struct ViewFunctionDef {
	pub name: syn::Ident,
	pub docs: Vec<syn::Expr>,
	pub args: Vec<syn::FnArg>,
	pub return_type: syn::Type,
}

impl TryFrom<syn::ImplItemFn> for ViewFunctionDef {
	type Error = syn::Error;
	fn try_from(method: syn::ImplItemFn) -> Result<Self, Self::Error> {
		let syn::ReturnType::Type(_, type_) = method.sig.output else {
			return Err(syn::Error::new(
				method.sig.output.span(),
				"view functions must return a value",
			))
		};

		Ok(Self {
			name: method.sig.ident.clone(),
			docs: get_doc_literals(&method.attrs),
			args: method.sig.inputs.iter().cloned().collect::<Vec<_>>(),
			return_type: *type_.clone(),
		})
	}
}

impl ViewFunctionDef {
	pub fn query_struct_ident(&self) -> syn::Ident {
		syn::Ident::new(
			&format!("{}Query", self.name.to_string().to_pascal_case()),
			self.name.span(),
		)
	}
}
