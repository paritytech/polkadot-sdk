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

use sp_runtime_proc_macro::*;

// This function is placed so it will conflict with any code generating a funcion with the same
// name.
fn _should_not_compile() {}

#[with_features_try_runtime_and_runtime_benchmarks(not(feature = "sp-runtime/runtime-benchmarks"))]
fn _should_not_compile() {}

#[with_features_try_runtime_and_runtime_benchmarks(not(feature = "sp-runtime/try-runtime"))]
fn _should_not_compile() {}

#[with_features_try_runtime_and_runtime_benchmarks(feature = "sp-runtime/try-runtime")]
fn should_compile1() {}

#[with_features_try_runtime_and_runtime_benchmarks(feature = "sp-runtime/runtime-benchmarks")]
fn should_compile2() {}

#[with_features_try_runtime_and_runtime_benchmarks(all(
	feature = "sp-runtime/runtime-benchmarks",
	feature = "sp-runtime/try-runtime"
))]
fn should_compile3() {}

#[with_features_try_runtime_and_runtime_benchmarks(any(
	not(feature = "sp-runtime/runtime-benchmarks"),
	feature = "sp-runtime/try-runtime"
))]
fn should_compile4() {}

#[with_features_try_runtime_and_runtime_benchmarks(any(
	feature = "sp-runtime/runtime-benchmarks",
	not(feature = "sp-runtime/try-runtime")
))]
fn should_compile5() {}

#[with_features_not_try_runtime_and_runtime_benchmarks(not(
	feature = "sp-runtime/runtime-benchmarks"
))]
fn _should_not_compile() {}

#[with_features_not_try_runtime_and_runtime_benchmarks(not(not(
	feature = "sp-runtime/try-runtime"
)))]
fn _should_not_compile() {}

#[with_features_not_try_runtime_and_runtime_benchmarks(not(feature = "sp-runtime/try-runtime"))]
fn should_compile6() {}

#[with_features_not_try_runtime_and_runtime_benchmarks(feature = "sp-runtime/runtime-benchmarks")]
fn should_compile7() {}

#[with_features_not_try_runtime_and_not_runtime_benchmarks(not(not(
	feature = "sp-runtime/runtime-benchmarks"
)))]
fn _should_not_compile() {}

#[with_features_not_try_runtime_and_not_runtime_benchmarks(not(not(
	feature = "sp-runtime/try-runtime"
)))]
fn _should_not_compile() {}

#[with_features_not_try_runtime_and_not_runtime_benchmarks(not(
	feature = "sp-runtime/try-runtime"
))]
fn should_compile8() {}

#[with_features_not_try_runtime_and_not_runtime_benchmarks(not(
	feature = "sp-runtime/runtime-benchmarks"
))]
fn should_compile9() {}

#[with_features_try_runtime_and_not_runtime_benchmarks(not(not(
	feature = "sp-runtime/runtime-benchmarks"
)))]
fn _should_not_compile() {}

#[with_features_try_runtime_and_not_runtime_benchmarks(not(feature = "sp-runtime/try-runtime"))]
fn _should_not_compile() {}

#[with_features_try_runtime_and_not_runtime_benchmarks(feature = "sp-runtime/try-runtime")]
fn should_compile10() {}

#[with_features_try_runtime_and_not_runtime_benchmarks(not(
	feature = "sp-runtime/runtime-benchmarks"
))]
fn should_compile11() {}

fn main() {
	should_compile1();
	should_compile2();
	should_compile3();
	should_compile4();
	should_compile5();
	should_compile6();
	should_compile7();
	should_compile8();
	should_compile9();
	should_compile10();
	should_compile11();
}
