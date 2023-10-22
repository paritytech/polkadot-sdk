//! # FRAME
//!
//! ```co_compile
//!   ______   ______    ________   ___ __ __   ______
//!  /_____/\ /_____/\  /_______/\ /__//_//_/\ /_____/\
//!  \::::_\/_\:::_ \ \ \::: _  \ \\::\| \| \ \\::::_\/_
//!   \:\/___/\\:(_) ) )_\::(_)  \ \\:.      \ \\:\/___/\
//!    \:::._\/ \: __ `\ \\:: __  \ \\:.\-/\  \ \\::___\/_
//!     \:\ \    \ \ `\ \ \\:.\ \  \ \\. \  \  \ \\:\____/\
//!      \_\/     \_\/ \_\/ \__\/\__\/ \__\/ \__\/ \_____\/
//! ```
//!
//! > **F**ramework for **R**untime **A**ggregation of **M**odularized **E**ntities: Substrate's
//! > State Transition Function (Runtime) Framework.
//!
//! ## Introduction
//!
//! recall from [`crate::reference_docs::wasm_meta_protocol`] that at a very high, a substrate-based
//! blockchain is made with it is composed of two parts:
//!
//! 1. A *runtime* which represents the state transition function (i.e. "Business Logic") of a
//! blockchain, and is encoded as a Wasm blob.
//! 2. A client whose primary purpose is to execute the given runtime.
#![doc = simple_mermaid::mermaid!("../../../docs/mermaid/substrate_simple.mmd")]
//!
//! *FRAME is the Substrate's framework of choice to build a runtime.*
//!
//! FRAME is composed of two major components, **pallets** and a **runtime**.
//!
//! ## Pallets
//!
//! A pallet is analogous to a _module_ in the runtime, which can itself be composed of multiple
//! components, most notable of which are:
//!
//! - Storage
//! - Dispatchables
//! - Events
//! - Errors
//!
//! TODO: link to dispatch and state ref doc, if any.
//!
//! Most of these components are defined using macros, the full list of which can be found in
//! [`frame::deps::frame_support::pallet_macros`]
//!
//! ### Example
//!
//! The following examples showcases a minimal pallet.
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", pallet)]
//!
//! ## Runtime
//!
//! A runtime is a collection of pallets that are amalgamated together. Each pallet typically has
//! some configurations (exposed as a `trait Config`) that needs to be specified in the runtime.
//! This is done with [`frame::runtime::prelude::construct_runtime`].
//!
//! A (real) runtime that actually wishes to compile to WASM needs to also implement a set of
//! runtime-apis that
//!
//! ### Example
//!
//! The following example shows a (test) runtime that is composing the pallet demonstrated above,
//! next to the [`frame::prelude::frame_system`] pallet, into a runtime.
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", runtime)]
//!
#![doc = simple_mermaid::mermaid!("../../../docs/mermaid/substrate_with_frame.mmd")]

#[cfg(test)]
mod tests {
	use frame::{prelude::*, testing_prelude::*};

	#[docify::export]
	#[frame::pallet(dev_mode)]
	pub mod pallet {
		use super::*;

		#[pallet::config]
		pub trait Config: frame_system::Config {
			type RuntimeEvent: IsType<<Self as frame_system::Config>::RuntimeEvent>
				+ From<Event<Self>>;
		}

		#[pallet::pallet]
		pub struct Pallet<T>(PhantomData<T>);

		#[pallet::event]
		pub enum Event<T: Config> {}

		#[pallet::storage]
		pub type Value<T> = StorageValue<Value = u32>;

		#[pallet::call]
		impl<T: Config> Pallet<T> {
			pub fn some_dispatchable(_origin: OriginFor<T>) -> DispatchResult {
				Ok(())
			}
		}
	}

	#[docify::export]
	pub mod runtime {
		use super::pallet as pallet_example;
		use frame::{prelude::*, testing_prelude::*};

		construct_runtime!(
			pub struct Runtime {
				System: frame_system,
				Example: pallet_example,
			}
		);

		#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
		impl frame_system::Config for Runtime {
			type Block = MockBlock<Self>;
		}

		impl pallet_example::Config for Runtime {
			type RuntimeEvent = RuntimeEvent;
		}
	}
}
