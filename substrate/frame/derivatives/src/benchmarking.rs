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

use super::{Pallet as Derivatives, *};
use frame_benchmarking::v2::*;

pub struct Pallet<T: Config<I>, I: 'static = ()>(Derivatives<T, I>);

pub trait Config<I: 'static = ()>: super::Config<I> {
	fn max_original() -> OriginalOf<Self, I>;
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_derivative() -> Result<(), BenchmarkError> {
		let create_origin =
			T::CreateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		let original = T::max_original();

		#[extrinsic_call]
		_(create_origin as T::RuntimeOrigin, original);

		Ok(())
	}

	#[benchmark]
	fn destroy_derivative() -> Result<(), BenchmarkError> {
		let create_origin =
			T::CreateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		let destroy_origin =
			T::DestroyOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		let original = T::max_original();

		<Derivatives<T, I>>::create_derivative(create_origin, original.clone())?;

		#[extrinsic_call]
		_(destroy_origin as T::RuntimeOrigin, original);

		Ok(())
	}
}
