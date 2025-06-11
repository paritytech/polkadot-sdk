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

//! Substrate runtime interface
//!
//! This crate provides types, traits and macros around runtime interfaces. A runtime interface is
//! a fixed interface between a Substrate runtime (also called the "guest") and a Substrate node
//! (also called the "host"). For a native runtime the interface maps to direct function calls of
//! the implementation. For a non-native runtime the interface maps to an external function call.
//! These external functions are exported by the runtime and they map to the same implementation
//! as the native calls, just with some extra code to marshal them through the FFI boundary.
//!
//! # Using a type in a runtime interface
//!
//! Every argument type and return type must be wrapped in a marker newtype specifying the
//! marshalling strategy used to pass the value through the FFI boundary between the host
//! and the runtime. The only exceptions to this rule are a couple of basic, primitive types
//! which can be passed directly through the FFI boundary and which don't require any special
//! handling besides a straightforward, direct conversion.
//!
//! You can find the strategy wrapper types in the [`crate::pass_by`] module.
//!
//! The newtype wrappers are automatically stripped away when the function is called
//! and applied when the function returns by the `runtime_interface` macro.
//!
//! # Declaring a runtime interface
//!
//! Declaring a runtime interface is similar to declaring a trait in Rust:
//!
//! ```
//! # mod wrapper {
//! # use sp_runtime_interface::pass_by::PassFatPointerAndRead;
//!
//! #[sp_runtime_interface::runtime_interface]
//! trait RuntimeInterface {
//!     fn some_function(value: PassFatPointerAndRead<&[u8]>) -> bool {
//!         value.iter().all(|v| *v > 125)
//!     }
//! }
//! # }
//! ```
//!
//! For more information on declaring a runtime interface, see
//! [`#[runtime_interface]`](./attr.runtime_interface.html).

#![no_std]

pub extern crate alloc;
extern crate self as sp_runtime_interface;

#[doc(hidden)]
#[cfg(not(substrate_runtime))]
pub use sp_wasm_interface;

#[doc(hidden)]
pub use sp_tracing;

#[doc(hidden)]
pub use sp_std;

