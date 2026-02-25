// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Provides implementations for the runtime interface types which can be
//! passed directly without any serialization strategy wrappers.

#[cfg(not(substrate_runtime))]
use crate::host::*;
#[cfg(substrate_runtime)]
use crate::wasm::*;
use crate::{Pointer, RIType};

#[cfg(not(substrate_runtime))]
use sp_wasm_interface::{FunctionContext, Result};

// Make sure that our assumptions for storing a pointer + its size in `u64` is valid.
#[cfg(all(substrate_runtime, not(feature = "disable_target_static_assertions")))]
const _: () = {
	assert!(core::mem::size_of::<usize>() == core::mem::size_of::<u32>());
	assert!(core::mem::size_of::<*const u8>() == core::mem::size_of::<u32>());
};

/// Implement the traits for the given primitive traits.
macro_rules! impl_traits_for_primitives {
	(
		$(
			$rty:ty, $fty:ty,
		)*
	) => {
		$(
			/// The type is passed directly.
			impl RIType for $rty {
				type FFIType = $fty;
				type Inner = Self;
			}

			#[cfg(substrate_runtime)]
			impl IntoFFIValue for $rty {
				type Destructor = ();

				fn into_ffi_value(value: &mut $rty) -> (Self::FFIType, Self::Destructor) {
					(*value as $fty, ())
				}
			}

			#[cfg(substrate_runtime)]
			impl FromFFIValue for $rty {
				fn from_ffi_value(arg: $fty) -> $rty {
					arg as $rty
				}
			}

			#[cfg(not(substrate_runtime))]
			impl<'a> FromFFIValue<'a> for $rty {
				type Owned = Self;

				fn from_ffi_value(_: &mut dyn FunctionContext, arg: $fty) -> Result<$rty> {
					Ok(arg as $rty)
				}

				fn take_from_owned(owned: &'a mut Self::Owned) -> Self::Inner {
					*owned
				}
			}

			#[cfg(not(substrate_runtime))]
			impl IntoFFIValue for $rty {
				fn into_ffi_value(value: Self::Inner, _: &mut dyn FunctionContext) -> Result<$fty> {
					Ok(value as $fty)
				}
			}
		)*
	}
}

impl_traits_for_primitives! {
	u8, u32,
	u16, u32,
	u32, u32,
	u64, u64,
	i8, i32,
	i16, i32,
	i32, i32,
	i64, i64,
}

/// `bool` is passed as `u32`.
///
/// - `1`: true
/// - `0`: false
impl RIType for bool {
	type FFIType = u32;
	type Inner = Self;
}

#[cfg(substrate_runtime)]
impl IntoFFIValue for bool {
	type Destructor = ();

	fn into_ffi_value(value: &mut bool) -> (Self::FFIType, Self::Destructor) {
		(if *value { 1 } else { 0 }, ())
	}
}

#[cfg(substrate_runtime)]
impl FromFFIValue for bool {
	fn from_ffi_value(arg: u32) -> bool {
		arg == 1
	}
}

#[cfg(not(substrate_runtime))]
impl<'a> FromFFIValue<'a> for bool {
	type Owned = Self;

	fn from_ffi_value(_: &mut dyn FunctionContext, arg: u32) -> Result<bool> {
		Ok(arg == 1)
	}

	fn take_from_owned(owned: &'a mut Self::Owned) -> Self::Inner {
		*owned
	}
}

#[cfg(not(substrate_runtime))]
impl IntoFFIValue for bool {
	fn into_ffi_value(value: Self, _: &mut dyn FunctionContext) -> Result<u32> {
		Ok(if value { 1 } else { 0 })
	}
}

#[cfg(not(substrate_runtime))]
impl<T: sp_wasm_interface::PointerType> RIType for Pointer<T> {
	type FFIType = u32;
	type Inner = Self;
}

/// The type is passed as `u32`.
#[cfg(substrate_runtime)]
impl<T> RIType for Pointer<T> {
	type FFIType = u32;
	type Inner = Self;
}

#[cfg(substrate_runtime)]
impl<T> IntoFFIValue for Pointer<T> {
	type Destructor = ();

	fn into_ffi_value(value: &mut Pointer<T>) -> (Self::FFIType, Self::Destructor) {
		(*value as u32, ())
	}
}

#[cfg(substrate_runtime)]
impl<T> FromFFIValue for Pointer<T> {
	fn from_ffi_value(arg: u32) -> Self {
		arg as _
	}
}

#[cfg(not(substrate_runtime))]
impl<'a, T: sp_wasm_interface::PointerType> FromFFIValue<'a> for Pointer<T> {
	type Owned = Self;

	fn from_ffi_value(_: &mut dyn FunctionContext, arg: u32) -> Result<Self> {
		Ok(Pointer::new(arg))
	}

	fn take_from_owned(owned: &'a mut Self::Owned) -> Self::Inner {
		*owned
	}
}

#[cfg(not(substrate_runtime))]
impl<T: sp_wasm_interface::PointerType> IntoFFIValue for Pointer<T> {
	fn into_ffi_value(value: Self, _: &mut dyn FunctionContext) -> Result<u32> {
		Ok(value.into())
	}
}
