use super::{Pallet as Derivatives, *};
use frame_benchmarking::v2::*;

pub struct Pallet<T: Config<I>, I: 'static = ()>(Derivatives<T, I>);

pub trait Config<I: 'static = ()>: super::Config<I> {
	fn original() -> OriginalOf<Self, I>;

	fn derivative_create_params() -> DerivativeCreateParamsOf<Self, I>;
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_derivative() -> Result<(), BenchmarkError> {
		let create_origin = <CreateOriginOf<T, I>>::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		let original = T::original();
		let params = T::derivative_create_params();

		#[extrinsic_call]
		_(create_origin as T::RuntimeOrigin, original, params);

		Ok(())
	}

	#[benchmark]
	fn destroy_derivative() -> Result<(), BenchmarkError> {
		let create_origin = <CreateOriginOf<T, I>>::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		let destroy_origin = <DestroyOriginOf<T, I>>::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		let original = T::original();
		let params = T::derivative_create_params();

		<Derivatives<T, I>>::create_derivative(create_origin, original.clone(), params)?;

		#[extrinsic_call]
		_(destroy_origin as T::RuntimeOrigin, original);

		Ok(())
	}
}