/// Attribute macro for transforming a trait declaration into a runtime interface.
///
/// A runtime interface is a fixed interface between a Substrate compatible runtime and the
/// native node. This interface is callable from a native and a wasm runtime. The macro will
/// generate the corresponding code for the native implementation and the code for calling from
/// the wasm side to the native implementation.
///
/// The macro expects the runtime interface declaration as trait declaration:
///
/// ```
/// # mod wrapper {
/// # use sp_runtime_interface::runtime_interface;
/// # use sp_runtime_interface::pass_by::{PassFatPointerAndDecode, PassFatPointerAndRead, AllocateAndReturnFatPointer};
///
/// #[runtime_interface]
/// trait Interface {
///     /// A function that can be called from native/wasm.
///     ///
///     /// The implementation given to this function is only compiled on native.
///     fn call(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnFatPointer<Vec<u8>> {
///         // Here you could call some rather complex code that only compiles on native or
///         // is way faster in native than executing it in wasm.
///         Vec::new()
///     }
///     /// Call function, but different version.
///     ///
///     /// For new runtimes, only function with latest version is reachable.
///     /// But old version (above) is still accessible for old runtimes.
///     /// Default version is 1.
///     #[version(2)]
///     fn call(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnFatPointer<Vec<u8>> {
///         // Here you could call some rather complex code that only compiles on native or
///         // is way faster in native than executing it in wasm.
///         [17].to_vec()
///     }
///
///     /// Call function, different version and only being registered.
///     ///
///     /// This `register_only` version is only being registered, aka exposed to the runtime,
///     /// but the runtime will still use the version 2 of this function. This is useful for when
///     /// new host functions should be introduced. Adding new host functions requires that all
///     /// nodes have the host functions available, because otherwise they fail at instantiation
///     /// of the runtime. With `register_only` the function will not be used when compiling the
///     /// runtime, but it will already be there for a future version of the runtime that will
///     /// switch to using these host function.
///     #[version(3, register_only)]
///     fn call(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnFatPointer<Vec<u8>> {
///         // Here you could call some rather complex code that only compiles on native or
///         // is way faster in native than executing it in wasm.
///         [18].to_vec()
///     }
///
///     /// A function can take a `&self` or `&mut self` argument to get access to the
///     /// `Externalities`. (The generated method does not require
///     /// this argument, so the function can be called just with the `optional` argument)
///     fn set_or_clear(&mut self, optional: PassFatPointerAndDecode<Option<Vec<u8>>>) {
///         match optional {
///             Some(value) => self.set_storage([1, 2, 3, 4].to_vec(), value),
///             None => self.clear_storage(&[1, 2, 3, 4]),
///         }
///     }
///
///     /// A function can be gated behind a configuration (`cfg`) attribute.
///     /// To prevent ambiguity and confusion about what will be the final exposed host
///     /// functions list, conditionally compiled functions can't be versioned.
///     /// That is, conditionally compiled functions with `version`s greater than 1
///     /// are not allowed.
///     #[cfg(feature = "experimental-function")]
///     fn gated_call(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnFatPointer<Vec<u8>> {
///         [42].to_vec()
///     }
/// }
/// # }
/// ```
///
/// The given example will generate roughly the following code for native:
///
/// ```
/// // The name of the trait is converted to snake case and used as mod name.
/// //
/// // Be aware that this module is not `public`, the visibility of the module is determined based
/// // on the visibility of the trait declaration.
/// mod interface {
///     trait Interface {
///         fn call_version_1(data: &[u8]) -> Vec<u8>;
///         fn call_version_2(data: &[u8]) -> Vec<u8>;
///         fn call_version_3(data: &[u8]) -> Vec<u8>;
///         fn set_or_clear_version_1(&mut self, optional: Option<Vec<u8>>);
///         #[cfg(feature = "experimental-function")]
///         fn gated_call_version_1(data: &[u8]) -> Vec<u8>;
///     }
///
///     impl Interface for &mut dyn sp_externalities::Externalities {
///         fn call_version_1(data: &[u8]) -> Vec<u8> { Vec::new() }
///         fn call_version_2(data: &[u8]) -> Vec<u8> { [17].to_vec() }
///         fn call_version_3(data: &[u8]) -> Vec<u8> { [18].to_vec() }
///         fn set_or_clear_version_1(&mut self, optional: Option<Vec<u8>>) {
///             match optional {
///                 Some(value) => self.set_storage([1, 2, 3, 4].to_vec(), value),
///                 None => self.clear_storage(&[1, 2, 3, 4]),
///             }
///         }
///         #[cfg(feature = "experimental-function")]
///         fn gated_call_version_1(data: &[u8]) -> Vec<u8> { [42].to_vec() }
///     }
///
///     pub fn call(data: &[u8]) -> Vec<u8> {
///         // only latest version is exposed
///         call_version_2(data)
///     }
///
///     fn call_version_1(data: &[u8]) -> Vec<u8> {
///         <&mut dyn sp_externalities::Externalities as Interface>::call_version_1(data)
///     }
///
///     fn call_version_2(data: &[u8]) -> Vec<u8> {
///         <&mut dyn sp_externalities::Externalities as Interface>::call_version_2(data)
///     }
///
///     fn call_version_3(data: &[u8]) -> Vec<u8> {
///         <&mut dyn sp_externalities::Externalities as Interface>::call_version_3(data)
///     }
///
///     pub fn set_or_clear(optional: Option<Vec<u8>>) {
///         set_or_clear_version_1(optional)
///     }
///
///     fn set_or_clear_version_1(optional: Option<Vec<u8>>) {
///         sp_externalities::with_externalities(|mut ext| Interface::set_or_clear_version_1(&mut ext, optional))
///             .expect("`set_or_clear` called outside of an Externalities-provided environment.")
///     }
///
///     #[cfg(feature = "experimental-function")]
///     pub fn gated_call(data: &[u8]) -> Vec<u8> {
///         gated_call_version_1(data)
///     }
///
///     #[cfg(feature = "experimental-function")]
///     fn gated_call_version_1(data: &[u8]) -> Vec<u8> {
///         <&mut dyn sp_externalities::Externalities as Interface>::gated_call_version_1(data)
///     }
///
///     /// This type implements the `HostFunctions` trait (from `sp-wasm-interface`) and
///     /// provides the host implementation for the wasm side. The host implementation converts the
///     /// arguments from wasm to native and calls the corresponding native function.
///     ///
///     /// This type needs to be passed to the wasm executor, so that the host functions will be
///     /// registered in the executor.
///     pub struct HostFunctions;
/// }
/// ```
///
/// The given example will generate roughly the following code for wasm:
///
/// ```
/// mod interface {
///     mod extern_host_functions_impls {
///         /// Every function is exported by the native code as `ext_FUNCTION_NAME_version_VERSION`.
///         ///
///         /// The type for each argument of the exported function depends on
///         /// `<ARGUMENT_TYPE as RIType>::FFIType`.
///         ///
///         /// `key` holds the pointer and the length to the `data` slice.
///         pub fn call(data: &[u8]) -> Vec<u8> {
///             extern "C" { pub fn ext_call_version_2(key: u64); }
///             // Should call into external `ext_call_version_2(<[u8] as IntoFFIValue>::into_ffi_value(key))`
///             // But this is too much to replicate in a doc test so here we just return a dummy vector.
///             // Note that we jump into the latest version not marked as `register_only` (i.e. version 2).
///             Vec::new()
///         }
///
///         /// `key` holds the pointer and the length of the `option` value.
///         pub fn set_or_clear(option: Option<Vec<u8>>) {
///             extern "C" { pub fn ext_set_or_clear_version_1(key: u64); }
///             // Same as above
///         }
///
///         /// `key` holds the pointer and the length to the `data` slice.
///         #[cfg(feature = "experimental-function")]
///         pub fn gated_call(data: &[u8]) -> Vec<u8> {
///             extern "C" { pub fn ext_gated_call_version_1(key: u64); }
///             /// Same as above
///             Vec::new()
///         }
///     }
///
///     /// The type is actually `ExchangeableFunction` (from `sp-runtime-interface`) and
///     /// by default this is initialized to jump into the corresponding function in
///     /// `extern_host_functions_impls`.
///     ///
///     /// This can be used to replace the implementation of the `call` function.
///     /// Instead of calling into the host, the callee will automatically call the other
///     /// implementation.
///     ///
///     /// To replace the implementation:
///     ///
///     /// `host_call.replace_implementation(some_other_impl)`
///     pub static host_call: () = ();
///     pub static host_set_or_clear: () = ();
///     #[cfg(feature = "experimental-feature")]
///     pub static gated_call: () = ();
///
///     pub fn call(data: &[u8]) -> Vec<u8> {
///         // This is the actual call: `host_call.get()(data)`
///         //
///         // But that does not work for several reasons in this example, so we just return an
///         // empty vector.
///         Vec::new()
///     }
///
///     pub fn set_or_clear(optional: Option<Vec<u8>>) {
///         // Same as above
///     }
///
///     #[cfg(feature = "experimental-feature")]
///     pub fn gated_call(data: &[u8]) -> Vec<u8> {
///         // Same as above
///         Vec::new()
///     }
/// }
/// ```
///
/// # Argument and return types
///
/// Every argument type and return type must be wrapped in a marker newtype specifying the
/// marshalling strategy used to pass the value through the FFI boundary between the host
/// and the runtime. The only exceptions to this rule are a couple of basic, primitive types
/// which can be passed directly through the FFI boundary and which don't require any special
/// handling besides a straightforward, direct conversion.
///
/// The following table documents those types which can be passed between the host and the
/// runtime without a marshalling strategy wrapper:
///
/// | Type | FFI type | Conversion |
/// |----|----|----|
/// | `u8` | `u32` | zero-extended to 32-bits |
/// | `u16` | `u32` | zero-extended to 32-bits |
/// | `u32` | `u32` | `Identity` |
/// | `u64` | `u64` | `Identity` |
/// | `i8` | `i32` | sign-extended to 32-bits |
/// | `i16` | `i32` | sign-extended to 32-bits |
/// | `i32` | `i32` | `Identity` |
/// | `i64` | `i64` | `Identity` |
/// | `bool` | `u32` | `if v { 1 } else { 0 }` |
/// | `*const T` | `u32` | `Identity` |
///
/// `Identity` means that the value is passed as-is directly in a bit-exact fashion.
///
/// You can find the strategy wrapper types in the [`crate::pass_by`] module.
///
/// The newtype wrappers are automatically stripped away when the function is called
/// and applied when the function returns by the `runtime_interface` macro.
///
/// # Wasm only interfaces
///
/// Some interfaces are only required from within the wasm runtime e.g. the allocator
/// interface. To support this, the macro can be called like `#[runtime_interface(wasm_only)]`.
/// This instructs the macro to make two significant changes to the generated code:
///
/// 1. The generated functions are not callable from the native side.
/// 2. The trait as shown above is not implemented for [`Externalities`] and is instead
/// implemented for `FunctionContext` (from `sp-wasm-interface`).
///
/// # Disable tracing
/// By adding `no_tracing` to the list of options you can prevent the wasm-side interface from
/// generating the default `sp-tracing`-calls. Note that this is rarely needed but only meant
/// for the case when that would create a circular dependency. You usually _do not_ want to add
/// this flag, as tracing doesn't cost you anything by default anyways (it is added as a no-op)
/// but is super useful for debugging later.
pub use sp_runtime_interface_proc_macro::runtime_interface;

