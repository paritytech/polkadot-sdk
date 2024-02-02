use codec::{Encode, MaxEncodedLen};

#[test]
fn parses() {
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
