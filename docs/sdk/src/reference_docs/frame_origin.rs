//! # FRAME Origin
//!
//! Let's start by clarifying a common wrong assumption about Origin:
//!
//! **ORIGIN IS NOT AN ACCOUNT ID**.
//!
//! FRAME's origin abstractions allow you to convey meanings far beyond just an account-id being the
//! caller of an extrinsic. Nonetheless, an account-id having signed an extrinsic is one of the
//! meanings that an origin can convey. This is the commonly used [`frame_system::ensure_signed`],
//! where the return value happens to be an account-id.
//!
//! Instead, let's establish the following as the correct definition of an origin:
//!
//! > The origin type represents the privilege level of the caller of an extrinsic.
//!
//! That is, an extrinsic, through checking the origin, can *express what privilege level it wishes
//! to impose on the caller of the extrinsic*. One of those checks can be as simple as "*any account
//! that has signed a statement can pass*".
//!
//! But the origin system can also express more abstract and complicated privilege levels. For
//! example:
//!
//! * If the majority of token holders agreed upon this. This is more or less what the
//!   [`pallet_democracy`] does under the hood ([reference](https://github.com/paritytech/polkadot-sdk/blob/edd95b3749754d2ed0c5738588e872c87be91624/substrate/frame/democracy/src/lib.rs#L1603-L1633)).
//! * If a specific ratio of an instance of [`pallet_collective`]/DAO agrees upon this.
//! * If another consensus system, for example a bridged network or a parachain, agrees upon this.
//! * If the majority of validator/authority set agrees upon this[^1].
//! * If caller holds a particular NFT.
//!
//! and many more.
//!
//! ## Context
//!
//! First, let's look at where the `origin` type is encountered in a typical pallet. The `origin:
//! OriginFor<T>` has to be the first argument of any given callable extrinsic in FRAME:
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", call_simple)]
//!
//! Typically, the code of an extrinsic starts with an origin check, such as
//! [`frame_system::ensure_signed`].
//!
//! Note that [`OriginFor`](frame_system::pallet_prelude::OriginFor) is merely a shorthand for
//! [`frame_system::Config::RuntimeOrigin`]. Given the name prefix `Runtime`, we can learn that
//! `RuntimeOrigin` is similar to `RuntimeCall` and others, a runtime composite enum that is
//! amalgamated at the runtime level. Read [`crate::reference_docs::frame_runtime_types`] to
//! familiarize yourself with these types.
//!
//! To understand this better, we will next create a pallet with a custom origin, which will add a
//! new variant to `RuntimeOrigin`.
//!
//! ## Adding Custom Pallet Origin to the Runtime
//!
//! For example, given a pallet that defines the following custom origin:
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", custom_origin)]
//!
//! And a runtime with the following pallets:
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", runtime_exp)]
//!
//! The type [`crate::reference_docs::frame_origin::runtime_for_origin::RuntimeOrigin`] is expanded.
//! This `RuntimeOrigin` contains a variant for the [`frame_system::RawOrigin`] and the custom
//! origin of the pallet.
//!
//! > Notice how the [`frame_system::ensure_signed`] is nothing more than a `match` statement. If
//! > you want to know where the actual origin of an extrinsic is set (and the signature
//! > verification happens, if any), see
//! > [`sp_runtime::generic::CheckedExtrinsic#trait-implementations`], specifically
//! > [`sp_runtime::traits::Applyable`]'s implementation.
//!
//! ## Asserting on a Custom Internal Origin
//!
//! In order to assert on a custom origin that is defined within your pallet, we need a way to first
//! convert the `<T as frame_system::Config>::RuntimeOrigin` into the local `enum Origin` of the
//! current pallet. This is a common process that is explained in
//! [`crate::reference_docs::frame_runtime_types#
//! adding-further-constraints-to-runtime-composite-enums`].
//!
//! We use the same process here to express that `RuntimeOrigin` has a number of additional bounds,
//! as follows.
//!
//! 1. Defining a custom `RuntimeOrigin` with further bounds in the pallet.
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", custom_origin_bound)]
//!
//! 2. Using it in the pallet.
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", custom_origin_usage)]
//!
//! ## Asserting on a Custom External Origin
//!
//! Very often, a pallet wants to have a parameterized origin that is **NOT** defined within the
//! pallet. In other words, a pallet wants to delegate an origin check to something that is
//! specified later at the runtime level. Like many other parameterizations in FRAME, this implies
//! adding a new associated type to `trait Config`.
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", external_origin_def)]
//!
//! Then, within the pallet, we can simply use this "unknown" origin check type:
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", external_origin_usage)]
//!
//! Finally, at the runtime, any implementation of [`frame::traits::EnsureOrigin`] can be passed.
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", external_origin_provide)]
//!
//! Indeed, some of these implementations of [`frame::traits::EnsureOrigin`] are similar to the ones
//! that we know about: [`frame::runtime::prelude::EnsureSigned`],
//! [`frame::runtime::prelude::EnsureSignedBy`], [`frame::runtime::prelude::EnsureRoot`],
//! [`frame::runtime::prelude::EnsureNone`], etc. But, there are also many more that are not known
//! to us, and are defined in other pallets.
//!
//! For example, [`pallet_collective`] defines [`pallet_collective::EnsureMember`] and
//! [`pallet_collective::EnsureProportionMoreThan`] and many more, which is exactly what we alluded
//! to earlier in this document.
//!
//! Make sure to check the full list of [implementors of
//! `EnsureOrigin`](frame::traits::EnsureOrigin#implementors) for more inspiration.
//!
//! ## Obtaining Abstract Origins
//!
//! So far we have learned that FRAME pallets can assert on custom and abstract origin types,
//! whether they are defined within the pallet or not. But how can we obtain these abstract origins?
//!
//! > All extrinsics that come from the outer world can generally only be obtained as either
//! > `signed` or `none` origin.
//!
//! Generally, these abstract origins are only obtained within the runtime, when a call is
//! dispatched within the runtime.
//!
//! ## Further References
//!
//! - [Gavin Wood's speech about FRAME features at Protocol Berg 2023.](https://youtu.be/j7b8Upipmeg?si=83_XUgYuJxMwWX4g&t=195)
//! - [A related StackExchange question.](https://substrate.stackexchange.com/questions/10992/how-do-you-find-the-public-key-for-the-medium-spender-track-origin)
//!
//! [^1]: Inherents are essentially unsigned extrinsics that need an [`frame_system::ensure_none`]
//! origin check, and through the virtue of being an inherent, are agreed upon by all validators.

