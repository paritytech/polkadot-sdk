use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use syn::{parse_macro_input, DeriveInput, Data, Error, Fields};
use quote::{quote, format_ident};

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
	let builder_impl = match &input.data {
		Data::Enum(data_enum) => generate_methods_for_enum(input.ident, data_enum),
		_ => return Error::new_spanned(&input, "Expected the `Instruction` enum")
			.to_compile_error().into(),
	};
	let output = quote! {
		pub struct XcmBuilder<Call>(Vec<Instruction<Call>>);
		impl<Call> Xcm<Call> {
			pub fn builder() -> XcmBuilder<Call> {
				XcmBuilder::<Call>(Vec::new())
			}
		}
		#builder_impl
	};
	output.into()
}

fn generate_methods_for_enum(name: syn::Ident, data_enum: &syn::DataEnum) -> TokenStream2 {
	let methods = data_enum.variants.iter().map(|variant| {
		let variant_name = &variant.ident;
		let method_name_string = to_snake_case(&variant_name.to_string());
		let method_name = syn::Ident::new(&method_name_string, variant_name.span());
		match &variant.fields {
			Fields::Unit => {
				quote! {
					pub fn #method_name(mut self) -> Self {
						self.0.push(#name::<Call>::#variant_name);
						self
					}
				}
			},
			Fields::Unnamed(fields) => {
				let arg_names: Vec<_> = fields.unnamed.iter().enumerate()
					.map(|(index, _)| format_ident!("arg{}", index))
					.collect();
				let arg_types: Vec<_> = fields.unnamed.iter().map(|field| &field.ty)
					.collect();
				quote! {
					pub fn #method_name(mut self, #(#arg_names: #arg_types),*) -> Self {
						self.0.push(#name::<Call>::#variant_name(#(#arg_names),*));
						self
					}
				}
			},
			Fields::Named(fields) => {
	            let arg_names: Vec<_> = fields.named.iter().map(|field| &field.ident).collect();
                let arg_types: Vec<_> = fields.named.iter().map(|field| &field.ty).collect();
				quote! {
					pub fn #method_name(mut self, #(#arg_names: #arg_types),*) -> Self {
						self.0.push(#name::<Call>::#variant_name { #(#arg_names),* });
						self
					}
				}
			},
		}
	});
	let output = quote! {
		impl<Call> XcmBuilder<Call> {
			#(#methods)*

			pub fn build(self) -> Xcm<Call> {
				Xcm(self.0)
			}
		}
	};
	output
}

fn to_snake_case(s: &str) -> String {
    let mut snake = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        // If it's an uppercase character and not the start
        if ch.is_uppercase() && !snake.is_empty() {
            // If the next character is lowercase, prepend an underscore
            if chars.peek().map_or(false, |next| next.is_lowercase()) {
                snake.push('_');
            }
        }

        snake.extend(ch.to_lowercase());
    }

    snake
}
