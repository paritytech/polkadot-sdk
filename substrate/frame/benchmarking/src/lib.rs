// This file is part of Substrate.

// Copyright (C) 2020 Parity Technologies (UK) Ltd.
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

//! Macro for benchmarking a FRAME runtime.

#![cfg_attr(not(feature = "std"), no_std)]

mod tests;
mod utils;
#[cfg(feature = "std")]
mod analysis;

pub use utils::*;
#[cfg(feature = "std")]
pub use analysis::{Analysis, BenchmarkSelector};
#[doc(hidden)]
pub use sp_io::storage::root as storage_root;
pub use sp_runtime::traits::Zero;
pub use frame_support;
pub use paste;

/// Construct pallet benchmarks for weighing dispatchables.
///
/// Works around the idea of complexity parameters, named by a single letter (which is usually
/// upper cased in complexity notation but is lower-cased for use in this macro).
///
/// Complexity parameters ("parameters") have a range which is a `u32` pair. Every time a benchmark
/// is prepared and run, this parameter takes a concrete value within the range. There is an
/// associated instancing block, which is a single expression that is evaluated during
/// preparation. It may use `?` (`i.e. `return Err(...)`) to bail with a string error. Here's a
/// few examples:
///
/// ```ignore
/// // These two are equivalent:
/// let x in 0 .. 10;
/// let x in 0 .. 10 => ();
/// // This one calls a setup function and might return an error (which would be terminal).
/// let y in 0 .. 10 => setup(y)?;
/// // This one uses a code block to do lots of stuff:
/// let z in 0 .. 10 => {
///   let a = z * z / 5;
///   let b = do_something(a)?;
///   combine_into(z, b);
/// }
/// ```
///
/// Note that due to parsing restrictions, if the `from` expression is not a single token (i.e. a
/// literal or constant), then it must be parenthesised.
///
/// The macro allows for a number of "arms", each representing an individual benchmark. Using the
/// simple syntax, the associated dispatchable function maps 1:1 with the benchmark and the name of
/// the benchmark is the same as that of the associated function. However, extended syntax allows
/// for arbitrary expresions to be evaluated in a benchmark (including for example,
/// `on_initialize`).
///
/// The macro allows for common parameters whose ranges and instancing expressions may be drawn upon
/// (or not) by each arm. Syntax is available to allow for only the range to be drawn upon if
/// desired, allowing an alternative instancing expression to be given.
///
/// Note that the ranges are *inclusive* on both sides. This is in contrast to ranges in Rust which
/// are left-inclusive right-exclusive.
///
/// Each arm may also have a block of code which is run prior to any instancing and a block of code
/// which is run afterwards. All code blocks may draw upon the specific value of each parameter
/// at any time. Local variables are shared between the two pre- and post- code blocks, but do not
/// leak from the interior of any instancing expressions.
///
/// Any common parameters that are unused in an arm do not have their instancing expressions
/// evaluated.
///
/// Example:
/// ```ignore
/// benchmarks! {
///   where_clause {  where T::A: From<u32> } // Optional line to give additional bound on `T`.
///
///   // common parameter; just one for this example.
///   // will be `1`, `MAX_LENGTH` or any value inbetween
///   _ {
///     let l in 1 .. MAX_LENGTH => initialize_l(l);
///   }
///
///   // first dispatchable: foo; this is a user dispatchable and operates on a `u8` vector of
///   // size `l`, which we allow to be initialized as usual.
///   foo {
///     let caller = account::<T>(b"caller", 0, benchmarks_seed);
///     let l = ...;
///   }: _(Origin::Signed(caller), vec![0u8; l])
///
///   // second dispatchable: bar; this is a root dispatchable and accepts a `u8` vector of size
///   // `l`. We don't want it pre-initialized like before so we override using the `=> ()` notation.
///   // In this case, we explicitly name the call using `bar` instead of `_`.
///   bar {
///     let l = _ .. _ => ();
///   }: bar(Origin::Root, vec![0u8; l])
///
///   // third dispatchable: baz; this is a user dispatchable. It isn't dependent on length like the
///   // other two but has its own complexity `c` that needs setting up. It uses `caller` (in the
///   // pre-instancing block) within the code block. This is only allowed in the param instancers
///   // of arms. Instancers of common params cannot optimistically draw upon hypothetical variables
///   // that the arm's pre-instancing code block might have declared.
///   baz1 {
///     let caller = account::<T>(b"caller", 0, benchmarks_seed);
///     let c = 0 .. 10 => setup_c(&caller, c);
///   }: baz(Origin::Signed(caller))
///
///   // this is a second benchmark of the baz dispatchable with a different setup.
///   baz2 {
///     let caller = account::<T>(b"caller", 0, benchmarks_seed);
///     let c = 0 .. 10 => setup_c_in_some_other_way(&caller, c);
///   }: baz(Origin::Signed(caller))
///
///   // this is benchmarking some code that is not a dispatchable.
///   populate_a_set {
///     let x in 0 .. 10_000;
///     let mut m = Vec::<u32>::new();
///     for i in 0..x {
///       m.insert(i);
///     }
///   }: { m.into_iter().collect::<BTreeSet>() }
/// }
/// ```
///
/// Test functions are automatically generated for each benchmark and are accessible to you when you
/// run `cargo test`. All tests are named `test_benchmark_<benchmark_name>`, expect you to pass them
/// the Runtime Trait, and run them in a test externalities environment. The test function runs your
/// benchmark just like a regular benchmark, but only testing at the lowest and highest values for
/// each component. The function will return `Ok(())` if the benchmarks return no errors.
///
/// You can optionally add a `verify` code block at the end of a benchmark to test any final state
/// of your benchmark in a unit test. For example:
///
/// ```ignore
/// sort_vector {
/// 	let x in 1 .. 10000;
/// 	let mut m = Vec::<u32>::new();
/// 	for i in (0..x).rev() {
/// 		m.push(i);
/// 	}
/// }: {
/// 	m.sort();
/// } verify {
/// 	ensure!(m[0] == 0, "You forgot to sort!")
/// }
/// ```
///
/// These `verify` blocks will not execute when running your actual benchmarks!
///
/// You can construct benchmark tests like so:
///
/// ```ignore
/// #[test]
/// fn test_benchmarks() {
///   new_test_ext().execute_with(|| {
///     assert_ok!(test_benchmark_dummy::<Test>());
///     assert_err!(test_benchmark_other_name::<Test>(), "Bad origin");
///     assert_ok!(test_benchmark_sort_vector::<Test>());
///     assert_err!(test_benchmark_broken_benchmark::<Test>(), "You forgot to sort!");
///   });
/// }
/// ```
#[macro_export]
macro_rules! benchmarks {
	(
		$( where_clause { where $( $where_ty:ty: $where_bound:path ),* $(,)? } )?
		_ {
			$(
				let $common:ident in $common_from:tt .. $common_to:expr => $common_instancer:expr;
			)*
		}
		$( $rest:tt )*
	) => {
		$crate::benchmarks_iter!(
			NO_INSTANCE
			{ $( $( $where_ty: $where_bound ),* )? }
			{ $( { $common , $common_from , $common_to , $common_instancer } )* }
			( )
			$( $rest )*
		);
	}
}

