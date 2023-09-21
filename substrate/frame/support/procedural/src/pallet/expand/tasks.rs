use crate::pallet::Def;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

pub fn expand_tasks(_def: &mut Def) -> TokenStream2 {
	quote!()
}
