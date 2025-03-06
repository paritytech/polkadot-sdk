use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use syn::{Path, Result};

/// Resolve the correct path for sc_tracing:
/// - If `polkadot-sdk` is in scope, returns a Path corresponding to `polkadot_sdk::sc_tracing`
/// - Otherwise, falls back to `sc_tracing`
pub fn resolve_sc_tracing() -> Result<Path> {
	match crate_name("polkadot-sdk") {
		Ok(FoundCrate::Itself) => syn::parse_str("polkadot_sdk::sc_tracing"),
		Ok(FoundCrate::Name(sdk_name)) => syn::parse_str(&format!("{}::sc_tracing", sdk_name)),
		Err(_) => match crate_name("sc-tracing") {
			Ok(FoundCrate::Itself) => syn::parse_str("sc_tracing"),
			Ok(FoundCrate::Name(name)) => syn::parse_str(&name),
			Err(e) => Err(syn::Error::new(Span::call_site(), e)),
		},
	}
}
