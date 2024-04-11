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

use frame_support::derive_impl;

trait Shape {
	fn area(&self) -> u32;
}

struct SomeRectangle {}

#[frame_support::register_default_impl(SomeRectangle)]
impl Shape for SomeRectangle {
	#[cfg(not(feature = "feature-frame-testing"))]
	fn area(&self) -> u32 {
		10
	}

	#[cfg(feature = "feature-frame-testing")]
	fn area(&self) -> u32 {
		0
	}
}

struct SomeSquare {}

#[derive_impl(SomeRectangle)]
impl Shape for SomeSquare {}

#[test]
fn test_feature_parsing() {
	let square = SomeSquare {};
	#[cfg(not(feature = "feature-frame-testing"))]
	assert_eq!(square.area(), 10);

	#[cfg(feature = "feature-frame-testing")]
	assert_eq!(square.area(), 0);
}
