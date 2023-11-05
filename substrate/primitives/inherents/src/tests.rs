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

use super::*;

#[test]
fn inherent_order_ord_works() {
	// `First` is the first:
	for i in 0..10 {
		assert!(InherentOrder::First < InherentOrder::Index(i));
	}
	assert!(InherentOrder::First < InherentOrder::Last);
	assert!(InherentOrder::First == InherentOrder::First);

	// `Last` is the last:
	for i in 0..10 {
		assert!(InherentOrder::Last > InherentOrder::Index(i));
	}
	assert!(InherentOrder::Last == InherentOrder::Last);
	assert!(InherentOrder::Last > InherentOrder::First);

	// `Index` is ordered correctly:
	for i in 0..10 {
		for j in 0..10 {
			let a = InherentOrder::Index(i);
			let b = InherentOrder::Index(j);

			if i < j {
				assert!(a < b);
			} else if i > j {
				assert!(a > b);
			} else {
				assert!(a == b);
			}
		}
	}
}
