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

//! Generates warnings for undesirable pallet code.

use crate::pallet::parse::call::{CallVariantDef, CallWeightDef};
use proc_macro_warning::Warning;
use syn::{
	spanned::Spanned,
	visit::{self, Visit},
};

/// Warn if any of the call arguments starts with a underscore and is used in a weight formula.
pub(crate) fn weight_witness_warning(
	method: &CallVariantDef,
	dev_mode: bool,
	warnings: &mut Vec<Warning>,
) {
	if dev_mode {
		return
	}
	let CallWeightDef::Immediate(w) = &method.weight else {
		return
	};

	let partial_warning = Warning::new_deprecated("UncheckedWeightWitness")
		.old("not check weight witness data")
		.new("ensure that all witness data for weight calculation is checked before usage")
		.help_link("https://github.com/paritytech/polkadot-sdk/pull/1818");

	for (_, arg_ident, _) in method.args.iter() {
		if !arg_ident.to_string().starts_with('_') || !contains_ident(w.clone(), &arg_ident) {
			continue
		}

		let warning = partial_warning
			.clone()
			.index(warnings.len())
			.span(arg_ident.span())
			.build_or_panic();

		warnings.push(warning);
	}
}

/// Warn if the weight is a constant and the pallet not in `dev_mode`.
pub(crate) fn weight_constant_warning(
	weight: &syn::Expr,
	dev_mode: bool,
	warnings: &mut Vec<Warning>,
) {
	if dev_mode {
		return
	}
	let syn::Expr::Lit(lit) = weight else {
		return
	};

	let warning = Warning::new_deprecated("ConstantWeight")
		.index(warnings.len())
		.old("use hard-coded constant as call weight")
		.new("benchmark all calls or put the pallet into `dev` mode")
		.help_link("https://github.com/paritytech/substrate/pull/13798")
		.span(lit.span())
		.build_or_panic();

	warnings.push(warning);
}

/// Returns whether `expr` contains `ident`.
fn contains_ident(mut expr: syn::Expr, ident: &syn::Ident) -> bool {
	struct ContainsIdent {
		ident: syn::Ident,
		found: bool,
	}

	impl<'a> Visit<'a> for ContainsIdent {
		fn visit_ident(&mut self, i: &syn::Ident) {
			if *i == self.ident {
				self.found = true;
			}
		}
	}

	let mut visitor = ContainsIdent { ident: ident.clone(), found: false };
	visit::visit_expr(&mut visitor, &mut expr);
	visitor.found
}
