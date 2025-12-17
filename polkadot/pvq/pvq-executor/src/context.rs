use polkavm::Linker;

/// Provides host integration for [`crate::PvqExecutor`].
///
/// A context is responsible for registering host functions into the [`Linker`] and for providing
/// the mutable user data value passed to guest calls.
pub trait PvqExecutorContext {
	/// The user data passed to host functions.
	///
	/// This is the `T` parameter of [`Linker<T, E>`].
	type UserData;
	/// The user-defined error type returned by host functions.
	///
	/// This is the `E` parameter of [`Linker<T, E>`] and becomes [`crate::PvqExecutorError::User`].
	type UserError;

	/// Registers host functions with the given [`Linker`].
	///
	/// This is called by [`crate::PvqExecutor::new`] exactly once during construction.
	fn register_host_functions(&mut self, linker: &mut Linker<Self::UserData, Self::UserError>);

	/// Returns a mutable reference to the user data.
	///
	/// The executor calls this right before invoking the guest entrypoint, and passes the returned
	/// reference to PolkaVM so it is accessible to host functions.
	fn data(&mut self) -> &mut Self::UserData;
}
