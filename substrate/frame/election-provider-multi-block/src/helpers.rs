// This file is part of Substrate.

// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

//! Some helper functions/macros for this crate.

#[macro_export]
macro_rules! log {
	($level:tt, $pattern:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: $crate::LOG_PREFIX,
			concat!("[#{:?}] 🗳🗳🗳  ", $pattern), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

macro_rules! sublog {
	($level:tt, $sub_pallet:tt, $pattern:expr $(, $values:expr)* $(,)?) => {
		#[cfg(not(feature = "std"))]
		log!($level, $pattern $(, $values )*);
		#[cfg(feature = "std")]
		log::$level!(
			target: format!("{}::{}", $crate::LOG_PREFIX, $sub_pallet).as_ref(),
			concat!("[#{:?}] 🗳🗳🗳  ", $pattern), <frame_system::Pallet<T>>::block_number() $(, $values )*
		)
	};
}
