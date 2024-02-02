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

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Result, Token};

pub mod multilocation {
	use super::*;

	pub fn generate_conversion_functions(input: proc_macro::TokenStream) -> Result<TokenStream> {
		if !input.is_empty() {
			return Err(syn::Error::new(Span::call_site(), "No arguments expected"))
		}

		// Support up to 8 Parents in a tuple, assuming that most use cases don't go past 8 parents.
		let from_tuples = generate_conversion_from_tuples(8);
		let from_v3 = generate_conversion_from_v3();

		Ok(quote! {
			#from_tuples
			#from_v3
		})
	}

	fn generate_conversion_from_tuples(max_parents: u8) -> TokenStream {
		let mut from_tuples = (0..8usize)
			.map(|num_junctions| {
				let junctions =
					(0..=num_junctions).map(|_| format_ident!("Junction")).collect::<Vec<_>>();
				let idents =
					(0..=num_junctions).map(|i| format_ident!("j{}", i)).collect::<Vec<_>>();
				let variant = &format_ident!("X{}", num_junctions + 1);
				let array_size = num_junctions + 1;

				let mut from_tuple = quote! {
					impl From<( #(#junctions,)* )> for MultiLocation {
						fn from( ( #(#idents,)* ): ( #(#junctions,)* ) ) -> Self {
							MultiLocation { parents: 0, interior: Junctions::#variant( #(#idents),* ) }
						}
					}

					impl From<(u8, #(#junctions),*)> for MultiLocation {
						fn from( ( parents, #(#idents),* ): (u8, #(#junctions),* ) ) -> Self {
							MultiLocation { parents, interior: Junctions::#variant( #(#idents),* ) }
						}
					}

					impl From<(Ancestor, #(#junctions),*)> for MultiLocation {
						fn from( ( Ancestor(parents), #(#idents),* ): (Ancestor, #(#junctions),* ) ) -> Self {
							MultiLocation { parents, interior: Junctions::#variant( #(#idents),* ) }
						}
					}

					impl From<[Junction; #array_size]> for MultiLocation {
						fn from(j: [Junction; #array_size]) -> Self {
							let [#(#idents),*] = j;
							MultiLocation { parents: 0, interior: Junctions::#variant( #(#idents),* ) }
						}
					}
				};

				let from_parent_tuples = (1..=max_parents).map(|cur_parents| {
					let parents =
						(0..cur_parents).map(|_| format_ident!("Parent")).collect::<Vec<_>>();
					let underscores =
						(0..cur_parents).map(|_| Token![_](Span::call_site())).collect::<Vec<_>>();

					quote! {
						impl From<( #(#parents,)* #(#junctions),* )> for MultiLocation {
							fn from( (#(#underscores,)* #(#idents),*): ( #(#parents,)* #(#junctions),* ) ) -> Self {
								MultiLocation { parents: #cur_parents, interior: Junctions::#variant( #(#idents),* ) }
							}
						}
					}
				});

				from_tuple.extend(from_parent_tuples);
				from_tuple
			})
			.collect::<TokenStream>();

		let from_parent_junctions_tuples = (1..=max_parents).map(|cur_parents| {
			let parents = (0..cur_parents).map(|_| format_ident!("Parent")).collect::<Vec<_>>();
			let underscores =
				(0..cur_parents).map(|_| Token![_](Span::call_site())).collect::<Vec<_>>();

			quote! {
				impl From<( #(#parents,)* Junctions )> for MultiLocation {
					fn from( (#(#underscores,)* junctions): ( #(#parents,)* Junctions ) ) -> Self {
						MultiLocation { parents: #cur_parents, interior: junctions }
					}
				}
			}
		});
		from_tuples.extend(from_parent_junctions_tuples);

		quote! {
			impl From<Junctions> for MultiLocation {
				fn from(junctions: Junctions) -> Self {
					MultiLocation { parents: 0, interior: junctions }
				}
			}

			impl From<(u8, Junctions)> for MultiLocation {
				fn from((parents, interior): (u8, Junctions)) -> Self {
					MultiLocation { parents, interior }
				}
			}

			impl From<(Ancestor, Junctions)> for MultiLocation {
				fn from((Ancestor(parents), interior): (Ancestor, Junctions)) -> Self {
					MultiLocation { parents, interior }
				}
			}

			impl From<()> for MultiLocation {
				fn from(_: ()) -> Self {
					MultiLocation { parents: 0, interior: Junctions::Here }
				}
			}

			impl From<(u8,)> for MultiLocation {
				fn from((parents,): (u8,)) -> Self {
					MultiLocation { parents, interior: Junctions::Here }
				}
			}

			impl From<Junction> for MultiLocation {
				fn from(x: Junction) -> Self {
					MultiLocation { parents: 0, interior: Junctions::X1(x) }
				}
			}

			impl From<[Junction; 0]> for MultiLocation {
				fn from(_: [Junction; 0]) -> Self {
					MultiLocation { parents: 0, interior: Junctions::Here }
				}
			}

			#from_tuples
		}
	}

	fn generate_conversion_from_v3() -> TokenStream {
		let match_variants = (0..8u8)
			.map(|cur_num| {
				let num_ancestors = cur_num + 1;
				let variant = format_ident!("X{}", num_ancestors);
				let idents = (0..=cur_num).map(|i| format_ident!("j{}", i)).collect::<Vec<_>>();

				quote! {
					crate::v3::Junctions::#variant( #(#idents),* ) =>
						#variant( #( core::convert::TryInto::try_into(#idents)? ),* ),
				}
			})
			.collect::<TokenStream>();

		quote! {
			impl core::convert::TryFrom<crate::v3::Junctions> for Junctions {
				type Error = ();
				fn try_from(mut new: crate::v3::Junctions) -> core::result::Result<Self, ()> {
					use Junctions::*;
					Ok(match new {
						crate::v3::Junctions::Here => Here,
						#match_variants
					})
				}
			}
		}
	}
}

pub mod junctions {
	use super::*;

	pub fn generate_conversion_functions(input: proc_macro::TokenStream) -> Result<TokenStream> {
		if !input.is_empty() {
			return Err(syn::Error::new(Span::call_site(), "No arguments expected"))
		}

		let from_slice_syntax = generate_conversion_from_slice_syntax();

		Ok(quote! {
			#from_slice_syntax
		})
	}

	fn generate_conversion_from_slice_syntax() -> TokenStream {
		quote! {
			macro_rules! impl_junction {
				($count:expr, $variant:ident, ($($index:literal),+)) => {
					/// Additional helper for building junctions
					/// Useful for converting to future XCM versions
					impl From<[Junction; $count]> for Junctions {
						fn from(junctions: [Junction; $count]) -> Self {
							Self::$variant($(junctions[$index].clone()),*)
						}
					}
				};
			}

			impl_junction!(1, X1, (0));
			impl_junction!(2, X2, (0, 1));
			impl_junction!(3, X3, (0, 1, 2));
			impl_junction!(4, X4, (0, 1, 2, 3));
			impl_junction!(5, X5, (0, 1, 2, 3, 4));
			impl_junction!(6, X6, (0, 1, 2, 3, 4, 5));
			impl_junction!(7, X7, (0, 1, 2, 3, 4, 5, 6));
			impl_junction!(8, X8, (0, 1, 2, 3, 4, 5, 6, 7));
		}
	}
}
