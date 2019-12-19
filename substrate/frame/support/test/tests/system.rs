use frame_support::codec::{Encode, Decode, EncodeLike};

pub trait Trait: 'static + Eq + Clone {
	type Origin: Into<Result<RawOrigin<Self::AccountId>, Self::Origin>>
		+ From<RawOrigin<Self::AccountId>>;

	type BlockNumber: Decode + Encode + EncodeLike + Clone + Default;
	type Hash;
	type AccountId: Encode + EncodeLike + Decode;
	type Event: From<Event>;
	type ModuleToIndex: frame_support::traits::ModuleToIndex;
}

frame_support::decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
	}
}

impl<T: Trait> Module<T> {
	pub fn deposit_event(_event: impl Into<T::Event>) {
	}
}

frame_support::decl_event!(
	pub enum Event {
		ExtrinsicSuccess,
		ExtrinsicFailed,
	}
);

frame_support::decl_error! {
	pub enum Error for Module<T: Trait> {
		/// Test error documentation
		TestError,
		/// Error documentation
		/// with multiple lines
		AnotherError
	}
}

/// Origin for the system module.
#[derive(PartialEq, Eq, Clone, sp_runtime::RuntimeDebug)]
pub enum RawOrigin<AccountId> {
	Root,
	Signed(AccountId),
	None,
}

impl<AccountId> From<Option<AccountId>> for RawOrigin<AccountId> {
	fn from(s: Option<AccountId>) -> RawOrigin<AccountId> {
		match s {
			Some(who) => RawOrigin::Signed(who),
			None => RawOrigin::None,
		}
	}
}

pub type Origin<T> = RawOrigin<<T as Trait>::AccountId>;

#[allow(dead_code)]
pub fn ensure_root<OuterOrigin, AccountId>(o: OuterOrigin) -> Result<(), &'static str>
	where OuterOrigin: Into<Result<RawOrigin<AccountId>, OuterOrigin>>
{
	o.into().map(|_| ()).map_err(|_| "bad origin: expected to be a root origin")
}
