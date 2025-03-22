use frame_support::{
	construct_runtime, derive_impl, storage_alias, stored,
	pallet_prelude::{OptionQuery},
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::sr25519;
use sp_runtime::{generic, traits::BlakeTwo256};
use test_pallet::{Config, Pallet};

pub type Signature = sr25519::Signature;
pub type BlockNumber = u32;
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<u32, RuntimeCall, Signature, ()>;

impl Config for Runtime {}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
    type BaseCallFilter = frame_support::traits::Everything;
    type RuntimeOrigin = RuntimeOrigin;
    type Nonce = u64;
    type RuntimeCall = RuntimeCall;
    type Hash = sp_runtime::testing::H256;
    type Hashing = sp_runtime::traits::BlakeTwo256;
    type AccountId = u64;
    type Lookup = sp_runtime::traits::IdentityLookup<Self::AccountId>;
    type Block = Block;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = frame_support::traits::ConstU32<250>;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

construct_runtime! {
    pub struct Runtime
    {
        System: frame_system,
        TestPallet: test_pallet,
    }
}

#[test]
fn stored_compiles() {
	// Unit struct
	#[stored]
	struct UnitStruct;
	#[storage_alias]
	pub type UnitStructStorage<T: Config> = StorageValue<Pallet<T>, UnitStruct, OptionQuery>;
	let _ = <UnitStructStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Tuple struct one value
	#[stored]
	struct TupleOneVal(u32);
	#[storage_alias]
	pub type TupleOneValStorage<T: Config> = StorageValue<Pallet<T>, TupleOneVal, OptionQuery>;
	let _ =
		<TupleOneValStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Tuple struct multiple values
	#[stored]
	struct TupleTwoVals(u32, u64);
	#[storage_alias]
	pub type TupleTwoValsStorage<T: Config> = StorageValue<Pallet<T>, TupleTwoVals, OptionQuery>;
	let _ =
		<TupleTwoValsStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Tuple struct with generics
	#[stored]
	struct TupleWithGenerics<T, U>(T, U);
	#[storage_alias]
	pub type TupleWithGenericsStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenerics<u32, u64>, OptionQuery>;
	let _ = <TupleWithGenericsStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Tuple struct with generics, bound in first position
	#[stored(skip(T))]
	struct TupleWithGenericsFirstBound<T: Config, U>(BlockNumberFor<T>, U);
	#[storage_alias]
	pub type TupleWithGenericsFirstBoundStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenericsFirstBound<T, u64>, OptionQuery>;
	let _ = <TupleWithGenericsFirstBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

    // Tuple struct with generics, bound in first position, default in second
	#[stored(skip(T))]
	struct TupleWithGenericsFirstBoundDefaultSecond<T: Config, U = ()>(BlockNumberFor<T>, U);
	#[storage_alias]
	pub type TupleWithGenericsFirstBoundDefaultSecondStorage<T: Config, U = ()> =
		StorageValue<Pallet<T>, TupleWithGenericsFirstBoundDefaultSecond<T, U>, OptionQuery>;
	let _ = <TupleWithGenericsFirstBoundDefaultSecondStorage<Runtime, ()> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Tuple struct with generics, bound in second position
	#[stored(skip(U))]
	struct TupleWithGenericsSecondBound<T, U: Config>(T, BlockNumberFor<U>);
	#[storage_alias]
	pub type TupleWithGenericsSecondBoundStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenericsSecondBound<u64, T>, OptionQuery>;
	let _ = <TupleWithGenericsSecondBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Tuple struct with generics, both bound
	#[stored(skip(T, U))]
	struct TupleWithGenericsBothBound<T: Config, U: Config>(BlockNumberFor<T>, BlockNumberFor<U>);
	#[storage_alias]
	pub type TupleWithGenericsBothBoundStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenericsBothBound<T, T>, OptionQuery>;
	let _ = <TupleWithGenericsBothBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Tuple struct with generics, bound in first position, where clause
	#[stored(skip(T))]
	struct TupleWithGenericsFirstBoundWhere<T, U>(BlockNumberFor<T>, U)
	where
		T: Config;
	#[storage_alias]
	pub type TupleWithGenericsFirstBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenericsFirstBoundWhere<T, u64>, OptionQuery>;
	let _ = <TupleWithGenericsFirstBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Tuple struct with generics, bound in second position, where clause
	#[stored(skip(U))]
	struct TupleWithGenericsSecondBoundWhere<T, U>(T, BlockNumberFor<U>)
	where
		U: Config;
	#[storage_alias]
	pub type TupleWithGenericsSecondBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenericsSecondBoundWhere<u64, T>, OptionQuery>;
	let _ = <TupleWithGenericsSecondBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Tuple struct with generics, both bound, where clause
	#[stored(skip(T, U))]
	struct TupleWithGenericsBothBoundWhere<T, U>(BlockNumberFor<T>, BlockNumberFor<U>)
	where
		T: Config,
		U: Config;
	#[storage_alias]
	pub type TupleWithGenericsBothBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenericsBothBoundWhere<T, T>, OptionQuery>;
	let _ = <TupleWithGenericsBothBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Named struct one value
	#[stored]
	struct NamedOneVal {
		value: u32,
	}
	#[storage_alias]
	pub type NamedOneValStorage<T: Config> = StorageValue<Pallet<T>, NamedOneVal, OptionQuery>;
	let _ =
		<NamedOneValStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Named struct one value, option
	#[stored]
	struct NamedOneValOption {
		value: Option<u32>,
	}
	#[storage_alias]
	pub type NamedOneValOptionStorage<T: Config> =
		StorageValue<Pallet<T>, NamedOneValOption, OptionQuery>;
	let _ = <NamedOneValOptionStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Named struct multiple values
	#[stored]
	struct NamedTwoVals {
		first: u32,
		second: u64,
	}
	#[storage_alias]
	pub type NamedTwoValsStorage<T: Config> = StorageValue<Pallet<T>, NamedTwoVals, OptionQuery>;
	let _ =
		<NamedTwoValsStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Named struct with generics
	#[stored]
	struct NamedWithGenerics<T, U> {
		first: T,
		second: U,
	}
	#[storage_alias]
	pub type NamedWithGenericsStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenerics<u32, u64>, OptionQuery>;
	let _ = <NamedWithGenericsStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Named struct with generics, bound in first field
	#[stored(skip(T))]
	struct NamedWithGenericsFirstBound<T: Config, U> {
		block: BlockNumberFor<T>,
		value: U,
	}
	#[storage_alias]
	pub type NamedWithGenericsFirstBoundStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenericsFirstBound<T, u64>, OptionQuery>;
	let _ = <NamedWithGenericsFirstBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Named struct with generics, bound in second field
	#[stored(skip(U))]
	struct NamedWithGenericsSecondBound<T, U: Config> {
		value: T,
		block: BlockNumberFor<U>,
	}
	#[storage_alias]
	pub type NamedWithGenericsSecondBoundStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenericsSecondBound<u64, T>, OptionQuery>;
	let _ = <NamedWithGenericsSecondBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Named struct with generics, both bound
	#[stored(skip(T, U))]
	struct NamedWithGenericsBothBound<T: Config, U: Config> {
		value: BlockNumberFor<T>,
		block: BlockNumberFor<U>,
	}
	#[storage_alias]
	pub type NamedWithGenericsBothBoundStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenericsBothBound<T, T>, OptionQuery>;
	let _ = <NamedWithGenericsBothBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Named struct with generics, bound in first field, where clause
	#[stored(skip(T))]
	struct NamedWithGenericsFirstBoundWhere<T, U>
	where
		T: Config,
	{
		block: BlockNumberFor<T>,
		value: U,
	}
	#[storage_alias]
	pub type NamedWithGenericsFirstBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenericsFirstBoundWhere<T, u64>, OptionQuery>;
	let _ = <NamedWithGenericsFirstBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Named struct with generics, bound in second field, where clause
	#[stored(skip(U))]
	struct NamedWithGenericsSecondBoundWhere<T, U>
	where
		U: Config,
	{
		value: T,
		block: BlockNumberFor<U>,
	}
	#[storage_alias]
	pub type NamedWithGenericsSecondBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenericsSecondBoundWhere<u64, T>, OptionQuery>;
	let _ = <NamedWithGenericsSecondBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Named struct with generics, both bound, where clause
	#[stored(skip(T, U))]
	struct NamedWithGenericsBothBoundWhere<T, U>
	where
		T: Config,
		U: Config,
	{
		value: BlockNumberFor<T>,
		block: BlockNumberFor<U>,
	}
	#[storage_alias]
	pub type NamedWithGenericsBothBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenericsBothBoundWhere<T, T>, OptionQuery>;
	let _ = <NamedWithGenericsBothBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Unit enum
	#[stored]
	enum UnitEnum {
		A,
		B,
	}
	#[storage_alias]
	pub type UnitEnumStorage<T: Config> = StorageValue<Pallet<T>, UnitEnum, OptionQuery>;
	let _ = <UnitEnumStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Tuple enum
	#[stored]
	enum TupleEnum {
		None,
		A(u32),
		B(u64, u32),
	}
	#[storage_alias]
	pub type TupleEnumStorage<T: Config> = StorageValue<Pallet<T>, TupleEnum, OptionQuery>;
	let _ = <TupleEnumStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Struct enum
	#[stored]
	enum StructEnum {
		None,
		A {
			x: u32,
		},
		B {
			y: u64,
			z: u32,
		},
	}
	#[storage_alias]
	pub type StructEnumStorage<T: Config> = StorageValue<Pallet<T>, StructEnum, OptionQuery>;
	let _ = <StructEnumStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Generic enum
	#[stored]
	enum GenericEnum<T, U> {
		None,
		A(T),
		B {
			first: T,
			second: U,
		},
	}
	#[storage_alias]
	pub type GenericEnumStorage<T: Config> =
		StorageValue<Pallet<T>, GenericEnum<u32, u64>, OptionQuery>;
	let _ =
		<GenericEnumStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Generic enum, first generic bounded
	#[stored(skip(T))]
	enum GenericEnumFirstBound<T: Config, U> {
		A(BlockNumberFor<T>),
		B {
			value: U,
		},
	}
	#[storage_alias]
	pub type GenericEnumFirstBoundStorage<T: Config> =
		StorageValue<Pallet<T>, GenericEnumFirstBound<T, u32>, OptionQuery>;
	let _ = <GenericEnumFirstBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

    // Generic enum, first bounded, second has default
	#[stored(skip(T))]
	enum GenericEnumFirstBoundSecondDefault<T: Config, U = ()> {
		A(BlockNumberFor<T>),
		B {
			value: U,
		},
	}
	#[storage_alias]
	pub type GenericEnumFirstBoundSecondDefaultStorage<T: Config> =
		StorageValue<Pallet<T>, GenericEnumFirstBoundSecondDefault<T, u32>, OptionQuery>;
	let _ = <GenericEnumFirstBoundSecondDefaultStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Generic enum, second bounded
	#[stored(skip(U))]
	enum GenericEnumSecondBound<T, U: Config> {
		A(T),
		B {
			value: BlockNumberFor<U>,
		},
	}
	#[storage_alias]
	pub type GenericEnumSecondBoundStorage<T: Config> =
		StorageValue<Pallet<T>, GenericEnumSecondBound<u32, T>, OptionQuery>;
	let _ = <GenericEnumSecondBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Generic enum, both bounded
	#[stored(skip(T, U))]
	enum GenericEnumBothBound<T: Config, U: Config> {
		A {
			first: BlockNumberFor<T>,
			second: BlockNumberFor<U>,
		},
		B(BlockNumberFor<T>, BlockNumberFor<U>),
	}
	#[storage_alias]
	pub type GenericEnumBothBoundStorage<T: Config> =
		StorageValue<Pallet<T>, GenericEnumBothBound<T, T>, OptionQuery>;
	let _ = <GenericEnumBothBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Generic enum, trait bound in where clause
	#[stored(skip(T))]
	enum GenericEnumFirstBoundWhere<T, U>
	where
		T: Config,
	{
		A(BlockNumberFor<T>),
		B {
			value: U,
		},
	}
	#[storage_alias]
	pub type GenericEnumFirstBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, GenericEnumFirstBoundWhere<T, u32>, OptionQuery>;
	let _ = <GenericEnumFirstBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Empty codec_bounds
	#[stored(skip(T, U), codec_bounds())]
	enum CodecBoundEmpty<T: Config, U: Config> {
		A(BlockNumberFor<T>),
		B(BlockNumberFor<U>),
	}
	#[storage_alias]
	pub type CodecBoundEmptyStorage<T: Config> =
		StorageValue<Pallet<T>, CodecBoundEmpty<T, T>, OptionQuery>;
	let _ = <CodecBoundEmptyStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Shorthand codec_bounds
	#[stored(codec_bounds(T, U))]
	enum CodecBoundShorthand<T, U> {
		A {
			value: T,
		},
		B {
			value: U,
		},
	}
	#[storage_alias]
	pub type CodecBoundShorthandStorage<T: Config> =
		StorageValue<Pallet<T>, CodecBoundShorthand<u32, u32>, OptionQuery>;
	let _ = <CodecBoundShorthandStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	// Explicit codec_bounds
	#[stored(codec_bounds(T: MaxEncodedLen, U))]
	enum CodecBoundExplicit<T, U> {
		A {
			value: T,
		},
		B {
			value: U,
		},
	}
	#[storage_alias]
	pub type CodecBoundExplicitStorage<T: Config> =
		StorageValue<Pallet<T>, CodecBoundExplicit<u32, u32>, OptionQuery>;
	let _ = <CodecBoundExplicitStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();
}
