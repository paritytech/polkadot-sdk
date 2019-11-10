// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

#[cfg(test)]
// Do not complain about unused `dispatch` and `dispatch_aux`.
#[allow(dead_code)]
mod tests {
	use support::metadata::*;
	use std::marker::PhantomData;
	use codec::{Encode, Decode, EncodeLike};

	support::decl_module! {
		pub struct Module<T: Trait> for enum Call where origin: T::Origin {}
	}

	pub trait Trait {
		type Origin: Encode + Decode + EncodeLike + std::default::Default;
		type BlockNumber;
	}

	support::decl_storage! {
		trait Store for Module<T: Trait> as TestStorage {
			// non-getters: pub / $default

			/// Hello, this is doc!
			U32 : Option<u32>;
			pub PUBU32 : Option<u32>;
			U32MYDEF : Option<u32>;
			pub PUBU32MYDEF : Option<u32>;

			// getters: pub / $default
			// we need at least one type which uses T, otherwise GenesisConfig will complain.
			GETU32 get(fn u32_getter): T::Origin;
			pub PUBGETU32 get(fn pub_u32_getter) build(|config: &GenesisConfig| config.u32_getter_with_config): u32;
			GETU32WITHCONFIG get(fn u32_getter_with_config) config(): u32;
			pub PUBGETU32WITHCONFIG get(fn pub_u32_getter_with_config) config(): u32;
			GETU32MYDEF get(fn u32_getter_mydef): Option<u32>;
			pub PUBGETU32MYDEF get(fn pub_u32_getter_mydef) config(): u32 = 3;
			GETU32WITHCONFIGMYDEF get(fn u32_getter_with_config_mydef) config(): u32 = 2;
			pub PUBGETU32WITHCONFIGMYDEF get(fn pub_u32_getter_with_config_mydef) config(): u32 = 1;
			PUBGETU32WITHCONFIGMYDEFOPT get(fn pub_u32_getter_with_config_mydef_opt) config(): Option<u32>;

			// map non-getters: pub / $default
			MAPU32 : map u32 => Option<String>;
			pub PUBMAPU32 : map u32 => Option<String>;
			MAPU32MYDEF : map u32 => Option<String>;
			pub PUBMAPU32MYDEF : map u32 => Option<String>;

			// map getters: pub / $default
			GETMAPU32 get(fn map_u32_getter): map u32 => String;
			pub PUBGETMAPU32 get(fn pub_map_u32_getter): map u32 => String;

			GETMAPU32MYDEF get(fn map_u32_getter_mydef): map u32 => String = "map".into();
			pub PUBGETMAPU32MYDEF get(fn pub_map_u32_getter_mydef): map u32 => String = "pubmap".into();

			// linked map
			LINKEDMAPU32 : linked_map u32 => Option<String>;
			pub PUBLINKEDMAPU32MYDEF : linked_map u32 => Option<String>;
			GETLINKEDMAPU32 get(fn linked_map_u32_getter): linked_map u32 => String;
			pub PUBGETLINKEDMAPU32MYDEF get(fn pub_linked_map_u32_getter_mydef): linked_map u32 => String = "pubmap".into();

			COMPLEXTYPE1: ::std::vec::Vec<<T as Trait>::Origin>;
			COMPLEXTYPE2: (Vec<Vec<(u16,Box<(  )>)>>, u32);
			COMPLEXTYPE3: ([u32;25]);
		}
		add_extra_genesis {
			build(|_| {});
		}
	}

	struct TraitImpl {}

	impl Trait for TraitImpl {
		type Origin = u32;
		type BlockNumber = u32;
	}

