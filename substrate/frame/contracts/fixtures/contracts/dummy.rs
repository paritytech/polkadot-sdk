#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[no_mangle]
pub fn deploy() {
    uapi::debug_message("contract deployed");
}

#[no_mangle]
pub fn call() {
	uapi::debug_message("contract called");
}
