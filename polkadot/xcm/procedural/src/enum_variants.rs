// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Simple derive macro for getting the number of variants in an enum.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Error, Result};

pub fn derive(input: DeriveInput) -> Result<TokenStream2> {
	let data_enum = match &input.data {
		Data::Enum(data_enum) => data_enum,
		_ => return Err(Error::new_spanned(&input, "Expected an enum.")),
	};
	let ident = format_ident!("{}NumVariants", input.ident);
	let number_of_variants: usize = data_enum.variants.iter().count();
	Ok(quote! {
		pub struct #ident;
		impl ::frame_support::traits::Get<u32> for #ident {
			fn get() -> u32 {
				#number_of_variants as u32
			}
		}
	})
}
