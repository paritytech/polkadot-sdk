use frame_support::{
	construct_runtime, derive_impl, pallet_prelude::*, storage_alias, stored, CloneNoBound,
	DefaultNoBound, EqNoBound, OrdNoBound, PartialEqNoBound, PartialOrdNoBound,
	RuntimeDebugNoBound,
};
use frame_system::pallet_prelude::BlockNumberFor;
use serde::{Deserialize, Serialize};
use sp_core::sr25519;
use sp_runtime::{generic, traits::BlakeTwo256};
use test_pallet::{Config, Pallet};

pub type Signature = sr25519::Signature;
pub type BlockNumber = u32;
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<u32, RuntimeCall, Signature, ()>;

impl Config for Runtime {}

fn main() {
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

	// Unit struct
	#[stored]
	struct UnitStruct;
	#[storage_alias]
	pub type UnitStructStorage<T: Config> = StorageValue<Pallet<T>, UnitStruct, ValueQuery>;
	let _ = <UnitStructStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct UnitStructGC<T: Config> {
		pub unit: UnitStruct,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Tuple struct one value
	#[stored]
	struct TupleOneVal(u32);
	#[storage_alias]
	pub type TupleOneValStorage<T: Config> = StorageValue<Pallet<T>, TupleOneVal, ValueQuery>;
	let _ =
		<TupleOneValStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct TupleOneValGC<T: Config> {
		pub tuple_one_val: TupleOneVal,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Tuple struct multiple values
	#[stored]
	struct TupleTwoVals(u32, u64);
	#[storage_alias]
	pub type TupleTwoValsStorage<T: Config> = StorageValue<Pallet<T>, TupleTwoVals, ValueQuery>;
	let _ =
		<TupleTwoValsStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct TupleTwoValsGC<T: Config> {
		pub tuple_two_vals: TupleTwoVals,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Tuple struct with generics
	#[stored]
	struct TupleWithGenerics<T, U>(T, U);
	#[storage_alias]
	pub type TupleWithGenericsStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenerics<u32, u64>, ValueQuery>;
	let _ = <TupleWithGenericsStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct TupleWithGenericsGC<T: Config> {
		pub tuple_with_generics: TupleWithGenerics<u32, u64>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Tuple struct with generics, bound in first position
	#[stored(no_bounds(T))]
	struct TupleWithGenericsFirstBound<T: Config, U>(BlockNumberFor<T>, U);
	#[storage_alias]
	pub type TupleWithGenericsFirstBoundStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenericsFirstBound<T, u64>, ValueQuery>;
	let _ = <TupleWithGenericsFirstBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct TupleWithGenericsFirstBoundGC<T: Config> {
		pub tuple_with_generics_first_bound: TupleWithGenericsFirstBound<T, u64>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Tuple struct with generics, bound in second position
	#[stored(no_bounds(U))]
	struct TupleWithGenericsSecondBound<T, U: Config>(T, BlockNumberFor<U>);
	#[storage_alias]
	pub type TupleWithGenericsSecondBoundStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenericsSecondBound<u64, T>, ValueQuery>;
	let _ = <TupleWithGenericsSecondBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct TupleWithGenericsSecondBoundGC<T: Config> {
		pub tuple_with_generics_second_bound: TupleWithGenericsSecondBound<u64, T>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Tuple struct with generics, both bound (double generics: pass T twice)
	#[stored(no_bounds(T, U))]
	struct TupleWithGenericsBothBound<T: Config, U: Config>(BlockNumberFor<T>, BlockNumberFor<U>);
	#[storage_alias]
	pub type TupleWithGenericsBothBoundStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenericsBothBound<T, T>, ValueQuery>;
	let _ = <TupleWithGenericsBothBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct TupleWithGenericsBothBoundGC<T: Config> {
		pub tuple_with_generics_both_bound: TupleWithGenericsBothBound<T, T>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Tuple struct with generics, bound in first position, where clause
	#[stored(no_bounds(T))]
	struct TupleWithGenericsFirstBoundWhere<T, U>(BlockNumberFor<T>, U)
	where
		T: Config;
	#[storage_alias]
	pub type TupleWithGenericsFirstBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenericsFirstBoundWhere<T, u64>, ValueQuery>;
	let _ = <TupleWithGenericsFirstBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct TupleWithGenericsFirstBoundWhereGC<T: Config> {
		pub tuple_with_generics_first_bound_where: TupleWithGenericsFirstBoundWhere<T, u64>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Tuple struct with generics, bound in second position, where clause
	#[stored(no_bounds(U))]
	struct TupleWithGenericsSecondBoundWhere<T, U>(T, BlockNumberFor<U>)
	where
		U: Config;
	#[storage_alias]
	pub type TupleWithGenericsSecondBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenericsSecondBoundWhere<u64, T>, ValueQuery>;
	let _ = <TupleWithGenericsSecondBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct TupleWithGenericsSecondBoundWhereGC<T: Config> {
		pub tuple_with_generics_second_bound_where: TupleWithGenericsSecondBoundWhere<u64, T>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Tuple struct with generics, both bound, where clause
	#[stored(no_bounds(T, U))]
	struct TupleWithGenericsBothBoundWhere<T, U>(BlockNumberFor<T>, BlockNumberFor<U>)
	where
		T: Config,
		U: Config;
	#[storage_alias]
	pub type TupleWithGenericsBothBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, TupleWithGenericsBothBoundWhere<T, T>, ValueQuery>;
	let _ = <TupleWithGenericsBothBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct TupleWithGenericsBothBoundWhereGC<T: Config> {
		pub tuple_with_generics_both_bound_where: TupleWithGenericsBothBoundWhere<T, T>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Named struct one value
	#[stored]
	struct NamedOneVal {
		value: u32,
	}
	#[storage_alias]
	pub type NamedOneValStorage<T: Config> = StorageValue<Pallet<T>, NamedOneVal, ValueQuery>;
	let _ =
		<NamedOneValStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct NamedOneValGC<T: Config> {
		pub named_one_val: NamedOneVal,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Named struct one value, option
	#[stored]
	struct NamedOneValOption {
		value: Option<u32>,
	}
	#[storage_alias]
	pub type NamedOneValOptionStorage<T: Config> =
		StorageValue<Pallet<T>, NamedOneValOption, ValueQuery>;
	let _ = <NamedOneValOptionStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct NamedOneValOptionGC<T: Config> {
		pub named_one_val_option: NamedOneValOption,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Named struct multiple values
	#[stored]
	struct NamedTwoVals {
		first: u32,
		second: u64,
	}
	#[storage_alias]
	pub type NamedTwoValsStorage<T: Config> = StorageValue<Pallet<T>, NamedTwoVals, ValueQuery>;
	let _ =
		<NamedTwoValsStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct NamedTwoValsGC<T: Config> {
		pub named_two_vals: NamedTwoVals,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Named struct with generics
	#[stored]
	struct NamedWithGenerics<T, U> {
		first: T,
		second: U,
	}
	#[storage_alias]
	pub type NamedWithGenericsStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenerics<u32, u64>, ValueQuery>;
	let _ = <NamedWithGenericsStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct NamedWithGenericsGC<T: Config> {
		pub named_with_generics: NamedWithGenerics<u32, u64>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Named struct with generics, bound in first field
	#[stored(no_bounds(T))]
	struct NamedWithGenericsFirstBound<T: Config, U> {
		block: BlockNumberFor<T>,
		value: U,
	}
	#[storage_alias]
	pub type NamedWithGenericsFirstBoundStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenericsFirstBound<T, u64>, ValueQuery>;
	let _ = <NamedWithGenericsFirstBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct NamedWithGenericsFirstBoundGC<T: Config> {
		pub named_with_generics_first_bound: NamedWithGenericsFirstBound<T, u64>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Named struct with generics, bound in second field
	#[stored(no_bounds(U))]
	struct NamedWithGenericsSecondBound<T, U: Config> {
		value: T,
		block: BlockNumberFor<U>,
	}
	#[storage_alias]
	pub type NamedWithGenericsSecondBoundStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenericsSecondBound<u64, T>, ValueQuery>;
	let _ = <NamedWithGenericsSecondBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct NamedWithGenericsSecondBoundGC<T: Config> {
		pub named_with_generics_second_bound: NamedWithGenericsSecondBound<u64, T>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Named struct with generics, both bound (double generics: T, T)
	#[stored(no_bounds(T, U))]
	struct NamedWithGenericsBothBound<T: Config, U: Config> {
		value: BlockNumberFor<T>,
		block: BlockNumberFor<U>,
	}
	#[storage_alias]
	pub type NamedWithGenericsBothBoundStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenericsBothBound<T, T>, ValueQuery>;
	let _ = <NamedWithGenericsBothBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct NamedWithGenericsBothBoundGC<T: Config> {
		pub named_with_generics_both_bound: NamedWithGenericsBothBound<T, T>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Named struct with generics, bound in first field, where clause
	#[stored(no_bounds(T))]
	struct NamedWithGenericsFirstBoundWhere<T, U>
	where
		T: Config,
	{
		block: BlockNumberFor<T>,
		value: U,
	}
	#[storage_alias]
	pub type NamedWithGenericsFirstBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenericsFirstBoundWhere<T, u64>, ValueQuery>;
	let _ = <NamedWithGenericsFirstBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct NamedWithGenericsFirstBoundWhereGC<T: Config> {
		pub named_with_generics_first_bound_where: NamedWithGenericsFirstBoundWhere<T, u64>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Named struct with generics, bound in second field, where clause
	#[stored(no_bounds(U))]
	struct NamedWithGenericsSecondBoundWhere<T, U>
	where
		U: Config,
	{
		value: T,
		block: BlockNumberFor<U>,
	}
	#[storage_alias]
	pub type NamedWithGenericsSecondBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, NamedWithGenericsSecondBoundWhere<u64, T>, ValueQuery>;
	let _ = <NamedWithGenericsSecondBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct NamedWithGenericsSecondBoundWhereGC<T: Config> {
		pub named_with_generics_second_bound_where: NamedWithGenericsSecondBoundWhere<u64, T>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Named struct with generics, both bound, where clause
	#[stored(no_bounds(T, U))]
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
		StorageValue<Pallet<T>, NamedWithGenericsBothBoundWhere<T, T>, ValueQuery>;
	let _ = <NamedWithGenericsBothBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct NamedWithGenericsBothBoundWhereGC<T: Config> {
		pub named_with_generics_both_bound_where: NamedWithGenericsBothBoundWhere<T, T>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Unit enum
	#[stored]
	enum UnitEnum {
		#[default]
		A,
		B,
	}
	#[storage_alias]
	pub type UnitEnumStorage<T: Config> = StorageValue<Pallet<T>, UnitEnum, ValueQuery>;
	let _ = <UnitEnumStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct UnitEnumGC<T: Config> {
		pub unit_enum: UnitEnum,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Tuple enum
	#[stored]
	enum TupleEnum {
		#[default]
		None,
		A(u32),
		B(u64, u32),
	}
	#[storage_alias]
	pub type TupleEnumStorage<T: Config> = StorageValue<Pallet<T>, TupleEnum, ValueQuery>;
	let _ = <TupleEnumStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct TupleEnumGC<T: Config> {
		pub tuple_enum: TupleEnum,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Struct enum
	#[stored]
	enum StructEnum {
		#[default]
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
	pub type StructEnumStorage<T: Config> = StorageValue<Pallet<T>, StructEnum, ValueQuery>;
	let _ = <StructEnumStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct StructEnumGC<T: Config> {
		pub struct_enum: StructEnum,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Generic enum
	#[stored]
	enum GenericEnum<T, U> {
		#[default]
		None,
		A(T),
		B {
			first: T,
			second: U,
		},
	}
	#[storage_alias]
	pub type GenericEnumStorage<T: Config> =
		StorageValue<Pallet<T>, GenericEnum<u32, u64>, ValueQuery>;
	let _ =
		<GenericEnumStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();

	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct GenericEnumGC<T: Config> {
		pub generic_enum: GenericEnum<u32, u64>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Generic enum with no_bounds(T): first generic is exempted.
	#[stored(no_bounds(T))]
	enum GenericEnumFirstBound<T: Config, U> {
		#[default]
		A(BlockNumberFor<T>),
		B {
			value: U,
		},
	}
	#[storage_alias]
	pub type GenericEnumFirstBoundStorage<T: Config> =
		StorageValue<Pallet<T>, GenericEnumFirstBound<T, u32>, ValueQuery>;
	let _ = <GenericEnumFirstBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();
	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct GenericEnumFirstBoundGC<T: Config> {
		pub generic_enum_first_bound: GenericEnumFirstBound<T, u32>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Generic enum with no_bounds(U): second generic is exempted.
	#[stored(no_bounds(U))]
	enum GenericEnumSecondBound<T, U: Config> {
		#[default]
		A(T),
		B {
			value: BlockNumberFor<U>,
		},
	}
	#[storage_alias]
	pub type GenericEnumSecondBoundStorage<T: Config> =
		StorageValue<Pallet<T>, GenericEnumSecondBound<u32, T>, ValueQuery>;
	let _ = <GenericEnumSecondBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();
	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct GenericEnumSecondBoundGC<T: Config> {
		pub generic_enum_second_bound: GenericEnumSecondBound<u32, T>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Generic enum with no_bounds(T, U): both generics are exempted.
	#[stored(no_bounds(T, U))]
	enum GenericEnumBothBound<T: Config, U: Config> {
		#[default]
		A {
			first: BlockNumberFor<T>,
			second: BlockNumberFor<U>,
		},
		B(BlockNumberFor<T>, BlockNumberFor<U>),
	}
	#[storage_alias]
	pub type GenericEnumBothBoundStorage<T: Config> =
		StorageValue<Pallet<T>, GenericEnumBothBound<T, T>, ValueQuery>;
	let _ = <GenericEnumBothBoundStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();
	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct GenericEnumBothBoundGC<T: Config> {
		pub generic_enum_both_bound: GenericEnumBothBound<T, T>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}

	// Generic enum with a where clause and no_bounds(T)
	#[stored(no_bounds(T))]
	enum GenericEnumFirstBoundWhere<T, U>
	where
		T: Config,
	{
		#[default]
		A(BlockNumberFor<T>),
		B {
			value: U,
		},
	}
	#[storage_alias]
	pub type GenericEnumFirstBoundWhereStorage<T: Config> =
		StorageValue<Pallet<T>, GenericEnumFirstBoundWhere<T, u32>, ValueQuery>;
	let _ = <GenericEnumFirstBoundWhereStorage<Runtime> as frame_support::traits::StorageInfoTrait>::storage_info();
	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct GenericEnumFirstBoundWhereGC<T: Config> {
		pub generic_enum_first_bound_where: GenericEnumFirstBoundWhere<T, u32>,
		#[serde(skip)]
		_marker: PhantomData<T>,
	}
}
