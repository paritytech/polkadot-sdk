//! # Frame storage derives
//!
//! > **Note:**
//! >
//! > In all examples, a few lines of boilerplate have been hidden from each snippet for
//! > conciseness.
//!
//! Let's begin by starting to store a `NewType` in a storage item:
//!
//! ```compile_fail
//! #[frame::pallet]
//! pub mod pallet {
//! 	# use frame::prelude::*;
//! 	# #[pallet::config]
//! 	# pub trait Config: frame_system::Config {}
//! 	# #[pallet::pallet]
//! 	# pub struct Pallet<T>(_);
//! 	pub struct NewType(u32);
//
//! 	#[pallet::storage]
//! 	pub type Something<T> = StorageValue<_, NewType>;
//! }
//! ```
//! 
//! This raises a number of compiler errors, like:
//! ```text
//! the trait `MaxEncodedLen` is not implemented for `NewType`, which is required by
//! `frame::prelude::StorageValue<_GeneratedPrefixForStorageSomething<T>, NewType>:
//! StorageInfoTrait`
//! ```
//! 
//! This implies the following set of traits that need to be derived for a type to be stored in
//! `frame` storage:
//! ```rust
//! #[frame::pallet]
//! pub mod pallet {
//! 	# use frame::prelude::*;
//! 	# #[pallet::config]
//! 	# pub trait Config: frame_system::Config {}
//! 	# #[pallet::pallet]
//! 	# pub struct Pallet<T>(_);
//! 	#[derive(codec::Encode, codec::Decode, codec::MaxEncodedLen, scale_info::TypeInfo)]
//! 	pub struct NewType(u32);
//!
//! 	#[pallet::storage]
//! 	pub type Something<T> = StorageValue<_, NewType>;
//! }
//! ```
//! 
//! Next, let's look at how this will differ if we are to store a type that is derived from `T` in
//! storage, such as [`frame::prelude::BlockNumberFor`]:
//! ```compile_fail
//! #[frame::pallet]
//! pub mod pallet {
//! 	# use frame::prelude::*;
//! 	# #[pallet::config]
//! 	# pub trait Config: frame_system::Config {}
//! 	# #[pallet::pallet]
//! 	# pub struct Pallet<T>(_);
//! 	#[derive(codec::Encode, codec::Decode, codec::MaxEncodedLen, scale_info::TypeInfo)]
//! 	pub struct NewType<T: Config>(BlockNumberFor<T>);
//!
//! 	#[pallet::storage]
//! 	pub type Something<T: Config> = StorageValue<_, NewType<T>>;
//! }
//! ```
//! 
//! Surprisingly, this will also raise a number of errors, like:
//! ```text
//! the trait `TypeInfo` is not implemented for `T`, which is required
//! by`frame_support::pallet_prelude::StorageValue<pallet_2::_GeneratedPrefixForStorageSomething<T>,
//! pallet_2::NewType<T>>:StorageEntryMetadataBuilder
//! ```
//! 
//! Why is that? The underlying reason is that the `TypeInfo` `derive` macro will only work for
//! `NewType` if all of `NewType`'s generics also implement `TypeInfo`. This is not the case for `T`
//! in the example above.
//!
//! If you expand an instance of the derive, you will find something along the lines of:
//! `impl<T> TypeInfo for NewType<T> where T: TypeInfo { ... }`. This is the reason why the
//! `TypeInfo` trait is required for `T`.
//!
//! To fix this, we need to add a `#[scale_info(skip_type_params(T))]`
//! attribute to `NewType`. This additional macro will instruct the `derive` to skip the bound on
//! `T`.
//! ```rust
//! #[frame::pallet]
//! pub mod pallet {
//! 	# use frame::prelude::*;
//! 	# #[pallet::config]
//! 	# pub trait Config: frame_system::Config {}
//! 	# #[pallet::pallet]
//! 	# pub struct Pallet<T>(_);
//! 	#[derive(codec::Encode, codec::Decode, codec::MaxEncodedLen, scale_info::TypeInfo)]
//! 	#[scale_info(skip_type_params(T))]
//! 	pub struct NewType<T: Config>(BlockNumberFor<T>);
//!
//! 	#[pallet::storage]
//! 	pub type Something<T: Config> = StorageValue<_, NewType<T>>;
//! }
//! ```
//! 
//! Next, let's say we wish to store `NewType` as [`frame::prelude::ValueQuery`], which means it
//! must also implement `Default`. This should be as simple as adding `derive(Default)` to it,
//! right?
//! ```compile_fail
//! #[frame::pallet]
//! pub mod pallet {
//! 	# use frame::prelude::*;
//! 	# #[pallet::config]
//! 	# pub trait Config: frame_system::Config {}
//! 	# #[pallet::pallet]
//! 	# pub struct Pallet<T>(_);
//! 	#[derive(codec::Encode, codec::Decode, codec::MaxEncodedLen, scale_info::TypeInfo, Default)]
//! 	#[scale_info(skip_type_params(T))]
//! 	pub struct NewType<T: Config>(BlockNumberFor<T>);
//!
//! 	#[pallet::storage]
//! 	pub type Something<T: Config> = StorageValue<_, NewType<T>, ValueQuery>;
//! }
//! ```
//! 
//! Under the hood, the expansion of the `derive(Default)` will suffer from the same restriction as
//! before: it will only work if `T: Default`, and `T` is not `Default`. Note that this is an
//! expected issue: `T` is merely a wrapper of many other types, such as `BlockNumberFor<T>`.
//! `BlockNumberFor<T>` should indeed implement `Default`, but `T` implementing `Default` is rather
//! meaningless.
//!
//! To fix this, frame provides a set of macros that are analogous to normal rust derive macros, but
//! work nicely on top of structs that are generic over `T: Config`. These macros are:
//!
//! - [`frame::prelude::DefaultNoBound`]
//! - [`frame::prelude::DebugNoBound`]
//! - [`frame::prelude::PartialEqNoBound`]
//! - [`frame::prelude::EqNoBound`]
//! - [`frame::prelude::CloneNoBound`]
//! - [`frame::prelude::PartialOrdNoBound`]
//! - [`frame::prelude::OrdNoBound`]
//!
//! The above traits are almost certainly needed for your tests - to print your type, assert equality
//! or clone it.
//!
//! We can fix the following example by using [`frame::prelude::DefaultNoBound`].
//! ```rust
//! #[frame::pallet]
//! pub mod pallet {
//! 	# use frame::prelude::*;
//! 	# #[pallet::config]
//! 	# pub trait Config: frame_system::Config {}
//! 	# #[pallet::pallet]
//! 	# pub struct Pallet<T>(_);
//! 	#[derive(
//! 		codec::Encode,
//! 		codec::Decode,
//! 		codec::MaxEncodedLen,
//! 		scale_info::TypeInfo,
//! 		DefaultNoBound
//!		)]
//! 	#[scale_info(skip_type_params(T))]
//! 	pub struct NewType<T:Config>(BlockNumberFor<T>);
//!
//! 	#[pallet::storage]
//! 	pub type Something<T: Config> = StorageValue<_, NewType<T>, ValueQuery>;
//! }
//! ```
//! 
//! Finally, if a custom type that is provided through `Config` is to be stored in the storage, it
//! is subject to the same trait requirements. The following does not work:
//! ```compile_fail
//! #[frame::pallet]
//! pub mod pallet {
//! 	use frame::prelude::*;
//! 	#[pallet::config]
//! 	pub trait Config: frame_system::Config {
//! 		type CustomType;
//! 	}
//! 	#[pallet::pallet]
//! 	pub struct Pallet<T>(_);
//! 	#[pallet::storage]
//! 	pub type Something<T: Config> = StorageValue<_, T::CustomType>;
//! }
//! ```
//! 
//! But adding the right trait bounds will fix it.
//! ```rust
//! #[frame::pallet]
//! pub mod pallet {
//! 	use frame::prelude::*;
//! 	#[pallet::config]
//! 	pub trait Config: frame_system::Config {
//! 		type CustomType: codec::FullCodec
//! 			+ codec::MaxEncodedLen
//! 			+ scale_info::TypeInfo
//! 			+ Debug
//! 			+ Default;
//! 	}
//! 	#[pallet::pallet]
//! 	pub struct Pallet<T>(_);
//! 	#[pallet::storage]
//! 	pub type Something<T: Config> = StorageValue<_, T::CustomType>;
//! }
//! ```