/// Same as [`benchmarks`] but for instantiable module.
#[macro_export]
macro_rules! benchmarks_instance {
	(
		$( where_clause { where $( $where_ty:ty: $where_bound:path ),* $(,)? } )?
		_ {
			$(
				let $common:ident in $common_from:tt .. $common_to:expr => $common_instancer:expr;
			)*
		}
		$( $rest:tt )*
	) => {
		$crate::benchmarks_iter!(
			INSTANCE
			{ $( $( $where_ty: $where_bound ),* )? }
			{ $( { $common , $common_from , $common_to , $common_instancer } )* }
			( )
			$( $rest )*
		);
	}
}

#[macro_export]
#[doc(hidden)]
macro_rules! benchmarks_iter {
	// mutation arm:
	(
		$instance:ident
		{ $( $where_clause:tt )* }
		{ $( $common:tt )* }
		( $( $names:ident )* )
		$name:ident { $( $code:tt )* }: _ ( $origin:expr $( , $arg:expr )* )
		verify $postcode:block
		$( $rest:tt )*
	) => {
		$crate::benchmarks_iter! {
			$instance
			{ $( $where_clause )* }
			{ $( $common )* }
			( $( $names )* )
			$name { $( $code )* }: $name ( $origin $( , $arg )* )
			verify $postcode
			$( $rest )*
		}
	};
	// no instance mutation arm:
	(
		NO_INSTANCE
		{ $( $where_clause:tt )* }
		{ $( $common:tt )* }
		( $( $names:ident )* )
		$name:ident { $( $code:tt )* }: $dispatch:ident ( $origin:expr $( , $arg:expr )* )
		verify $postcode:block
		$( $rest:tt )*
	) => {
		$crate::benchmarks_iter! {
			NO_INSTANCE
			{ $( $where_clause )* }
			{ $( $common )* }
			( $( $names )* )
			$name { $( $code )* }: {
				<
					Call<T> as $crate::frame_support::traits::UnfilteredDispatchable
				>::dispatch_bypass_filter(Call::<T>::$dispatch($($arg),*), $origin.into())?;
			}
			verify $postcode
			$( $rest )*
		}
	};
	// instance mutation arm:
	(
		INSTANCE
		{ $( $where_clause:tt )* }
		{ $( $common:tt )* }
		( $( $names:ident )* )
		$name:ident { $( $code:tt )* }: $dispatch:ident ( $origin:expr $( , $arg:expr )* )
		verify $postcode:block
		$( $rest:tt )*
	) => {
		$crate::benchmarks_iter! {
			INSTANCE
			{ $( $where_clause )* }
			{ $( $common )* }
			( $( $names )* )
			$name { $( $code )* }: {
				<
					Call<T, I> as $crate::frame_support::traits::UnfilteredDispatchable
				>::dispatch_bypass_filter(Call::<T, I>::$dispatch($($arg),*), $origin.into())?;
			}
			verify $postcode
			$( $rest )*
		}
	};
	// iteration arm:
	(
		$instance:ident
		{ $( $where_clause:tt )* }
		{ $( $common:tt )* }
		( $( $names:ident )* )
		$name:ident { $( $code:tt )* }: $eval:block
		verify $postcode:block
		$( $rest:tt )*
	) => {
		$crate::benchmark_backend! {
			$instance
			$name
			{ $( $where_clause )* }
			{ $( $common )* }
			{ }
			{ $eval }
			{ $( $code )* }
			$postcode
		}

		#[cfg(test)]
		$crate::impl_benchmark_test!( { $( $where_clause )* } $instance $name );

		$crate::benchmarks_iter!(
			$instance
			{ $( $where_clause )* }
			{ $( $common )* }
			( $( $names )* $name )
			$( $rest )*
		);
	};
	// iteration-exit arm
	( $instance:ident { $( $where_clause:tt )* } { $( $common:tt )* } ( $( $names:ident )* ) ) => {
		$crate::selected_benchmark!( { $( $where_clause)* } $instance $( $names ),* );
		$crate::impl_benchmark!( { $( $where_clause )* } $instance $( $names ),* );
	};
	// add verify block to _() format
	(
		$instance:ident
		{ $( $where_clause:tt )* }
		{ $( $common:tt )* }
		( $( $names:ident )* )
		$name:ident { $( $code:tt )* }: _ ( $origin:expr $( , $arg:expr )* )
		$( $rest:tt )*
	) => {
		$crate::benchmarks_iter! {
			$instance
			{ $( $where_clause )* }
			{ $( $common )* }
			( $( $names )* )
			$name { $( $code )* }: _ ( $origin $( , $arg )* )
			verify { }
			$( $rest )*
		}
	};
	// add verify block to name() format
	(
		$instance:ident
		{ $( $where_clause:tt )* }
		{ $( $common:tt )* }
		( $( $names:ident )* )
		$name:ident { $( $code:tt )* }: $dispatch:ident ( $origin:expr $( , $arg:expr )* )
		$( $rest:tt )*
	) => {
		$crate::benchmarks_iter! {
			$instance
			{ $( $where_clause )* }
			{ $( $common )* }
			( $( $names )* )
			$name { $( $code )* }: $dispatch ( $origin $( , $arg )* )
			verify { }
			$( $rest )*
		}
	};
	// add verify block to {} format
	(
		$instance:ident
		{ $( $where_clause:tt )* }
		{ $( $common:tt )* }
		( $( $names:ident )* )
		$name:ident { $( $code:tt )* }: $eval:block
		$( $rest:tt )*
	) => {
		$crate::benchmarks_iter!(
			$instance
			{ $( $where_clause )* }
			{ $( $common )* }
			( $( $names )* )
			$name { $( $code )* }: $eval
			verify { }
			$( $rest )*
		);
	};
}

