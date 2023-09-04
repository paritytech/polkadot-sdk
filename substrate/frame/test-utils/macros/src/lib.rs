#[doc(hidden)]
pub mod __private {
	pub use frame_support::{assert_ok, storage};
	pub use sp_runtime::{DispatchError, TransactionOutcome};
}

/// Do something hypothetically by rolling back any changes afterwards.
///
/// Returns the original result of the closure.
#[macro_export]
macro_rules! hypothetically {
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
macro_rules! hypothetically_ok {
	($e:expr $(, $args:expr)* $(,)?) => {
		$crate::__private::assert_ok!($crate::hypothetically!($e) $(, $args)*);
	};
}
