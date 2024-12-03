#![feature(prelude_import)]
#![allow(useless_deprecated, clippy::deprecated_semver)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
use frame_support::{derive_impl, traits::ConstU32};
use scale_info::{form::MetaForm, meta_type};
use sp_metadata_ir::{
    DeprecationStatusIR, RuntimeApiMetadataIR, RuntimeApiMethodMetadataIR,
    RuntimeApiMethodParamMetadataIR,
};
use sp_runtime::traits::Block as BlockT;
pub type BlockNumber = u64;
pub type Header = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;
pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<
    u32,
    RuntimeCall,
    (),
    (),
>;
impl frame_system::Config for Runtime {
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
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
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
    type ExtensionsWeightInfo = <frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig>::ExtensionsWeightInfo;
    type RuntimeTask = RuntimeTask;
    type BlockHashCount = <frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig>::BlockHashCount;
    type SingleBlockMigrations = <frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig>::SingleBlockMigrations;
    type MultiBlockMigrator = <frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig>::MultiBlockMigrator;
    type PreInherents = <frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig>::PreInherents;
    type PostInherents = <frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig>::PostInherents;
    type PostTransactions = <frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig>::PostTransactions;
}
#[doc(hidden)]
mod sp_api_hidden_includes_construct_runtime {
    pub use frame_support as hidden_include;
}
const _: () = {
    #[allow(unused)]
    type __hidden_use_of_unchecked_extrinsic = <<Runtime as frame_system::Config>::Block as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block>::Extrinsic;
};
pub struct Runtime;
#[automatically_derived]
impl ::core::clone::Clone for Runtime {
    #[inline]
    fn clone(&self) -> Runtime {
        *self
    }
}
#[automatically_derived]
impl ::core::marker::Copy for Runtime {}
#[automatically_derived]
impl ::core::marker::StructuralPartialEq for Runtime {}
#[automatically_derived]
impl ::core::cmp::PartialEq for Runtime {
    #[inline]
    fn eq(&self, other: &Runtime) -> bool {
        true
    }
}
#[automatically_derived]
impl ::core::cmp::Eq for Runtime {
    #[inline]
    #[doc(hidden)]
    #[coverage(off)]
    fn assert_receiver_is_total_eq(&self) -> () {}
}
impl core::fmt::Debug for Runtime {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        fmt.debug_tuple("Runtime").finish()
    }
}
#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
const _: () = {
    impl ::scale_info::TypeInfo for Runtime {
        type Identity = Self;
        fn type_info() -> ::scale_info::Type {
            ::scale_info::Type::builder()
                .path(
                    ::scale_info::Path::new_with_replace(
                        "Runtime",
                        "runtime_metadata",
                        &[],
                    ),
                )
                .type_params(::alloc::vec::Vec::new())
                .composite(::scale_info::build::Fields::unit())
        }
    }
};
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::GetRuntimeBlockType
for Runtime {
    type RuntimeBlock = <Runtime as frame_system::Config>::Block;
}
#[doc(hidden)]
trait InternalConstructRuntime {
    #[inline(always)]
    fn runtime_metadata(
        &self,
    ) -> self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Vec<
        self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::metadata_ir::RuntimeApiMetadataIR,
    > {
        Default::default()
    }
}
#[doc(hidden)]
impl InternalConstructRuntime for &Runtime {}
#[allow(non_camel_case_types)]
#[allow(deprecated)]
pub enum RuntimeEvent {
    #[codec(index = 0u8)]
    System(frame_system::Event<Runtime>),
}
#[automatically_derived]
#[allow(non_camel_case_types)]
#[allow(deprecated)]
impl ::core::clone::Clone for RuntimeEvent {
    #[inline]
    fn clone(&self) -> RuntimeEvent {
        match self {
            RuntimeEvent::System(__self_0) => {
                RuntimeEvent::System(::core::clone::Clone::clone(__self_0))
            }
        }
    }
}
#[automatically_derived]
#[allow(non_camel_case_types)]
#[allow(deprecated)]
impl ::core::marker::StructuralPartialEq for RuntimeEvent {}
#[automatically_derived]
#[allow(non_camel_case_types)]
#[allow(deprecated)]
impl ::core::cmp::PartialEq for RuntimeEvent {
    #[inline]
    fn eq(&self, other: &RuntimeEvent) -> bool {
        match (self, other) {
            (RuntimeEvent::System(__self_0), RuntimeEvent::System(__arg1_0)) => {
                __self_0 == __arg1_0
            }
        }
    }
}
#[automatically_derived]
#[allow(non_camel_case_types)]
#[allow(deprecated)]
impl ::core::cmp::Eq for RuntimeEvent {
    #[inline]
    #[doc(hidden)]
    #[coverage(off)]
    fn assert_receiver_is_total_eq(&self) -> () {
        let _: ::core::cmp::AssertParamIsEq<frame_system::Event<Runtime>>;
    }
}
#[allow(deprecated)]
const _: () = {
    #[allow(non_camel_case_types)]
    #[allow(deprecated)]
    #[automatically_derived]
    impl ::codec::Encode for RuntimeEvent {
        fn size_hint(&self) -> usize {
            1_usize
                + match *self {
                    RuntimeEvent::System(ref aa) => {
                        0_usize.saturating_add(::codec::Encode::size_hint(aa))
                    }
                    _ => 0_usize,
                }
        }
        fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
            &self,
            __codec_dest_edqy: &mut __CodecOutputEdqy,
        ) {
            match *self {
                RuntimeEvent::System(ref aa) => {
                    __codec_dest_edqy.push_byte(0u8 as ::core::primitive::u8);
                    ::codec::Encode::encode_to(aa, __codec_dest_edqy);
                }
                _ => {}
            }
        }
    }
    #[automatically_derived]
    impl ::codec::EncodeLike for RuntimeEvent {}
};
#[allow(deprecated)]
const _: () = {
    #[allow(non_camel_case_types)]
    #[allow(deprecated)]
    #[automatically_derived]
    impl ::codec::Decode for RuntimeEvent {
        fn decode<__CodecInputEdqy: ::codec::Input>(
            __codec_input_edqy: &mut __CodecInputEdqy,
        ) -> ::core::result::Result<Self, ::codec::Error> {
            match __codec_input_edqy
                .read_byte()
                .map_err(|e| {
                    e
                        .chain(
                            "Could not decode `RuntimeEvent`, failed to read variant byte",
                        )
                })?
            {
                #[allow(clippy::unnecessary_cast)]
                __codec_x_edqy if __codec_x_edqy == 0u8 as ::core::primitive::u8 => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Ok(
                            RuntimeEvent::System({
                                let __codec_res_edqy = <frame_system::Event<
                                    Runtime,
                                > as ::codec::Decode>::decode(__codec_input_edqy);
                                match __codec_res_edqy {
                                    ::core::result::Result::Err(e) => {
                                        return ::core::result::Result::Err(
                                            e.chain("Could not decode `RuntimeEvent::System.0`"),
                                        );
                                    }
                                    ::core::result::Result::Ok(__codec_res_edqy) => {
                                        __codec_res_edqy
                                    }
                                }
                            }),
                        )
                    })();
                }
                _ => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Err(
                            <_ as ::core::convert::Into<
                                _,
                            >>::into(
                                "Could not decode `RuntimeEvent`, variant doesn't exist",
                            ),
                        )
                    })();
                }
            }
        }
    }
};
#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
const _: () = {
    impl ::scale_info::TypeInfo for RuntimeEvent {
        type Identity = Self;
        fn type_info() -> ::scale_info::Type {
            ::scale_info::Type::builder()
                .path(
                    ::scale_info::Path::new_with_replace(
                        "RuntimeEvent",
                        "runtime_metadata",
                        &[],
                    ),
                )
                .type_params(::alloc::vec::Vec::new())
                .variant(
                    ::scale_info::build::Variants::new()
                        .variant(
                            "System",
                            |v| {
                                v
                                    .index(0u8 as ::core::primitive::u8)
                                    .fields(
                                        ::scale_info::build::Fields::unnamed()
                                            .field(|f| {
                                                f
                                                    .ty::<frame_system::Event<Runtime>>()
                                                    .type_name("frame_system::Event<Runtime>")
                                            }),
                                    )
                            },
                        ),
                )
        }
    }
};
impl core::fmt::Debug for RuntimeEvent {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::System(ref a0) => {
                fmt.debug_tuple("RuntimeEvent::System").field(a0).finish()
            }
            _ => Ok(()),
        }
    }
}
#[allow(deprecated)]
impl From<frame_system::Event<Runtime>> for RuntimeEvent {
    fn from(x: frame_system::Event<Runtime>) -> Self {
        RuntimeEvent::System(x)
    }
}
#[allow(deprecated)]
impl TryInto<frame_system::Event<Runtime>> for RuntimeEvent {
    type Error = ();
    fn try_into(
        self,
    ) -> ::core::result::Result<frame_system::Event<Runtime>, Self::Error> {
        match self {
            Self::System(evt) => Ok(evt),
            _ => Err(()),
        }
    }
}
#[allow(non_camel_case_types)]
#[allow(deprecated)]
pub enum RuntimeError {
    #[codec(index = 0u8)]
    System(frame_system::Error<Runtime>),
}
#[allow(deprecated)]
const _: () = {
    #[allow(non_camel_case_types)]
    #[allow(deprecated)]
    #[automatically_derived]
    impl ::codec::Encode for RuntimeError {
        fn size_hint(&self) -> usize {
            1_usize
                + match *self {
                    RuntimeError::System(ref aa) => {
                        0_usize.saturating_add(::codec::Encode::size_hint(aa))
                    }
                    _ => 0_usize,
                }
        }
        fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
            &self,
            __codec_dest_edqy: &mut __CodecOutputEdqy,
        ) {
            match *self {
                RuntimeError::System(ref aa) => {
                    __codec_dest_edqy.push_byte(0u8 as ::core::primitive::u8);
                    ::codec::Encode::encode_to(aa, __codec_dest_edqy);
                }
                _ => {}
            }
        }
    }
    #[automatically_derived]
    impl ::codec::EncodeLike for RuntimeError {}
};
#[allow(deprecated)]
const _: () = {
    #[allow(non_camel_case_types)]
    #[allow(deprecated)]
    #[automatically_derived]
    impl ::codec::Decode for RuntimeError {
        fn decode<__CodecInputEdqy: ::codec::Input>(
            __codec_input_edqy: &mut __CodecInputEdqy,
        ) -> ::core::result::Result<Self, ::codec::Error> {
            match __codec_input_edqy
                .read_byte()
                .map_err(|e| {
                    e
                        .chain(
                            "Could not decode `RuntimeError`, failed to read variant byte",
                        )
                })?
            {
                #[allow(clippy::unnecessary_cast)]
                __codec_x_edqy if __codec_x_edqy == 0u8 as ::core::primitive::u8 => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Ok(
                            RuntimeError::System({
                                let __codec_res_edqy = <frame_system::Error<
                                    Runtime,
                                > as ::codec::Decode>::decode(__codec_input_edqy);
                                match __codec_res_edqy {
                                    ::core::result::Result::Err(e) => {
                                        return ::core::result::Result::Err(
                                            e.chain("Could not decode `RuntimeError::System.0`"),
                                        );
                                    }
                                    ::core::result::Result::Ok(__codec_res_edqy) => {
                                        __codec_res_edqy
                                    }
                                }
                            }),
                        )
                    })();
                }
                _ => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Err(
                            <_ as ::core::convert::Into<
                                _,
                            >>::into(
                                "Could not decode `RuntimeError`, variant doesn't exist",
                            ),
                        )
                    })();
                }
            }
        }
    }
};
#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
const _: () = {
    impl ::scale_info::TypeInfo for RuntimeError {
        type Identity = Self;
        fn type_info() -> ::scale_info::Type {
            ::scale_info::Type::builder()
                .path(
                    ::scale_info::Path::new_with_replace(
                        "RuntimeError",
                        "runtime_metadata",
                        &[],
                    ),
                )
                .type_params(::alloc::vec::Vec::new())
                .variant(
                    ::scale_info::build::Variants::new()
                        .variant(
                            "System",
                            |v| {
                                v
                                    .index(0u8 as ::core::primitive::u8)
                                    .fields(
                                        ::scale_info::build::Fields::unnamed()
                                            .field(|f| {
                                                f
                                                    .ty::<frame_system::Error<Runtime>>()
                                                    .type_name("frame_system::Error<Runtime>")
                                            }),
                                    )
                            },
                        ),
                )
        }
    }
};
impl core::fmt::Debug for RuntimeError {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::System(ref a0) => {
                fmt.debug_tuple("RuntimeError::System").field(a0).finish()
            }
            _ => Ok(()),
        }
    }
}
#[allow(deprecated)]
impl From<frame_system::Error<Runtime>> for RuntimeError {
    fn from(x: frame_system::Error<Runtime>) -> Self {
        RuntimeError::System(x)
    }
}
#[allow(deprecated)]
impl TryInto<frame_system::Error<Runtime>> for RuntimeError {
    type Error = ();
    fn try_into(
        self,
    ) -> ::core::result::Result<frame_system::Error<Runtime>, Self::Error> {
        match self {
            Self::System(evt) => Ok(evt),
            _ => Err(()),
        }
    }
}
impl RuntimeError {
    /// Optionally convert the `DispatchError` into the `RuntimeError`.
    ///
    /// Returns `Some` if the error matches the `DispatchError::Module` variant, otherwise `None`.
    pub fn from_dispatch_error(
        err: self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::DispatchError,
    ) -> Option<Self> {
        let self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::DispatchError::Module(
            module_error,
        ) = err else { return None };
        let bytes = self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::codec::Encode::encode(
            &module_error,
        );
        self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::codec::Decode::decode(
                &mut &bytes[..],
            )
            .ok()
    }
}
/// The runtime origin type representing the origin of a call.
///
/// Origin is always created with the base filter configured in [`frame_system::Config::BaseCallFilter`].
pub struct RuntimeOrigin {
    pub caller: OriginCaller,
    filter: self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Rc<
        self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Box<
            dyn Fn(&<Runtime as frame_system::Config>::RuntimeCall) -> bool,
        >,
    >,
}
#[automatically_derived]
impl ::core::clone::Clone for RuntimeOrigin {
    #[inline]
    fn clone(&self) -> RuntimeOrigin {
        RuntimeOrigin {
            caller: ::core::clone::Clone::clone(&self.caller),
            filter: ::core::clone::Clone::clone(&self.filter),
        }
    }
}
#[cfg(feature = "std")]
impl core::fmt::Debug for RuntimeOrigin {
    fn fmt(
        &self,
        fmt: &mut core::fmt::Formatter,
    ) -> core::result::Result<(), core::fmt::Error> {
        fmt.debug_struct("Origin")
            .field("caller", &self.caller)
            .field("filter", &"[function ptr]")
            .finish()
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::OriginTrait
for RuntimeOrigin {
    type Call = <Runtime as frame_system::Config>::RuntimeCall;
    type PalletsOrigin = OriginCaller;
    type AccountId = <Runtime as frame_system::Config>::AccountId;
    fn add_filter(&mut self, filter: impl Fn(&Self::Call) -> bool + 'static) {
        let f = self.filter.clone();
        self.filter = self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Rc::new(
            self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Box::new(move |
                call|
            { f(call) && filter(call) }),
        );
    }
    fn reset_filter(&mut self) {
        let filter = <<Runtime as frame_system::Config>::BaseCallFilter as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::Contains<
            <Runtime as frame_system::Config>::RuntimeCall,
        >>::contains;
        self.filter = self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Rc::new(
            self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Box::new(
                filter,
            ),
        );
    }
    fn set_caller(&mut self, caller: OriginCaller) {
        self.caller = caller;
    }
    fn set_caller_from(&mut self, other: impl Into<Self>) {
        self.caller = other.into().caller;
    }
    fn filter_call(&self, call: &Self::Call) -> bool {
        match self.caller {
            OriginCaller::system(frame_system::Origin::<Runtime>::Root) => true,
            _ => (self.filter)(call),
        }
    }
    fn caller(&self) -> &Self::PalletsOrigin {
        &self.caller
    }
    fn into_caller(self) -> Self::PalletsOrigin {
        self.caller
    }
    fn try_with_caller<R>(
        mut self,
        f: impl FnOnce(Self::PalletsOrigin) -> Result<R, Self::PalletsOrigin>,
    ) -> Result<R, Self> {
        match f(self.caller) {
            Ok(r) => Ok(r),
            Err(caller) => {
                self.caller = caller;
                Err(self)
            }
        }
    }
    fn none() -> Self {
        frame_system::RawOrigin::None.into()
    }
    fn root() -> Self {
        frame_system::RawOrigin::Root.into()
    }
    fn signed(by: Self::AccountId) -> Self {
        frame_system::RawOrigin::Signed(by).into()
    }
}
#[allow(non_camel_case_types)]
pub enum OriginCaller {
    #[codec(index = 0u8)]
    system(frame_system::Origin<Runtime>),
    #[allow(dead_code)]
    Void(
        self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Void,
    ),
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::clone::Clone for OriginCaller {
    #[inline]
    fn clone(&self) -> OriginCaller {
        match self {
            OriginCaller::system(__self_0) => {
                OriginCaller::system(::core::clone::Clone::clone(__self_0))
            }
            OriginCaller::Void(__self_0) => {
                OriginCaller::Void(::core::clone::Clone::clone(__self_0))
            }
        }
    }
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::marker::StructuralPartialEq for OriginCaller {}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::cmp::PartialEq for OriginCaller {
    #[inline]
    fn eq(&self, other: &OriginCaller) -> bool {
        let __self_discr = ::core::intrinsics::discriminant_value(self);
        let __arg1_discr = ::core::intrinsics::discriminant_value(other);
        __self_discr == __arg1_discr
            && match (self, other) {
                (OriginCaller::system(__self_0), OriginCaller::system(__arg1_0)) => {
                    __self_0 == __arg1_0
                }
                (OriginCaller::Void(__self_0), OriginCaller::Void(__arg1_0)) => {
                    __self_0 == __arg1_0
                }
                _ => unsafe { ::core::intrinsics::unreachable() }
            }
    }
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::cmp::Eq for OriginCaller {
    #[inline]
    #[doc(hidden)]
    #[coverage(off)]
    fn assert_receiver_is_total_eq(&self) -> () {
        let _: ::core::cmp::AssertParamIsEq<frame_system::Origin<Runtime>>;
        let _: ::core::cmp::AssertParamIsEq<
            self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Void,
        >;
    }
}
impl core::fmt::Debug for OriginCaller {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::system(ref a0) => {
                fmt.debug_tuple("OriginCaller::system").field(a0).finish()
            }
            Self::Void(ref a0) => {
                fmt.debug_tuple("OriginCaller::Void").field(a0).finish()
            }
            _ => Ok(()),
        }
    }
}
#[allow(deprecated)]
const _: () = {
    #[allow(non_camel_case_types)]
    #[automatically_derived]
    impl ::codec::Encode for OriginCaller {
        fn size_hint(&self) -> usize {
            1_usize
                + match *self {
                    OriginCaller::system(ref aa) => {
                        0_usize.saturating_add(::codec::Encode::size_hint(aa))
                    }
                    OriginCaller::Void(ref aa) => {
                        0_usize.saturating_add(::codec::Encode::size_hint(aa))
                    }
                    _ => 0_usize,
                }
        }
        fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
            &self,
            __codec_dest_edqy: &mut __CodecOutputEdqy,
        ) {
            match *self {
                OriginCaller::system(ref aa) => {
                    __codec_dest_edqy.push_byte(0u8 as ::core::primitive::u8);
                    ::codec::Encode::encode_to(aa, __codec_dest_edqy);
                }
                OriginCaller::Void(ref aa) => {
                    __codec_dest_edqy.push_byte(1usize as ::core::primitive::u8);
                    ::codec::Encode::encode_to(aa, __codec_dest_edqy);
                }
                _ => {}
            }
        }
    }
    #[automatically_derived]
    impl ::codec::EncodeLike for OriginCaller {}
};
#[allow(deprecated)]
const _: () = {
    #[allow(non_camel_case_types)]
    #[automatically_derived]
    impl ::codec::Decode for OriginCaller {
        fn decode<__CodecInputEdqy: ::codec::Input>(
            __codec_input_edqy: &mut __CodecInputEdqy,
        ) -> ::core::result::Result<Self, ::codec::Error> {
            match __codec_input_edqy
                .read_byte()
                .map_err(|e| {
                    e
                        .chain(
                            "Could not decode `OriginCaller`, failed to read variant byte",
                        )
                })?
            {
                #[allow(clippy::unnecessary_cast)]
                __codec_x_edqy if __codec_x_edqy == 0u8 as ::core::primitive::u8 => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Ok(
                            OriginCaller::system({
                                let __codec_res_edqy = <frame_system::Origin<
                                    Runtime,
                                > as ::codec::Decode>::decode(__codec_input_edqy);
                                match __codec_res_edqy {
                                    ::core::result::Result::Err(e) => {
                                        return ::core::result::Result::Err(
                                            e.chain("Could not decode `OriginCaller::system.0`"),
                                        );
                                    }
                                    ::core::result::Result::Ok(__codec_res_edqy) => {
                                        __codec_res_edqy
                                    }
                                }
                            }),
                        )
                    })();
                }
                #[allow(clippy::unnecessary_cast)]
                __codec_x_edqy if __codec_x_edqy == 1usize as ::core::primitive::u8 => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Ok(
                            OriginCaller::Void({
                                let __codec_res_edqy = <self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Void as ::codec::Decode>::decode(
                                    __codec_input_edqy,
                                );
                                match __codec_res_edqy {
                                    ::core::result::Result::Err(e) => {
                                        return ::core::result::Result::Err(
                                            e.chain("Could not decode `OriginCaller::Void.0`"),
                                        );
                                    }
                                    ::core::result::Result::Ok(__codec_res_edqy) => {
                                        __codec_res_edqy
                                    }
                                }
                            }),
                        )
                    })();
                }
                _ => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Err(
                            <_ as ::core::convert::Into<
                                _,
                            >>::into(
                                "Could not decode `OriginCaller`, variant doesn't exist",
                            ),
                        )
                    })();
                }
            }
        }
    }
};
#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
const _: () = {
    impl ::scale_info::TypeInfo for OriginCaller {
        type Identity = Self;
        fn type_info() -> ::scale_info::Type {
            ::scale_info::Type::builder()
                .path(
                    ::scale_info::Path::new_with_replace(
                        "OriginCaller",
                        "runtime_metadata",
                        &[],
                    ),
                )
                .type_params(::alloc::vec::Vec::new())
                .variant(
                    ::scale_info::build::Variants::new()
                        .variant(
                            "system",
                            |v| {
                                v
                                    .index(0u8 as ::core::primitive::u8)
                                    .fields(
                                        ::scale_info::build::Fields::unnamed()
                                            .field(|f| {
                                                f
                                                    .ty::<frame_system::Origin<Runtime>>()
                                                    .type_name("frame_system::Origin<Runtime>")
                                            }),
                                    )
                            },
                        )
                        .variant(
                            "Void",
                            |v| {
                                v
                                    .index(1usize as ::core::primitive::u8)
                                    .fields(
                                        ::scale_info::build::Fields::unnamed()
                                            .field(|f| {
                                                f
                                                    .ty::<
                                                        self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Void,
                                                    >()
                                                    .type_name(
                                                        "self::sp_api_hidden_includes_construct_runtime::hidden_include::\n__private::Void",
                                                    )
                                            }),
                                    )
                            },
                        ),
                )
        }
    }
};
const _: () = {
    impl ::codec::MaxEncodedLen for OriginCaller {
        fn max_encoded_len() -> ::core::primitive::usize {
            0_usize
                .max(
                    0_usize
                        .saturating_add(
                            <frame_system::Origin<Runtime>>::max_encoded_len(),
                        ),
                )
                .max(
                    0_usize
                        .saturating_add(
                            <self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Void>::max_encoded_len(),
                        ),
                )
                .saturating_add(1)
        }
    }
};
#[allow(dead_code)]
impl RuntimeOrigin {
    /// Create with system none origin and [`frame_system::Config::BaseCallFilter`].
    pub fn none() -> Self {
        <RuntimeOrigin as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::OriginTrait>::none()
    }
    /// Create with system root origin and [`frame_system::Config::BaseCallFilter`].
    pub fn root() -> Self {
        <RuntimeOrigin as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::OriginTrait>::root()
    }
    /// Create with system signed origin and [`frame_system::Config::BaseCallFilter`].
    pub fn signed(by: <Runtime as frame_system::Config>::AccountId) -> Self {
        <RuntimeOrigin as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::OriginTrait>::signed(
            by,
        )
    }
}
impl From<frame_system::Origin<Runtime>> for OriginCaller {
    fn from(x: frame_system::Origin<Runtime>) -> Self {
        OriginCaller::system(x)
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::CallerTrait<
    <Runtime as frame_system::Config>::AccountId,
> for OriginCaller {
    fn into_system(
        self,
    ) -> Option<frame_system::RawOrigin<<Runtime as frame_system::Config>::AccountId>> {
        match self {
            OriginCaller::system(x) => Some(x),
            _ => None,
        }
    }
    fn as_system_ref(
        &self,
    ) -> Option<&frame_system::RawOrigin<<Runtime as frame_system::Config>::AccountId>> {
        match &self {
            OriginCaller::system(o) => Some(o),
            _ => None,
        }
    }
}
impl TryFrom<OriginCaller> for frame_system::Origin<Runtime> {
    type Error = OriginCaller;
    fn try_from(
        x: OriginCaller,
    ) -> core::result::Result<frame_system::Origin<Runtime>, OriginCaller> {
        if let OriginCaller::system(l) = x { Ok(l) } else { Err(x) }
    }
}
impl From<frame_system::Origin<Runtime>> for RuntimeOrigin {
    /// Convert to runtime origin, using as filter: [`frame_system::Config::BaseCallFilter`].
    fn from(x: frame_system::Origin<Runtime>) -> Self {
        let o: OriginCaller = x.into();
        o.into()
    }
}
impl From<OriginCaller> for RuntimeOrigin {
    fn from(x: OriginCaller) -> Self {
        let mut o = RuntimeOrigin {
            caller: x,
            filter: self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Rc::new(
                self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Box::new(|
                    _|
                true),
            ),
        };
        self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::OriginTrait::reset_filter(
            &mut o,
        );
        o
    }
}
impl From<RuntimeOrigin>
for core::result::Result<frame_system::Origin<Runtime>, RuntimeOrigin> {
    /// NOTE: converting to pallet origin loses the origin filter information.
    fn from(val: RuntimeOrigin) -> Self {
        if let OriginCaller::system(l) = val.caller { Ok(l) } else { Err(val) }
    }
}
impl From<Option<<Runtime as frame_system::Config>::AccountId>> for RuntimeOrigin {
    /// Convert to runtime origin with caller being system signed or none and use filter [`frame_system::Config::BaseCallFilter`].
    fn from(x: Option<<Runtime as frame_system::Config>::AccountId>) -> Self {
        <frame_system::Origin<Runtime>>::from(x).into()
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::AsSystemOriginSigner<
    <Runtime as frame_system::Config>::AccountId,
> for RuntimeOrigin {
    fn as_system_origin_signer(
        &self,
    ) -> Option<&<Runtime as frame_system::Config>::AccountId> {
        if let OriginCaller::system(
            frame_system::Origin::<Runtime>::Signed(ref signed),
        ) = &self.caller
        {
            Some(signed)
        } else {
            None
        }
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::AsTransactionAuthorizedOrigin
for RuntimeOrigin {
    fn is_transaction_authorized(&self) -> bool {
        !match &self.caller {
            OriginCaller::system(frame_system::Origin::<Runtime>::None) => true,
            _ => false,
        }
    }
}
pub type System = frame_system::Pallet<Runtime>;
#[cfg(all())]
/// All pallets included in the runtime as a nested tuple of types.
pub type AllPalletsWithSystem = (System,);
#[cfg(all())]
/// All pallets included in the runtime as a nested tuple of types.
/// Excludes the System pallet.
pub type AllPalletsWithoutSystem = ();
/// Provides an implementation of `PalletInfo` to provide information
/// about the pallet setup in the runtime.
pub struct PalletInfo;
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::PalletInfo
for PalletInfo {
    fn index<P: 'static>() -> Option<usize> {
        let type_id = core::any::TypeId::of::<P>();
        if type_id == core::any::TypeId::of::<System>() {
            return Some(0usize);
        }
        None
    }
    fn name<P: 'static>() -> Option<&'static str> {
        let type_id = core::any::TypeId::of::<P>();
        if type_id == core::any::TypeId::of::<System>() {
            return Some("System");
        }
        None
    }
    fn name_hash<P: 'static>() -> Option<[u8; 16]> {
        let type_id = core::any::TypeId::of::<P>();
        if type_id == core::any::TypeId::of::<System>() {
            return Some([
                38u8,
                170u8,
                57u8,
                78u8,
                234u8,
                86u8,
                48u8,
                224u8,
                124u8,
                72u8,
                174u8,
                12u8,
                149u8,
                88u8,
                206u8,
                247u8,
            ]);
        }
        None
    }
    fn module_name<P: 'static>() -> Option<&'static str> {
        let type_id = core::any::TypeId::of::<P>();
        if type_id == core::any::TypeId::of::<System>() {
            return Some("frame_system");
        }
        None
    }
    fn crate_version<P: 'static>() -> Option<
        self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::CrateVersion,
    > {
        let type_id = core::any::TypeId::of::<P>();
        if type_id == core::any::TypeId::of::<System>() {
            return Some(
                <frame_system::Pallet<
                    Runtime,
                > as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::PalletInfoAccess>::crate_version(),
            );
        }
        None
    }
}
/// The aggregated runtime call type.
pub enum RuntimeCall {
    #[codec(index = 0u8)]
    System(
        self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::CallableCallFor<
            System,
            Runtime,
        >,
    ),
}
#[automatically_derived]
impl ::core::clone::Clone for RuntimeCall {
    #[inline]
    fn clone(&self) -> RuntimeCall {
        match self {
            RuntimeCall::System(__self_0) => {
                RuntimeCall::System(::core::clone::Clone::clone(__self_0))
            }
        }
    }
}
#[automatically_derived]
impl ::core::marker::StructuralPartialEq for RuntimeCall {}
#[automatically_derived]
impl ::core::cmp::PartialEq for RuntimeCall {
    #[inline]
    fn eq(&self, other: &RuntimeCall) -> bool {
        match (self, other) {
            (RuntimeCall::System(__self_0), RuntimeCall::System(__arg1_0)) => {
                __self_0 == __arg1_0
            }
        }
    }
}
#[automatically_derived]
impl ::core::cmp::Eq for RuntimeCall {
    #[inline]
    #[doc(hidden)]
    #[coverage(off)]
    fn assert_receiver_is_total_eq(&self) -> () {
        let _: ::core::cmp::AssertParamIsEq<
            self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::CallableCallFor<
                System,
                Runtime,
            >,
        >;
    }
}
#[allow(deprecated)]
const _: () = {
    #[automatically_derived]
    impl ::codec::Encode for RuntimeCall {
        fn size_hint(&self) -> usize {
            1_usize
                + match *self {
                    RuntimeCall::System(ref aa) => {
                        0_usize.saturating_add(::codec::Encode::size_hint(aa))
                    }
                    _ => 0_usize,
                }
        }
        fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
            &self,
            __codec_dest_edqy: &mut __CodecOutputEdqy,
        ) {
            match *self {
                RuntimeCall::System(ref aa) => {
                    __codec_dest_edqy.push_byte(0u8 as ::core::primitive::u8);
                    ::codec::Encode::encode_to(aa, __codec_dest_edqy);
                }
                _ => {}
            }
        }
    }
    #[automatically_derived]
    impl ::codec::EncodeLike for RuntimeCall {}
};
#[allow(deprecated)]
const _: () = {
    #[automatically_derived]
    impl ::codec::Decode for RuntimeCall {
        fn decode<__CodecInputEdqy: ::codec::Input>(
            __codec_input_edqy: &mut __CodecInputEdqy,
        ) -> ::core::result::Result<Self, ::codec::Error> {
            match __codec_input_edqy
                .read_byte()
                .map_err(|e| {
                    e
                        .chain(
                            "Could not decode `RuntimeCall`, failed to read variant byte",
                        )
                })?
            {
                #[allow(clippy::unnecessary_cast)]
                __codec_x_edqy if __codec_x_edqy == 0u8 as ::core::primitive::u8 => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Ok(
                            RuntimeCall::System({
                                let __codec_res_edqy = <self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::CallableCallFor<
                                    System,
                                    Runtime,
                                > as ::codec::Decode>::decode(__codec_input_edqy);
                                match __codec_res_edqy {
                                    ::core::result::Result::Err(e) => {
                                        return ::core::result::Result::Err(
                                            e.chain("Could not decode `RuntimeCall::System.0`"),
                                        );
                                    }
                                    ::core::result::Result::Ok(__codec_res_edqy) => {
                                        __codec_res_edqy
                                    }
                                }
                            }),
                        )
                    })();
                }
                _ => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Err(
                            <_ as ::core::convert::Into<
                                _,
                            >>::into(
                                "Could not decode `RuntimeCall`, variant doesn't exist",
                            ),
                        )
                    })();
                }
            }
        }
    }
};
#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
const _: () = {
    impl ::scale_info::TypeInfo for RuntimeCall {
        type Identity = Self;
        fn type_info() -> ::scale_info::Type {
            ::scale_info::Type::builder()
                .path(
                    ::scale_info::Path::new_with_replace(
                        "RuntimeCall",
                        "runtime_metadata",
                        &[],
                    ),
                )
                .type_params(::alloc::vec::Vec::new())
                .docs(&["The aggregated runtime call type."])
                .variant(
                    ::scale_info::build::Variants::new()
                        .variant(
                            "System",
                            |v| {
                                v
                                    .index(0u8 as ::core::primitive::u8)
                                    .fields(
                                        ::scale_info::build::Fields::unnamed()
                                            .field(|f| {
                                                f
                                                    .ty::<
                                                        self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::CallableCallFor<
                                                            System,
                                                            Runtime,
                                                        >,
                                                    >()
                                                    .type_name(
                                                        "self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch\n::CallableCallFor<System, Runtime>",
                                                    )
                                            }),
                                    )
                            },
                        ),
                )
        }
    }
};
impl core::fmt::Debug for RuntimeCall {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::System(ref a0) => {
                fmt.debug_tuple("RuntimeCall::System").field(a0).finish()
            }
            _ => Ok(()),
        }
    }
}
#[cfg(test)]
impl RuntimeCall {
    /// Return a list of the module names together with their size in memory.
    pub const fn sizes() -> &'static [(&'static str, usize)] {
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::Callable;
        use core::mem::size_of;
        &[("System", size_of::<<System as Callable<Runtime>>::RuntimeCall>())]
    }
    /// Panics with diagnostic information if the size is greater than the given `limit`.
    pub fn assert_size_under(limit: usize) {
        let size = core::mem::size_of::<Self>();
        let call_oversize = size > limit;
        if call_oversize {
            {
                ::std::io::_print(
                    format_args!(
                        "Size of `Call` is {0} bytes (provided limit is {1} bytes)\n",
                        size,
                        limit,
                    ),
                );
            };
            let mut sizes = Self::sizes().to_vec();
            sizes.sort_by_key(|x| -(x.1 as isize));
            for (i, &(name, size)) in sizes.iter().enumerate().take(5) {
                {
                    ::std::io::_print(
                        format_args!(
                            "Offender #{0}: {1} at {2} bytes\n",
                            i + 1,
                            name,
                            size,
                        ),
                    );
                };
            }
            if let Some((_, next_size)) = sizes.get(5) {
                {
                    ::std::io::_print(
                        format_args!(
                            "{0} others of size {1} bytes or less\n",
                            sizes.len() - 5,
                            next_size,
                        ),
                    );
                };
            }
            {
                ::core::panicking::panic_fmt(
                    format_args!(
                        "Size of `Call` is more than limit; use `Box` on complex parameter types to reduce the\n\t\t\t\t\t\tsize of `Call`.\n\t\t\t\t\t\tIf the limit is too strong, maybe consider providing a higher limit.",
                    ),
                );
            };
        }
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::GetDispatchInfo
for RuntimeCall {
    fn get_dispatch_info(
        &self,
    ) -> self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::DispatchInfo {
        match self {
            RuntimeCall::System(call) => call.get_dispatch_info(),
        }
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::CheckIfFeeless
for RuntimeCall {
    type Origin = frame_system::pallet_prelude::OriginFor<Runtime>;
    fn is_feeless(&self, origin: &Self::Origin) -> bool {
        match self {
            RuntimeCall::System(call) => call.is_feeless(origin),
        }
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::GetCallMetadata
for RuntimeCall {
    fn get_call_metadata(
        &self,
    ) -> self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::CallMetadata {
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::GetCallName;
        match self {
            RuntimeCall::System(call) => {
                let function_name = call.get_call_name();
                let pallet_name = "System";
                self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::CallMetadata {
                    function_name,
                    pallet_name,
                }
            }
        }
    }
    fn get_module_names() -> &'static [&'static str] {
        &["System"]
    }
    fn get_call_names(module: &str) -> &'static [&'static str] {
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::{
            dispatch::Callable, traits::GetCallName,
        };
        match module {
            "System" => {
                <<System as Callable<
                    Runtime,
                >>::RuntimeCall as GetCallName>::get_call_names()
            }
            _ => ::core::panicking::panic("internal error: entered unreachable code"),
        }
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Dispatchable
for RuntimeCall {
    type RuntimeOrigin = RuntimeOrigin;
    type Config = RuntimeCall;
    type Info = self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::DispatchInfo;
    type PostInfo = self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::PostDispatchInfo;
    fn dispatch(
        self,
        origin: RuntimeOrigin,
    ) -> self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::DispatchResultWithPostInfo {
        if !<Self::RuntimeOrigin as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::OriginTrait>::filter_call(
            &origin,
            &self,
        ) {
            return ::core::result::Result::Err(
                frame_system::Error::<Runtime>::CallFiltered.into(),
            );
        }
        self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::UnfilteredDispatchable::dispatch_bypass_filter(
            self,
            origin,
        )
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::UnfilteredDispatchable
for RuntimeCall {
    type RuntimeOrigin = RuntimeOrigin;
    fn dispatch_bypass_filter(
        self,
        origin: RuntimeOrigin,
    ) -> self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::DispatchResultWithPostInfo {
        match self {
            RuntimeCall::System(call) => {
                self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::UnfilteredDispatchable::dispatch_bypass_filter(
                    call,
                    origin,
                )
            }
        }
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::IsSubType<
    self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::CallableCallFor<
        System,
        Runtime,
    >,
> for RuntimeCall {
    #[allow(unreachable_patterns)]
    fn is_sub_type(
        &self,
    ) -> Option<
        &self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::CallableCallFor<
            System,
            Runtime,
        >,
    > {
        match self {
            RuntimeCall::System(call) => Some(call),
            _ => None,
        }
    }
}
impl From<
    self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::CallableCallFor<
        System,
        Runtime,
    >,
> for RuntimeCall {
    fn from(
        call: self::sp_api_hidden_includes_construct_runtime::hidden_include::dispatch::CallableCallFor<
            System,
            Runtime,
        >,
    ) -> Self {
        RuntimeCall::System(call)
    }
}
/// An aggregation of all `Task` enums across all pallets included in the current runtime.
pub enum RuntimeTask {}
#[automatically_derived]
impl ::core::clone::Clone for RuntimeTask {
    #[inline]
    fn clone(&self) -> RuntimeTask {
        match *self {}
    }
}
#[automatically_derived]
impl ::core::cmp::Eq for RuntimeTask {
    #[inline]
    #[doc(hidden)]
    #[coverage(off)]
    fn assert_receiver_is_total_eq(&self) -> () {}
}
#[automatically_derived]
impl ::core::marker::StructuralPartialEq for RuntimeTask {}
#[automatically_derived]
impl ::core::cmp::PartialEq for RuntimeTask {
    #[inline]
    fn eq(&self, other: &RuntimeTask) -> bool {
        match *self {}
    }
}
#[allow(deprecated)]
const _: () = {
    #[automatically_derived]
    impl ::codec::Encode for RuntimeTask {}
    #[automatically_derived]
    impl ::codec::EncodeLike for RuntimeTask {}
};
#[allow(deprecated)]
const _: () = {
    #[automatically_derived]
    impl ::codec::Decode for RuntimeTask {
        fn decode<__CodecInputEdqy: ::codec::Input>(
            __codec_input_edqy: &mut __CodecInputEdqy,
        ) -> ::core::result::Result<Self, ::codec::Error> {
            match __codec_input_edqy
                .read_byte()
                .map_err(|e| {
                    e
                        .chain(
                            "Could not decode `RuntimeTask`, failed to read variant byte",
                        )
                })?
            {
                _ => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Err(
                            <_ as ::core::convert::Into<
                                _,
                            >>::into(
                                "Could not decode `RuntimeTask`, variant doesn't exist",
                            ),
                        )
                    })();
                }
            }
        }
    }
};
#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
const _: () = {
    impl ::scale_info::TypeInfo for RuntimeTask {
        type Identity = Self;
        fn type_info() -> ::scale_info::Type {
            ::scale_info::Type::builder()
                .path(
                    ::scale_info::Path::new_with_replace(
                        "RuntimeTask",
                        "runtime_metadata",
                        &[],
                    ),
                )
                .type_params(::alloc::vec::Vec::new())
                .docs(
                    &[
                        "An aggregation of all `Task` enums across all pallets included in the current runtime.",
                    ],
                )
                .variant(::scale_info::build::Variants::new())
        }
    }
};
impl core::fmt::Debug for RuntimeTask {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            _ => Ok(()),
        }
    }
}
#[automatically_derived]
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::Task
for RuntimeTask {
    type Enumeration = self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::tasks::__private::IntoIter<
        RuntimeTask,
    >;
    fn is_valid(&self) -> bool {
        match self {
            _ => {
                ::core::panicking::panic_fmt(
                    format_args!(
                        "internal error: entered unreachable code: {0}",
                        format_args!(
                            "cannot have an instantiated RuntimeTask without some Task variant in the runtime. QED",
                        ),
                    ),
                );
            }
        }
    }
    fn run(
        &self,
    ) -> Result<
        (),
        self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::tasks::__private::DispatchError,
    > {
        match self {
            _ => {
                ::core::panicking::panic_fmt(
                    format_args!(
                        "internal error: entered unreachable code: {0}",
                        format_args!(
                            "cannot have an instantiated RuntimeTask without some Task variant in the runtime. QED",
                        ),
                    ),
                );
            }
        }
    }
    fn weight(
        &self,
    ) -> self::sp_api_hidden_includes_construct_runtime::hidden_include::pallet_prelude::Weight {
        match self {
            _ => {
                ::core::panicking::panic_fmt(
                    format_args!(
                        "internal error: entered unreachable code: {0}",
                        format_args!(
                            "cannot have an instantiated RuntimeTask without some Task variant in the runtime. QED",
                        ),
                    ),
                );
            }
        }
    }
    fn task_index(&self) -> u32 {
        match self {
            _ => {
                ::core::panicking::panic_fmt(
                    format_args!(
                        "internal error: entered unreachable code: {0}",
                        format_args!(
                            "cannot have an instantiated RuntimeTask without some Task variant in the runtime. QED",
                        ),
                    ),
                );
            }
        }
    }
    fn iter() -> Self::Enumeration {
        let mut all_tasks = Vec::new();
        all_tasks.into_iter()
    }
}
impl Runtime {
    #[allow(deprecated)]
    fn metadata_ir() -> self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::metadata_ir::MetadataIR {
        let rt = Runtime;
        let ty = self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::scale_info::meta_type::<
            <<Runtime as frame_system::Config>::Block as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block>::Extrinsic,
        >();
        let address_ty = self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::scale_info::meta_type::<
            <<<Runtime as frame_system::Config>::Block as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block>::Extrinsic as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::SignedTransactionBuilder>::Address,
        >();
        let call_ty = self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::scale_info::meta_type::<
            <<<Runtime as frame_system::Config>::Block as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block>::Extrinsic as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::ExtrinsicCall>::Call,
        >();
        let signature_ty = self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::scale_info::meta_type::<
            <<<Runtime as frame_system::Config>::Block as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block>::Extrinsic as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::SignedTransactionBuilder>::Signature,
        >();
        let extra_ty = self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::scale_info::meta_type::<
            <<<Runtime as frame_system::Config>::Block as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block>::Extrinsic as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::SignedTransactionBuilder>::Extension,
        >();
        self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::metadata_ir::MetadataIR {
            pallets: <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::metadata_ir::PalletMetadataIR {
                        name: "System",
                        index: 0u8,
                        storage: Some(
                            frame_system::Pallet::<Runtime>::storage_metadata(),
                        ),
                        calls: Some(frame_system::Pallet::<Runtime>::call_functions()),
                        event: Some(
                            frame_system::Event::<
                                Runtime,
                            >::event_metadata::<frame_system::Event<Runtime>>(),
                        ),
                        constants: frame_system::Pallet::<
                            Runtime,
                        >::pallet_constants_metadata(),
                        error: frame_system::Pallet::<Runtime>::error_metadata(),
                        docs: frame_system::Pallet::<
                            Runtime,
                        >::pallet_documentation_metadata(),
                        associated_types: frame_system::Pallet::<
                            Runtime,
                        >::pallet_associated_types_metadata(),
                        deprecation_info: frame_system::Pallet::<
                            Runtime,
                        >::deprecation_info(),
                    },
                ]),
            ),
            extrinsic: self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::metadata_ir::ExtrinsicMetadataIR {
                ty,
                version: <<<Runtime as frame_system::Config>::Block as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block>::Extrinsic as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::ExtrinsicMetadata>::VERSION,
                address_ty,
                call_ty,
                signature_ty,
                extra_ty,
                extensions: <<<<Runtime as frame_system::Config>::Block as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block>::Extrinsic as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::ExtrinsicMetadata>::TransactionExtensions as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::TransactionExtension<
                    <Runtime as frame_system::Config>::RuntimeCall,
                >>::metadata()
                    .into_iter()
                    .map(|meta| self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::metadata_ir::TransactionExtensionMetadataIR {
                        identifier: meta.identifier,
                        ty: meta.ty,
                        implicit: meta.implicit,
                    })
                    .collect(),
            },
            ty: self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::scale_info::meta_type::<
                Runtime,
            >(),
            apis: (&rt).runtime_metadata(),
            outer_enums: self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::metadata_ir::OuterEnumsIR {
                call_enum_ty: self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::scale_info::meta_type::<
                    <Runtime as frame_system::Config>::RuntimeCall,
                >(),
                event_enum_ty: self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::scale_info::meta_type::<
                    RuntimeEvent,
                >(),
                error_enum_ty: self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::scale_info::meta_type::<
                    RuntimeError,
                >(),
            },
        }
    }
    pub fn metadata() -> self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::metadata::RuntimeMetadataPrefixed {
        self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::metadata_ir::into_v14(
            Runtime::metadata_ir(),
        )
    }
    pub fn metadata_at_version(
        version: u32,
    ) -> Option<
        self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::OpaqueMetadata,
    > {
        self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::metadata_ir::into_version(
                Runtime::metadata_ir(),
                version,
            )
            .map(|prefixed| {
                self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::OpaqueMetadata::new(
                    prefixed.into(),
                )
            })
    }
    pub fn metadata_versions() -> self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Vec<
        u32,
    > {
        self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::metadata_ir::supported_versions()
    }
}
pub type SystemConfig = frame_system::GenesisConfig<Runtime>;
use self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::serde as __genesis_config_serde_import__;
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[serde(crate = "__genesis_config_serde_import__")]
pub struct RuntimeGenesisConfig {
    pub system: SystemConfig,
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    use __genesis_config_serde_import__ as _serde;
    #[automatically_derived]
    impl __genesis_config_serde_import__::Serialize for RuntimeGenesisConfig {
        fn serialize<__S>(
            &self,
            __serializer: __S,
        ) -> __genesis_config_serde_import__::__private::Result<__S::Ok, __S::Error>
        where
            __S: __genesis_config_serde_import__::Serializer,
        {
            let mut __serde_state = _serde::Serializer::serialize_struct(
                __serializer,
                "RuntimeGenesisConfig",
                false as usize + 1,
            )?;
            _serde::ser::SerializeStruct::serialize_field(
                &mut __serde_state,
                "system",
                &self.system,
            )?;
            _serde::ser::SerializeStruct::end(__serde_state)
        }
    }
};
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    use __genesis_config_serde_import__ as _serde;
    #[automatically_derived]
    impl<'de> __genesis_config_serde_import__::Deserialize<'de>
    for RuntimeGenesisConfig {
        fn deserialize<__D>(
            __deserializer: __D,
        ) -> __genesis_config_serde_import__::__private::Result<Self, __D::Error>
        where
            __D: __genesis_config_serde_import__::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            #[doc(hidden)]
            enum __Field {
                __field0,
            }
            #[doc(hidden)]
            struct __FieldVisitor;
            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(
                        __formatter,
                        "field identifier",
                    )
                }
                fn visit_u64<__E>(
                    self,
                    __value: u64,
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        _ => {
                            _serde::__private::Err(
                                _serde::de::Error::invalid_value(
                                    _serde::de::Unexpected::Unsigned(__value),
                                    &"field index 0 <= i < 1",
                                ),
                            )
                        }
                    }
                }
                fn visit_str<__E>(
                    self,
                    __value: &str,
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "system" => _serde::__private::Ok(__Field::__field0),
                        _ => {
                            _serde::__private::Err(
                                _serde::de::Error::unknown_field(__value, FIELDS),
                            )
                        }
                    }
                }
                fn visit_bytes<__E>(
                    self,
                    __value: &[u8],
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"system" => _serde::__private::Ok(__Field::__field0),
                        _ => {
                            let __value = &_serde::__private::from_utf8_lossy(__value);
                            _serde::__private::Err(
                                _serde::de::Error::unknown_field(__value, FIELDS),
                            )
                        }
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(
                    __deserializer: __D,
                ) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(
                        __deserializer,
                        __FieldVisitor,
                    )
                }
            }
            #[doc(hidden)]
            struct __Visitor<'de> {
                marker: _serde::__private::PhantomData<RuntimeGenesisConfig>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                type Value = RuntimeGenesisConfig;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(
                        __formatter,
                        "struct RuntimeGenesisConfig",
                    )
                }
                #[inline]
                fn visit_seq<__A>(
                    self,
                    mut __seq: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::SeqAccess<'de>,
                {
                    let __field0 = match _serde::de::SeqAccess::next_element::<
                        SystemConfig,
                    >(&mut __seq)? {
                        _serde::__private::Some(__value) => __value,
                        _serde::__private::None => {
                            return _serde::__private::Err(
                                _serde::de::Error::invalid_length(
                                    0usize,
                                    &"struct RuntimeGenesisConfig with 1 element",
                                ),
                            );
                        }
                    };
                    _serde::__private::Ok(RuntimeGenesisConfig {
                        system: __field0,
                    })
                }
                #[inline]
                fn visit_map<__A>(
                    self,
                    mut __map: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::MapAccess<'de>,
                {
                    let mut __field0: _serde::__private::Option<SystemConfig> = _serde::__private::None;
                    while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                        __Field,
                    >(&mut __map)? {
                        match __key {
                            __Field::__field0 => {
                                if _serde::__private::Option::is_some(&__field0) {
                                    return _serde::__private::Err(
                                        <__A::Error as _serde::de::Error>::duplicate_field("system"),
                                    );
                                }
                                __field0 = _serde::__private::Some(
                                    _serde::de::MapAccess::next_value::<
                                        SystemConfig,
                                    >(&mut __map)?,
                                );
                            }
                        }
                    }
                    let __field0 = match __field0 {
                        _serde::__private::Some(__field0) => __field0,
                        _serde::__private::None => {
                            _serde::__private::de::missing_field("system")?
                        }
                    };
                    _serde::__private::Ok(RuntimeGenesisConfig {
                        system: __field0,
                    })
                }
            }
            #[doc(hidden)]
            const FIELDS: &'static [&'static str] = &["system"];
            _serde::Deserializer::deserialize_struct(
                __deserializer,
                "RuntimeGenesisConfig",
                FIELDS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<RuntimeGenesisConfig>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
#[automatically_derived]
impl ::core::default::Default for RuntimeGenesisConfig {
    #[inline]
    fn default() -> RuntimeGenesisConfig {
        RuntimeGenesisConfig {
            system: ::core::default::Default::default(),
        }
    }
}
#[cfg(any(feature = "std", test))]
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::BuildStorage
for RuntimeGenesisConfig {
    fn assimilate_storage(
        &self,
        storage: &mut self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::Storage,
    ) -> std::result::Result<(), String> {
        self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::BasicExternalities::execute_with_storage(
            storage,
            || {
                <Self as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::BuildGenesisConfig>::build(
                    &self,
                );
                Ok(())
            },
        )
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::BuildGenesisConfig
for RuntimeGenesisConfig {
    fn build(&self) {
        <SystemConfig as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::BuildGenesisConfig>::build(
            &self.system,
        );
        <AllPalletsWithSystem as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::OnGenesis>::on_genesis();
    }
}
extern crate test;
#[cfg(test)]
#[rustc_test_marker = "test_genesis_config_builds"]
pub const test_genesis_config_builds: test::TestDescAndFn = test::TestDescAndFn {
    desc: test::TestDesc {
        name: test::StaticTestName("test_genesis_config_builds"),
        ignore: false,
        ignore_message: ::core::option::Option::None,
        source_file: "substrate/frame/support/test/tests/runtime_metadata.rs",
        start_line: 58usize,
        start_col: 1usize,
        end_line: 63usize,
        end_col: 2usize,
        compile_fail: false,
        no_run: false,
        should_panic: test::ShouldPanic::No,
        test_type: test::TestType::IntegrationTest,
    },
    testfn: test::StaticTestFn(
        #[coverage(off)]
        || test::assert_test_result(test_genesis_config_builds()),
    ),
};
/// Test the `Default` derive impl of the `RuntimeGenesisConfig`.
#[cfg(test)]
fn test_genesis_config_builds() {
    self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::sp_io::TestExternalities::default()
        .execute_with(|| {
            <RuntimeGenesisConfig as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::BuildGenesisConfig>::build(
                &RuntimeGenesisConfig::default(),
            );
        });
}
trait InherentDataExt {
    fn create_extrinsics(
        &self,
    ) -> self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Vec<
        <<Runtime as frame_system::Config>::Block as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block>::Extrinsic,
    >;
    fn check_extrinsics(
        &self,
        block: &<Runtime as frame_system::Config>::Block,
    ) -> self::sp_api_hidden_includes_construct_runtime::hidden_include::inherent::CheckInherentsResult;
}
impl InherentDataExt
for self::sp_api_hidden_includes_construct_runtime::hidden_include::inherent::InherentData {
    fn create_extrinsics(
        &self,
    ) -> self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Vec<
        <<Runtime as frame_system::Config>::Block as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block>::Extrinsic,
    > {
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::{
            inherent::ProvideInherent, traits::InherentBuilder,
        };
        let mut inherents = self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::Vec::new();
        inherents
    }
    fn check_extrinsics(
        &self,
        block: &<Runtime as frame_system::Config>::Block,
    ) -> self::sp_api_hidden_includes_construct_runtime::hidden_include::inherent::CheckInherentsResult {
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::inherent::{
            ProvideInherent, IsFatalError,
        };
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::{
            IsSubType, ExtrinsicCall,
        };
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block as _;
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::{
            sp_inherents::Error, log,
        };
        let mut result = self::sp_api_hidden_includes_construct_runtime::hidden_include::inherent::CheckInherentsResult::new();
        fn handle_put_error_result(res: Result<(), Error>) {
            const LOG_TARGET: &str = "runtime::inherent";
            match res {
                Ok(()) => {}
                Err(Error::InherentDataExists(id)) => {
                    let lvl = ::log::Level::Debug;
                    if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                        ::log::__private_api::log(
                            format_args!(
                                "Some error already reported for inherent {0:?}, new non fatal error is ignored",
                                id,
                            ),
                            lvl,
                            &(
                                LOG_TARGET,
                                "runtime_metadata",
                                ::log::__private_api::loc(),
                            ),
                            (),
                        );
                    }
                }
                Err(Error::FatalErrorReported) => {
                    let lvl = ::log::Level::Error;
                    if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                        ::log::__private_api::log(
                            format_args!(
                                "Fatal error already reported, unexpected considering there is only one fatal error",
                            ),
                            lvl,
                            &(
                                LOG_TARGET,
                                "runtime_metadata",
                                ::log::__private_api::loc(),
                            ),
                            (),
                        );
                    }
                }
                Err(_) => {
                    let lvl = ::log::Level::Error;
                    if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                        ::log::__private_api::log(
                            format_args!("Unexpected error from `put_error` operation"),
                            lvl,
                            &(
                                LOG_TARGET,
                                "runtime_metadata",
                                ::log::__private_api::loc(),
                            ),
                            (),
                        );
                    }
                }
            }
        }
        for xt in block.extrinsics() {
            if !(self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::ExtrinsicLike::is_bare(
                xt,
            )) {
                break;
            }
            let mut is_inherent = false;
            if !is_inherent {
                break;
            }
        }
        result
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::IsInherent<
    <<Runtime as frame_system::Config>::Block as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block>::Extrinsic,
> for Runtime {
    fn is_inherent(
        ext: &<<Runtime as frame_system::Config>::Block as self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block>::Extrinsic,
    ) -> bool {
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::inherent::ProvideInherent;
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::{
            IsSubType, ExtrinsicCall,
        };
        let is_bare = self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::ExtrinsicLike::is_bare(
            ext,
        );
        if !is_bare {
            return false;
        }
        false
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::EnsureInherentsAreFirst<
    <Runtime as frame_system::Config>::Block,
> for Runtime {
    fn ensure_inherents_are_first(
        block: &<Runtime as frame_system::Config>::Block,
    ) -> Result<u32, u32> {
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::inherent::ProvideInherent;
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::{
            IsSubType, ExtrinsicCall,
        };
        use self::sp_api_hidden_includes_construct_runtime::hidden_include::sp_runtime::traits::Block as _;
        let mut num_inherents = 0u32;
        for (i, xt) in block.extrinsics().iter().enumerate() {
            if <Self as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::IsInherent<
                _,
            >>::is_inherent(xt) {
                if num_inherents != i as u32 {
                    return Err(i as u32);
                }
                num_inherents += 1;
            }
        }
        Ok(num_inherents)
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::unsigned::ValidateUnsigned
for Runtime {
    type Call = RuntimeCall;
    fn pre_dispatch(
        call: &Self::Call,
    ) -> Result<
        (),
        self::sp_api_hidden_includes_construct_runtime::hidden_include::unsigned::TransactionValidityError,
    > {
        #[allow(unreachable_patterns)]
        match call {
            RuntimeCall::System(inner_call) => System::pre_dispatch(inner_call),
            _ => Ok(()),
        }
    }
    fn validate_unsigned(
        #[allow(unused_variables)]
        source: self::sp_api_hidden_includes_construct_runtime::hidden_include::unsigned::TransactionSource,
        call: &Self::Call,
    ) -> self::sp_api_hidden_includes_construct_runtime::hidden_include::unsigned::TransactionValidity {
        #[allow(unreachable_patterns)]
        match call {
            RuntimeCall::System(inner_call) => {
                System::validate_unsigned(source, inner_call)
            }
            _ => {
                self::sp_api_hidden_includes_construct_runtime::hidden_include::unsigned::UnknownTransaction::NoUnsignedValidator
                    .into()
            }
        }
    }
}
/// A reason for placing a freeze on funds.
pub enum RuntimeFreezeReason {}
#[automatically_derived]
impl ::core::marker::Copy for RuntimeFreezeReason {}
#[automatically_derived]
impl ::core::clone::Clone for RuntimeFreezeReason {
    #[inline]
    fn clone(&self) -> RuntimeFreezeReason {
        *self
    }
}
#[automatically_derived]
impl ::core::cmp::Eq for RuntimeFreezeReason {
    #[inline]
    #[doc(hidden)]
    #[coverage(off)]
    fn assert_receiver_is_total_eq(&self) -> () {}
}
#[automatically_derived]
impl ::core::marker::StructuralPartialEq for RuntimeFreezeReason {}
#[automatically_derived]
impl ::core::cmp::PartialEq for RuntimeFreezeReason {
    #[inline]
    fn eq(&self, other: &RuntimeFreezeReason) -> bool {
        match *self {}
    }
}
#[allow(deprecated)]
const _: () = {
    #[automatically_derived]
    impl ::codec::Encode for RuntimeFreezeReason {}
    #[automatically_derived]
    impl ::codec::EncodeLike for RuntimeFreezeReason {}
};
#[allow(deprecated)]
const _: () = {
    #[automatically_derived]
    impl ::codec::Decode for RuntimeFreezeReason {
        fn decode<__CodecInputEdqy: ::codec::Input>(
            __codec_input_edqy: &mut __CodecInputEdqy,
        ) -> ::core::result::Result<Self, ::codec::Error> {
            match __codec_input_edqy
                .read_byte()
                .map_err(|e| {
                    e
                        .chain(
                            "Could not decode `RuntimeFreezeReason`, failed to read variant byte",
                        )
                })?
            {
                _ => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Err(
                            <_ as ::core::convert::Into<
                                _,
                            >>::into(
                                "Could not decode `RuntimeFreezeReason`, variant doesn't exist",
                            ),
                        )
                    })();
                }
            }
        }
    }
};
const _: () = {
    impl ::codec::MaxEncodedLen for RuntimeFreezeReason {
        fn max_encoded_len() -> ::core::primitive::usize {
            0_usize.saturating_add(1)
        }
    }
};
#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
const _: () = {
    impl ::scale_info::TypeInfo for RuntimeFreezeReason {
        type Identity = Self;
        fn type_info() -> ::scale_info::Type {
            ::scale_info::Type::builder()
                .path(
                    ::scale_info::Path::new_with_replace(
                        "RuntimeFreezeReason",
                        "runtime_metadata",
                        &[],
                    ),
                )
                .type_params(::alloc::vec::Vec::new())
                .docs(&["A reason for placing a freeze on funds."])
                .variant(::scale_info::build::Variants::new())
        }
    }
};
impl core::fmt::Debug for RuntimeFreezeReason {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            _ => Ok(()),
        }
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::VariantCount
for RuntimeFreezeReason {
    const VARIANT_COUNT: u32 = 0;
}
/// A reason for placing a hold on funds.
pub enum RuntimeHoldReason {}
#[automatically_derived]
impl ::core::marker::Copy for RuntimeHoldReason {}
#[automatically_derived]
impl ::core::clone::Clone for RuntimeHoldReason {
    #[inline]
    fn clone(&self) -> RuntimeHoldReason {
        *self
    }
}
#[automatically_derived]
impl ::core::cmp::Eq for RuntimeHoldReason {
    #[inline]
    #[doc(hidden)]
    #[coverage(off)]
    fn assert_receiver_is_total_eq(&self) -> () {}
}
#[automatically_derived]
impl ::core::marker::StructuralPartialEq for RuntimeHoldReason {}
#[automatically_derived]
impl ::core::cmp::PartialEq for RuntimeHoldReason {
    #[inline]
    fn eq(&self, other: &RuntimeHoldReason) -> bool {
        match *self {}
    }
}
#[allow(deprecated)]
const _: () = {
    #[automatically_derived]
    impl ::codec::Encode for RuntimeHoldReason {}
    #[automatically_derived]
    impl ::codec::EncodeLike for RuntimeHoldReason {}
};
#[allow(deprecated)]
const _: () = {
    #[automatically_derived]
    impl ::codec::Decode for RuntimeHoldReason {
        fn decode<__CodecInputEdqy: ::codec::Input>(
            __codec_input_edqy: &mut __CodecInputEdqy,
        ) -> ::core::result::Result<Self, ::codec::Error> {
            match __codec_input_edqy
                .read_byte()
                .map_err(|e| {
                    e
                        .chain(
                            "Could not decode `RuntimeHoldReason`, failed to read variant byte",
                        )
                })?
            {
                _ => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Err(
                            <_ as ::core::convert::Into<
                                _,
                            >>::into(
                                "Could not decode `RuntimeHoldReason`, variant doesn't exist",
                            ),
                        )
                    })();
                }
            }
        }
    }
};
const _: () = {
    impl ::codec::MaxEncodedLen for RuntimeHoldReason {
        fn max_encoded_len() -> ::core::primitive::usize {
            0_usize.saturating_add(1)
        }
    }
};
#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
const _: () = {
    impl ::scale_info::TypeInfo for RuntimeHoldReason {
        type Identity = Self;
        fn type_info() -> ::scale_info::Type {
            ::scale_info::Type::builder()
                .path(
                    ::scale_info::Path::new_with_replace(
                        "RuntimeHoldReason",
                        "runtime_metadata",
                        &[],
                    ),
                )
                .type_params(::alloc::vec::Vec::new())
                .docs(&["A reason for placing a hold on funds."])
                .variant(::scale_info::build::Variants::new())
        }
    }
};
impl core::fmt::Debug for RuntimeHoldReason {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            _ => Ok(()),
        }
    }
}
impl self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::VariantCount
for RuntimeHoldReason {
    const VARIANT_COUNT: u32 = 0;
}
/// An identifier for each lock placed on funds.
pub enum RuntimeLockId {}
#[automatically_derived]
impl ::core::marker::Copy for RuntimeLockId {}
#[automatically_derived]
impl ::core::clone::Clone for RuntimeLockId {
    #[inline]
    fn clone(&self) -> RuntimeLockId {
        *self
    }
}
#[automatically_derived]
impl ::core::cmp::Eq for RuntimeLockId {
    #[inline]
    #[doc(hidden)]
    #[coverage(off)]
    fn assert_receiver_is_total_eq(&self) -> () {}
}
#[automatically_derived]
impl ::core::marker::StructuralPartialEq for RuntimeLockId {}
#[automatically_derived]
impl ::core::cmp::PartialEq for RuntimeLockId {
    #[inline]
    fn eq(&self, other: &RuntimeLockId) -> bool {
        match *self {}
    }
}
#[allow(deprecated)]
const _: () = {
    #[automatically_derived]
    impl ::codec::Encode for RuntimeLockId {}
    #[automatically_derived]
    impl ::codec::EncodeLike for RuntimeLockId {}
};
#[allow(deprecated)]
const _: () = {
    #[automatically_derived]
    impl ::codec::Decode for RuntimeLockId {
        fn decode<__CodecInputEdqy: ::codec::Input>(
            __codec_input_edqy: &mut __CodecInputEdqy,
        ) -> ::core::result::Result<Self, ::codec::Error> {
            match __codec_input_edqy
                .read_byte()
                .map_err(|e| {
                    e
                        .chain(
                            "Could not decode `RuntimeLockId`, failed to read variant byte",
                        )
                })?
            {
                _ => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Err(
                            <_ as ::core::convert::Into<
                                _,
                            >>::into(
                                "Could not decode `RuntimeLockId`, variant doesn't exist",
                            ),
                        )
                    })();
                }
            }
        }
    }
};
const _: () = {
    impl ::codec::MaxEncodedLen for RuntimeLockId {
        fn max_encoded_len() -> ::core::primitive::usize {
            0_usize.saturating_add(1)
        }
    }
};
#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
const _: () = {
    impl ::scale_info::TypeInfo for RuntimeLockId {
        type Identity = Self;
        fn type_info() -> ::scale_info::Type {
            ::scale_info::Type::builder()
                .path(
                    ::scale_info::Path::new_with_replace(
                        "RuntimeLockId",
                        "runtime_metadata",
                        &[],
                    ),
                )
                .type_params(::alloc::vec::Vec::new())
                .docs(&["An identifier for each lock placed on funds."])
                .variant(::scale_info::build::Variants::new())
        }
    }
};
impl core::fmt::Debug for RuntimeLockId {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            _ => Ok(()),
        }
    }
}
/// A reason for slashing funds.
pub enum RuntimeSlashReason {}
#[automatically_derived]
impl ::core::marker::Copy for RuntimeSlashReason {}
#[automatically_derived]
impl ::core::clone::Clone for RuntimeSlashReason {
    #[inline]
    fn clone(&self) -> RuntimeSlashReason {
        *self
    }
}
#[automatically_derived]
impl ::core::cmp::Eq for RuntimeSlashReason {
    #[inline]
    #[doc(hidden)]
    #[coverage(off)]
    fn assert_receiver_is_total_eq(&self) -> () {}
}
#[automatically_derived]
impl ::core::marker::StructuralPartialEq for RuntimeSlashReason {}
#[automatically_derived]
impl ::core::cmp::PartialEq for RuntimeSlashReason {
    #[inline]
    fn eq(&self, other: &RuntimeSlashReason) -> bool {
        match *self {}
    }
}
#[allow(deprecated)]
const _: () = {
    #[automatically_derived]
    impl ::codec::Encode for RuntimeSlashReason {}
    #[automatically_derived]
    impl ::codec::EncodeLike for RuntimeSlashReason {}
};
#[allow(deprecated)]
const _: () = {
    #[automatically_derived]
    impl ::codec::Decode for RuntimeSlashReason {
        fn decode<__CodecInputEdqy: ::codec::Input>(
            __codec_input_edqy: &mut __CodecInputEdqy,
        ) -> ::core::result::Result<Self, ::codec::Error> {
            match __codec_input_edqy
                .read_byte()
                .map_err(|e| {
                    e
                        .chain(
                            "Could not decode `RuntimeSlashReason`, failed to read variant byte",
                        )
                })?
            {
                _ => {
                    #[allow(clippy::redundant_closure_call)]
                    return (move || {
                        ::core::result::Result::Err(
                            <_ as ::core::convert::Into<
                                _,
                            >>::into(
                                "Could not decode `RuntimeSlashReason`, variant doesn't exist",
                            ),
                        )
                    })();
                }
            }
        }
    }
};
const _: () = {
    impl ::codec::MaxEncodedLen for RuntimeSlashReason {
        fn max_encoded_len() -> ::core::primitive::usize {
            0_usize.saturating_add(1)
        }
    }
};
#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
const _: () = {
    impl ::scale_info::TypeInfo for RuntimeSlashReason {
        type Identity = Self;
        fn type_info() -> ::scale_info::Type {
            ::scale_info::Type::builder()
                .path(
                    ::scale_info::Path::new_with_replace(
                        "RuntimeSlashReason",
                        "runtime_metadata",
                        &[],
                    ),
                )
                .type_params(::alloc::vec::Vec::new())
                .docs(&["A reason for slashing funds."])
                .variant(::scale_info::build::Variants::new())
        }
    }
};
impl core::fmt::Debug for RuntimeSlashReason {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            _ => Ok(()),
        }
    }
}
#[cfg(test)]
mod __construct_runtime_integrity_test {
    use super::*;
    extern crate test;
    #[cfg(test)]
    #[rustc_test_marker = "__construct_runtime_integrity_test::runtime_integrity_tests"]
    pub const runtime_integrity_tests: test::TestDescAndFn = test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName(
                "__construct_runtime_integrity_test::runtime_integrity_tests",
            ),
            ignore: false,
            ignore_message: ::core::option::Option::None,
            source_file: "substrate/frame/support/test/tests/runtime_metadata.rs",
            start_line: 58usize,
            start_col: 1usize,
            end_line: 63usize,
            end_col: 2usize,
            compile_fail: false,
            no_run: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(
            #[coverage(off)]
            || test::assert_test_result(runtime_integrity_tests()),
        ),
    };
    pub fn runtime_integrity_tests() {
        self::sp_api_hidden_includes_construct_runtime::hidden_include::__private::sp_tracing::try_init_simple();
        <AllPalletsWithSystem as self::sp_api_hidden_includes_construct_runtime::hidden_include::traits::IntegrityTest>::integrity_test();
    }
}
#[allow(deprecated)]
const _: () = if !(<frame_system::Error<
    Runtime,
> as ::frame_support::traits::PalletError>::MAX_ENCODED_SIZE
    <= ::frame_support::MAX_MODULE_ERROR_ENCODED_SIZE)
{
    {
        ::core::panicking::panic_fmt(
            format_args!(
                "The maximum encoded size of the error type in the `System` pallet exceeds `MAX_MODULE_ERROR_ENCODED_SIZE`",
            ),
        );
    }
};
#[doc(hidden)]
#[allow(dead_code)]
#[allow(deprecated)]
pub mod runtime_decl_for_api {
    pub use super::*;
    /// ApiWithCustomVersion trait documentation
    ///
    /// Documentation on multiline.
    #[deprecated]
    #[allow(deprecated)]
    pub trait ApiV1<Block: sp_api::__private::BlockT> {
        fn test(data: u64);
        /// something_with_block.
        fn something_with_block(block: Block) -> Block;
        #[deprecated = "example"]
        fn function_with_two_args(data: u64, block: Block);
        #[deprecated(note = "example", since = "example")]
        fn same_name();
        #[deprecated(note = "example")]
        fn wild_card(__runtime_api_generated_name_0__: u32);
    }
    pub use ApiV1 as Api;
    #[inline(always)]
    pub fn runtime_metadata<Block: sp_api::__private::BlockT>(
        impl_version: u32,
    ) -> sp_api::__private::metadata_ir::RuntimeApiMetadataIR
    where
        u64: sp_api::__private::scale_info::TypeInfo + 'static,
        Block: sp_api::__private::scale_info::TypeInfo + 'static,
        Block: sp_api::__private::scale_info::TypeInfo + 'static,
        u64: sp_api::__private::scale_info::TypeInfo + 'static,
        Block: sp_api::__private::scale_info::TypeInfo + 'static,
        u32: sp_api::__private::scale_info::TypeInfo + 'static,
    {
        sp_api::__private::metadata_ir::RuntimeApiMetadataIR {
            name: "Api",
            methods: [
                if 1u32 <= impl_version {
                    Some(sp_api::__private::metadata_ir::RuntimeApiMethodMetadataIR {
                        name: "test",
                        inputs: <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                sp_api::__private::metadata_ir::RuntimeApiMethodParamMetadataIR {
                                    name: "data",
                                    ty: sp_api::__private::scale_info::meta_type::<u64>(),
                                },
                            ]),
                        ),
                        output: sp_api::__private::scale_info::meta_type::<()>(),
                        docs: ::alloc::vec::Vec::new(),
                        deprecation_info: sp_api::__private::metadata_ir::DeprecationStatusIR::NotDeprecated,
                    })
                } else {
                    None
                },
                if 1u32 <= impl_version {
                    Some(sp_api::__private::metadata_ir::RuntimeApiMethodMetadataIR {
                        name: "something_with_block",
                        inputs: <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                sp_api::__private::metadata_ir::RuntimeApiMethodParamMetadataIR {
                                    name: "block",
                                    ty: sp_api::__private::scale_info::meta_type::<Block>(),
                                },
                            ]),
                        ),
                        output: sp_api::__private::scale_info::meta_type::<Block>(),
                        docs: <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([" something_with_block."]),
                        ),
                        deprecation_info: sp_api::__private::metadata_ir::DeprecationStatusIR::NotDeprecated,
                    })
                } else {
                    None
                },
                if 1u32 <= impl_version {
                    Some(sp_api::__private::metadata_ir::RuntimeApiMethodMetadataIR {
                        name: "function_with_two_args",
                        inputs: <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                sp_api::__private::metadata_ir::RuntimeApiMethodParamMetadataIR {
                                    name: "data",
                                    ty: sp_api::__private::scale_info::meta_type::<u64>(),
                                },
                                sp_api::__private::metadata_ir::RuntimeApiMethodParamMetadataIR {
                                    name: "block",
                                    ty: sp_api::__private::scale_info::meta_type::<Block>(),
                                },
                            ]),
                        ),
                        output: sp_api::__private::scale_info::meta_type::<()>(),
                        docs: ::alloc::vec::Vec::new(),
                        deprecation_info: sp_api::__private::metadata_ir::DeprecationStatusIR::Deprecated {
                            note: "example",
                            since: None,
                        },
                    })
                } else {
                    None
                },
                if 1u32 <= impl_version {
                    Some(sp_api::__private::metadata_ir::RuntimeApiMethodMetadataIR {
                        name: "same_name",
                        inputs: ::alloc::vec::Vec::new(),
                        output: sp_api::__private::scale_info::meta_type::<()>(),
                        docs: ::alloc::vec::Vec::new(),
                        deprecation_info: sp_api::__private::metadata_ir::DeprecationStatusIR::Deprecated {
                            note: "example",
                            since: Some("example"),
                        },
                    })
                } else {
                    None
                },
                if 1u32 <= impl_version {
                    Some(sp_api::__private::metadata_ir::RuntimeApiMethodMetadataIR {
                        name: "wild_card",
                        inputs: <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                sp_api::__private::metadata_ir::RuntimeApiMethodParamMetadataIR {
                                    name: "__runtime_api_generated_name_0__",
                                    ty: sp_api::__private::scale_info::meta_type::<u32>(),
                                },
                            ]),
                        ),
                        output: sp_api::__private::scale_info::meta_type::<()>(),
                        docs: ::alloc::vec::Vec::new(),
                        deprecation_info: sp_api::__private::metadata_ir::DeprecationStatusIR::Deprecated {
                            note: "example",
                            since: None,
                        },
                    })
                } else {
                    None
                },
            ]
                .into_iter()
                .filter_map(|maybe_m| maybe_m)
                .collect(),
            docs: <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    " ApiWithCustomVersion trait documentation",
                    "",
                    " Documentation on multiline.",
                ]),
            ),
            deprecation_info: sp_api::__private::metadata_ir::DeprecationStatusIR::DeprecatedWithoutNote,
        }
    }
    pub const VERSION: u32 = 1u32;
    pub const ID: [u8; 8] = [9u8, 134u8, 4u8, 33u8, 62u8, 203u8, 89u8, 227u8];
}
/// ApiWithCustomVersion trait documentation
///
/// Documentation on multiline.
#[deprecated]
#[allow(deprecated)]
pub trait Api<Block: sp_api::__private::BlockT>: sp_api::__private::Core<Block> {
    fn test(
        &self,
        __runtime_api_at_param__: <Block as sp_api::__private::BlockT>::Hash,
        data: u64,
    ) -> std::result::Result<(), sp_api::__private::ApiError> {
        let __runtime_api_impl_params_encoded__ = sp_api::__private::Encode::encode(
            &(&data),
        );
        <Self as Api<
            _,
        >>::__runtime_api_internal_call_api_at(
                self,
                __runtime_api_at_param__,
                __runtime_api_impl_params_encoded__,
                &(|_version| { "Api_test" }),
            )
            .and_then(|r| std::result::Result::map_err(
                <() as sp_api::__private::Decode>::decode(&mut &r[..]),
                |err| sp_api::__private::ApiError::FailedToDecodeReturnValue {
                    function: "Api_test",
                    error: err,
                    raw: r.clone(),
                },
            ))
    }
    /// something_with_block.
    fn something_with_block(
        &self,
        __runtime_api_at_param__: <Block as sp_api::__private::BlockT>::Hash,
        block: Block,
    ) -> std::result::Result<Block, sp_api::__private::ApiError> {
        let __runtime_api_impl_params_encoded__ = sp_api::__private::Encode::encode(
            &(&block),
        );
        <Self as Api<
            _,
        >>::__runtime_api_internal_call_api_at(
                self,
                __runtime_api_at_param__,
                __runtime_api_impl_params_encoded__,
                &(|_version| { "Api_something_with_block" }),
            )
            .and_then(|r| std::result::Result::map_err(
                <Block as sp_api::__private::Decode>::decode(&mut &r[..]),
                |err| sp_api::__private::ApiError::FailedToDecodeReturnValue {
                    function: "Api_something_with_block",
                    error: err,
                    raw: r.clone(),
                },
            ))
    }
    #[deprecated = "example"]
    fn function_with_two_args(
        &self,
        __runtime_api_at_param__: <Block as sp_api::__private::BlockT>::Hash,
        data: u64,
        block: Block,
    ) -> std::result::Result<(), sp_api::__private::ApiError> {
        let __runtime_api_impl_params_encoded__ = sp_api::__private::Encode::encode(
            &(&data, &block),
        );
        <Self as Api<
            _,
        >>::__runtime_api_internal_call_api_at(
                self,
                __runtime_api_at_param__,
                __runtime_api_impl_params_encoded__,
                &(|_version| { "Api_function_with_two_args" }),
            )
            .and_then(|r| std::result::Result::map_err(
                <() as sp_api::__private::Decode>::decode(&mut &r[..]),
                |err| sp_api::__private::ApiError::FailedToDecodeReturnValue {
                    function: "Api_function_with_two_args",
                    error: err,
                    raw: r.clone(),
                },
            ))
    }
    #[deprecated(note = "example", since = "example")]
    fn same_name(
        &self,
        __runtime_api_at_param__: <Block as sp_api::__private::BlockT>::Hash,
    ) -> std::result::Result<(), sp_api::__private::ApiError> {
        let __runtime_api_impl_params_encoded__ = sp_api::__private::Encode::encode(&());
        <Self as Api<
            _,
        >>::__runtime_api_internal_call_api_at(
                self,
                __runtime_api_at_param__,
                __runtime_api_impl_params_encoded__,
                &(|_version| { "Api_same_name" }),
            )
            .and_then(|r| std::result::Result::map_err(
                <() as sp_api::__private::Decode>::decode(&mut &r[..]),
                |err| sp_api::__private::ApiError::FailedToDecodeReturnValue {
                    function: "Api_same_name",
                    error: err,
                    raw: r.clone(),
                },
            ))
    }
    #[deprecated(note = "example")]
    fn wild_card(
        &self,
        __runtime_api_at_param__: <Block as sp_api::__private::BlockT>::Hash,
        __runtime_api_generated_name_0__: u32,
    ) -> std::result::Result<(), sp_api::__private::ApiError> {
        let __runtime_api_impl_params_encoded__ = sp_api::__private::Encode::encode(
            &(&__runtime_api_generated_name_0__),
        );
        <Self as Api<
            _,
        >>::__runtime_api_internal_call_api_at(
                self,
                __runtime_api_at_param__,
                __runtime_api_impl_params_encoded__,
                &(|_version| { "Api_wild_card" }),
            )
            .and_then(|r| std::result::Result::map_err(
                <() as sp_api::__private::Decode>::decode(&mut &r[..]),
                |err| sp_api::__private::ApiError::FailedToDecodeReturnValue {
                    function: "Api_wild_card",
                    error: err,
                    raw: r.clone(),
                },
            ))
    }
    /// !!INTERNAL USE ONLY!!
    #[doc(hidden)]
    fn __runtime_api_internal_call_api_at(
        &self,
        at: <Block as sp_api::__private::BlockT>::Hash,
        params: std::vec::Vec<u8>,
        fn_name: &dyn Fn(sp_api::__private::RuntimeVersion) -> &'static str,
    ) -> std::result::Result<std::vec::Vec<u8>, sp_api::__private::ApiError>;
}
#[allow(deprecated)]
impl<Block: sp_api::__private::BlockT> sp_api::__private::RuntimeApiInfo
for dyn Api<Block> {
    const ID: [u8; 8] = [9u8, 134u8, 4u8, 33u8, 62u8, 203u8, 89u8, 227u8];
    const VERSION: u32 = 1u32;
}
pub struct RuntimeApi {}
/// Implements all runtime apis for the client side.
pub struct RuntimeApiImpl<
    Block: sp_api::__private::BlockT,
    C: sp_api::__private::CallApiAt<Block> + 'static,
