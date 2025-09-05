// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod blake2f;
mod bn128;
mod ecrecover;
mod identity;
mod modexp;
mod point_eval;
mod ripemd160;
mod sha256;
mod system;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(feature = "runtime-benchmarks")]
use crate::{
	precompiles::{ExtWithInfo, Instance, Precompiles},
	Config,
};

#[cfg(feature = "runtime-benchmarks")]
pub use self::{
	benchmarking::{IBenchmarking, NoInfo, WithInfo},
	system::{ISystem, System},
};

#[cfg(not(feature = "runtime-benchmarks"))]
pub type Builtin<T> = Production<T>;

#[cfg(feature = "runtime-benchmarks")]
pub type Builtin<T> = (Production<T>, Benchmarking<T>);

type Production<T> = (
	ecrecover::EcRecover<T>,
	sha256::Sha256<T>,
	ripemd160::Ripemd160<T>,
	identity::Identity<T>,
	modexp::Modexp<T>,
	bn128::Bn128Add<T>,
	bn128::Bn128Mul<T>,
	bn128::Bn128Pairing<T>,
	blake2f::Blake2F<T>,
	point_eval::PointEval<T>,
	system::System<T>,
);

#[cfg(feature = "runtime-benchmarks")]
type Benchmarking<T> = (benchmarking::WithInfo<T>, benchmarking::NoInfo<T>);

#[cfg(feature = "runtime-benchmarks")]
impl<T: Config> Precompiles<T> for (Production<T>, Benchmarking<T>) {
	const CHECK_COLLISION: () = ();
	const USES_EXTERNAL_RANGE: bool =
		Production::<T>::USES_EXTERNAL_RANGE || Benchmarking::<T>::USES_EXTERNAL_RANGE;

	fn code(address: &[u8; 20]) -> Option<&'static [u8]> {
		<Production<T>>::code(address).or_else(|| Benchmarking::<T>::code(address))
	}

	fn get<E: ExtWithInfo<T = T>>(address: &[u8; 20]) -> Option<Instance<E>> {
		let _ = <Self as Precompiles<T>>::CHECK_COLLISION;
		<Production<T>>::get(address).or_else(|| Benchmarking::<T>::get(address))
	}
}
