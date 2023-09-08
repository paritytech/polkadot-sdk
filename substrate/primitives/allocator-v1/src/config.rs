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

use core::alloc::GlobalAlloc;
use lol_alloc::{AssumeSingleThreaded, FreeListAllocator};

#[global_allocator]
pub static ALLOCATOR: AssumeSingleThreaded<FreeListAllocator> =
	unsafe { AssumeSingleThreaded::new(FreeListAllocator::new()) };

#[no_mangle]
unsafe fn alloc(size: usize) -> *mut u8 {
	// return 1 as * mut u8;
	ALLOCATOR.alloc(core::alloc::Layout::array::<u8>(size).unwrap())
}

// #[no_mangle]
// unsafe fn dealloc(ptr: *mut u8, size: usize) {
// 	ALLOCATOR.dealloc(ptr, core::alloc::Layout::array::<u8>(size).unwrap())
// }
//
// #[no_mangle]
// unsafe fn realloc(ptr: *mut u8, size: usize, new_size: usize) -> *mut u8 {
// 	ALLOCATOR.realloc(ptr, core::alloc::Layout::array::<u8>(size).unwrap(), new_size)
// }

// TODO: maybe it's better to rename this crate to `sp-runtime-abi`.
/// The dummy function represents the version of runtime ABI.
#[no_mangle]
fn v1() {
	// nop
}

// TODO: design a better error message for panic.
/// A default panic handler for WASM environment.
#[cfg(all(not(feature = "disable_panic_handler"), not(feature = "std")))]
#[panic_handler]
#[no_mangle]
pub fn panic(info: &core::panic::PanicInfo) -> ! {
	// let message = sp_std::alloc::format!("{}", info);
	// #[cfg(feature = "improved_panic_error_reporting")]
	// {
	// 	sp_io::panic_handler::abort_on_panic(&message);
	// }
	#[cfg(not(feature = "improved_panic_error_reporting"))]
	{
		// sp_io::logging::log(LogLevel::Error, "runtime", message.as_bytes());
		core::arch::wasm32::unreachable();
	}
}


/// A default OOM handler for WASM environment.
#[cfg(all(not(feature = "disable_oom"), enable_alloc_error_handler))]
#[alloc_error_handler]
pub fn oom(_layout: core::alloc::Layout) -> ! {
	// #[cfg(feature = "improved_panic_error_reporting")]
	// {
	// 	panic_handler::abort_on_panic("Runtime memory exhausted.");
	// }
	#[cfg(not(feature = "improved_panic_error_reporting"))]
	{
		// sp_io::logging::log(LogLevel::Error, "runtime", b"Runtime memory exhausted. Aborting");
		core::arch::wasm32::unreachable();
	}
}