> {
    call: &'static C,
    transaction_depth: std::cell::RefCell<u16>,
    changes: std::cell::RefCell<
        sp_api::__private::OverlayedChanges<sp_api::__private::HashingFor<Block>>,
    >,
    recorder: std::option::Option<sp_api::__private::ProofRecorder<Block>>,
    call_context: sp_api::__private::CallContext,
    extensions: std::cell::RefCell<sp_api::__private::Extensions>,
    extensions_generated_for: std::cell::RefCell<std::option::Option<Block::Hash>>,
}
#[automatically_derived]
impl<
    Block: sp_api::__private::BlockT,
    C: sp_api::__private::CallApiAt<Block>,
> sp_api::__private::ApiExt<Block> for RuntimeApiImpl<Block, C> {
    fn execute_in_transaction<
        F: FnOnce(&Self) -> sp_api::__private::TransactionOutcome<R>,
        R,
    >(&self, call: F) -> R
    where
        Self: Sized,
    {
        self.start_transaction();
        *std::cell::RefCell::borrow_mut(&self.transaction_depth) += 1;
        let res = call(self);
        std::cell::RefCell::borrow_mut(&self.transaction_depth)
            .checked_sub(1)
            .expect("Transactions are opened and closed together; qed");
        self.commit_or_rollback_transaction(
            match res {
                sp_api::__private::TransactionOutcome::Commit(_) => true,
                _ => false,
            },
        );
        res.into_inner()
    }
    fn has_api<A: sp_api::__private::RuntimeApiInfo + ?Sized>(
        &self,
        at: <Block as sp_api::__private::BlockT>::Hash,
    ) -> std::result::Result<bool, sp_api::__private::ApiError>
    where
        Self: Sized,
    {
        sp_api::__private::CallApiAt::<Block>::runtime_version_at(self.call, at)
            .map(|v| sp_api::__private::RuntimeVersion::has_api_with(
                &v,
                &A::ID,
                |v| v == A::VERSION,
            ))
    }
    fn has_api_with<A: sp_api::__private::RuntimeApiInfo + ?Sized, P: Fn(u32) -> bool>(
        &self,
        at: <Block as sp_api::__private::BlockT>::Hash,
        pred: P,
    ) -> std::result::Result<bool, sp_api::__private::ApiError>
    where
        Self: Sized,
    {
        sp_api::__private::CallApiAt::<Block>::runtime_version_at(self.call, at)
            .map(|v| sp_api::__private::RuntimeVersion::has_api_with(&v, &A::ID, pred))
    }
    fn api_version<A: sp_api::__private::RuntimeApiInfo + ?Sized>(
        &self,
        at: <Block as sp_api::__private::BlockT>::Hash,
    ) -> std::result::Result<Option<u32>, sp_api::__private::ApiError>
    where
        Self: Sized,
    {
        sp_api::__private::CallApiAt::<Block>::runtime_version_at(self.call, at)
            .map(|v| sp_api::__private::RuntimeVersion::api_version(&v, &A::ID))
    }
    fn record_proof(&mut self) {
        self.recorder = std::option::Option::Some(std::default::Default::default());
    }
    fn proof_recorder(
        &self,
    ) -> std::option::Option<sp_api::__private::ProofRecorder<Block>> {
        std::clone::Clone::clone(&self.recorder)
    }
    fn extract_proof(&mut self) -> std::option::Option<sp_api::__private::StorageProof> {
        let recorder = std::option::Option::take(&mut self.recorder);
        std::option::Option::map(
            recorder,
            |recorder| {
                sp_api::__private::ProofRecorder::<Block>::drain_storage_proof(recorder)
            },
        )
    }
    fn into_storage_changes<
        B: sp_api::__private::StateBackend<sp_api::__private::HashingFor<Block>>,
    >(
        &self,
        backend: &B,
        parent_hash: Block::Hash,
    ) -> ::core::result::Result<sp_api::__private::StorageChanges<Block>, String>
    where
        Self: Sized,
    {
        let state_version = sp_api::__private::CallApiAt::<
            Block,
        >::runtime_version_at(self.call, std::clone::Clone::clone(&parent_hash))
            .map(|v| sp_api::__private::RuntimeVersion::state_version(&v))
            .map_err(|e| ::alloc::__export::must_use({
                let res = ::alloc::fmt::format(
                    format_args!("Failed to get state version: {0}", e),
                );
                res
            }))?;
        sp_api::__private::OverlayedChanges::drain_storage_changes(
            &mut std::cell::RefCell::borrow_mut(&self.changes),
            backend,
            state_version,
        )
    }
    fn set_call_context(&mut self, call_context: sp_api::__private::CallContext) {
        self.call_context = call_context;
    }
    fn register_extension<E: sp_api::__private::Extension>(&mut self, extension: E) {
        std::cell::RefCell::borrow_mut(&self.extensions).register(extension);
    }
}
#[automatically_derived]
impl<
    Block: sp_api::__private::BlockT,
    C,
