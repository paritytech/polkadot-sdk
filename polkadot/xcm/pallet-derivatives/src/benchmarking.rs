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
