// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! Traits required by the runtime interface from the host side.

use crate::RIType;

use sp_wasm_interface::{FunctionContext, Result};

/// Something that can be converted into a ffi value.
pub trait IntoFFIValue: RIType {
	/// Convert `self` into a ffi value.
	fn into_ffi_value(self, context: &mut dyn FunctionContext) -> Result<Self::FFIType>;
}

/// Something that can be converted into a preallocated ffi value.
///
/// Every type parameter that should be given as `&mut` into a runtime interface function, needs
/// to implement this trait. After executing the host implementation of the runtime interface
/// function, the value is copied into the preallocated wasm memory.
///
/// This should only be used for types which have a fixed size, like slices. Other types like a vec
/// do not work with this interface, as we can not call into wasm to reallocate memory. So, this
/// trait should be implemented carefully.
pub trait IntoPreallocatedFFIValue: RIType {
	/// As `Self` can be an unsized type, it needs to be represented by a sized type at the host.
	/// This `SelfInstance` is the sized type.
	type SelfInstance;

	/// Convert `self_instance` into the given preallocated ffi value.
	fn into_preallocated_ffi_value(
		self_instance: Self::SelfInstance,
		context: &mut dyn FunctionContext,
		allocated: Self::FFIType,
	) -> Result<()>;
}

/// Something that can be created from a ffi value.
pub trait FromFFIValue: RIType {
	/// As `Self` can be an unsized type, it needs to be represented by a sized type at the host.
	/// This `SelfInstance` is the sized type.
	type SelfInstance;

	/// Create `SelfInstance` from the given
	fn from_ffi_value(
		context: &mut dyn FunctionContext,
		arg: Self::FFIType,
	) -> Result<Self::SelfInstance>;
}
