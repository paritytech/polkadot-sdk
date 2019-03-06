// Copyright 2019 Parity Technologies (UK) Ltd.
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
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! All storage functions that are replaced by `validate_block` in the Substrate runtime.

use crate::WitnessData;
use rstd::{slice, ptr, cmp};

pub static mut STORAGE: Option<WitnessData> = None;
const STORAGE_SET_EXPECT: &str = "`STORAGE` needs to be set before calling this function.";

pub unsafe fn ext_get_allocated_storage(key_data: *const u8, key_len: u32, written_out: *mut u32) -> *mut u8 {
	let key = slice::from_raw_parts(key_data, key_len as usize);
	match STORAGE.as_mut().expect(STORAGE_SET_EXPECT).get_mut(key) {
		Some(value) => {
			*written_out = value.len() as u32;
			value.as_mut_ptr()
		},
		None => {
			*written_out = u32::max_value();
			ptr::null_mut()
		}
	}
}

pub unsafe fn ext_set_storage(key_data: *const u8, key_len: u32, value_data: *const u8, value_len: u32) {
	let key = slice::from_raw_parts(key_data, key_len as usize);
	let value = slice::from_raw_parts(value_data, value_len as usize);

	STORAGE.as_mut().map(|s| {
		s.insert(key.to_vec(), value.to_vec());
	});
}

pub unsafe fn ext_get_storage_into(key_data: *const u8, key_len: u32, value_data: *mut u8, value_len: u32, value_offset: u32) -> u32 {
	let key = slice::from_raw_parts(key_data, key_len as usize);
	let out_value = slice::from_raw_parts_mut(value_data, value_len as usize);

	match STORAGE.as_mut().expect(STORAGE_SET_EXPECT).get_mut(key) {
		Some(value) => {
			let value = &value[value_offset as usize..];
			let len = cmp::min(value_len as usize, value.len());
			out_value[..len].copy_from_slice(&value[..len]);
			len as u32
		},
		None => {
			u32::max_value()
		}
	}
}

pub unsafe fn ext_exists_storage(key_data: *const u8, key_len: u32) -> u32 {
	let key = slice::from_raw_parts(key_data, key_len as usize);

	if STORAGE.as_mut().expect(STORAGE_SET_EXPECT).contains_key(key) {
		1
	} else {
		0
	}
}

pub unsafe fn ext_clear_storage(key_data: *const u8, key_len: u32) {
	let key = slice::from_raw_parts(key_data, key_len as usize);

	STORAGE.as_mut().expect(STORAGE_SET_EXPECT).remove(key);
}