	const EXPECTED_METADATA: StorageMetadata = StorageMetadata {
		prefix: DecodeDifferent::Encode("TestStorage"),
		entries: DecodeDifferent::Encode(
			&[
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("U32"),
					modifier: StorageEntryModifier::Optional,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("u32")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructU32(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[ " Hello, this is doc!" ]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBU32"),
					modifier: StorageEntryModifier::Optional,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("u32")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBU32(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("U32MYDEF"),
					modifier: StorageEntryModifier::Optional,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("u32")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructU32MYDEF(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBU32MYDEF"),
					modifier: StorageEntryModifier::Optional,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("u32")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBU32MYDEF(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("GETU32"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("T::Origin")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructGETU32(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBGETU32"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("u32")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBGETU32(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("GETU32WITHCONFIG"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("u32")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructGETU32WITHCONFIG(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBGETU32WITHCONFIG"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("u32")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBGETU32WITHCONFIG(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("GETU32MYDEF"),
					modifier: StorageEntryModifier::Optional,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("u32")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructGETU32MYDEF(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBGETU32MYDEF"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("u32")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBGETU32MYDEF(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("GETU32WITHCONFIGMYDEF"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("u32")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructGETU32WITHCONFIGMYDEF(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBGETU32WITHCONFIGMYDEF"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("u32")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBGETU32WITHCONFIGMYDEF(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBGETU32WITHCONFIGMYDEFOPT"),
					modifier: StorageEntryModifier::Optional,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("u32")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBGETU32WITHCONFIGMYDEFOPT(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},

				StorageEntryMetadata {
					name: DecodeDifferent::Encode("MAPU32"),
					modifier: StorageEntryModifier::Optional,
					ty: StorageEntryType::Map {
						hasher: StorageHasher::Blake2_256,
						key: DecodeDifferent::Encode("u32"),
						value: DecodeDifferent::Encode("String"),
						is_linked: false,
					},
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructMAPU32(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBMAPU32"),
					modifier: StorageEntryModifier::Optional,
					ty: StorageEntryType::Map {
						hasher: StorageHasher::Blake2_256,
						key: DecodeDifferent::Encode("u32"),
						value: DecodeDifferent::Encode("String"),
						is_linked: false,
					},
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBMAPU32(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("MAPU32MYDEF"),
					modifier: StorageEntryModifier::Optional,
					ty: StorageEntryType::Map {
						hasher: StorageHasher::Blake2_256,
						key: DecodeDifferent::Encode("u32"),
						value: DecodeDifferent::Encode("String"),
						is_linked: false,
					},
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructMAPU32MYDEF(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBMAPU32MYDEF"),
					modifier: StorageEntryModifier::Optional,
					ty: StorageEntryType::Map {
						hasher: StorageHasher::Blake2_256,
						key: DecodeDifferent::Encode("u32"),
						value: DecodeDifferent::Encode("String"),
						is_linked: false,
					},
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBMAPU32MYDEF(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("GETMAPU32"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Map {
						hasher: StorageHasher::Blake2_256,
						key: DecodeDifferent::Encode("u32"),
						value: DecodeDifferent::Encode("String"),
						is_linked: false,
					},
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructGETMAPU32(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBGETMAPU32"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Map {
						hasher: StorageHasher::Blake2_256,
						key: DecodeDifferent::Encode("u32"),
						value: DecodeDifferent::Encode("String"),
						is_linked: false,
					},
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBGETMAPU32(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("GETMAPU32MYDEF"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Map {
						hasher: StorageHasher::Blake2_256,
						key: DecodeDifferent::Encode("u32"),
						value: DecodeDifferent::Encode("String"),
						is_linked: false,
					},
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructGETMAPU32MYDEF(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBGETMAPU32MYDEF"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Map {
						hasher: StorageHasher::Blake2_256,
						key: DecodeDifferent::Encode("u32"),
						value: DecodeDifferent::Encode("String"),
						is_linked: false,
					},
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBGETMAPU32MYDEF(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("LINKEDMAPU32"),
					modifier: StorageEntryModifier::Optional,
					ty: StorageEntryType::Map {
						hasher: StorageHasher::Blake2_256,
						key: DecodeDifferent::Encode("u32"),
						value: DecodeDifferent::Encode("String"),
						is_linked: true,
					},
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructLINKEDMAPU32(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBLINKEDMAPU32MYDEF"),
					modifier: StorageEntryModifier::Optional,
					ty: StorageEntryType::Map {
						hasher: StorageHasher::Blake2_256,
						key: DecodeDifferent::Encode("u32"),
						value: DecodeDifferent::Encode("String"),
						is_linked: true,
					},
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBLINKEDMAPU32MYDEF(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("GETLINKEDMAPU32"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Map {
						hasher: StorageHasher::Blake2_256,
						key: DecodeDifferent::Encode("u32"),
						value: DecodeDifferent::Encode("String"),
						is_linked: true,
					},
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructGETLINKEDMAPU32(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("PUBGETLINKEDMAPU32MYDEF"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Map {
						hasher: StorageHasher::Blake2_256,
						key: DecodeDifferent::Encode("u32"),
						value: DecodeDifferent::Encode("String"),
						is_linked: true,
					},
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructPUBGETLINKEDMAPU32MYDEF(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("COMPLEXTYPE1"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("::std::vec::Vec<<T as Trait>::Origin>")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructCOMPLEXTYPE1(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("COMPLEXTYPE2"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("(Vec<Vec<(u16, Box<()>)>>, u32)")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructCOMPLEXTYPE2(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
				StorageEntryMetadata {
					name: DecodeDifferent::Encode("COMPLEXTYPE3"),
					modifier: StorageEntryModifier::Default,
					ty: StorageEntryType::Plain(DecodeDifferent::Encode("([u32; 25])")),
					default: DecodeDifferent::Encode(
						DefaultByteGetter(&__GetByteStructCOMPLEXTYPE3(PhantomData::<TraitImpl>))
					),
					documentation: DecodeDifferent::Encode(&[]),
				},
			]
		),
	};

	#[test]
	fn store_metadata() {
		let metadata = Module::<TraitImpl>::storage_metadata();
		assert_eq!(EXPECTED_METADATA, metadata);
	}

	#[test]
	fn check_genesis_config() {
		let config = GenesisConfig::default();
		assert_eq!(config.u32_getter_with_config, 0u32);
		assert_eq!(config.pub_u32_getter_with_config, 0u32);

		assert_eq!(config.pub_u32_getter_mydef, 3u32);
		assert_eq!(config.u32_getter_with_config_mydef, 2u32);
		assert_eq!(config.pub_u32_getter_with_config_mydef, 1u32);
		assert_eq!(config.pub_u32_getter_with_config_mydef_opt, 0u32);
	}

}

#[cfg(test)]
#[allow(dead_code)]
mod test2 {
	pub trait Trait {
		type Origin;
		type BlockNumber;
	}

	support::decl_module! {
		pub struct Module<T: Trait> for enum Call where origin: T::Origin {}
	}

	type PairOf<T> = (T, T);

	support::decl_storage! {
		trait Store for Module<T: Trait> as TestStorage {
			SingleDef : u32;
			PairDef : PairOf<u32>;
			Single : Option<u32>;
			Pair : (u32, u32);
		}
		add_extra_genesis {
			config(_marker) : ::std::marker::PhantomData<T>;
			config(extra_field) : u32 = 32;
			build(|_| {});
		}
	}

	struct TraitImpl {}

	impl Trait for TraitImpl {
		type Origin = u32;
		type BlockNumber = u32;
	}
}

#[cfg(test)]
#[allow(dead_code)]
mod test3 {
	pub trait Trait {
		type Origin;
		type BlockNumber;
	}
	support::decl_module! {
		pub struct Module<T: Trait> for enum Call where origin: T::Origin {}
	}
	support::decl_storage! {
		trait Store for Module<T: Trait> as Test {
			Foo get(fn foo) config(initial_foo): u32;
		}
	}

	type PairOf<T> = (T, T);

	struct TraitImpl {}

	impl Trait for TraitImpl {
		type Origin = u32;
		type BlockNumber = u32;
	}
}

#[cfg(test)]
#[allow(dead_code)]
mod test_append_and_len {
	use runtime_io::TestExternalities;
	use codec::{Encode, Decode};

	pub trait Trait {
		type Origin;
		type BlockNumber;
	}

	support::decl_module! {
		pub struct Module<T: Trait> for enum Call where origin: T::Origin {}
	}

	#[derive(PartialEq, Eq, Clone, Encode, Decode)]
	struct NoDef(u32);

	support::decl_storage! {
		trait Store for Module<T: Trait> as Test {
			NoDefault: Option<NoDef>;

			JustVec: Vec<u32>;
			JustVecWithDefault: Vec<u32> = vec![6, 9];
			OptionVec: Option<Vec<u32>>;

			MapVec: map u32 => Vec<u32>;
			MapVecWithDefault: map u32 => Vec<u32> = vec![6, 9];
			OptionMapVec: map u32 => Option<Vec<u32>>;

			LinkedMapVec: linked_map u32 => Vec<u32>;
			LinkedMapVecWithDefault: linked_map u32 => Vec<u32> = vec![6, 9];
			OptionLinkedMapVec: linked_map u32 => Option<Vec<u32>>;
		}
	}

	struct Test {}

	impl Trait for Test {
		type Origin = u32;
		type BlockNumber = u32;
	}

	#[test]
	fn default_for_option() {
		TestExternalities::default().execute_with(|| {
			assert_eq!(OptionVec::get(), None);
			assert_eq!(JustVec::get(), vec![]);
		});
	}

	#[test]
	fn append_works() {
		TestExternalities::default().execute_with(|| {
			let _ = MapVec::append(1, [1, 2, 3].iter());
			let _ = MapVec::append(1, [4, 5].iter());
			assert_eq!(MapVec::get(1), vec![1, 2, 3, 4, 5]);

			let _ = JustVec::append([1, 2, 3].iter());
			let _ = JustVec::append([4, 5].iter());
			assert_eq!(JustVec::get(), vec![1, 2, 3, 4, 5]);
		});
	}

	#[test]
	fn append_works_for_default() {
		TestExternalities::default().execute_with(|| {
			assert_eq!(JustVecWithDefault::get(), vec![6, 9]);
			let _ = JustVecWithDefault::append([1].iter());
			assert_eq!(JustVecWithDefault::get(), vec![6, 9, 1]);

			assert_eq!(MapVecWithDefault::get(0), vec![6, 9]);
			let _ = MapVecWithDefault::append(0, [1].iter());
			assert_eq!(MapVecWithDefault::get(0), vec![6, 9, 1]);

			assert_eq!(OptionVec::get(), None);
			let _ = OptionVec::append([1].iter());
			assert_eq!(OptionVec::get(), Some(vec![1]));
		});
	}

	#[test]
	fn append_or_put_works() {
		TestExternalities::default().execute_with(|| {
			let _ = MapVec::append_or_insert(1, &[1, 2, 3][..]);
			let _ = MapVec::append_or_insert(1, &[4, 5][..]);
			assert_eq!(MapVec::get(1), vec![1, 2, 3, 4, 5]);

			let _ = JustVec::append_or_put(&[1, 2, 3][..]);
			let _ = JustVec::append_or_put(&[4, 5][..]);
			assert_eq!(JustVec::get(), vec![1, 2, 3, 4, 5]);

			let _ = OptionVec::append_or_put(&[1, 2, 3][..]);
			let _ = OptionVec::append_or_put(&[4, 5][..]);
			assert_eq!(OptionVec::get(), Some(vec![1, 2, 3, 4, 5]));
		});
	}

	#[test]
	fn len_works() {
		TestExternalities::default().execute_with(|| {
			JustVec::put(&vec![1, 2, 3, 4]);
			OptionVec::put(&vec![1, 2, 3, 4, 5]);
			MapVec::insert(1, &vec![1, 2, 3, 4, 5, 6]);
			LinkedMapVec::insert(2, &vec![1, 2, 3]);

			assert_eq!(JustVec::decode_len().unwrap(), 4);
			assert_eq!(OptionVec::decode_len().unwrap(), 5);
			assert_eq!(MapVec::decode_len(1).unwrap(), 6);
			assert_eq!(LinkedMapVec::decode_len(2).unwrap(), 3);
		});
	}

	#[test]
	fn len_works_for_default() {
		TestExternalities::default().execute_with(|| {
			// vec
			assert_eq!(JustVec::get(), vec![]);
			assert_eq!(JustVec::decode_len(), Ok(0));

			assert_eq!(JustVecWithDefault::get(), vec![6, 9]);
			assert_eq!(JustVecWithDefault::decode_len(), Ok(2));

			assert_eq!(OptionVec::get(), None);
			assert_eq!(OptionVec::decode_len(), Ok(0));

			// map
			assert_eq!(MapVec::get(0), vec![]);
			assert_eq!(MapVec::decode_len(0), Ok(0));

			assert_eq!(MapVecWithDefault::get(0), vec![6, 9]);
			assert_eq!(MapVecWithDefault::decode_len(0), Ok(2));

			assert_eq!(OptionMapVec::get(0), None);
			assert_eq!(OptionMapVec::decode_len(0), Ok(0));

			// linked map
			assert_eq!(LinkedMapVec::get(0), vec![]);
			assert_eq!(LinkedMapVec::decode_len(0), Ok(0));

			assert_eq!(LinkedMapVecWithDefault::get(0), vec![6, 9]);
			assert_eq!(LinkedMapVecWithDefault::decode_len(0), Ok(2));

			assert_eq!(OptionLinkedMapVec::get(0), None);
			assert_eq!(OptionLinkedMapVec::decode_len(0), Ok(0));
		});
	}
}
