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
//! As described in [`crate::reference_docs::wasm_meta_protocol`], at a high-level Substrate-based
//! blockchains are composed of two parts:
//!
//! 1. A *runtime* which represents the state transition function (i.e. "Business Logic") of a
//! blockchain, and is encoded as a WASM blob.
//! 2. A node whose primary purpose is to execute the given runtime.
#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_simple.mmd")]
//!
//! *FRAME is the Substrate's framework of choice to build a runtime.*
//!
//! FRAME is composed of two major components, **pallets** and a **runtime**.
//!
//! ## Pallets
//!
//! A pallet is a unit of encapsulated logic. It has a clearly defined responsibility and can be
//! linked to other pallets. In order to be reusable, pallets shipped with FRAME strive to only care
//! about its own responsibilities and make as few assumptions about the general runtime as
//! possible. A pallet is analogous to a _module_ in the runtime.
//!
//! A pallet is defined as a `mod pallet` wrapped by the [`frame::pallet`] macro. Within this macro,
//! pallet components/parts can be defined. Most notable of these parts are:
//!
//! - [Config](frame::pallet_macros::config), allowing a pallet to make itself configurable and
//!   generic over types, values and such.
//! - [Storage](frame::pallet_macros::storage), allowing a pallet to define onchain storage.
//! - [Dispatchable function](frame::pallet_macros::call), allowing a pallet to define extrinsics
//!   that are callable by end users, from the outer world.
//! - [Events](frame::pallet_macros::event), allowing a pallet to emit events.
//! - [Errors](frame::pallet_macros::error), allowing a pallet to emit well-formed errors.
//!
//! Some of these pallet components resemble the building blocks of a smart contract. While both
//! models are programming state transition functions of blockchains, there are crucial differences
//! between the two. See [`crate::reference_docs::runtime_vs_smart_contract`] for more.
//!
//! Most of these components are defined using macros, the full list of which can be found in
//! [`frame::pallet_macros`].
//!
//! ### Example
//!
//! The following examples showcases a minimal pallet.
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", pallet)]
//!
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
//!
//! ## More Examples
//!
//! You can find more FRAME examples that revolve around specific features at [`pallet_examples`].
//!
//! ## Alternatives 🌈
//!
//! There is nothing in the Substrate's node side code-base that mandates the use of FRAME. While
//! FRAME makes it very simple to write Substrate-based runtimes, it is by no means intended to be
//! the only one. At the end of the day, any WASM blob that exposes the right set of runtime APIs is
//! a valid Runtime form the point of view of a Substrate client (see
//! [`crate::reference_docs::wasm_meta_protocol`]). Notable examples are:
//!
//! * writing a runtime in pure Rust, as done in [this template](https://github.com/JoshOrndorff/frameless-node-template).
//! * writing a runtime in AssemblyScript,as explored in [this project](https://github.com/LimeChain/subsembly).

use frame::prelude::*;

/// A FRAME based pallet. This `mod` is the entry point for everything else. All
/// `#[pallet::xxx]` macros must be defined in this `mod`. Although, frame also provides an
/// experimental feature to break these parts into different `mod`s. See [`pallet_examples`] for
/// more.
#[docify::export]
#[frame::pallet(dev_mode)]
pub mod pallet {
	use super::*;

	/// The configuration trait of a pallet. Mandatory. Allows a pallet to receive types at a
	/// later point from the runtime that wishes to contain it. It allows the pallet to be
	/// parameterized over both types and values.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// A type that is not known now, but the runtime that will contain this pallet will
		/// know it later, therefore we define it here as an associated type.
		type RuntimeEvent: IsType<<Self as frame_system::Config>::RuntimeEvent> + From<Event<Self>>;

		/// A parameterize-able value that we receive later via the `Get<_>` trait.
		type ValueParameter: Get<u32>;

		/// Similar to [`Config::ValueParameter`], but using `const`. Both are functionally
		/// equal, but offer different tradeoffs.
		const ANOTHER_VALUE_PARAMETER: u32;
	}

	/// A mandatory struct in each pallet. All functions callable by external users (aka.
	/// transactions) must be attached to this type (see [`frame::pallet_macros::call`]). For
	/// convenience, internal (private) functions can also be attached to this type.
	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	/// The events tha this pallet can emit.
	#[pallet::event]
	pub enum Event<T: Config> {}

	/// A storage item that this pallet contains. This will be part of the state root trie/root
	/// of the blockchain.
	#[pallet::storage]
	pub type Value<T> = StorageValue<Value = u32>;

	/// All *dispatchable* call functions (aka. transactions) are attached to `Pallet` in a
	/// `impl` block.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// This will be callable by external users, and has two u32s as a parameter.
		pub fn some_dispatchable(
			_origin: OriginFor<T>,
			_param: u32,
			_other_para: u32,
		) -> DispatchResult {
			Ok(())
		}
	}
}

/// A simple runtime that contains the above pallet and `frame_system`, the mandatory pallet of
/// all runtimes. This runtime is for testing, but it shares a lot of similarities with a *real*
/// runtime.
#[docify::export]
pub mod runtime {
	use super::pallet as pallet_example;
	use frame::{prelude::*, testing_prelude::*};

	// The major macro that amalgamates pallets into `enum Runtime`
	construct_runtime!(
		pub enum Runtime {
			System: frame_system,
			Example: pallet_example,
		}
	);

	// These `impl` blocks specify the parameters of each pallet's `trait Config`.
	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = MockBlock<Self>;
	}

	impl pallet_example::Config for Runtime {
		type RuntimeEvent = RuntimeEvent;
		type ValueParameter = ConstU32<42>;
		const ANOTHER_VALUE_PARAMETER: u32 = 42;
	}
}
