use codec::*;
use frame::{derive::*, prelude::BlockNumberFor};
use crate::Config;

#[derive(MaxEncodedLen, Encode, Default, Decode, TypeInfo)]
pub struct TestType<T: Config<I>, I: 'static = ()> {
    pub id: u32,
    pub good: bool,
    pub generic: BlockNumberFor<T>,
    _marker: std::marker::PhantomData<I>,
}