> sp_api::__private::ConstructRuntimeApi<Block, C> for RuntimeApi
where
    C: sp_api::__private::CallApiAt<Block> + 'static,
{
    type RuntimeApi = RuntimeApiImpl<Block, C>;
    fn construct_runtime_api<'a>(
        call: &'a C,
    ) -> sp_api::__private::ApiRef<'a, Self::RuntimeApi> {
        RuntimeApiImpl {
            call: unsafe { std::mem::transmute(call) },
            transaction_depth: 0.into(),
            changes: std::default::Default::default(),
            recorder: std::default::Default::default(),
            call_context: sp_api::__private::CallContext::Offchain,
            extensions: std::default::Default::default(),
            extensions_generated_for: std::default::Default::default(),
        }
            .into()
    }
}
#[automatically_derived]
impl<
    Block: sp_api::__private::BlockT,
    C: sp_api::__private::CallApiAt<Block>,
> RuntimeApiImpl<Block, C> {
    fn commit_or_rollback_transaction(&self, commit: bool) {
        let proof = "\
                    We only close a transaction when we opened one ourself.
                    Other parts of the runtime that make use of transactions (state-machine)
                    also balance their transactions. The runtime cannot close client initiated
                    transactions; qed";
        let res = if commit {
            let res = if let Some(recorder) = &self.recorder {
                sp_api::__private::ProofRecorder::<Block>::commit_transaction(&recorder)
            } else {
                Ok(())
            };
            let res2 = sp_api::__private::OverlayedChanges::commit_transaction(
                &mut std::cell::RefCell::borrow_mut(&self.changes),
            );
            std::result::Result::and(res, std::result::Result::map_err(res2, drop))
        } else {
            let res = if let Some(recorder) = &self.recorder {
                sp_api::__private::ProofRecorder::<
                    Block,
                >::rollback_transaction(&recorder)
            } else {
                Ok(())
            };
            let res2 = sp_api::__private::OverlayedChanges::rollback_transaction(
                &mut std::cell::RefCell::borrow_mut(&self.changes),
            );
            std::result::Result::and(res, std::result::Result::map_err(res2, drop))
        };
        std::result::Result::expect(res, proof);
    }
    fn start_transaction(&self) {
        sp_api::__private::OverlayedChanges::start_transaction(
            &mut std::cell::RefCell::borrow_mut(&self.changes),
        );
        if let Some(recorder) = &self.recorder {
            sp_api::__private::ProofRecorder::<Block>::start_transaction(&recorder);
        }
    }
}
#[allow(deprecated)]
impl self::runtime_decl_for_api::Api<Block> for Runtime {
    fn test(_data: u64) {
        ::core::panicking::panic("not implemented")
    }
    fn something_with_block(_: Block) -> Block {
        ::core::panicking::panic("not implemented")
    }
    fn function_with_two_args(_: u64, _: Block) {
        ::core::panicking::panic("not implemented")
    }
    fn same_name() {}
    fn wild_card(_: u32) {}
}
impl sp_api::runtime_decl_for_core::Core<Block> for Runtime {
    fn version() -> sp_version::RuntimeVersion {
        ::core::panicking::panic("not implemented")
    }
    fn execute_block(_: Block) {
        ::core::panicking::panic("not implemented")
    }
    fn initialize_block(
        _: &<Block as BlockT>::Header,
    ) -> sp_runtime::ExtrinsicInclusionMode {
        ::core::panicking::panic("not implemented")
    }
}
#[allow(deprecated)]
#[automatically_derived]
impl<
    __SrApiBlock__: sp_api::__private::BlockT,
    RuntimeApiImplCall: sp_api::__private::CallApiAt<__SrApiBlock__> + 'static,
