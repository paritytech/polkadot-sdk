//! # FRAME
//!
//! ```no_compile
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
//! A pallet is a unit of encapsulated logic. It has a clearly defined responsibility and can be
//! linked to other pallets. Each pallet should try to only care about its own responsibilities and
//! make as few assumptions about the general runtime as possible. A pallet is analogous to a
//! _module_ in the runtime.
//!
//! A pallet is defined as a `mod pallet` wrapped by the [`frame::pallet`] macro. Within this macro,
//! pallet components/parts can be defined. Most notable of these parts are:
//!
//! - [Config](frame::pallet_macros::config), allowing a pallet to make itself configurable and
//!   generic over types, values and such.
//! - [Storage](frame::pallet_macros::storage), allowing a pallet to define onchain storage.
//! - [Dispatchable function aka. Extrinsics](frame::pallet_macros::call), allowing a pallet to
//!   define extrinsics that are callable by end users, from the outer world.
//! - [Events](frame::pallet_macros::event), allowing a pallet to emit events.
//! - [Errors](frame::pallet_macros::error), allowing a pallet to emit well-formed errors.
//!
//! Most of these components are defined using macros, the full list of which can be found in
//! [`frame::pallet_macros`]
//!
//! ### Example
//!
//! The following examples showcases a minimal pallet.
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", pallet)]
//!
//! ## Runtime
//!
//! A runtime is a collection of pallets that are amalgamated together. Each pallet typically has
//! some configurations (exposed as a `trait Config`) that needs to be *specified* in the runtime.
//! This is done with [`frame::runtime::prelude::construct_runtime`].
//!
//! A (real) runtime that actually wishes to compile to WASM needs to also implement a set of
//! runtime-apis. These implementation can be specified using the
//! [`frame::runtime::prelude::impl_runtime_apis`] macro.
//!
//! ### Example
//!
//! The following example shows a (test) runtime that is composing the pallet demonstrated above,
//! next to the [`frame::prelude::frame_system`] pallet, into a runtime.
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", runtime)]

#[cfg(test)]
mod tests {
	use frame::prelude::*;

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
