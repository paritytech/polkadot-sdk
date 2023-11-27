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
use crate::SENTINEL;

pub fn extract_from_slice(output: &mut &mut [u8], new_len: usize) {
	debug_assert!(new_len <= output.len());
	let tmp = core::mem::take(output);
	*output = &mut tmp[..new_len];
}

pub fn ptr_len_or_sentinel(data: &mut Option<&mut [u8]>) -> (*mut u8, u32) {
	match data {
		Some(ref mut data) => (data.as_mut_ptr(), data.len() as _),
		None => (SENTINEL as _, 0),
	}
}

pub fn ptr_or_sentinel(data: &Option<&[u8]>) -> *const u8 {
	match data {
		Some(ref data) => data.as_ptr(),
		None => SENTINEL as _,
	}
}
