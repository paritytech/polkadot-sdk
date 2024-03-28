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

use syn::parse_quote;

#[test]
fn test_parse_pallet_with_task_enum_missing_impl() {
	assert_pallet_parse_error! {
		#[manifest_dir("../../examples/basic")]
		#[error_regex("Missing `\\#\\[pallet::tasks_experimental\\]` impl")]
		#[frame_support::pallet]
		pub mod pallet {
			#[pallet::task_enum]
			pub enum Task<T: Config> {
				Something,
			}

			#[pallet::config]
			pub trait Config: frame_system::Config {}

			#[pallet::pallet]
			pub struct Pallet<T>(_);
		}
	}
}

#[test]
fn test_parse_pallet_with_task_enum_wrong_attribute() {
	assert_pallet_parse_error! {
		#[manifest_dir("../../examples/basic")]
		#[error_regex("expected one of")]
		#[frame_support::pallet]
		pub mod pallet {
			#[pallet::wrong_attribute]
			pub enum Task<T: Config> {
				Something,
			}

			#[pallet::task_list]
			impl<T: Config> frame_support::traits::Task for Task<T>
			where
				T: TypeInfo,
			{}

			#[pallet::config]
			pub trait Config: frame_system::Config {}

			#[pallet::pallet]
			pub struct Pallet<T>(_);
		}
	}
}

#[test]
fn test_parse_pallet_missing_task_enum() {
	assert_pallet_parses! {
		#[manifest_dir("../../examples/basic")]
		#[frame_support::pallet]
		pub mod pallet {
			#[pallet::tasks_experimental]
			#[cfg(test)] // aha, this means it's being eaten
			impl<T: Config> frame_support::traits::Task for Task<T>
			where
				T: TypeInfo,
			{}

			#[pallet::config]
			pub trait Config: frame_system::Config {}

			#[pallet::pallet]
			pub struct Pallet<T>(_);
		}
	};
}

#[test]
fn test_parse_pallet_task_list_in_wrong_place() {
	assert_pallet_parse_error! {
		#[manifest_dir("../../examples/basic")]
		#[error_regex("can only be used on items within an `impl` statement.")]
		#[frame_support::pallet]
		pub mod pallet {
			pub enum MyCustomTaskEnum<T: Config> {
				Something,
			}

			#[pallet::task_list]
			pub fn something() {
				println!("hey");
			}

			#[pallet::config]
			pub trait Config: frame_system::Config {}

			#[pallet::pallet]
			pub struct Pallet<T>(_);
		}
	}
}

#[test]
fn test_parse_pallet_manual_tasks_impl_without_manual_tasks_enum() {
	assert_pallet_parse_error! {
		#[manifest_dir("../../examples/basic")]
		#[error_regex(".*attribute must be attached to your.*")]
		#[frame_support::pallet]
		pub mod pallet {

			impl<T: Config> frame_support::traits::Task for Task<T>
			where
				T: TypeInfo,
			{
				type Enumeration = sp_std::vec::IntoIter<Task<T>>;

				fn iter() -> Self::Enumeration {
					sp_std::vec![Task::increment, Task::decrement].into_iter()
				}
			}

			#[pallet::config]
			pub trait Config: frame_system::Config {}

			#[pallet::pallet]
			pub struct Pallet<T>(_);
		}
	}
}

#[test]
fn test_parse_pallet_manual_task_enum_non_manual_impl() {
	assert_pallet_parses! {
		#[manifest_dir("../../examples/basic")]
		#[frame_support::pallet]
		pub mod pallet {
			pub enum MyCustomTaskEnum<T: Config> {
				Something,
			}

			#[pallet::tasks_experimental]
			impl<T: Config> frame_support::traits::Task for MyCustomTaskEnum<T>
			where
				T: TypeInfo,
			{}

			#[pallet::config]
			pub trait Config: frame_system::Config {}

			#[pallet::pallet]
			pub struct Pallet<T>(_);
		}
	};
}

#[test]
fn test_parse_pallet_non_manual_task_enum_manual_impl() {
	assert_pallet_parses! {
		#[manifest_dir("../../examples/basic")]
		#[frame_support::pallet]
		pub mod pallet {
			#[pallet::task_enum]
			pub enum MyCustomTaskEnum<T: Config> {
				Something,
			}

			impl<T: Config> frame_support::traits::Task for MyCustomTaskEnum<T>
			where
				T: TypeInfo,
			{}

			#[pallet::config]
			pub trait Config: frame_system::Config {}

			#[pallet::pallet]
			pub struct Pallet<T>(_);
		}
	};
}

#[test]
fn test_parse_pallet_manual_task_enum_manual_impl() {
	assert_pallet_parses! {
		#[manifest_dir("../../examples/basic")]
		#[frame_support::pallet]
		pub mod pallet {
			pub enum MyCustomTaskEnum<T: Config> {
				Something,
			}

			impl<T: Config> frame_support::traits::Task for MyCustomTaskEnum<T>
			where
				T: TypeInfo,
			{}

			#[pallet::config]
			pub trait Config: frame_system::Config {}

			#[pallet::pallet]
			pub struct Pallet<T>(_);
		}
	};
}

#[test]
fn test_parse_pallet_manual_task_enum_mismatch_ident() {
	assert_pallet_parses! {
		#[manifest_dir("../../examples/basic")]
		#[frame_support::pallet]
		pub mod pallet {
			pub enum WrongIdent<T: Config> {
				Something,
			}

			#[pallet::tasks_experimental]
			impl<T: Config> frame_support::traits::Task for MyCustomTaskEnum<T>
			where
				T: TypeInfo,
			{}

			#[pallet::config]
			pub trait Config: frame_system::Config {}

			#[pallet::pallet]
			pub struct Pallet<T>(_);
		}
	};
}
