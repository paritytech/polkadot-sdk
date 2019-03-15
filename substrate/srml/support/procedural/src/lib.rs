// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

// tag::description[]
//! Proc macro of Support code for the runtime.
// end::description[]

#![recursion_limit="256"]

extern crate proc_macro;

mod storage;

use proc_macro::TokenStream;

/// Declares strongly-typed wrappers around codec-compatible types in storage.
///
/// ## Example
///
/// ```nocompile
/// decl_storage! {
/// 	trait Store for Module<T: Trait> as Example {
/// 		Dummy get(dummy) config(): Option<T::Balance>;
/// 		Foo get(foo) config(): T::Balance;
/// 	}
/// }
/// ```
///
/// For now we implement a convenience trait with pre-specialised associated types, one for each
/// storage item. This allows you to gain access to publicly visible storage items from a
/// module type. Currently you must disambiguate by using `<Module as Store>::Item` rather than
/// the simpler `Module::Item`. Hopefully the rust guys with fix this soon.
///
/// An optional `GenesisConfig` struct for storage initialization can be defined, either specifically as in :
/// ```nocompile
/// decl_storage! {
/// 	trait Store for Module<T: Trait> as Example {
/// 	}
///		add_extra_genesis {
///			config(genesis_field): GenesisFieldType;
///			build(|_: &mut StorageOverlay, _: &mut ChildrenStorageOverlay, _: &GenesisConfig<T>| {
///			})
///		}
/// }
/// ```
/// or when at least one storage field requires default initialization (both `get` and `config` or `build`).
/// This struct can be expose as `Config` by `decl_runtime` macro.
///
/// ### Module with instances
///
/// `decl_storage!` macro support building modules with instances with the following syntax: (DefaultInstance type
/// is optionnal)
/// ```nocompile
/// trait Store for Module<T: Trait<I>, I: Instance=DefaultInstance> as Example {}
/// ```
///
/// Then the genesis config is generated with two generic parameter `GenesisConfig<T, I>`
/// and storages are now accessible using two generic parameters like:
/// `<Dummy<T, I>>::get()` or `Dummy::<T, I>::get()`
#[proc_macro]
pub fn decl_storage(input: TokenStream) -> TokenStream {
	storage::transformation::decl_storage_impl(input)
}
