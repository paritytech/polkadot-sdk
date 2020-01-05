// Copyright 2017-2020 Parity Technologies (UK) Ltd.
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

//! Externalities extensions storage.
//!
//! Externalities support to register a wide variety custom extensions. The [`Extensions`] provides
//! some convenience functionality to store and retrieve these extensions.
//!
//! It is required that each extension implements the [`Extension`] trait.

use std::{collections::HashMap, any::{Any, TypeId}, ops::DerefMut};

/// Marker trait for types that should be registered as [`Externalities`](crate::Externalities) extension.
///
/// As extensions are stored as `Box<Any>`, this trait should give more confidence that the correct
/// type is registered and requested.
pub trait Extension: Send + Any {
	/// Return the extension as `&mut dyn Any`.
	///
	/// This is a trick to make the trait type castable into an `Any`.
	fn as_mut_any(&mut self) -> &mut dyn Any;
}

/// Macro for declaring an extension that usable with [`Extensions`].
///
/// The extension will be an unit wrapper struct that implements [`Extension`], `Deref` and
/// `DerefMut`. The wrapped type is given by the user.
///
/// # Example
/// ```
/// # use sp_externalities::decl_extension;
/// decl_extension! {
///     /// Some test extension
///     struct TestExt(String);
/// }
/// ```
#[macro_export]
macro_rules! decl_extension {
	(
		$( #[ $attr:meta ] )*
		$vis:vis struct $ext_name:ident ($inner:ty);
	) => {
		$( #[ $attr ] )*
		$vis struct $ext_name (pub $inner);

		impl $crate::Extension for $ext_name {
			fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
				self
			}
		}

		impl std::ops::Deref for $ext_name {
			type Target = $inner;

			fn deref(&self) -> &Self::Target {
				&self.0
			}
		}

		impl std::ops::DerefMut for $ext_name {
			fn deref_mut(&mut self) -> &mut Self::Target {
				&mut self.0
			}
		}
	}
}

/// Something that provides access to the [`Extensions`] store.
///
/// This is a super trait of the [`Externalities`](crate::Externalities).
pub trait ExtensionStore {
	/// Tries to find a registered extension by the given `type_id` and returns it as a `&mut dyn Any`.
	///
	/// It is advised to use [`ExternalitiesExt::extension`](crate::ExternalitiesExt::extension)
	/// instead of this function to get type system support and automatic type downcasting.
	fn extension_by_type_id(&mut self, type_id: TypeId) -> Option<&mut dyn Any>;
}

/// Stores extensions that should be made available through the externalities.
#[derive(Default)]
pub struct Extensions {
	extensions: HashMap<TypeId, Box<dyn Extension>>,
}

impl Extensions {
	/// Create new instance of `Self`.
	pub fn new() -> Self {
		Self::default()
	}

	/// Register the given extension.
	pub fn register<E: Extension>(&mut self, ext: E) {
		self.extensions.insert(ext.type_id(), Box::new(ext));
	}

	/// Return a mutable reference to the requested extension.
	pub fn get_mut(&mut self, ext_type_id: TypeId) -> Option<&mut dyn Any> {
		self.extensions.get_mut(&ext_type_id).map(DerefMut::deref_mut).map(Extension::as_mut_any)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	decl_extension! {
		struct DummyExt(u32);
	}
	decl_extension! {
		struct DummyExt2(u32);
	}

	#[test]
	fn register_and_retrieve_extension() {
		let mut exts = Extensions::new();
		exts.register(DummyExt(1));
		exts.register(DummyExt2(2));

		let ext = exts.get_mut(TypeId::of::<DummyExt>()).expect("Extension is registered");
		let ext_ty = ext.downcast_mut::<DummyExt>().expect("Downcasting works");

		assert_eq!(ext_ty.0, 1);
	}
}
