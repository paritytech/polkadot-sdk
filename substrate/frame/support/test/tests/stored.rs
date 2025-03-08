#[test]
fn stored_compiles() {
    use frame::prelude::*;

    // Unit struct
    #[stored]
    struct UnitStruct;
    #[storage_alias]
    pub type UnitStructStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        UnitStruct,
        ValueQuery,
    >;
    let _ = <UnitStructStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct UnitStructGC<T: crate::Config> {
        pub unit: UnitStruct,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Tuple struct one value
    #[stored]
    struct TupleOneVal(u32);
    #[storage_alias]
    pub type TupleOneValStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        TupleOneVal,
        ValueQuery,
    >;
    let _ = <TupleOneValStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct TupleOneValGC<T: crate::Config> {
        pub tuple_one_val: TupleOneVal,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Tuple struct multiple values
    #[stored]
    struct TupleTwoVals(u32, u64);
    #[storage_alias]
    pub type TupleTwoValsStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        TupleTwoVals,
        ValueQuery,
    >;
    let _ = <TupleTwoValsStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct TupleTwoValsGC<T: crate::Config> {
        pub tuple_two_vals: TupleTwoVals,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Tuple struct with generics
    #[stored]
    struct TupleWithGenerics<T, U>(T, U);
    #[storage_alias]
    pub type TupleWithGenericsStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        TupleWithGenerics<u32, u64>,
        ValueQuery,
    >;
    let _ = <TupleWithGenericsStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct TupleWithGenericsGC<T: crate::Config> {
        pub tuple_with_generics: TupleWithGenerics<u32, u64>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Tuple struct with generics, bound in first position
    #[stored(no_bounds(T))]
    struct TupleWithGenericsFirstBound<T: crate::Config, U>(BlockNumberFor<T>, U);
    #[storage_alias]
    pub type TupleWithGenericsFirstBoundStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        TupleWithGenericsFirstBound<T, u64>,
        ValueQuery,
    >;
    let _ = <TupleWithGenericsFirstBoundStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct TupleWithGenericsFirstBoundGC<T: crate::Config> {
        pub tuple_with_generics_first_bound: TupleWithGenericsFirstBound<T, u64>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Tuple struct with generics, bound in second position
    #[stored(no_bounds(U))]
    struct TupleWithGenericsSecondBound<T, U: crate::Config>(T, BlockNumberFor<U>);
    #[storage_alias]
    pub type TupleWithGenericsSecondBoundStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        TupleWithGenericsSecondBound<u64, T>,
        ValueQuery,
    >;
    let _ = <TupleWithGenericsSecondBoundStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct TupleWithGenericsSecondBoundGC<T: crate::Config> {
        pub tuple_with_generics_second_bound: TupleWithGenericsSecondBound<u64, T>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Tuple struct with generics, both bound (double generics: pass T twice)
    #[stored(no_bounds(T, U))]
    struct TupleWithGenericsBothBound<T: crate::Config, U: crate::Config>(BlockNumberFor<T>, BlockNumberFor<U>);
    #[storage_alias]
    pub type TupleWithGenericsBothBoundStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        TupleWithGenericsBothBound<T, T>,
        ValueQuery,
    >;
    let _ = <TupleWithGenericsBothBoundStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct TupleWithGenericsBothBoundGC<T: crate::Config> {
        pub tuple_with_generics_both_bound: TupleWithGenericsBothBound<T, T>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Tuple struct with generics, bound in first position, where clause
    #[stored(no_bounds(T))]
    struct TupleWithGenericsFirstBoundWhere<T, U>(BlockNumberFor<T>, U)
    where T: crate::Config;
    #[storage_alias]
    pub type TupleWithGenericsFirstBoundWhereStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        TupleWithGenericsFirstBoundWhere<T, u64>,
        ValueQuery,
    >;
    let _ = <TupleWithGenericsFirstBoundWhereStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct TupleWithGenericsFirstBoundWhereGC<T: crate::Config> {
        pub tuple_with_generics_first_bound_where: TupleWithGenericsFirstBoundWhere<T, u64>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Tuple struct with generics, bound in second position, where clause
    #[stored(no_bounds(U))]
    struct TupleWithGenericsSecondBoundWhere<T, U>(T, BlockNumberFor<U>)
    where U: crate::Config;
    #[storage_alias]
    pub type TupleWithGenericsSecondBoundWhereStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        TupleWithGenericsSecondBoundWhere<u64, T>,
        ValueQuery,
    >;
    let _ = <TupleWithGenericsSecondBoundWhereStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct TupleWithGenericsSecondBoundWhereGC<T: crate::Config> {
        pub tuple_with_generics_second_bound_where: TupleWithGenericsSecondBoundWhere<u64, T>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Tuple struct with generics, both bound, where clause
    #[stored(no_bounds(T, U))]
    struct TupleWithGenericsBothBoundWhere<T, U>(BlockNumberFor<T>, BlockNumberFor<U>)
    where T: crate::Config, U: crate::Config;
    #[storage_alias]
    pub type TupleWithGenericsBothBoundWhereStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        TupleWithGenericsBothBoundWhere<T, T>,
        ValueQuery,
    >;
    let _ = <TupleWithGenericsBothBoundWhereStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct TupleWithGenericsBothBoundWhereGC<T: crate::Config> {
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
    pub type NamedOneValStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        NamedOneVal,
        ValueQuery,
    >;
    let _ = <NamedOneValStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct NamedOneValGC<T: crate::Config> {
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
    pub type NamedOneValOptionStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        NamedOneValOption,
        ValueQuery,
    >;
    let _ = <NamedOneValOptionStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct NamedOneValOptionGC<T: crate::Config> {
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
    pub type NamedTwoValsStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        NamedTwoVals,
        ValueQuery,
    >;
    let _ = <NamedTwoValsStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct NamedTwoValsGC<T: crate::Config> {
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
    pub type NamedWithGenericsStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        NamedWithGenerics<u32, u64>,
        ValueQuery,
    >;
    let _ = <NamedWithGenericsStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct NamedWithGenericsGC<T: crate::Config> {
        pub named_with_generics: NamedWithGenerics<u32, u64>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Named struct with generics, bound in first field
    #[stored(no_bounds(T))]
    struct NamedWithGenericsFirstBound<T: crate::Config, U> {
        block: BlockNumberFor<T>,
        value: U,
    }
    #[storage_alias]
    pub type NamedWithGenericsFirstBoundStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        NamedWithGenericsFirstBound<T, u64>,
        ValueQuery,
    >;
    let _ = <NamedWithGenericsFirstBoundStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct NamedWithGenericsFirstBoundGC<T: crate::Config> {
        pub named_with_generics_first_bound: NamedWithGenericsFirstBound<T, u64>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Named struct with generics, bound in second field
    #[stored(no_bounds(U))]
    struct NamedWithGenericsSecondBound<T, U: crate::Config> {
        value: T,
        block: BlockNumberFor<U>,
    }
    #[storage_alias]
    pub type NamedWithGenericsSecondBoundStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        NamedWithGenericsSecondBound<u64, T>,
        ValueQuery,
    >;
    let _ = <NamedWithGenericsSecondBoundStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct NamedWithGenericsSecondBoundGC<T: crate::Config> {
        pub named_with_generics_second_bound: NamedWithGenericsSecondBound<u64, T>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Named struct with generics, both bound (double generics: T, T)
    #[stored(no_bounds(T, U))]
    struct NamedWithGenericsBothBound<T: crate::Config, U: crate::Config> {
        value: BlockNumberFor<T>,
        block: BlockNumberFor<U>,
    }
    #[storage_alias]
    pub type NamedWithGenericsBothBoundStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        NamedWithGenericsBothBound<T, T>,
        ValueQuery,
    >;
    let _ = <NamedWithGenericsBothBoundStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct NamedWithGenericsBothBoundGC<T: crate::Config> {
        pub named_with_generics_both_bound: NamedWithGenericsBothBound<T, T>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Named struct with generics, bound in first field, where clause
    #[stored(no_bounds(T))]
    struct NamedWithGenericsFirstBoundWhere<T, U>
    where
        T: crate::Config,
    {
        block: BlockNumberFor<T>,
        value: U,
    }
    #[storage_alias]
    pub type NamedWithGenericsFirstBoundWhereStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        NamedWithGenericsFirstBoundWhere<T, u64>,
        ValueQuery,
    >;
    let _ = <NamedWithGenericsFirstBoundWhereStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct NamedWithGenericsFirstBoundWhereGC<T: crate::Config> {
        pub named_with_generics_first_bound_where: NamedWithGenericsFirstBoundWhere<T, u64>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Named struct with generics, bound in second field, where clause
    #[stored(no_bounds(U))]
    struct NamedWithGenericsSecondBoundWhere<T, U>
    where
        U: crate::Config,
    {
        value: T,
        block: BlockNumberFor<U>,
    }
    #[storage_alias]
    pub type NamedWithGenericsSecondBoundWhereStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        NamedWithGenericsSecondBoundWhere<u64, T>,
        ValueQuery,
    >;
    let _ = <NamedWithGenericsSecondBoundWhereStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct NamedWithGenericsSecondBoundWhereGC<T: crate::Config> {
        pub named_with_generics_second_bound_where: NamedWithGenericsSecondBoundWhere<u64, T>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Named struct with generics, both bound, where clause
    #[stored(no_bounds(T, U))]
    struct NamedWithGenericsBothBoundWhere<T, U>
    where
        T: crate::Config,
        U: crate::Config,
    {
        value: BlockNumberFor<T>,
        block: BlockNumberFor<U>,
    }
    #[storage_alias]
    pub type NamedWithGenericsBothBoundWhereStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        NamedWithGenericsBothBoundWhere<T, T>,
        ValueQuery,
    >;
    let _ = <NamedWithGenericsBothBoundWhereStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct NamedWithGenericsBothBoundWhereGC<T: crate::Config> {
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
    pub type UnitEnumStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        UnitEnum,
        ValueQuery,
    >;
    let _ = <UnitEnumStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct UnitEnumGC<T: crate::Config> {
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
    pub type TupleEnumStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        TupleEnum,
        ValueQuery,
    >;
    let _ = <TupleEnumStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct TupleEnumGC<T: crate::Config> {
        pub tuple_enum: TupleEnum,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Struct enum
    #[stored]
    enum StructEnum {
		#[default]
		None,
        A { x: u32 },
        B { y: u64, z: u32 },
    }
    #[storage_alias]
    pub type StructEnumStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        StructEnum,
        ValueQuery,
    >;
    let _ = <StructEnumStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct StructEnumGC<T: crate::Config> {
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
        B { first: T, second: U },
    }
    #[storage_alias]
    pub type GenericEnumStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        GenericEnum<u32, u64>,
        ValueQuery,
    >;
    let _ = <GenericEnumStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();

    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct GenericEnumGC<T: crate::Config> {
        pub generic_enum: GenericEnum<u32, u64>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

	// Generic enum with no_bounds(T): first generic is exempted.
    #[stored(no_bounds(T))]
    enum GenericEnumFirstBound<T: crate::Config, U> {
		#[default]
        A(BlockNumberFor<T>),
        B { value: U },
    }
    #[storage_alias]
    pub type GenericEnumFirstBoundStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        GenericEnumFirstBound<T, u32>,
        ValueQuery,
    >;
    let _ = <GenericEnumFirstBoundStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();
    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct GenericEnumFirstBoundGC<T: crate::Config> {
        pub generic_enum_first_bound: GenericEnumFirstBound<T, u32>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Generic enum with no_bounds(U): second generic is exempted.
    #[stored(no_bounds(U))]
    enum GenericEnumSecondBound<T, U: crate::Config> {
		#[default]
        A(T),
        B { value: BlockNumberFor<U> },
    }
    #[storage_alias]
    pub type GenericEnumSecondBoundStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        GenericEnumSecondBound<u32, T>,
        ValueQuery,
    >;
    let _ = <GenericEnumSecondBoundStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();
    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct GenericEnumSecondBoundGC<T: crate::Config> {
        pub generic_enum_second_bound: GenericEnumSecondBound<u32, T>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Generic enum with no_bounds(T, U): both generics are exempted.
    #[stored(no_bounds(T, U))]
    enum GenericEnumBothBound<T: crate::Config, U: crate::Config> {
		#[default]
        A { first: BlockNumberFor<T>, second: BlockNumberFor<U> },
        B(BlockNumberFor<T>, BlockNumberFor<U>),
    }
    #[storage_alias]
    pub type GenericEnumBothBoundStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        GenericEnumBothBound<T, T>,
        ValueQuery,
    >;
    let _ = <GenericEnumBothBoundStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();
    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct GenericEnumBothBoundGC<T: crate::Config> {
        pub generic_enum_both_bound: GenericEnumBothBound<T, T>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }

    // Generic enum with a where clause and no_bounds(T)
    #[stored(no_bounds(T))]
    enum GenericEnumFirstBoundWhere<T, U>
    where
        T: crate::Config,
    {
		#[default]
        A(BlockNumberFor<T>),
        B { value: U },
    }
    #[storage_alias]
    pub type GenericEnumFirstBoundWhereStorage<T: crate::Config> = StorageValue<
        crate::Pallet<T>,
        GenericEnumFirstBoundWhere<T, u32>,
        ValueQuery,
    >;
    let _ = <GenericEnumFirstBoundWhereStorage<crate::mock::Test> as frame_support::traits::StorageInfoTrait>::storage_info();
    #[derive(frame::derive::Serialize, frame::derive::Deserialize)]
    #[serde(bound(serialize = "", deserialize = ""), crate = "frame::derive::serde")]
    pub struct GenericEnumFirstBoundWhereGC<T: crate::Config> {
        pub generic_enum_first_bound_where: GenericEnumFirstBoundWhere<T, u32>,
        #[serde(skip)]
        _marker: PhantomData<T>,
    }
}