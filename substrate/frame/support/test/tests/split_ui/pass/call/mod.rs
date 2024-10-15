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

use frame_support::pallet_macros::pallet_section;

#[pallet_section]
mod call {
	#[pallet::call]
	impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        pub fn noop0(origin: OriginFor<T>) -> DispatchResult {
            ensure_signed(origin)?;
            Ok(())
        }

        #[pallet::call_index(1)]
        pub fn noop1(origin: OriginFor<T>, _x: u64) -> DispatchResult {
            ensure_signed(origin)?;
            Ok(())
        }
        
        #[pallet::call_index(2)]
        pub fn noop2(origin: OriginFor<T>, _x: u64, _y: u64) -> DispatchResult {
            ensure_signed(origin)?;
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::feeless_if(|_origin: &OriginFor<T>| -> bool { true })]
        pub fn noop_feeless0(origin: OriginFor<T>) -> DispatchResult {
            ensure_signed(origin)?;
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::feeless_if(|_origin: &OriginFor<T>, x: &u64| -> bool { *x == 1 })]
        pub fn noop_feeless1(origin: OriginFor<T>, _x: u64) -> DispatchResult {
            ensure_signed(origin)?;
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::feeless_if(|_origin: &OriginFor<T>, x: &u64, y: &u64| -> bool { *x == *y })]
        pub fn noop_feeless2(origin: OriginFor<T>, _x: u64, _y: u64) -> DispatchResult {
            ensure_signed(origin)?;
            Ok(())
        }
	}
}
