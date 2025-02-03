use proc_macro::TokenStream;
use quote::quote;
use syn;

pub fn derive_stored(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    // Extract the name of the struct or enum
    let name = input.ident;

    // Generate the desired derives
    let expanded = quote! {
        // #[derive(MaxEncodedLen, Encode, Decode, DefaultNoBound, TypeInfo)]
        #name
    };

    // Return the generated code as a TokenStream
    expanded.into()
}
