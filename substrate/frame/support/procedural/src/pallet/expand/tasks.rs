use crate::pallet::{parse::tasks::*, Def};
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::{parse_quote, spanned::Spanned};

impl TaskEnumDef {
	pub fn generate(
		tasks: &TasksDef,
		type_decl_bounded_generics: TokenStream2,
		type_use_generics: TokenStream2,
	) -> Self {
		let variants = tasks.tasks.iter().map(|task| task.item.sig.ident.clone());
		let mut task_enum_def: TaskEnumDef = parse_quote! {
			/// Auto-generated enum that encapsulates all tasks defined by this pallet.
			///
			/// Conceptually similar to the [`Call`] enum, but for tasks. This is only
			/// generated if there are tasks present in this pallet.
			#[allow(non_camel_case_types)]
			#[pallet::task_enum]
			pub enum Task<#type_decl_bounded_generics> {
				#(
					#[allow(non_camel_case_types)]
					#variants
				),*
			}
		};
		task_enum_def.type_use_generics = type_use_generics;
		task_enum_def
	}
}

impl ToTokens for TaskEnumDef {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let item_enum = &self.item_enum;
		let ident = &item_enum.ident;
		let vis = &item_enum.vis;
		let attrs = &item_enum.attrs;
		let generics = &item_enum.generics;
		let variants = &item_enum.variants;
		let scrate = &self.scrate;
		let type_use_generics = &self.type_use_generics;
		if self.attr.is_some() {
			// `item_enum` is short-hand / generated enum
			tokens.extend(quote! {
				#(#attrs)*
				#[derive(
					Clone,
					PartialEq,
					Eq,
					#scrate::pallet_prelude::Encode,
					#scrate::pallet_prelude::Decode,
					#scrate::pallet_prelude::TypeInfo,
				)]
				#vis enum #ident #generics {
					#variants
					#[doc(hidden)]
					#[codec(skip)]
					__Ignore(core::marker::PhantomData<T>, #scrate::Never),
				}

				impl<T: Config> core::fmt::Debug for #ident<#type_use_generics> {
					fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
						f.debug_struct(stringify!(#ident)).field("value", self).finish()
					}
				}
			});
		} else {
			// `item_enum` is a manually specified enum (no attribute)
			tokens.extend(item_enum.to_token_stream());
		}
	}
}

pub fn expand_tasks(def: &mut Def) -> TokenStream2 {
	let tasks = &def.tasks;
	if let Some(tasks_def) = tasks {
		if def.task_enum.is_none() {
			def.task_enum = Some(TaskEnumDef::generate(
				&tasks_def,
				def.type_decl_bounded_generics(tasks_def.item_impl.span()),
				def.type_use_generics(tasks_def.item_impl.span()),
			));
		}
	}
	let task_enum = &def.task_enum;
	// TODO: add ToTokens impl for TasksDef so we can output it here
	quote! {
		#task_enum
	}
}