> self::Api<__SrApiBlock__> for RuntimeApiImpl<__SrApiBlock__, RuntimeApiImplCall>
where
    RuntimeApiImplCall::StateBackend: sp_api::__private::StateBackend<
        sp_api::__private::HashingFor<__SrApiBlock__>,
    >,
    &'static RuntimeApiImplCall: Send,
{
    fn __runtime_api_internal_call_api_at(
        &self,
        at: <__SrApiBlock__ as sp_api::__private::BlockT>::Hash,
        params: std::vec::Vec<u8>,
        fn_name: &dyn Fn(sp_api::__private::RuntimeVersion) -> &'static str,
    ) -> std::result::Result<std::vec::Vec<u8>, sp_api::__private::ApiError> {
        let transaction_depth = *std::cell::RefCell::borrow(&self.transaction_depth);
        if transaction_depth == 0 {
            self.start_transaction();
        }
        let res = (|| {
            let version = sp_api::__private::CallApiAt::<
                __SrApiBlock__,
            >::runtime_version_at(self.call, at)?;
            match &mut *std::cell::RefCell::borrow_mut(&self.extensions_generated_for) {
                Some(generated_for) => {
                    if *generated_for != at {
                        return std::result::Result::Err(
                            sp_api::__private::ApiError::UsingSameInstanceForDifferentBlocks,
                        );
                    }
                }
                generated_for @ None => {
                    sp_api::__private::CallApiAt::<
                        __SrApiBlock__,
                    >::initialize_extensions(
                        self.call,
                        at,
                        &mut std::cell::RefCell::borrow_mut(&self.extensions),
                    )?;
                    *generated_for = Some(at);
                }
            }
            let params = sp_api::__private::CallApiAtParams {
                at,
                function: (*fn_name)(version),
                arguments: params,
                overlayed_changes: &self.changes,
                call_context: self.call_context,
                recorder: &self.recorder,
                extensions: &self.extensions,
            };
            sp_api::__private::CallApiAt::<
                __SrApiBlock__,
            >::call_api_at(self.call, params)
        })();
        if transaction_depth == 0 {
            self.commit_or_rollback_transaction(std::result::Result::is_ok(&res));
        }
        res
    }
}
#[automatically_derived]
impl<
    __SrApiBlock__: sp_api::__private::BlockT,
    RuntimeApiImplCall: sp_api::__private::CallApiAt<__SrApiBlock__> + 'static,