#[doc(hidden)]
#[cfg(not(substrate_runtime))]
pub use sp_externalities::{
	set_and_run_with_externalities, with_externalities, ExtensionStore, Externalities,
	ExternalitiesExt,
};

#[doc(hidden)]
pub use codec;

#[cfg(all(any(target_arch = "riscv32", target_arch = "riscv64"), substrate_runtime))]
pub mod polkavm;

#[cfg(not(substrate_runtime))]
pub mod host;
pub(crate) mod impls;
pub mod pass_by;
#[cfg(any(substrate_runtime, doc))]
pub mod wasm;

mod util;

pub use util::{pack_ptr_and_len, unpack_ptr_and_len};

/// Something that can be used by the runtime interface as type to communicate between the runtime
/// and the host.
///
/// Every type that should be used in a runtime interface function signature needs to implement
/// this trait.
pub trait RIType: Sized {
	/// The raw FFI type that is used to pass `Self` through the host <-> runtime boundary.
	#[cfg(not(substrate_runtime))]
	type FFIType: sp_wasm_interface::IntoValue
		+ sp_wasm_interface::TryFromValue
		+ sp_wasm_interface::WasmTy;

	#[cfg(substrate_runtime)]
	type FFIType;

	/// The inner type without any serialization strategy wrapper.
	type Inner;
}

/// A raw pointer that can be used in a runtime interface function signature.
#[cfg(substrate_runtime)]
pub type Pointer<T> = *mut T;

/// A raw pointer that can be used in a runtime interface function signature.
#[cfg(not(substrate_runtime))]
pub type Pointer<T> = sp_wasm_interface::Pointer<T>;