#[macro_export]
#[doc(hidden)]
macro_rules! benchmark_backend {
	// parsing arms
	($instance:ident $name:ident {
		$( $where_clause:tt )*
	} {
		$( $common:tt )*
	} {
		$( PRE { $( $pre_parsed:tt )* } )*
	} { $eval:block } {
			let $pre_id:tt : $pre_ty:ty = $pre_ex:expr;
			$( $rest:tt )*
	} $postcode:block) => {
		$crate::benchmark_backend! {
			$instance $name { $( $where_clause )* } { $( $common )* } {
				$( PRE { $( $pre_parsed )* } )*
				PRE { $pre_id , $pre_ty , $pre_ex }
			} { $eval } { $( $rest )* } $postcode
		}
	};
	($instance:ident $name:ident {
		$( $where_clause:tt )*
	} {
		$( $common:tt )*
	} {
		$( $parsed:tt )*
	} { $eval:block } {
		let $param:ident in ( $param_from:expr ) .. $param_to:expr => $param_instancer:expr;
		$( $rest:tt )*
	} $postcode:block) => {
		$crate::benchmark_backend! {
			$instance $name { $( $where_clause )* } { $( $common )* } {
				$( $parsed )*
				PARAM { $param , $param_from , $param_to , $param_instancer }
			} { $eval } { $( $rest )* } $postcode
		}
	};
	// mutation arm to look after defaulting to a common param
	($instance:ident $name:ident {
		$( $where_clause:tt )*
	} {
		$( { $common:ident , $common_from:tt , $common_to:expr , $common_instancer:expr } )*
	} {
		$( $parsed:tt )*
	} { $eval:block } {
		let $param:ident in ...;
		$( $rest:tt )*
	} $postcode:block) => {
		$crate::benchmark_backend! {
			$instance $name { $( $where_clause )* } {
				$( { $common , $common_from , $common_to , $common_instancer } )*
			} {
				$( $parsed )*
			} { $eval } {
				let $param
					in ({ $( let $common = $common_from; )* $param })
					.. ({ $( let $common = $common_to; )* $param })
					=> ({ $( let $common = || -> Result<(), &'static str> { $common_instancer ; Ok(()) }; )* $param()? });
				$( $rest )*
			} $postcode
		}
	};
	// mutation arm to look after defaulting only the range to common param
	($instance:ident $name:ident {
		$( $where_clause:tt )*
	} {
		$( { $common:ident , $common_from:tt , $common_to:expr , $common_instancer:expr } )*
	} {
		$( $parsed:tt )*
	} { $eval:block } {
		let $param:ident in _ .. _ => $param_instancer:expr ;
		$( $rest:tt )*
	} $postcode:block) => {
		$crate::benchmark_backend! {
			$instance $name { $( $where_clause )* } {
				$( { $common , $common_from , $common_to , $common_instancer } )*
			} {
				$( $parsed )*
			} { $eval } {
				let $param
					in ({ $( let $common = $common_from; )* $param })
					.. ({ $( let $common = $common_to; )* $param })
					=> $param_instancer ;
				$( $rest )*
			} $postcode
		}
	};
	// mutation arm to look after a single tt for param_from.
	($instance:ident $name:ident {
		$( $where_clause:tt )*
	} {
		$( $common:tt )*
	} {
		$( $parsed:tt )*
	} { $eval:block } {
		let $param:ident in $param_from:tt .. $param_to:expr => $param_instancer:expr ;
		$( $rest:tt )*
	} $postcode:block) => {
		$crate::benchmark_backend! {
			$instance $name { $( $where_clause )* } { $( $common )* } { $( $parsed )* } { $eval } {
				let $param in ( $param_from ) .. $param_to => $param_instancer;
				$( $rest )*
			} $postcode
		}
	};
	// mutation arm to look after the default tail of `=> ()`
	($instance:ident $name:ident {
		$( $where_clause:tt )*
	} {
		$( $common:tt )*
	} {
		$( $parsed:tt )*
	} { $eval:block } {
		let $param:ident in $param_from:tt .. $param_to:expr;
		$( $rest:tt )*
	} $postcode:block) => {
		$crate::benchmark_backend! {
			$instance $name { $( $where_clause )* } { $( $common )* } { $( $parsed )* } { $eval } {
				let $param in $param_from .. $param_to => ();
				$( $rest )*
			} $postcode
		}
	};
	// mutation arm to look after `let _ =`
	($instance:ident $name:ident {
		$( $where_clause:tt )*
	} {
		$( $common:tt )*
	} {
		$( $parsed:tt )*
	} { $eval:block } {
		let $pre_id:tt = $pre_ex:expr;
		$( $rest:tt )*
	} $postcode:block) => {
		$crate::benchmark_backend! {
			$instance $name { $( $where_clause )* } { $( $common )* } { $( $parsed )* } { $eval } {
				let $pre_id : _ = $pre_ex;
				$( $rest )*
			} $postcode
		}
	};
	// no instance actioning arm
	(NO_INSTANCE $name:ident {
		$( $where_clause:tt )*
	} {
		$( { $common:ident , $common_from:tt , $common_to:expr , $common_instancer:expr } )*
	} {
		$( PRE { $pre_id:tt , $pre_ty:ty , $pre_ex:expr } )*
		$( PARAM { $param:ident , $param_from:expr , $param_to:expr , $param_instancer:expr } )*
	} { $eval:block } { $( $post:tt )* } $postcode:block) => {
		#[allow(non_camel_case_types)]
		struct $name;
		#[allow(unused_variables)]
		impl<T: Trait> $crate::BenchmarkingSetup<T> for $name
			where $( $where_clause )*
		{
			fn components(&self) -> Vec<($crate::BenchmarkParameter, u32, u32)> {
				vec! [
					$(
						($crate::BenchmarkParameter::$param, $param_from, $param_to)
					),*
				]
			}

			fn instance(&self, components: &[($crate::BenchmarkParameter, u32)])
				-> Result<Box<dyn FnOnce() -> Result<(), &'static str>>, &'static str>
			{
				$(
					let $common = $common_from;
				)*
				$(
					// Prepare instance
					let $param = components.iter()
						.find(|&c| c.0 == $crate::BenchmarkParameter::$param)
						.unwrap().1;
				)*
				$(
					let $pre_id : $pre_ty = $pre_ex;
				)*
				$( $param_instancer ; )*
				$( $post )*

				Ok(Box::new(move || -> Result<(), &'static str> { $eval; Ok(()) }))
			}

			fn verify(&self, components: &[($crate::BenchmarkParameter, u32)])
				-> Result<Box<dyn FnOnce() -> Result<(), &'static str>>, &'static str>
			{
				$(
					let $common = $common_from;
				)*
				$(
					// Prepare instance
					let $param = components.iter()
						.find(|&c| c.0 == $crate::BenchmarkParameter::$param)
						.unwrap().1;
				)*
				$(
					let $pre_id : $pre_ty = $pre_ex;
				)*
				$( $param_instancer ; )*
				$( $post )*

				Ok(Box::new(move || -> Result<(), &'static str> { $eval; $postcode; Ok(()) }))
			}
		}
	};
	// instance actioning arm
	(INSTANCE $name:ident {
		$( $where_clause:tt )*
	} {
		$( { $common:ident , $common_from:tt , $common_to:expr , $common_instancer:expr } )*
	} {
		$( PRE { $pre_id:tt , $pre_ty:ty , $pre_ex:expr } )*
		$( PARAM { $param:ident , $param_from:expr , $param_to:expr , $param_instancer:expr } )*
	} { $eval:block } { $( $post:tt )* } $postcode:block) => {
		#[allow(non_camel_case_types)]
		struct $name;
		#[allow(unused_variables)]
		impl<T: Trait<I>, I: Instance> $crate::BenchmarkingSetupInstance<T, I> for $name
			where $( $where_clause )*
		{
			fn components(&self) -> Vec<($crate::BenchmarkParameter, u32, u32)> {
				vec! [
					$(
						($crate::BenchmarkParameter::$param, $param_from, $param_to)
					),*
				]
			}

			fn instance(&self, components: &[($crate::BenchmarkParameter, u32)])
				-> Result<Box<dyn FnOnce() -> Result<(), &'static str>>, &'static str>
			{
				$(
					let $common = $common_from;
				)*
				$(
					// Prepare instance
					let $param = components.iter()
						.find(|&c| c.0 == $crate::BenchmarkParameter::$param)
						.unwrap().1;
				)*
				$(
					let $pre_id : $pre_ty = $pre_ex;
				)*
				$( $param_instancer ; )*
				$( $post )*

				Ok(Box::new(move || -> Result<(), &'static str> { $eval; Ok(()) }))
			}

			fn verify(&self, components: &[($crate::BenchmarkParameter, u32)])
				-> Result<Box<dyn FnOnce() -> Result<(), &'static str>>, &'static str>
			{
				$(
					let $common = $common_from;
				)*
				$(
					// Prepare instance
					let $param = components.iter()
						.find(|&c| c.0 == $crate::BenchmarkParameter::$param)
						.unwrap().1;
				)*
				$(
					let $pre_id : $pre_ty = $pre_ex;
				)*
				$( $param_instancer ; )*
				$( $post )*

				Ok(Box::new(move || -> Result<(), &'static str> { $eval; $postcode; Ok(()) }))
			}
		}
	}
}

// Creates a `SelectedBenchmark` enum implementing `BenchmarkingSetup`.
//
// Every variant must implement [`BenchmarkingSetup`].
//
// ```nocompile
//
// struct Transfer;
// impl BenchmarkingSetup for Transfer { ... }
//
// struct SetBalance;
// impl BenchmarkingSetup for SetBalance { ... }
//
// selected_benchmark!(Transfer, SetBalance);
// ```
#[macro_export]
#[doc(hidden)]
macro_rules! selected_benchmark {
	(
		{ $( $where_clause:tt )* }
		NO_INSTANCE $( $bench:ident ),*
	) => {
		// The list of available benchmarks for this pallet.
		#[allow(non_camel_case_types)]
		enum SelectedBenchmark {
			$( $bench, )*
		}

		// Allow us to select a benchmark from the list of available benchmarks.
		impl<T: Trait> $crate::BenchmarkingSetup<T> for SelectedBenchmark
			where $( $where_clause )*
		{
			fn components(&self) -> Vec<($crate::BenchmarkParameter, u32, u32)> {
				match self {
					$( Self::$bench => <$bench as $crate::BenchmarkingSetup<T>>::components(&$bench), )*
				}
			}

			fn instance(&self, components: &[($crate::BenchmarkParameter, u32)])
				-> Result<Box<dyn FnOnce() -> Result<(), &'static str>>, &'static str>
			{
				match self {
					$( Self::$bench => <$bench as $crate::BenchmarkingSetup<T>>::instance(&$bench, components), )*
				}
			}

			fn verify(&self, components: &[($crate::BenchmarkParameter, u32)])
				-> Result<Box<dyn FnOnce() -> Result<(), &'static str>>, &'static str>
			{
				match self {
					$( Self::$bench => <$bench as $crate::BenchmarkingSetup<T>>::verify(&$bench, components), )*
				}
			}
		}
	};
	(
		{ $( $where_clause:tt )* }
		INSTANCE $( $bench:ident ),*
	) => {
		// The list of available benchmarks for this pallet.
		#[allow(non_camel_case_types)]
		enum SelectedBenchmark {
			$( $bench, )*
		}

		// Allow us to select a benchmark from the list of available benchmarks.
		impl<T: Trait<I>, I: Instance> $crate::BenchmarkingSetupInstance<T, I> for SelectedBenchmark
			where $( $where_clause )*
		{
			fn components(&self) -> Vec<($crate::BenchmarkParameter, u32, u32)> {
				match self {
					$( Self::$bench => <$bench as $crate::BenchmarkingSetupInstance<T, I>>::components(&$bench), )*
				}
			}

			fn instance(&self, components: &[($crate::BenchmarkParameter, u32)])
				-> Result<Box<dyn FnOnce() -> Result<(), &'static str>>, &'static str>
			{
				match self {
					$( Self::$bench => <$bench as $crate::BenchmarkingSetupInstance<T, I>>::instance(&$bench, components), )*
				}
			}

			fn verify(&self, components: &[($crate::BenchmarkParameter, u32)])
				-> Result<Box<dyn FnOnce() -> Result<(), &'static str>>, &'static str>
			{
				match self {
					$( Self::$bench => <$bench as $crate::BenchmarkingSetupInstance<T, I>>::verify(&$bench, components), )*
				}
			}
		}
	}
}

#[macro_export]
#[doc(hidden)]
macro_rules! impl_benchmark {
	(
		{ $( $where_clause:tt )* }
		NO_INSTANCE $( $name:ident ),*
	) => {
		impl<T: Trait> $crate::Benchmarking<$crate::BenchmarkResults> for Module<T>
			where T: frame_system::Trait, $( $where_clause )*
		{
			fn benchmarks() -> Vec<&'static [u8]> {
				vec![ $( stringify!($name).as_ref() ),* ]
			}

			fn run_benchmark(
				extrinsic: &[u8],
				lowest_range_values: &[u32],
				highest_range_values: &[u32],
				steps: &[u32],
				repeat: u32,
				whitelist: &[Vec<u8>]
			) -> Result<Vec<$crate::BenchmarkResults>, &'static str> {
				// Map the input to the selected benchmark.
				let extrinsic = sp_std::str::from_utf8(extrinsic)
					.map_err(|_| "`extrinsic` is not a valid utf8 string!")?;
				let selected_benchmark = match extrinsic {
					$( stringify!($name) => SelectedBenchmark::$name, )*
					_ => return Err("Could not find extrinsic."),
				};

				// Add whitelist to DB
				$crate::benchmarking::set_whitelist(whitelist.to_vec());

				// Warm up the DB
				$crate::benchmarking::commit_db();
				$crate::benchmarking::wipe_db();

				let components = <SelectedBenchmark as $crate::BenchmarkingSetup<T>>::components(&selected_benchmark);
				let mut results: Vec<$crate::BenchmarkResults> = Vec::new();

				// Default number of steps for a component.
				let mut prev_steps = 10;

				// Select the component we will be benchmarking. Each component will be benchmarked.
				for (idx, (name, low, high)) in components.iter().enumerate() {
					// Get the number of steps for this component.
					let steps = steps.get(idx).cloned().unwrap_or(prev_steps);
					prev_steps = steps;

					// Skip this loop if steps is zero
					if steps == 0 { continue }

					let lowest = lowest_range_values.get(idx).cloned().unwrap_or(*low);
					let highest = highest_range_values.get(idx).cloned().unwrap_or(*high);

					let diff = highest - lowest;

					// Create up to `STEPS` steps for that component between high and low.
					let step_size = (diff / steps).max(1);
					let num_of_steps = diff / step_size + 1;

					for s in 0..num_of_steps {
						// This is the value we will be testing for component `name`
						let component_value = lowest + step_size * s;

						// Select the max value for all the other components.
						let c: Vec<($crate::BenchmarkParameter, u32)> = components.iter()
							.enumerate()
							.map(|(idx, (n, _, h))|
								if n == name {
									(*n, component_value)
								} else {
									(*n, *highest_range_values.get(idx).unwrap_or(h))
								}
							)
							.collect();

						// Run the benchmark `repeat` times.
						for _ in 0..repeat {
							// Set up the externalities environment for the setup we want to
							// benchmark.
							let closure_to_benchmark = <
								SelectedBenchmark as $crate::BenchmarkingSetup<T>
							>::instance(&selected_benchmark, &c)?;

							// Set the block number to at least 1 so events are deposited.
							if $crate::Zero::is_zero(&frame_system::Module::<T>::block_number()) {
								frame_system::Module::<T>::set_block_number(1.into());
							}

							// Commit the externalities to the database, flushing the DB cache.
							// This will enable worst case scenario for reading from the database.
							$crate::benchmarking::commit_db();

							// Reset the read/write counter so we don't count operations in the setup process.
							$crate::benchmarking::reset_read_write_count();

							// Time the extrinsic logic.
							frame_support::debug::trace!(
								target: "benchmark",
								"Start Benchmark: {:?} {:?}", name, component_value
							);

							let start_extrinsic = $crate::benchmarking::current_time();
							closure_to_benchmark()?;
							let finish_extrinsic = $crate::benchmarking::current_time();
							let elapsed_extrinsic = finish_extrinsic - start_extrinsic;
							// Commit the changes to get proper write count
							$crate::benchmarking::commit_db();
							frame_support::debug::trace!(
								target: "benchmark",
								"End Benchmark: {} ns", elapsed_extrinsic
							);
							let read_write_count = $crate::benchmarking::read_write_count();
							frame_support::debug::trace!(
								target: "benchmark",
								"Read/Write Count {:?}", read_write_count
							);

							// Time the storage root recalculation.
							let start_storage_root = $crate::benchmarking::current_time();
							$crate::storage_root();
							let finish_storage_root = $crate::benchmarking::current_time();
							let elapsed_storage_root = finish_storage_root - start_storage_root;

							results.push($crate::BenchmarkResults {
								components: c.clone(),
								extrinsic_time: elapsed_extrinsic,
								storage_root_time: elapsed_storage_root,
								reads: read_write_count.0,
								repeat_reads: read_write_count.1,
								writes: read_write_count.2,
								repeat_writes: read_write_count.3,
							});

							// Wipe the DB back to the genesis state.
							$crate::benchmarking::wipe_db();
						}
					}
				}
				return Ok(results);
			}
		}
	};
	(
		{ $( $where_clause:tt )* }
		INSTANCE $( $name:ident ),*
	) => {
		impl<T: Trait<I>, I: Instance> $crate::Benchmarking<$crate::BenchmarkResults>
			for Module<T, I>
			where T: frame_system::Trait, $( $where_clause )*
		{
			fn benchmarks() -> Vec<&'static [u8]> {
				vec![ $( stringify!($name).as_ref() ),* ]
			}

			fn run_benchmark(
				extrinsic: &[u8],
				lowest_range_values: &[u32],
				highest_range_values: &[u32],
				steps: &[u32],
				repeat: u32,
				whitelist: &[Vec<u8>]
			) -> Result<Vec<$crate::BenchmarkResults>, &'static str> {
				// Map the input to the selected benchmark.
				let extrinsic = sp_std::str::from_utf8(extrinsic)
					.map_err(|_| "`extrinsic` is not a valid utf8 string!")?;
				let selected_benchmark = match extrinsic {
					$( stringify!($name) => SelectedBenchmark::$name, )*
					_ => return Err("Could not find extrinsic."),
				};

				// Add whitelist to DB
				$crate::benchmarking::set_whitelist(whitelist.to_vec());

				// Warm up the DB
				$crate::benchmarking::commit_db();
				$crate::benchmarking::wipe_db();

				let components = <
					SelectedBenchmark as $crate::BenchmarkingSetupInstance<T, I>
				>::components(&selected_benchmark);
				let mut results: Vec<$crate::BenchmarkResults> = Vec::new();

				// Default number of steps for a component.
				let mut prev_steps = 10;

				// Select the component we will be benchmarking. Each component will be benchmarked.
				for (idx, (name, low, high)) in components.iter().enumerate() {
					// Get the number of steps for this component.
					let steps = steps.get(idx).cloned().unwrap_or(prev_steps);
					prev_steps = steps;

					// Skip this loop if steps is zero
					if steps == 0 { continue }

					let lowest = lowest_range_values.get(idx).cloned().unwrap_or(*low);
					let highest = highest_range_values.get(idx).cloned().unwrap_or(*high);

					let diff = highest - lowest;

					// Create up to `STEPS` steps for that component between high and low.
					let step_size = (diff / steps).max(1);
					let num_of_steps = diff / step_size + 1;

					for s in 0..num_of_steps {
						// This is the value we will be testing for component `name`
						let component_value = lowest + step_size * s;

						// Select the max value for all the other components.
						let c: Vec<($crate::BenchmarkParameter, u32)> = components.iter()
							.enumerate()
							.map(|(idx, (n, _, h))|
								if n == name {
									(*n, component_value)
								} else {
									(*n, *highest_range_values.get(idx).unwrap_or(h))
								}
							)
							.collect();

						// Run the benchmark `repeat` times.
						for _ in 0..repeat {
							// Set up the externalities environment for the setup we want to benchmark.
							let closure_to_benchmark = <
								SelectedBenchmark as $crate::BenchmarkingSetupInstance<T, I>
							>::instance(&selected_benchmark, &c)?;

							// Set the block number to at least 1 so events are deposited.
							if $crate::Zero::is_zero(&frame_system::Module::<T>::block_number()) {
								frame_system::Module::<T>::set_block_number(1.into());
							}

							// Commit the externalities to the database, flushing the DB cache.
							// This will enable worst case scenario for reading from the database.
							$crate::benchmarking::commit_db();

							// Reset the read/write counter so we don't count operations in the setup process.
							$crate::benchmarking::reset_read_write_count();

							// Time the extrinsic logic.
							frame_support::debug::trace!(
								target: "benchmark",
								"Start Benchmark: {:?} {:?}", name, component_value
							);

							let start_extrinsic = $crate::benchmarking::current_time();
							closure_to_benchmark()?;
							let finish_extrinsic = $crate::benchmarking::current_time();
							let elapsed_extrinsic = finish_extrinsic - start_extrinsic;
							// Commit the changes to get proper write count
							$crate::benchmarking::commit_db();
							frame_support::debug::trace!(
								target: "benchmark",
								"End Benchmark: {} ns", elapsed_extrinsic
							);
							let read_write_count = $crate::benchmarking::read_write_count();
							frame_support::debug::trace!(
								target: "benchmark",
								"Read/Write Count {:?}", read_write_count
							);

							// Time the storage root recalculation.
							let start_storage_root = $crate::benchmarking::current_time();
							$crate::storage_root();
							let finish_storage_root = $crate::benchmarking::current_time();
							let elapsed_storage_root = finish_storage_root - start_storage_root;

							results.push($crate::BenchmarkResults {
								components: c.clone(),
								extrinsic_time: elapsed_extrinsic,
								storage_root_time: elapsed_storage_root,
								reads: read_write_count.0,
								repeat_reads: read_write_count.1,
								writes: read_write_count.2,
								repeat_writes: read_write_count.3,
							});

							// Wipe the DB back to the genesis state.
							$crate::benchmarking::wipe_db();
						}
					}
				}
				return Ok(results);
			}
		}
	}
}

// This creates a unit test for one benchmark of the main benchmark macro.
// It runs the benchmark using the `high` and `low` value for each component
// and ensure that everything completes successfully.
#[macro_export]
#[doc(hidden)]
macro_rules! impl_benchmark_test {
	(
		{ $( $where_clause:tt )* }
		NO_INSTANCE
		$name:ident
	) => {
		$crate::paste::item! {
			fn [<test_benchmark_ $name>] <T: Trait> () -> Result<(), &'static str>
				where T: frame_system::Trait, $( $where_clause )*
			{
				let selected_benchmark = SelectedBenchmark::$name;
				let components = <
					SelectedBenchmark as $crate::BenchmarkingSetup<T>
				>::components(&selected_benchmark);

				assert!(
					components.len() != 0,
					"You need to add components to your benchmark!",
				);
				for (_, (name, low, high)) in components.iter().enumerate() {
					// Test only the low and high value, assuming values in the middle won't break
					for component_value in vec![low, high] {
						// Select the max value for all the other components.
						let c: Vec<($crate::BenchmarkParameter, u32)> = components.iter()
							.enumerate()
							.map(|(_, (n, _, h))|
								if n == name {
									(*n, *component_value)
								} else {
									(*n, *h)
								}
							)
							.collect();

						// Set up the verification state
						let closure_to_verify = <
							SelectedBenchmark as $crate::BenchmarkingSetup<T>
						>::verify(&selected_benchmark, &c)?;

						// Set the block number to at least 1 so events are deposited.
						if $crate::Zero::is_zero(&frame_system::Module::<T>::block_number()) {
							frame_system::Module::<T>::set_block_number(1.into());
						}

						// Run verification
						closure_to_verify()?;

						// Reset the state
						$crate::benchmarking::wipe_db();
					}
				}
				Ok(())
			}
		}
	};
	(
		{ $( $where_clause:tt )* }
		INSTANCE
		$name:ident
	) => {
		$crate::paste::item! {
			fn [<test_benchmark_ $name>] <T: Trait> () -> Result<(), &'static str>
				where T: frame_system::Trait, $( $where_clause )*
			{
				let selected_benchmark = SelectedBenchmark::$name;
				let components = <
					SelectedBenchmark as $crate::BenchmarkingSetupInstance<T, _>
				>::components(&selected_benchmark);

				for (_, (name, low, high)) in components.iter().enumerate() {
					// Test only the low and high value, assuming values in the middle won't break
					for component_value in vec![low, high] {
						// Select the max value for all the other components.
						let c: Vec<($crate::BenchmarkParameter, u32)> = components.iter()
							.enumerate()
							.map(|(_, (n, _, h))|
								if n == name {
									(*n, *component_value)
								} else {
									(*n, *h)
								}
							)
							.collect();

						// Set up the verification state
						let closure_to_verify = <
							SelectedBenchmark as $crate::BenchmarkingSetupInstance<T, _>
						>::verify(&selected_benchmark, &c)?;

						// Set the block number to at least 1 so events are deposited.
						if $crate::Zero::is_zero(&frame_system::Module::<T>::block_number()) {
							frame_system::Module::<T>::set_block_number(1.into());
						}

						// Run verification
						closure_to_verify()?;

						// Reset the state
						$crate::benchmarking::wipe_db();
					}
				}
				Ok(())
			}
		}
	};
}


/// This macro adds pallet benchmarks to a `Vec<BenchmarkBatch>` object.
///
/// First create an object that holds in the input parameters for the benchmark:
///
/// ```ignore
/// let params = (&pallet, &benchmark, &lowest_range_values, &highest_range_values, &steps, repeat);
/// ```
///
/// Then define a mutable local variable to hold your `BenchmarkBatch` object:
///
/// ```ignore
/// let mut batches = Vec::<BenchmarkBatch>::new();
/// ````
///
/// Then add the pallets you want to benchmark to this object, including the string
/// you want to use target a particular pallet:
///
/// ```ignore
/// add_benchmark!(params, batches, b"balances", Balances);
/// add_benchmark!(params, batches, b"identity", Identity);
/// add_benchmark!(params, batches, b"session", SessionBench::<Runtime>);
/// ...
/// ```
///
/// At the end of `dispatch_benchmark`, you should return this batches object.
#[macro_export]
macro_rules! add_benchmark {
	( $params:ident, $batches:ident, $name:literal, $( $location:tt )* ) => (
		let (pallet, benchmark, lowest_range_values, highest_range_values, steps, repeat, whitelist) = $params;
		if &pallet[..] == &$name[..] || &pallet[..] == &b"*"[..] {
			if &pallet[..] == &b"*"[..] || &benchmark[..] == &b"*"[..] {
				for benchmark in $( $location )*::benchmarks().into_iter() {
					$batches.push($crate::BenchmarkBatch {
						results: $( $location )*::run_benchmark(
							benchmark,
							&lowest_range_values[..],
							&highest_range_values[..],
							&steps[..],
							repeat,
							whitelist,
						)?,
						pallet: $name.to_vec(),
						benchmark: benchmark.to_vec(),
					});
				}
			} else {
				$batches.push($crate::BenchmarkBatch {
					results: $( $location )*::run_benchmark(
						&benchmark[..],
						&lowest_range_values[..],
						&highest_range_values[..],
						&steps[..],
						repeat,
						whitelist,
					)?,
					pallet: $name.to_vec(),
					benchmark: benchmark.clone(),
				});
			}
		}
	)
}