> sp_api::Core<__SrApiBlock__> for RuntimeApiImpl<__SrApiBlock__, RuntimeApiImplCall>
where
    RuntimeApiImplCall::StateBackend: sp_api::__private::StateBackend<
        sp_api::__private::HashingFor<__SrApiBlock__>,
    >,
    &'static RuntimeApiImplCall: Send,
{
    fn __runtime_api_internal_call_api_at(
        &self,
        at: <__SrApiBlock__ as sp_api::__private::BlockT>::Hash,
        params: std::vec::Vec<u8>,
        fn_name: &dyn Fn(sp_api::__private::RuntimeVersion) -> &'static str,
    ) -> std::result::Result<std::vec::Vec<u8>, sp_api::__private::ApiError> {
        let transaction_depth = *std::cell::RefCell::borrow(&self.transaction_depth);
        if transaction_depth == 0 {
            self.start_transaction();
        }
        let res = (|| {
            let version = sp_api::__private::CallApiAt::<
                __SrApiBlock__,
            >::runtime_version_at(self.call, at)?;
            match &mut *std::cell::RefCell::borrow_mut(&self.extensions_generated_for) {
                Some(generated_for) => {
                    if *generated_for != at {
                        return std::result::Result::Err(
                            sp_api::__private::ApiError::UsingSameInstanceForDifferentBlocks,
                        );
                    }
                }
                generated_for @ None => {
                    sp_api::__private::CallApiAt::<
                        __SrApiBlock__,
                    >::initialize_extensions(
                        self.call,
                        at,
                        &mut std::cell::RefCell::borrow_mut(&self.extensions),
                    )?;
                    *generated_for = Some(at);
                }
            }
            let params = sp_api::__private::CallApiAtParams {
                at,
                function: (*fn_name)(version),
                arguments: params,
                overlayed_changes: &self.changes,
                call_context: self.call_context,
                recorder: &self.recorder,
                extensions: &self.extensions,
            };
            sp_api::__private::CallApiAt::<
                __SrApiBlock__,
            >::call_api_at(self.call, params)
        })();
        if transaction_depth == 0 {
            self.commit_or_rollback_transaction(std::result::Result::is_ok(&res));
        }
        res
    }
}
pub const RUNTIME_API_VERSIONS: sp_api::__private::ApisVec = ::sp_version::Cow::Borrowed(
    &[
        #[allow(deprecated)]
        (self::runtime_decl_for_api::ID, self::runtime_decl_for_api::VERSION),
        (sp_api::runtime_decl_for_core::ID, sp_api::runtime_decl_for_core::VERSION),
    ],
);
#[doc(hidden)]
trait InternalImplRuntimeApis {
    #[inline(always)]
    fn runtime_metadata(
        &self,
    ) -> sp_api::__private::vec::Vec<
        sp_api::__private::metadata_ir::RuntimeApiMetadataIR,
    > {
        <[_]>::into_vec(
            #[rustc_box]
            ::alloc::boxed::Box::new([
                self::runtime_decl_for_api::runtime_metadata::<
                    Block,
                >(self::runtime_decl_for_api::VERSION),
                sp_api::runtime_decl_for_core::runtime_metadata::<
                    Block,
                >(sp_api::runtime_decl_for_core::VERSION),
            ]),
        )
    }
}
#[doc(hidden)]
impl InternalImplRuntimeApis for Runtime {}
pub mod api {
    use super::*;
    pub fn dispatch(method: &str, mut _sp_api_input_data_: &[u8]) -> Option<Vec<u8>> {
        match method {
            #[allow(deprecated)]
            "Api_test" => {
                Some(
                    sp_api::__private::Encode::encode(
                        &{
                            let _data: u64 = match sp_api::__private::DecodeLimit::decode_all_with_depth_limit(
                                sp_api::__private::MAX_EXTRINSIC_DEPTH,
                                &mut _sp_api_input_data_,
                            ) {
                                Ok(res) => res,
                                Err(e) => {
                                    ::core::panicking::panic_fmt(
                                        format_args!(
                                            "Bad input data provided to {0}: {1}",
                                            "test",
                                            e,
                                        ),
                                    );
                                }
                            };
                            #[allow(deprecated)]
                            <Runtime as self::runtime_decl_for_api::Api<
                                Block,
                            >>::test(_data)
                        },
                    ),
                )
            }
            #[allow(deprecated)]
            "Api_something_with_block" => {
                Some(
                    sp_api::__private::Encode::encode(
                        &{
                            let __runtime_api_generated_name_0__: Block = match sp_api::__private::DecodeLimit::decode_all_with_depth_limit(
                                sp_api::__private::MAX_EXTRINSIC_DEPTH,
                                &mut _sp_api_input_data_,
                            ) {
                                Ok(res) => res,
                                Err(e) => {
                                    ::core::panicking::panic_fmt(
                                        format_args!(
                                            "Bad input data provided to {0}: {1}",
                                            "something_with_block",
                                            e,
                                        ),
                                    );
                                }
                            };
                            #[allow(deprecated)]
                            <Runtime as self::runtime_decl_for_api::Api<
                                Block,
                            >>::something_with_block(__runtime_api_generated_name_0__)
                        },
                    ),
                )
            }
            #[allow(deprecated)]
            "Api_function_with_two_args" => {
                Some(
                    sp_api::__private::Encode::encode(
                        &{
                            let (
                                __runtime_api_generated_name_0__,
                                __runtime_api_generated_name_1__,
                            ): (u64, Block) = match sp_api::__private::DecodeLimit::decode_all_with_depth_limit(
                                sp_api::__private::MAX_EXTRINSIC_DEPTH,
                                &mut _sp_api_input_data_,
                            ) {
                                Ok(res) => res,
                                Err(e) => {
                                    ::core::panicking::panic_fmt(
                                        format_args!(
                                            "Bad input data provided to {0}: {1}",
                                            "function_with_two_args",
                                            e,
                                        ),
                                    );
                                }
                            };
                            #[allow(deprecated)]
                            <Runtime as self::runtime_decl_for_api::Api<
                                Block,
                            >>::function_with_two_args(
                                __runtime_api_generated_name_0__,
                                __runtime_api_generated_name_1__,
                            )
                        },
                    ),
                )
            }
            #[allow(deprecated)]
            "Api_same_name" => {
                Some(
                    sp_api::__private::Encode::encode(
                        &{
                            if !_sp_api_input_data_.is_empty() {
                                {
                                    ::core::panicking::panic_fmt(
                                        format_args!(
                                            "Bad input data provided to {0}: expected no parameters, but input buffer is not empty.",
                                            "same_name",
                                        ),
                                    );
                                };
                            }
                            #[allow(deprecated)]
                            <Runtime as self::runtime_decl_for_api::Api<
                                Block,
                            >>::same_name()
                        },
                    ),
                )
            }
            #[allow(deprecated)]
            "Api_wild_card" => {
                Some(
                    sp_api::__private::Encode::encode(
                        &{
                            let __runtime_api_generated_name_0__: u32 = match sp_api::__private::DecodeLimit::decode_all_with_depth_limit(
                                sp_api::__private::MAX_EXTRINSIC_DEPTH,
                                &mut _sp_api_input_data_,
                            ) {
                                Ok(res) => res,
                                Err(e) => {
                                    ::core::panicking::panic_fmt(
                                        format_args!(
                                            "Bad input data provided to {0}: {1}",
                                            "wild_card",
                                            e,
                                        ),
                                    );
                                }
                            };
                            #[allow(deprecated)]
                            <Runtime as self::runtime_decl_for_api::Api<
                                Block,
                            >>::wild_card(__runtime_api_generated_name_0__)
                        },
                    ),
                )
            }
            "Core_version" => {
                Some(
                    sp_api::__private::Encode::encode(
                        &{
                            if !_sp_api_input_data_.is_empty() {
                                {
                                    ::core::panicking::panic_fmt(
                                        format_args!(
                                            "Bad input data provided to {0}: expected no parameters, but input buffer is not empty.",
                                            "version",
                                        ),
                                    );
                                };
                            }
                            #[allow(deprecated)]
                            <Runtime as sp_api::runtime_decl_for_core::Core<
                                Block,
                            >>::version()
                        },
                    ),
                )
            }
            "Core_execute_block" => {
                Some(
                    sp_api::__private::Encode::encode(
                        &{
                            let __runtime_api_generated_name_0__: Block = match sp_api::__private::DecodeLimit::decode_all_with_depth_limit(
                                sp_api::__private::MAX_EXTRINSIC_DEPTH,
                                &mut _sp_api_input_data_,
                            ) {
                                Ok(res) => res,
                                Err(e) => {
                                    ::core::panicking::panic_fmt(
                                        format_args!(
                                            "Bad input data provided to {0}: {1}",
                                            "execute_block",
                                            e,
                                        ),
                                    );
                                }
                            };
                            #[allow(deprecated)]
                            <Runtime as sp_api::runtime_decl_for_core::Core<
                                Block,
                            >>::execute_block(__runtime_api_generated_name_0__)
                        },
                    ),
                )
            }
            "Core_initialize_block" => {
                Some(
                    sp_api::__private::Encode::encode(
                        &{
                            let __runtime_api_generated_name_0__: <Block as BlockT>::Header = match sp_api::__private::DecodeLimit::decode_all_with_depth_limit(
                                sp_api::__private::MAX_EXTRINSIC_DEPTH,
                                &mut _sp_api_input_data_,
                            ) {
                                Ok(res) => res,
                                Err(e) => {
                                    ::core::panicking::panic_fmt(
                                        format_args!(
                                            "Bad input data provided to {0}: {1}",
                                            "initialize_block",
                                            e,
                                        ),
                                    );
                                }
                            };
                            #[allow(deprecated)]
                            <Runtime as sp_api::runtime_decl_for_core::Core<
                                Block,
                            >>::initialize_block(&__runtime_api_generated_name_0__)
                        },
                    ),
                )
            }
            _ => None,
        }
    }
}
extern crate test;
#[cfg(test)]
#[rustc_test_marker = "runtime_metadata"]
pub const runtime_metadata: test::TestDescAndFn = test::TestDescAndFn {
    desc: test::TestDesc {
        name: test::StaticTestName("runtime_metadata"),
        ignore: false,
        ignore_message: ::core::option::Option::None,
        source_file: "substrate/frame/support/test/tests/runtime_metadata.rs",
        start_line: 118usize,
        start_col: 4usize,
        end_line: 118usize,
        end_col: 20usize,
        compile_fail: false,
        no_run: false,
        should_panic: test::ShouldPanic::No,
        test_type: test::TestType::IntegrationTest,
    },
    testfn: test::StaticTestFn(
        #[coverage(off)]
        || test::assert_test_result(runtime_metadata()),
    ),
};
fn runtime_metadata() {
    fn maybe_docs(doc: Vec<&'static str>) -> Vec<&'static str> {
        if false { ::alloc::vec::Vec::new() } else { doc }
    }
    let expected_runtime_metadata = <[_]>::into_vec(
        #[rustc_box]
        ::alloc::boxed::Box::new([
            RuntimeApiMetadataIR {
                name: "Api",
                methods: <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        RuntimeApiMethodMetadataIR {
                            name: "test",
                            inputs: <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RuntimeApiMethodParamMetadataIR::<MetaForm> {
                                        name: "data",
                                        ty: meta_type::<u64>(),
                                    },
                                ]),
                            ),
                            output: meta_type::<()>(),
                            docs: ::alloc::vec::Vec::new(),
                            deprecation_info: DeprecationStatusIR::NotDeprecated,
                        },
                        RuntimeApiMethodMetadataIR {
                            name: "something_with_block",
                            inputs: <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RuntimeApiMethodParamMetadataIR::<MetaForm> {
                                        name: "block",
                                        ty: meta_type::<Block>(),
                                    },
                                ]),
                            ),
                            output: meta_type::<Block>(),
                            docs: maybe_docs(
                                <[_]>::into_vec(
                                    #[rustc_box]
                                    ::alloc::boxed::Box::new([" something_with_block."]),
                                ),
                            ),
                            deprecation_info: DeprecationStatusIR::NotDeprecated,
                        },
                        RuntimeApiMethodMetadataIR {
                            name: "function_with_two_args",
                            inputs: <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RuntimeApiMethodParamMetadataIR::<MetaForm> {
                                        name: "data",
                                        ty: meta_type::<u64>(),
                                    },
                                    RuntimeApiMethodParamMetadataIR::<MetaForm> {
                                        name: "block",
                                        ty: meta_type::<Block>(),
                                    },
                                ]),
                            ),
                            output: meta_type::<()>(),
                            docs: ::alloc::vec::Vec::new(),
                            deprecation_info: DeprecationStatusIR::Deprecated {
                                note: "example",
                                since: None,
                            },
                        },
                        RuntimeApiMethodMetadataIR {
                            name: "same_name",
                            inputs: ::alloc::vec::Vec::new(),
                            output: meta_type::<()>(),
                            docs: ::alloc::vec::Vec::new(),
                            deprecation_info: DeprecationStatusIR::Deprecated {
                                note: "example",
                                since: Some("example"),
                            },
                        },
                        RuntimeApiMethodMetadataIR {
                            name: "wild_card",
                            inputs: <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RuntimeApiMethodParamMetadataIR::<MetaForm> {
                                        name: "__runtime_api_generated_name_0__",
                                        ty: meta_type::<u32>(),
                                    },
                                ]),
                            ),
                            output: meta_type::<()>(),
                            docs: ::alloc::vec::Vec::new(),
                            deprecation_info: DeprecationStatusIR::Deprecated {
                                note: "example",
                                since: None,
                            },
                        },
                    ]),
                ),
                docs: maybe_docs(
                    <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            " ApiWithCustomVersion trait documentation",
                            "",
                            " Documentation on multiline.",
                        ]),
                    ),
                ),
                deprecation_info: DeprecationStatusIR::DeprecatedWithoutNote,
            },
            RuntimeApiMetadataIR {
                name: "Core",
                methods: <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        RuntimeApiMethodMetadataIR {
                            name: "version",
                            inputs: ::alloc::vec::Vec::new(),
                            output: meta_type::<sp_version::RuntimeVersion>(),
                            docs: maybe_docs(
                                <[_]>::into_vec(
                                    #[rustc_box]
                                    ::alloc::boxed::Box::new([
                                        " Returns the version of the runtime.",
                                    ]),
                                ),
                            ),
                            deprecation_info: DeprecationStatusIR::NotDeprecated,
                        },
                        RuntimeApiMethodMetadataIR {
                            name: "execute_block",
                            inputs: <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RuntimeApiMethodParamMetadataIR::<MetaForm> {
                                        name: "block",
                                        ty: meta_type::<Block>(),
                                    },
                                ]),
                            ),
                            output: meta_type::<()>(),
                            docs: maybe_docs(
                                <[_]>::into_vec(
                                    #[rustc_box]
                                    ::alloc::boxed::Box::new([" Execute the given block."]),
                                ),
                            ),
                            deprecation_info: DeprecationStatusIR::NotDeprecated,
                        },
                        RuntimeApiMethodMetadataIR {
                            name: "initialize_block",
                            inputs: <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RuntimeApiMethodParamMetadataIR::<MetaForm> {
                                        name: "header",
                                        ty: meta_type::<&<Block as BlockT>::Header>(),
                                    },
                                ]),
                            ),
                            output: meta_type::<sp_runtime::ExtrinsicInclusionMode>(),
                            docs: maybe_docs(
                                <[_]>::into_vec(
                                    #[rustc_box]
                                    ::alloc::boxed::Box::new([
                                        " Initialize a block with the given header and return the runtime executive mode.",
                                    ]),
                                ),
                            ),
                            deprecation_info: DeprecationStatusIR::NotDeprecated,
                        },
                    ]),
                ),
                docs: maybe_docs(
                    <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            " The `Core` runtime api that every Substrate runtime needs to implement.",
                        ]),
                    ),
                ),
                deprecation_info: DeprecationStatusIR::NotDeprecated,
            },
        ]),
    );
    let rt = Runtime;
    let runtime_metadata = (&rt).runtime_metadata();
    {
        {
            match (&(runtime_metadata), &(expected_runtime_metadata)) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        use ::pretty_assertions::private::CreateComparison;
                        {
                            ::core::panicking::panic_fmt(
                                format_args!(
                                    "assertion failed: `(left == right)`{0}{1}\n\n{2}\n",
                                    "",
                                    format_args!(""),
                                    (left_val, right_val).create_comparison(),
                                ),
                            );
                        }
                    }
                }
            }
        };
    };
}
#[rustc_main]
#[coverage(off)]
pub fn main() -> () {
    extern crate test;
    test::test_main_static(
        &[&runtime_integrity_tests, &runtime_metadata, &test_genesis_config_builds],
    )
}
