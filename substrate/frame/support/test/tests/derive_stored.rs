use codec::{Encode, MaxEncodedLen};

#[test]
fn parses_struct() {
	{
		#[frame_support::stored]
		struct A<S, T>(::core::marker::PhantomData<(S, T)>);
	}
	{
		#[frame_support::stored(mel(S))]
		struct A<S, T>(::core::marker::PhantomData<(S, T)>);
	}
	{
		#[frame_support::stored(mel(S, T))]
		struct A<S, T>(::core::marker::PhantomData<(S, T)>);
	}
	{
		#[frame_support::stored(skip(S, T), mel_bound(S: MaxEncodedLen + Encode, T: MaxEncodedLen + Encode))]
		struct A<S, T>(::core::marker::PhantomData<(S, T)>);
	}
	{
		#[frame_support::stored(mel_bound(S: MaxEncodedLen + Encode, T: MaxEncodedLen + Encode))]
		struct A<S, T>(::core::marker::PhantomData<(S, T)>);
	}
}

#[test]
fn parses_enum() {
	{
		#[frame_support::stored]
		enum A<S, T> {
			A(::core::marker::PhantomData<(S, T)>),
		}
	}
	{
		#[frame_support::stored(mel(S))]
		enum A<S, T> {
			A(::core::marker::PhantomData<(S, T)>),
		}
	}
	{
		#[frame_support::stored(mel(S, T))]
		enum A<S, T> {
			A(::core::marker::PhantomData<(S, T)>),
		}
	}
	{
		#[frame_support::stored(skip(S, T), mel_bound(S: MaxEncodedLen + Encode, T: MaxEncodedLen + Encode))]
		enum A<S, T> {
			A(::core::marker::PhantomData<(S, T)>),
		}
	}
	{
		#[frame_support::stored(mel_bound(S: MaxEncodedLen + Encode, T: MaxEncodedLen + Encode))]
		enum A<S, T> {
			A(::core::marker::PhantomData<(S, T)>),
		}
	}
}
