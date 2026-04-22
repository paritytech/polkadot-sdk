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

use crate::{pallet::Def, COUNTER};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, Ident};

/// expand the `is_origin_part_defined` macro and the `AccountLike` impl.
pub fn expand_origin(def: &mut Def) -> TokenStream {
	let count = COUNTER.with(|counter| counter.borrow_mut().inc());
	let macro_ident = Ident::new(&format!("__is_origin_part_defined_{}", count), def.item.span());

	let maybe_compile_error = if def.origin.is_none() {
		quote! {
			compile_error!(concat!(
				"`",
				stringify!($pallet_name),
				"` does not have #[pallet::origin] defined, perhaps you should \
				remove `Origin` from construct_runtime?",
			));
		}
	} else {
		TokenStream::new()
	};

	let account_like_impl = generate_account_like_impl(def);

	quote! {
		#[doc(hidden)]
		pub mod __substrate_origin_check {
			#[macro_export]
			#[doc(hidden)]
			macro_rules! #macro_ident {
				($pallet_name:ident) => {
					#maybe_compile_error
				}
			}

			#[doc(hidden)]
			pub use #macro_ident as is_origin_part_defined;
		}

		#account_like_impl
	}
}

fn generate_account_like_impl(def: &Def) -> TokenStream {
	let origin_def = match &def.origin {
		Some(o) => o,
		None => return TokenStream::new(),
	};

	let frame_support = &def.frame_support;
	let frame_system = &def.frame_system;
	let span = def.item.span();

	// Type alias origins: delegate to the `AccountLike` trait impl on the aliased type.
	let is_type_alias = def
		.item
		.content
		.as_ref()
		.map(|(_, items)| {
			items.iter().any(|item| {
				if let syn::Item::Type(t) = item {
					t.ident == "Origin"
				} else {
					false
				}
			})
		})
		.unwrap_or(false);

	let type_impl_gen = &def.type_impl_generics(span);
	let type_use_gen = &def.type_use_generics(span);
	let where_clause = &def.config.where_clause;

	if is_type_alias {
		return quote! {
			impl<#type_impl_gen> Pallet<#type_use_gen> #where_clause {
				#[doc(hidden)]
				pub fn __as_account_for_origin(
					origin: &Origin<#type_use_gen>,
				) -> Option<<T as #frame_system::Config>::AccountId> {
					<Origin<#type_use_gen> as
						#frame_support::traits::AccountLike<
							<T as #frame_system::Config>::AccountId
						>
					>::as_account(origin)
				}

				#[doc(hidden)]
				pub fn __nonce_provider_for_origin(
					origin: &Origin<#type_use_gen>,
				) -> Option<<T as #frame_system::Config>::AccountId> {
					<Origin<#type_use_gen> as
						#frame_support::traits::AccountLike<
							<T as #frame_system::Config>::AccountId
						>
					>::nonce_provider(origin)
				}

				#[doc(hidden)]
				pub fn __fee_payer_for_origin(
					origin: &Origin<#type_use_gen>,
				) -> Option<<T as #frame_system::Config>::AccountId> {
					<Origin<#type_use_gen> as
						#frame_support::traits::AccountLike<
							<T as #frame_system::Config>::AccountId
						>
					>::fee_payer(origin)
				}
			}
		};
	}

	// The origin type reference for method parameters.
	let origin_type_ref = if origin_def.is_generic {
		quote! { &Origin<#type_use_gen> }
	} else {
		quote! { &Origin }
	};

	// Generate getter functions and match arms for account_like_defs.
	let mut getter_fns = TokenStream::new();
	let mut as_account_match_arms = TokenStream::new();
	let mut nonce_provider_match_arms = TokenStream::new();
	let mut fee_payer_match_arms = TokenStream::new();

	for al in &origin_def.account_like_defs {
		let variant_ident = &al.variant_ident;
		let expr = &al.expr;

		let getter_name =
			Ident::new(&format!("__as_account_for_{}", variant_ident), variant_ident.span());

		let field_types: Vec<_> = al.fields.iter().map(|f| &f.ty).collect();
		let field_bindings: Vec<_> = (0..al.fields.len())
			.map(|i| Ident::new(&format!("field_{}", i), variant_ident.span()))
			.collect();
		let destructure = build_destructure_pattern(&al.fields, &field_bindings);

		// Getter function on Pallet<T> — gives closures access to Self and T::AccountId.
		getter_fns.extend(quote::quote_spanned!(expr.span() =>
			impl<#type_impl_gen> Pallet<#type_use_gen> #where_clause {
				#[doc(hidden)]
				#[allow(non_snake_case)]
				fn #getter_name() -> impl Fn(
					#( &#field_types ),*
				) -> Option<<T as #frame_system::Config>::AccountId> {
					#expr
				}
			}
		));

		// as_account: all variants with #[pallet::as_account(...)]
		as_account_match_arms.extend(quote! {
			Origin::#variant_ident #destructure => {
				let f = Pallet::<#type_use_gen>::#getter_name();
				f(#( #field_bindings ),*)
			},
		});

		// nonce_provider: only variants with #[pallet::nonce_provider]
		if al.is_nonce_provider {
			nonce_provider_match_arms.extend(quote! {
				Origin::#variant_ident #destructure => {
					let f = Pallet::<#type_use_gen>::#getter_name();
					f(#( #field_bindings ),*)
				},
			});
		}

		// fee_payer: only variants with #[pallet::fee_payer]
		if al.is_fee_payer {
			fee_payer_match_arms.extend(quote! {
				Origin::#variant_ident #destructure => {
					let f = Pallet::<#type_use_gen>::#getter_name();
					f(#( #field_bindings ),*)
				},
			});
		}
	}

	// Generate method bodies for each of the three methods.
	let has_as_account = !origin_def.account_like_defs.is_empty();
	let has_nonce_providers = origin_def.account_like_defs.iter().any(|al| al.is_nonce_provider);
	let has_fee_payers = origin_def.account_like_defs.iter().any(|al| al.is_fee_payer);

	let as_account_body = if has_as_account {
		quote! {
			match origin {
				#as_account_match_arms
				_ => None,
			}
		}
	} else {
		quote! { None }
	};

	let nonce_provider_body = if has_nonce_providers {
		quote! {
			match origin {
				#nonce_provider_match_arms
				_ => None,
			}
		}
	} else {
		quote! { None }
	};

	let fee_payer_body = if has_fee_payers {
		quote! {
			match origin {
				#fee_payer_match_arms
				_ => None,
			}
		}
	} else {
		quote! { None }
	};

	let as_account_param =
		if has_as_account { Ident::new("origin", span) } else { Ident::new("_origin", span) };
	let nonce_provider_param =
		if has_nonce_providers { Ident::new("origin", span) } else { Ident::new("_origin", span) };
	let fee_payer_param =
		if has_fee_payers { Ident::new("origin", span) } else { Ident::new("_origin", span) };

	let pallet_methods = quote! {
		#getter_fns

		impl<#type_impl_gen> Pallet<#type_use_gen> #where_clause {
			#[doc(hidden)]
			pub fn __as_account_for_origin(
				#as_account_param: #origin_type_ref,
			) -> Option<<T as #frame_system::Config>::AccountId> {
				#as_account_body
			}

			#[doc(hidden)]
			pub fn __nonce_provider_for_origin(
				#nonce_provider_param: #origin_type_ref,
			) -> Option<<T as #frame_system::Config>::AccountId> {
				#nonce_provider_body
			}

			#[doc(hidden)]
			pub fn __fee_payer_for_origin(
				#fee_payer_param: #origin_type_ref,
			) -> Option<<T as #frame_system::Config>::AccountId> {
				#fee_payer_body
			}
		}
	};

	// Generate AccountLike trait impl.
	let trait_impl = if origin_def.is_generic {
		let as_account_method = if has_as_account {
			quote! {
				fn as_account(&self) -> Option<<T as #frame_system::Config>::AccountId> {
					Pallet::<#type_use_gen>::__as_account_for_origin(self)
				}
			}
		} else {
			TokenStream::new()
		};
		let nonce_provider_method = if has_nonce_providers {
			quote! {
				fn nonce_provider(&self) -> Option<<T as #frame_system::Config>::AccountId> {
					Pallet::<#type_use_gen>::__nonce_provider_for_origin(self)
				}
			}
		} else {
			TokenStream::new()
		};
		let fee_payer_method = if has_fee_payers {
			quote! {
				fn fee_payer(&self) -> Option<<T as #frame_system::Config>::AccountId> {
					Pallet::<#type_use_gen>::__fee_payer_for_origin(self)
				}
			}
		} else {
			TokenStream::new()
		};

		quote! {
			impl<#type_impl_gen> #frame_support::traits::AccountLike<
				<T as #frame_system::Config>::AccountId
			> for Origin<#type_use_gen> #where_clause {
				#as_account_method
				#nonce_provider_method
				#fee_payer_method
			}
		}
	} else {
		// For non-generic origins (like `enum Origin {..}`) we don't implement the `AccountLike`
		// trait. Instead we rely on the generated code in the pallet, `__as_account_for_*` which is
		// called in `__as_account_for_origin`. These are then used in the `AccountLike` impl on
		// `OriginCaller` in `construct_runtime!`. We need to implement this trait here regardless
		// to satisfy trait bounds on `CallerTrait`. This is why nobody should use `AccountLike`
		// directly on origins and instead rely on methods exposed by `CallerTrait` on the
		// `RuntimeOrigin`, which will be correct because they are generated as stated above in
		// `construct_runtime!`.
		//
		// If we just got rid of the need to be able to access `T` in the closure, we could get rid
		// of this, but we will lose the ability to access storage and other pallet methods inside
		// natively.
		quote! {
			impl<AccountId> #frame_support::traits::AccountLike<AccountId> for Origin {}
		}
	};

	quote! {
		#pallet_methods
		#trait_impl
	}
}

fn build_destructure_pattern(fields: &syn::Fields, bindings: &[Ident]) -> TokenStream {
	match fields {
		syn::Fields::Named(named) => {
			let field_names: Vec<_> = named.named.iter().map(|f| &f.ident).collect();
			quote! { { #( #field_names: #bindings ),* } }
		},
		syn::Fields::Unnamed(_) => {
			quote! { ( #( #bindings ),* ) }
		},
		syn::Fields::Unit => {
			quote! {}
		},
	}
}