use frame::prelude::*;

#[frame::pallet(dev_mode)]
pub mod pallet_for_origin {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export(call_simple)]
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn do_something(_origin: OriginFor<T>) -> DispatchResult {
			//              ^^^^^^^^^^^^^^^^^^^^^
			todo!();
		}
	}
}

#[frame::pallet(dev_mode)]
pub mod pallet_with_custom_origin {
	use super::*;

	#[docify::export(custom_origin_bound)]
	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeOrigin: From<<Self as frame_system::Config>::RuntimeOrigin>
			+ Into<Result<Origin, <Self as Config>::RuntimeOrigin>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export(custom_origin)]
	/// A dummy custom origin.
	#[pallet::origin]
	#[derive(PartialEq, Eq, Clone, RuntimeDebug, Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub enum Origin {
		/// If all holders of a particular NFT have agreed upon this.
		AllNftHolders,
		/// If all validators have agreed upon this.
		ValidatorSet,
	}

	#[docify::export(custom_origin_usage)]
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn only_validators(origin: OriginFor<T>) -> DispatchResult {
			// first, we convert from `<T as frame_system::Config>::RuntimeOrigin` to `<T as
			// Config>::RuntimeOrigin`
			let local_runtime_origin = <<T as Config>::RuntimeOrigin as From<
				<T as frame_system::Config>::RuntimeOrigin,
			>>::from(origin);
			// then we convert to `origin`, if possible
			let local_origin =
				local_runtime_origin.into().map_err(|_| "invalid origin type provided")?;
			ensure!(matches!(local_origin, Origin::ValidatorSet), "Not authorized");
			todo!();
		}
	}
}

pub mod runtime_for_origin {
	use super::pallet_with_custom_origin;
	use frame::{runtime::prelude::*, testing_prelude::*};

	#[docify::export(runtime_exp)]
	construct_runtime!(
		pub struct Runtime {
			System: frame_system,
			PalletWithCustomOrigin: pallet_with_custom_origin,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = MockBlock<Self>;
	}

	impl pallet_with_custom_origin::Config for Runtime {
		type RuntimeOrigin = RuntimeOrigin;
	}
}

#[frame::pallet(dev_mode)]
pub mod pallet_with_external_origin {
	use super::*;
	#[docify::export(external_origin_def)]
	#[pallet::config]
	pub trait Config: frame_system::Config {
		type ExternalOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export(external_origin_usage)]
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn externally_checked_ext(origin: OriginFor<T>) -> DispatchResult {
			let _ = T::ExternalOrigin::ensure_origin(origin)?;
			todo!();
		}
	}
}

pub mod runtime_for_external_origin {
	use super::*;
	use frame::{runtime::prelude::*, testing_prelude::*};

	construct_runtime!(
		pub struct Runtime {
			System: frame_system,
			PalletWithExternalOrigin: pallet_with_external_origin,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = MockBlock<Self>;
	}

	#[docify::export(external_origin_provide)]
	impl pallet_with_external_origin::Config for Runtime {
		type ExternalOrigin = EnsureSigned<<Self as frame_system::Config>::AccountId>;
	}
}
