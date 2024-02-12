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
#![no_std]

pub use uapi::{HostFn, HostFnImpl as api};

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
	#[cfg(target_arch = "wasm32")]
	core::arch::wasm32::unreachable();

	#[cfg(target_arch = "riscv32")]
	// Safety: The unimp instruction is guaranteed to trap
	unsafe {
		core::arch::asm!("unimp");
		core::hint::unreachable_unchecked();
	}
}

/// Utility macro to read input passed to a contract.
///
/// Example:
///
/// ```
/// input$!(
/// 		var1: u32,      // [0, 4)   var1 decoded as u32
/// 		var2: [u8; 32], // [4, 36)  var2 decoded as a [u8] slice
/// 		var3: u8,       // [36, 37) var3 decoded as a u8
/// );
///
/// // Input and size can be specified as well:
/// input$!(
/// 		input,      // input buffer (optional)
/// 		512,        // input size (optional)
/// 		var4: u32,  // [0, 4)  var4 decoded as u32
/// 		var5: [u8], // [4, ..) var5 decoded as a [u8] slice
/// );
/// ```
#[macro_export]
macro_rules! input {
	(@inner $input:expr, $cursor:expr,) => {};
	(@size $size:expr, ) => { $size };

	// Match a u8 variable.
	// e.g input!(var1: u8, );
	(@inner $input:expr, $cursor:expr, $var:ident: u8, $($rest:tt)*) => {
		let $var = $input[$cursor];
		input!(@inner $input, $cursor + 1, $($rest)*);
	};

	// Size of u8 variable.
	(@size $size:expr, $var:ident: u8, $($rest:tt)*) => {
		input!(@size $size + 1, $($rest)*)
	};

	// Match a u64 variable.
	// e.g input!(var1: u64, );
	(@inner $input:expr, $cursor:expr, $var:ident: u64, $($rest:tt)*) => {
		let $var = u64::from_le_bytes($input[$cursor..$cursor + 8].try_into().unwrap());
		input!(@inner $input, $cursor + 8, $($rest)*);
	};

	// Size of u64 variable.
	(@size $size:expr, $var:ident: u64, $($rest:tt)*) => {
		input!(@size $size + 8, $($rest)*)
	};

	// Match a u32 variable.
	// e.g input!(var1: u32, );
	(@inner $input:expr, $cursor:expr, $var:ident: u32, $($rest:tt)*) => {
		let $var = u32::from_le_bytes($input[$cursor..$cursor + 4].try_into().unwrap());
		input!(@inner $input, $cursor + 4, $($rest)*);
	};

	// Size of u32 variable.
	(@size $size:expr, $var:ident: u32, $($rest:tt)*) => {
		input!(@size $size + 4, $($rest)*)
	};

	// Match a u8 slice with the remaining bytes.
	// e.g input!(512, var1: [u8; 32], var2: [u8], );
	(@inner $input:expr, $cursor:expr, $var:ident: [u8],) => {
		let $var = &$input[$cursor..];
	};

	// Match a u8 slice of the given size.
	// e.g input!(var1: [u8; 32], );
	(@inner $input:expr, $cursor:expr, $var:ident: [u8; $n:expr], $($rest:tt)*) => {
		let $var = &$input[$cursor..$cursor+$n];
		input!(@inner $input, $cursor + $n, $($rest)*);
	};

	// Size of a u8 slice.
	(@size $size:expr, $var:ident: [u8; $n:expr], $($rest:tt)*) => {
		input!(@size $size + $n, $($rest)*)
	};

	// Entry point, with the buffer and it's size specified first.
	// e.g input!(buffer, 512, var1: u32, var2: [u8], );
	($buffer:ident, $size:expr, $($rest:tt)*) => {
		let mut $buffer = [0u8; $size];
		let $buffer = &mut &mut $buffer[..];
		$crate::api::input($buffer);
		input!(@inner $buffer, 0, $($rest)*);
	};

	// Entry point, with the name of the buffer specified and size of the input buffer computed.
	// e.g input!(buffer, var1: u32, var2: u64, );
	($buffer: ident, $($rest:tt)*) => {
		input!($buffer, input!(@size 0, $($rest)*), $($rest)*);
	};

	// Entry point, with the size of the input buffer computed.
	// e.g input!(var1: u32, var2: u64, );
	($($rest:tt)*) => {
		input!(buffer, $($rest)*);
	};
}

/// Utility macro to invoke a host function that expect a `output: &mut &mut [u8]` as last argument.
///
/// Example:
/// ```
/// // call `api::caller` and store the output in `caller`
/// output!(caller, [0u8; 32], api::caller,);
///
/// // call `api::get_storage` and store the output in `address`
/// output!(address, [0u8; 32], api::get_storage, &[1u8; 32]);
/// ```
#[macro_export]
macro_rules! output {
	($output: ident, $buffer: expr, $host_fn:path, $($arg:expr),*) => {
		let mut $output = $buffer;
		let $output = &mut &mut $output[..];
		$host_fn($($arg,)* $output);
	};
}
