// Copyright (C) Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! External C API to communicate with substrate contracts runtime module.
//!
//! Refer to substrate FRAME contract module for more documentation.

#![cfg_attr(not(feature = "std"), no_std)]
use core::marker::PhantomData;
use scale::Encode;

#[cfg(not(feature = "std"))]
#[allow(unused_variables)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
	// This code gets removed in release builds where the macro will expand into nothing.
	debug_print!("{}\n", info);

	cfg_if::cfg_if! {
		if #[cfg(target_arch = "wasm32")] {
			core::arch::wasm32::unreachable();
		} else if #[cfg(target_arch = "riscv32")] {
			// Safety: The unimp instruction is guaranteed to trap
			unsafe {
				core::arch::asm!("unimp");
				core::hint::unreachable_unchecked();
			}
		} else {
			core::compile_error!("ink! only supports wasm32 and riscv32");
		}
	}
}

cfg_if::cfg_if! {
	if #[cfg(target_arch = "wasm32")] {
		mod wasm32;
		pub use wasm32::*;
	} else if #[cfg(target_arch = "riscv32")] {
		mod riscv32;
		pub use riscv32::*;
	}
}

macro_rules! define_error_codes {
    (
        $(
            $( #[$attr:meta] )*
            $name:ident = $discr:literal,
        )*
    ) => {
        /// Every error that can be returned to a contract when it calls any of the host functions.
        #[repr(u32)]
        pub enum Error {
            $(
                $( #[$attr] )*
                $name = $discr,
            )*
            /// Returns if an unknown error was received from the host module.
            Unknown,
        }

        impl From<ReturnCode> for Result {
            #[inline]
            fn from(return_code: ReturnCode) -> Self {
                match return_code.0 {
                    0 => Ok(()),
                    $(
                        $discr => Err(Error::$name),
                    )*
                    _ => Err(Error::Unknown),
                }
            }
        }
    };
}

define_error_codes! {
	/// The called function trapped and has its state changes reverted.
	/// In this case no output buffer is returned.
	/// Can only be returned from `call` and `instantiate`.
	CalleeTrapped = 1,
	/// The called function ran to completion but decided to revert its state.
	/// An output buffer is returned when one was supplied.
	/// Can only be returned from `call` and `instantiate`.
	CalleeReverted = 2,
	/// The passed key does not exist in storage.
	KeyNotFound = 3,
	/// Deprecated and no longer returned: There is only the minimum balance.
	_BelowSubsistenceThreshold = 4,
	/// Transfer failed for other not further specified reason. Most probably
	/// reserved or locked balance of the sender that was preventing the transfer.
	TransferFailed = 5,
	/// Deprecated and no longer returned: Endowment is no longer required.
	_EndowmentTooLow = 6,
	/// No code could be found at the supplied code hash.
	CodeNotFound = 7,
	/// The account that was called is no contract.
	NotCallable = 8,
	/// The call to `debug_message` had no effect because debug message
	/// recording was disabled.
	LoggingDisabled = 9,
	/// The call dispatched by `call_runtime` was executed but returned an error.
	CallRuntimeFailed = 10,
	/// ECDSA public key recovery failed. Most probably wrong recovery id or signature.
	EcdsaRecoveryFailed = 11,
}

/// The flags to indicate further information about the end of a contract execution.
#[derive(Default)]
pub struct ReturnFlags {
	value: u32,
}

impl ReturnFlags {
	/// Initialize [`ReturnFlags`] with the reverted flag.
	pub fn new_with_reverted(has_reverted: bool) -> Self {
		Self::default().set_reverted(has_reverted)
	}

	/// Sets the bit to indicate that the execution is going to be reverted.
	#[must_use]
	pub fn set_reverted(mut self, has_reverted: bool) -> Self {
		match has_reverted {
			true => self.value |= has_reverted as u32,
			false => self.value &= !has_reverted as u32,
		}
		self
	}

	/// Returns the underlying `u32` representation.
	#[cfg(not(feature = "std"))]
	pub(crate) fn into_u32(self) -> u32 {
		self.value
	}
}

/// Thin-wrapper around a `u32` representing a pointer for Wasm32.
///
/// Only for shared references.
///
/// # Note
///
/// Can only be constructed from shared reference types and encapsulates the
/// conversion from reference to raw `u32`.
/// Does not allow accessing the internal `u32` value.
#[derive(Debug, Encode)]
#[repr(transparent)]
pub struct Ptr32<'a, T>
where
	T: ?Sized,
{
	/// The internal Wasm32 raw pointer value.
	///
	/// Must not be readable or directly usable by any safe Rust code.
	_value: u32,
	/// We handle types like these as if the associated lifetime was exclusive.
	marker: PhantomData<fn() -> &'a T>,
}

impl<'a, T> Ptr32<'a, T>
where
	T: ?Sized,
{
	/// Creates a new Wasm32 pointer for the given raw pointer value.
	fn new(value: u32) -> Self {
		Self { _value: value, marker: Default::default() }
	}
}

impl<'a, T> Ptr32<'a, [T]> {
	/// Creates a new Wasm32 pointer from the given shared slice.
	pub fn from_slice(slice: &'a [T]) -> Self {
		Self::new(slice.as_ptr() as u32)
	}
}

/// Thin-wrapper around a `u32` representing a pointer for Wasm32.
///
/// Only for exclusive references.
///
/// # Note
///
/// Can only be constructed from exclusive reference types and encapsulates the
/// conversion from reference to raw `u32`.
/// Does not allow accessing the internal `u32` value.
#[derive(Debug, Encode)]
#[repr(transparent)]
pub struct Ptr32Mut<'a, T>
where
	T: ?Sized,
{
	/// The internal Wasm32 raw pointer value.
	///
	/// Must not be readable or directly usable by any safe Rust code.
	_value: u32,
	/// We handle types like these as if the associated lifetime was exclusive.
	marker: PhantomData<fn() -> &'a mut T>,
}

impl<'a, T> Ptr32Mut<'a, T>
where
	T: ?Sized,
{
	/// Creates a new Wasm32 pointer for the given raw pointer value.
	fn new(value: u32) -> Self {
		Self { _value: value, marker: Default::default() }
	}
}

impl<'a, T> Ptr32Mut<'a, [T]> {
	/// Creates a new Wasm32 pointer from the given exclusive slice.
	pub fn from_slice(slice: &'a mut [T]) -> Self {
		Self::new(slice.as_ptr() as u32)
	}
}

impl<'a, T> Ptr32Mut<'a, T>
where
	T: Sized,
{
	/// Creates a new Wasm32 pointer from the given exclusive reference.
	pub fn from_ref(a_ref: &'a mut T) -> Self {
		let a_ptr: *mut T = a_ref;
		Self::new(a_ptr as u32)
	}
}

/// The raw return code returned by the host side.
#[repr(transparent)]
pub struct ReturnCode(u32);

impl From<ReturnCode> for Option<u32> {
	fn from(code: ReturnCode) -> Self {
		/// Used as a sentinel value when reading and writing contract memory.
		///
		/// We use this value to signal `None` to a contract when only a primitive is
		/// allowed and we don't want to go through encoding a full Rust type.
		/// Using `u32::Max` is a safe sentinel because contracts are never
		/// allowed to use such a large amount of resources. So this value doesn't
		/// make sense for a memory location or length.
		const SENTINEL: u32 = u32::MAX;

		(code.0 < SENTINEL).then_some(code.0)
	}
}

impl ReturnCode {
	/// Returns the raw underlying `u32` representation.
	pub fn into_u32(self) -> u32 {
		self.0
	}
	/// Returns the underlying `u32` converted into `bool`.
	pub fn into_bool(self) -> bool {
		self.0.ne(&0)
	}
}

type Result = core::result::Result<(), Error>;

#[cfg(not(feature = "std"))]
#[inline(always)]
fn extract_from_slice(output: &mut &mut [u8], new_len: usize) {
	debug_assert!(new_len <= output.len());
	let tmp = core::mem::take(output);
	*output = &mut tmp[..new_len];
}

// is not recognizing its allocator and panic handler definitions.
#[cfg(not(any(feature = "std", feature = "no-allocator")))]
mod allocator;

mod prelude;

cfg_if::cfg_if! {
	if #[cfg(any(feature = "ink-debug", feature = "std"))] {
		/// Required by the `debug_print*` macros below, because there is no guarantee that
		/// contracts will have a direct `ink_prelude` dependency. In the future we could introduce
		/// an "umbrella" crate containing all the `ink!` crates which could also host these macros.
		#[doc(hidden)]
		pub use prelude::format;

		/// Appends a formatted string to the `debug_message` buffer if message recording is
		/// enabled in the contracts pallet and if the call is performed via RPC (**not** via an
		/// extrinsic). The `debug_message` buffer will be:
		///  - Returned to the RPC caller.
		///  - Logged as a `debug!` message on the Substrate node, which will be printed to the
		///    node console's `stdout` when the log level is set to `-lruntime::contracts=debug`.
		///
		/// # Note
		///
		/// This depends on the `debug_message` interface which requires the
		/// `"pallet-contracts/unstable-interface"` feature to be enabled in the target runtime.
		#[macro_export]
		macro_rules! debug_print {
			($($arg:tt)*) => ($crate::debug_message(&$crate::format!($($arg)*)));
		}

		/// Appends a formatted string to the `debug_message` buffer, as per [`debug_print`] but
		/// with a newline appended.
		///
		/// # Note
		///
		/// This depends on the `debug_message` interface which requires the
		/// `"pallet-contracts/unstable-interface"` feature to be enabled in the target runtime.
		#[macro_export]
		macro_rules! debug_println {
			() => ($crate::debug_print!("\n"));
			($($arg:tt)*) => (
				$crate::debug_print!("{}\n", $crate::format!($($arg)*));
			)
		}
	} else {
		#[macro_export]
		/// Debug messages disabled. Enable the `ink-debug` feature for contract debugging.
		macro_rules! debug_print {
			($($arg:tt)*) => ();
		}

		#[macro_export]
		/// Debug messages disabled. Enable the `ink-debug` feature for contract debugging.
		macro_rules! debug_println {
			() => ();
			($($arg:tt)*) => ();
		}
	}
}
