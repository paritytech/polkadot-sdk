#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_simple.mmd")]
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", pallet)]
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", runtime)]

/// A FRAME based pallet. This `mod` is the entry point for everything else. All
/// `#[pallet::xxx]` macros must be defined in this `mod`. Although, frame also provides an
/// experimental feature to break these parts into different `mod`s. See [``] for
/// more.
#[docify::export]
#[frame::pallet(dev_mode)]
pub mod pallet {
	use frame::prelude::*;

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
	/// transactions) must be attached to this type (see [`call`]). For
	/// convenience, internal (private) functions can also be attached to this type.
	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	/// The events that this pallet can emit.
	#[pallet::event]
	pub enum Event<T: Config> {}

	/// A storage item that this pallet contains. This will be part of the state root trie
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

// Link References

// Link References







#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_simple.mmd")]
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", pallet)]
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", runtime)]

/// A FRAME based pallet. This `mod` is the entry point for everything else. All
/// `#[pallet::xxx]` macros must be defined in this `mod`. Although, frame also provides an
/// experimental feature to break these parts into different `mod`s. See [``] for
/// more.
#[docify::export]
#[frame::pallet(dev_mode)]
pub mod pallet {
	use frame::prelude::*;

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
	/// transactions) must be attached to this type (see [`call`]). For
	/// convenience, internal (private) functions can also be attached to this type.
	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	/// The events that this pallet can emit.
	#[pallet::event]
	pub enum Event<T: Config> {}

	/// A storage item that this pallet contains. This will be part of the state root trie
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

// Link References

// Link References








#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_simple.mmd")]
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", pallet)]
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", runtime)]

/// A FRAME based pallet. This `mod` is the entry point for everything else. All
/// `#[pallet::xxx]` macros must be defined in this `mod`. Although, frame also provides an
/// experimental feature to break these parts into different `mod`s. See [``] for
/// more.
#[docify::export]
#[frame::pallet(dev_mode)]
pub mod pallet {
	use frame::prelude::*;

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
	/// transactions) must be attached to this type (see [`call`]). For
	/// convenience, internal (private) functions can also be attached to this type.
	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	/// The events that this pallet can emit.
	#[pallet::event]
	pub enum Event<T: Config> {}

	/// A storage item that this pallet contains. This will be part of the state root trie
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

// Link References

// Link References







#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_simple.mmd")]
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", pallet)]
#![doc = docify::embed!("src/polkadot_sdk/frame_runtime.rs", runtime)]

/// A FRAME based pallet. This `mod` is the entry point for everything else. All
/// `#[pallet::xxx]` macros must be defined in this `mod`. Although, frame also provides an
/// experimental feature to break these parts into different `mod`s. See [``] for
/// more.
#[docify::export]
#[frame::pallet(dev_mode)]
pub mod pallet {
	use frame::prelude::*;

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
	/// transactions) must be attached to this type (see [`call`]). For
	/// convenience, internal (private) functions can also be attached to this type.
	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	/// The events that this pallet can emit.
	#[pallet::event]
	pub enum Event<T: Config> {}

	/// A storage item that this pallet contains. This will be part of the state root trie
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

// Link References

// Link References





























// [``]:
