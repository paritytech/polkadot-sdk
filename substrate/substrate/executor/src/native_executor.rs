// Copyright 2017 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use error::{Error, ErrorKind, Result};
use state_machine::{CodeExecutor, Externalities};
use wasm_executor::WasmExecutor;
use runtime_version::RuntimeVersion;
use codec::Slicable;
use RuntimeInfo;

fn safe_call<F, U>(f: F) -> Result<U>
	where F: ::std::panic::UnwindSafe + FnOnce() -> U
{
	::std::panic::catch_unwind(f).map_err(|_| ErrorKind::Runtime.into())
}

/// Set up the externalities and safe calling environment to execute calls to a native runtime.
///
/// If the inner closure panics, it will be caught and return an error.
pub fn with_native_environment<F, U>(ext: &mut Externalities, f: F) -> Result<U>
	where F: ::std::panic::UnwindSafe + FnOnce() -> U
{
	::runtime_io::with_externalities(ext, move || safe_call(f))
}

/// Delegate for dispatching a CodeExecutor call to native code.
pub trait NativeExecutionDispatch {
	/// Get the wasm code that the native dispatch will be equivalent to.
	fn native_equivalent() -> &'static [u8];

	/// Dispatch a method and input data to be executed natively. Returns `Some` result or `None`
	/// if the `method` is unknown. Panics if there's an unrecoverable error.
	fn dispatch(ext: &mut Externalities, method: &str, data: &[u8]) -> Result<Vec<u8>>;

	/// Get native runtime version.
	const VERSION: RuntimeVersion;
}

/// A generic `CodeExecutor` implementation that uses a delegate to determine wasm code equivalence
/// and dispatch to native code when possible, falling back on `WasmExecutor` when not.
#[derive(Debug)]
pub struct NativeExecutor<D: NativeExecutionDispatch + Sync + Send> {
	/// Dummy field to avoid the compiler complaining about us not using `D`.
	_dummy: ::std::marker::PhantomData<D>,
}

impl<D: NativeExecutionDispatch + Sync + Send> NativeExecutor<D> {
	/// Create new instance.
	pub fn new() -> Self {
		NativeExecutor {
			_dummy: Default::default(),
		}
	}
}

impl<D: NativeExecutionDispatch + Sync + Send> Clone for NativeExecutor<D> {
	fn clone(&self) -> Self {
		NativeExecutor {
			_dummy: Default::default(),
		}
	}
}

impl<D: NativeExecutionDispatch + Sync + Send> RuntimeInfo for NativeExecutor<D> {
	const NATIVE_VERSION: Option<RuntimeVersion> = Some(D::VERSION);
}

impl<D: NativeExecutionDispatch + Sync + Send> CodeExecutor for NativeExecutor<D> {
	type Error = Error;

	fn call<E: Externalities>(
		&self,
		ext: &mut E,
		code: &[u8],
		method: &str,
		data: &[u8],
	) -> Result<Vec<u8>> {
		if code == D::native_equivalent() {
			// call native
			D::dispatch(ext, method, data)
		} else {
			let version = WasmExecutor.call(ext, code, "version", &[])?;
			let version = RuntimeVersion::decode(&mut version.as_slice());
			if version.map_or(false, |v| D::VERSION.can_call_with(&v)) {
				return D::dispatch(ext, method, data)
			}
			// call into wasm.
			WasmExecutor.call(ext, code, method, data)
		}
	}
}

#[macro_export]
macro_rules! native_executor_instance {
	(pub $name:ident, $dispatcher:path, $version:path, $code:expr) => {
		pub struct $name;
		native_executor_instance!(IMPL $name, $dispatcher, $version, $code);
	};
	($name:ident, $dispatcher:path, $version:path, $code:expr) => {
		/// A unit struct which implements `NativeExecutionDispatch` feeding in the hard-coded runtime.
		struct $name;
		native_executor_instance!(IMPL $name, $dispatcher, $version, $code);
	};
	(IMPL $name:ident, $dispatcher:path, $version:path, $code:expr) => {
		impl $crate::NativeExecutionDispatch for $name {
			const VERSION: $crate::RuntimeVersion = $version;
			fn native_equivalent() -> &'static [u8] {
				// WARNING!!! This assumes that the runtime was built *before* the main project. Until we
				// get a proper build script, this must be strictly adhered to or things will go wrong.
				$code
			}

			fn dispatch(ext: &mut $crate::Externalities, method: &str, data: &[u8]) -> $crate::error::Result<Vec<u8>> {
				$crate::with_native_environment(ext, move || $dispatcher(method, data))?
					.ok_or_else(|| $crate::error::ErrorKind::MethodNotFound(method.to_owned()).into())
			}
		}

		impl $name {
			pub fn new() -> $crate::NativeExecutor<$name> {
				$crate::NativeExecutor::new()
			}
		}
	}
}
