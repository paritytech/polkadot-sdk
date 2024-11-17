//! # FRAME Origin
//!
//!
//!
//! caller of an extrinsic. Nonetheless, an account-id having signed an extrinsic is one of the
//! where the return value happens to be an account-id.
//!
//!
//!
//! to impose on the caller of the extrinsic*. One of those checks can be as simple as "*any account
//!
//! example:
//!
//!   [`pallet_democracy`] does under the hood ([`reference`]).
//! * If another consensus system, for example a bridged network or a parachain, agrees upon this.
//! * If caller holds a particular NFT.
//!
//!
//!
//! OriginFor<T>` has to be the first argument of any given callable extrinsic in FRAME:
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", call_simple)]
//!
//! [`ensure_signed`].
//!
//! [`RuntimeOrigin`]. Given the name prefix `Runtime`, we can learn that
//! amalgamated at the runtime level. Read [`frame_runtime_types`] to
//!
//! new variant to `RuntimeOrigin`.
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", custom_origin)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", runtime_exp)]
//!
//! This `RuntimeOrigin` contains a variant for the [`RawOrigin`] and the custom
//!
//! > you want to know where the actual origin of an extrinsic is set (and the signature
//! > [`CheckedExtrinsic`], specifically
//!
//!
//! convert the `<T as frame_system::Config>::RuntimeOrigin` into the local `enum Origin` of the
//! [`frame_runtime_types`].
//!
//! as follows.
//!
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", custom_origin_bound)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", custom_origin_usage)]
//!
//!
//! pallet. In other words, a pallet wants to delegate an origin check to something that is
//! adding a new associated type to `trait Config`.
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", external_origin_def)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", external_origin_usage)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_origin.rs", external_origin_provide)]
//!
//! that we know about: [`EnsureSigned`],
//! [`EnsureNone`], etc. But, there are also many more that are not known
//!
//! [`EnsureProportionMoreThan`] and many more, which is exactly what we alluded
//!
//! `EnsureOrigin`] for more inspiration.
//!
//!
//! whether they are defined within the pallet or not. But how can we obtain these abstract origins?
//!
//! > `signed` or `none` origin.
//!
//! dispatched within the runtime.
//!
//!
//! - [`A related StackExchange question.`]
//!
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

// [`A related StackExchange question.`]: https://substrate.stackexchange.com/questions/10992/how-do-you-find-the-public-key-for-the-medium-spender-track-origin
// [`Applyable`]: sp_runtime::traits::Applyable
// [`CheckedExtrinsic`]: sp_runtime::generic::CheckedExtrinsic#trait-implementations
// [`EnsureMember`]: pallet_collective::EnsureMember
// [`EnsureNone`]: frame::runtime::prelude::EnsureNone
// [`EnsureOrigin`]: frame::traits::EnsureOrigin
// [`EnsureProportionMoreThan`]: pallet_collective::EnsureProportionMoreThan
// [`EnsureRoot`]: frame::runtime::prelude::EnsureRoot
// [`EnsureSigned`]: frame::runtime::prelude::EnsureSigned
// [`EnsureSignedBy`]: frame::runtime::prelude::EnsureSignedBy
// [`Gavin Wood's speech about FRAME features at Protocol Berg 2023.`]: https://youtu.be/j7b8Upipmeg?si=83_XUgYuJxMwWX4g&t=195
// [`OriginFor`]: frame_system::pallet_prelude::OriginFor
// [`RawOrigin`]: frame_system::RawOrigin
// [`RuntimeOrigin`]: crate::reference_docs::frame_origin::runtime_for_origin::RuntimeOrigin
// [`ensure_none`]: frame_system::ensure_none
// [`ensure_signed`]: frame_system::ensure_signed
// [`frame_runtime_types`]: crate::reference_docs::frame_runtime_types
// [`implementors of
//! `EnsureOrigin`]: frame::traits::EnsureOrigin#implementors
// [`pallet_collective`]: pallet_collective
// [`pallet_democracy`]: pallet_democracy
// [`reference`]: https://github.com/paritytech/polkadot-sdk/blob/edd95b3749754d2ed0c5738588e872c87be91624/substrate/frame/democracy/src/lib.rs#L1603-L1633
