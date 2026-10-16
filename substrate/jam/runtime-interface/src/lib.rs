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

//! Declare and implement the host calls of a JAM runtime.
//!
//! A JAM runtime gets a flat, unversioned list of host calls, each pinned to a fixed PolkaVM
//! import index. The [`host_calls!`] macro declares them as bare `extern "C"` functions using
//! plain C types (`u32`, `i32`, `u64`, `i64`, `*const u8`, `*mut u8`); every marshalling of
//! richer types is done by the caller.
//!
//! # Grammar
//!
//! ```ignore
//! sp_jam_runtime_interface::host_calls! {
//! 	/// Doc comments.
//! 	#[index = 301]
//! 	pub fn ext_storage_read(
//! 		ctx: &mut dyn FunctionContext,
//! 		key: *const u8,
//! 		key_len: u64,
//! 	) -> i64 {
//! 		let key = ctx.read_memory(key, key_len)?;
//! 		// ...
//! 		Ok(-1)
//! 	}
//!
//! 	/// Runtime-only host call, it has no node side.
//! 	#[index = 0]
//! 	pub fn ext_pvm_gas() -> u64;
//! }
//! ```
//!
//! A host call with a node side takes the [`FunctionContext`] as first parameter `ctx`; it is not
//! part of the import. Its body returns `Result<_, String>`, an error traps the runtime.
//!
//! # What is emitted
//!
//! - Runtime side (`cfg(substrate_runtime)`): the `extern "C"` import of every host call, with
//!   `#[polkavm_import(index = N)]` on riscv64, re-exported with the given visibility.
//! - Node side (`cfg(not(substrate_runtime))`): for every host call with a body a unit struct of
//!   the same name whose `call` function runs the body. Pointers are guest addresses, the body
//!   accesses the guest memory through `ctx`. Each struct implements [`HostFunction`] and a
//!   `HostFunctions` type implements [`HostFunctions`] for all of them.
//! - Always: `pub const HOST_CALL_INDICES: &[(u32, &str)]` listing every host call.

#![no_std]

extern crate alloc;

#[cfg(not(substrate_runtime))]
mod context;
#[cfg(not(substrate_runtime))]
mod host_functions;
#[cfg(all(test, not(substrate_runtime)))]
mod tests;

#[cfg(not(substrate_runtime))]
pub use context::FunctionContext;
#[cfg(not(substrate_runtime))]
pub use host_functions::{HostFunction, HostFunctions, Registers, MAX_ARGS};

/// The PolkaVM ABI of the host-call imports.
#[cfg(all(target_arch = "riscv64", substrate_runtime))]
pub mod polkavm {
	pub use polkavm_derive::polkavm_import;

	#[polkavm_derive::polkavm_define_abi(allow_extra_input_registers)]
	pub mod polkavm_abi {}
}

#[doc(hidden)]
pub mod __private {
	pub use alloc::{string::String, vec::Vec};
}

