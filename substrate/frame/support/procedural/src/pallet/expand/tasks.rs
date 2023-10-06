use crate::pallet::{parse::tasks::*, Def};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::spanned::Spanned;

pub fn expand_tasks(def: &mut Def) -> TokenStream2 {
	let tasks = &def.tasks;
	if let Some(tasks_def) = tasks {
		if def.task_enum.is_none() {
			def.task_enum = Some(TaskEnumDef::generate(
				&tasks_def,
				def.type_decl_bounded_generics(tasks_def.item_impl.span()),
			));
		}
	}
	let _task_enum = &def.task_enum;
	quote!()
}
