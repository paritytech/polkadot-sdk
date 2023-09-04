//! Macros for testing FRAME pallets.
//!
//! The macros are tested in `frame-test-utils` to check that they work from the outside.

#[doc(hidden)]
pub mod __private {
	pub use frame_support::{assert_err, assert_ok, storage};
	pub use sp_runtime::{DispatchError, TransactionOutcome};
}

/// Do something hypothetically by rolling back any changes afterwards.
///
/// Returns the original result of the closure.
#[macro_export]
macro_rules! hypothetical {
	( $e:expr ) => {
		$crate::__private::storage::transactional::with_transaction(
			|| -> $crate::__private::TransactionOutcome<Result<_, $crate::__private::DispatchError>> {
				$crate::__private::TransactionOutcome::Rollback(Ok($e))
			},
		)
		.expect("Always returning Ok; qed")
	};
}

/// Assert something to be *hypothetically* `Ok` without actually committing it.
///
/// Reverts any storage changes made by the closure.
#[macro_export]
macro_rules! hypothetical_ok {
	($e:expr $(, $args:expr)* $(,)?) => {
		$crate::__private::assert_ok!($crate::hypothetical!($e) $(, $args)*);
	};
}

/// Assert an expression returns an error specified.
///
/// This can be used on `DispatchResultWithPostInfo` when the post info should
/// be ignored.
#[macro_export]
macro_rules! assert_err_ignore_postinfo {
	( $x:expr , $y:expr $(,)? ) => {
		$crate::__private::assert_err!($x.map(|_| ()).map_err(|e| e.error), $y);
	};
}

/// Assert an expression returns error with the given weight.
#[macro_export]
macro_rules! assert_err_with_weight {
	($call:expr, $err:expr, $weight:expr $(,)? ) => {
		if let Err(dispatch_err_with_post) = $call {
			$crate::__private::assert_err!($call.map(|_| ()).map_err(|e| e.error), $err);
			assert_eq!(dispatch_err_with_post.post_info.actual_weight, $weight);
		} else {
			panic!("expected Err(_), got Ok(_).")
		}
	};
}