/// Declares the host calls of a module.
///
/// See the [crate documentation](crate) for the grammar and the emitted items.
#[macro_export]
macro_rules! host_calls {
	// Host call with a node side.
	(@munch [$($acc:tt)*]
		$(#[doc = $doc:literal])*
		#[index = $idx:literal]
		$vis:vis fn $name:ident(
			$ctx:ident : &mut dyn $ctx_ty:path $(, $an:ident : $at:ty)* $(,)?
		) $(-> $ret:ty)? { $($body:tt)* }
		$($rest:tt)*
	) => {
		$crate::host_calls!(@munch [$($acc)* {
			[$($doc)*] $idx $vis $name ($($an: $at),*) ($($ret)?) [$ctx $ctx_ty { $($body)* }]
		}] $($rest)*);
	};
	// Runtime-only host call.
	(@munch [$($acc:tt)*]
		$(#[doc = $doc:literal])*
		#[index = $idx:literal]
		$vis:vis fn $name:ident($($an:ident : $at:ty),* $(,)?) $(-> $ret:ty)?;
		$($rest:tt)*
	) => {
		$crate::host_calls!(@munch [$($acc)* {
			[$($doc)*] $idx $vis $name ($($an: $at),*) ($($ret)?) []
		}] $($rest)*);
	};
	(@munch [$($entry:tt)*]) => {
		$crate::host_calls!(@emit $($entry)*);
	};
	(@munch [$($acc:tt)*] $($rest:tt)+) => {
		compile_error!("invalid host call declaration");
	};

	(@emit $({
		[$($doc:literal)*] $idx:literal $vis:vis $name:ident ($($an:ident : $at:ty),*)
		($($ret:ty)?) [$($node:tt)*]
	})*) => {
		#[cfg(substrate_runtime)]
		mod __jam_host_calls {
			#[cfg(target_arch = "riscv64")]
			use $crate::polkavm::{polkavm_abi, polkavm_import};

			#[cfg(target_arch = "riscv64")]
			#[polkavm_import(abi = polkavm_abi)]
			extern "C" {
				$(
					$(#[doc = $doc])*
					#[polkavm_import(index = $idx)]
					pub fn $name($($an: $at),*) $(-> $ret)?;
				)*
			}

			#[cfg(not(target_arch = "riscv64"))]
			extern "C" {
				$(
					$(#[doc = $doc])*
					pub fn $name($($an: $at),*) $(-> $ret)?;
				)*
			}
		}

		$(
			#[cfg(substrate_runtime)]
			$vis use self::__jam_host_calls::$name;
		)*

		$(
			$crate::host_calls!(@node [$($doc)*] $vis $name ($($an: $at),*) ($($ret)?) $($node)*);
		)*

		/// The PolkaVM host-call indices of this module.
		pub const HOST_CALL_INDICES: &[(u32, &str)] = &[$(($idx, stringify!($name)),)*];

		/// The node side of this module's host calls.
		#[cfg(not(substrate_runtime))]
		pub struct HostFunctions;

		#[cfg(not(substrate_runtime))]
		impl $crate::HostFunctions for HostFunctions {
			fn host_functions() -> $crate::__private::Vec<&'static dyn $crate::HostFunction> {
				#[allow(unused_mut)]
				let mut host_functions: $crate::__private::Vec<&'static dyn $crate::HostFunction> =
					$crate::__private::Vec::new();
				$(
					$crate::host_calls!(@push host_functions $name $($node)*);
				)*
				host_functions
			}
		}
	};

	// Runtime-only host call: nothing on the node side.
	(@node [$($doc:literal)*] $vis:vis $name:ident ($($an:ident : $at:ty),*) ($($ret:ty)?)) => {};
	(@node [$($doc:literal)*] $vis:vis $name:ident ($($an:ident : $at:ty),*) ($($ret:ty)?)
		$ctx:ident $ctx_ty:path { $($body:tt)* }
	) => {
		$(#[doc = $doc])*
		#[cfg(not(substrate_runtime))]
		#[allow(non_camel_case_types)]
		$vis struct $name;

		#[cfg(not(substrate_runtime))]
		impl $name {
			/// Runs the node side of the host call.
			pub fn call(
				$ctx: &mut dyn $ctx_ty,
				$($an: $at),*
			) -> core::result::Result<$crate::host_calls!(@ret $($ret)?), $crate::__private::String> {
				let _ = &$ctx;
				$($body)*
			}
		}

		#[cfg(not(substrate_runtime))]
		impl $crate::HostFunction for $name {
			fn name(&self) -> &'static str {
				stringify!($name)
			}

			fn execute(
				&self,
				__ctx: &mut dyn $crate::FunctionContext,
				__regs: &dyn $crate::Registers,
			) -> core::result::Result<Option<u64>, $crate::__private::String> {
				let mut __index = 0;
				$(
					let $an = __regs.arg(__index) as $at;
					__index += 1;
				)*
				let _ = (__regs, __index);
				let __result = Self::call(__ctx, $($an),*)?;
				Ok($crate::host_calls!(@return_value __result $($ret)?))
			}
		}

		#[cfg(not(substrate_runtime))]
		const _: () = assert!(
			$crate::host_calls!(@args $($an)*) <= $crate::MAX_ARGS,
			concat!("host call '", stringify!($name), "' has too many arguments"),
		);
	};

	(@ret) => { () };
	(@ret $ret:ty) => { $ret };

	(@args $($an:ident)*) => { <[&str]>::len(&[$(stringify!($an)),*]) };

	(@return_value $result:ident) => {{
		let () = $result;
		None
	}};
	(@return_value $result:ident $ret:ty) => { Some($result as u64) };

	(@push $list:ident $name:ident) => {};
	(@push $list:ident $name:ident $ctx:ident $ctx_ty:path { $($body:tt)* }) => {
		$list.push(&$name);
	};

	($($input:tt)*) => {
		$crate::host_calls!(@munch [] $($input)*);
	};
}
