#![feature(prelude_import)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
pub use penpal_runtime::{self, xcm_config::RelayNetworkId as PenpalRelayNetworkId};
mod genesis {
    use frame_support::parameter_types;
    use sp_core::{sr25519, storage::Storage};
    use emulated_integration_tests_common::{
        accounts, build_genesis_storage, collators, get_account_id_from_seed,
        SAFE_XCM_VERSION,
    };
    use parachains_common::{AccountId, Balance};
    use penpal_runtime::xcm_config::{
        LocalReservableFromAssetHub, RelayLocation, UsdtFromAssetHub,
    };
    pub const PARA_ID_A: u32 = 2000;
    pub const PARA_ID_B: u32 = 2001;
    pub const ED: Balance = penpal_runtime::EXISTENTIAL_DEPOSIT;
    pub const USDT_ED: Balance = 70_000;
    pub struct PenpalSudoAccount;
    impl PenpalSudoAccount {
        /// Returns the value of this parameter type.
        pub fn get() -> AccountId {
            get_account_id_from_seed::<sr25519::Public>("Alice")
        }
    }
    impl<_I: From<AccountId>> ::frame_support::traits::Get<_I> for PenpalSudoAccount {
        fn get() -> _I {
            _I::from(Self::get())
        }
    }
    impl ::frame_support::traits::TypedGet for PenpalSudoAccount {
        type Type = AccountId;
        fn get() -> AccountId {
            Self::get()
        }
    }
    pub struct PenpalAssetOwner;
    impl PenpalAssetOwner {
        /// Returns the value of this parameter type.
        pub fn get() -> AccountId {
            PenpalSudoAccount::get()
        }
    }
    impl<_I: From<AccountId>> ::frame_support::traits::Get<_I> for PenpalAssetOwner {
        fn get() -> _I {
            _I::from(Self::get())
        }
    }
    impl ::frame_support::traits::TypedGet for PenpalAssetOwner {
        type Type = AccountId;
        fn get() -> AccountId {
            Self::get()
        }
    }
    pub fn genesis(para_id: u32) -> Storage {
        let genesis_config = penpal_runtime::RuntimeGenesisConfig {
            system: penpal_runtime::SystemConfig::default(),
            balances: penpal_runtime::BalancesConfig {
                balances: accounts::init_balances()
                    .iter()
                    .cloned()
                    .map(|k| (k, ED * 4096))
                    .collect(),
            },
            parachain_info: penpal_runtime::ParachainInfoConfig {
                parachain_id: para_id.into(),
                ..Default::default()
            },
            collator_selection: penpal_runtime::CollatorSelectionConfig {
                invulnerables: collators::invulnerables()
                    .iter()
                    .cloned()
                    .map(|(acc, _)| acc)
                    .collect(),
                candidacy_bond: ED * 16,
                ..Default::default()
            },
            session: penpal_runtime::SessionConfig {
                keys: collators::invulnerables()
                    .into_iter()
                    .map(|(acc, aura)| {
                        (
                            acc.clone(),
                            acc,
                            penpal_runtime::SessionKeys {
                                aura,
                            },
                        )
                    })
                    .collect(),
                ..Default::default()
            },
            polkadot_xcm: penpal_runtime::PolkadotXcmConfig {
                safe_xcm_version: Some(SAFE_XCM_VERSION),
                ..Default::default()
            },
            sudo: penpal_runtime::SudoConfig {
                key: Some(PenpalSudoAccount::get()),
            },
            assets: penpal_runtime::AssetsConfig {
                assets: <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (
                            penpal_runtime::xcm_config::TELEPORTABLE_ASSET_ID,
                            PenpalAssetOwner::get(),
                            false,
                            ED,
                        ),
                    ]),
                ),
                ..Default::default()
            },
            foreign_assets: penpal_runtime::ForeignAssetsConfig {
                assets: <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (RelayLocation::get(), PenpalAssetOwner::get(), true, ED),
                        (
                            LocalReservableFromAssetHub::get(),
                            PenpalAssetOwner::get(),
                            true,
                            ED,
                        ),
                        (UsdtFromAssetHub::get(), PenpalAssetOwner::get(), true, USDT_ED),
                    ]),
                ),
                ..Default::default()
            },
            ..Default::default()
        };
        build_genesis_storage(
            &genesis_config,
            penpal_runtime::WASM_BINARY
                .expect("WASM binary was not built, please build it!"),
        )
    }
}
pub use genesis::{
    genesis, PenpalAssetOwner, PenpalSudoAccount, ED, PARA_ID_A, PARA_ID_B,
};
use frame_support::traits::OnInitialize;
use sp_core::Encode;
use emulated_integration_tests_common::{
    impl_accounts_helpers_for_parachain, impl_assert_events_helpers_for_parachain,
    impl_assets_helpers_for_parachain, impl_foreign_assets_helpers_for_parachain,
    impl_xcm_helpers_for_parachain, impls::{NetworkId, Parachain},
    xcm_emulator::decl_test_parachains,
};
pub struct PenpalA<N>(::xcm_emulator::PhantomData<N>);
#[automatically_derived]
impl<N: ::core::clone::Clone> ::core::clone::Clone for PenpalA<N> {
    #[inline]
    fn clone(&self) -> PenpalA<N> {
        PenpalA(::core::clone::Clone::clone(&self.0))
    }
}
impl<N: ::xcm_emulator::Network> ::xcm_emulator::Chain for PenpalA<N> {
    type Runtime = penpal_runtime::Runtime;
    type RuntimeCall = penpal_runtime::RuntimeCall;
    type RuntimeOrigin = penpal_runtime::RuntimeOrigin;
    type RuntimeEvent = penpal_runtime::RuntimeEvent;
    type System = ::xcm_emulator::SystemPallet<Self::Runtime>;
    type OriginCaller = penpal_runtime::OriginCaller;
    type Network = N;
    fn account_data_of(
        account: ::xcm_emulator::AccountIdOf<Self::Runtime>,
    ) -> ::xcm_emulator::AccountData<::xcm_emulator::Balance> {
        <Self as ::xcm_emulator::TestExt>::ext_wrapper(|| {
            ::xcm_emulator::SystemPallet::<Self::Runtime>::account(account).data.into()
        })
    }
    fn events() -> Vec<<Self as ::xcm_emulator::Chain>::RuntimeEvent> {
        Self::System::events().iter().map(|record| record.event.clone()).collect()
    }
}
impl<N: ::xcm_emulator::Network> ::xcm_emulator::Parachain for PenpalA<N> {
    type XcmpMessageHandler = penpal_runtime::XcmpQueue;
    type LocationToAccountId = penpal_runtime::xcm_config::LocationToAccountId;
    type ParachainSystem = ::xcm_emulator::ParachainSystemPallet<
        <Self as ::xcm_emulator::Chain>::Runtime,
    >;
    type ParachainInfo = penpal_runtime::ParachainInfo;
    type MessageProcessor = ::xcm_emulator::DefaultParaMessageProcessor<
        PenpalA<N>,
        cumulus_primitives_core::AggregateMessageOrigin,
    >;
    fn init() {
        use ::xcm_emulator::{
            Chain, HeadData, Network, Hooks, Encode, Parachain, TestExt,
        };
        LOCAL_EXT_PENPALA
            .with(|v| *v.borrow_mut() = Self::build_new_ext(genesis(PARA_ID_A)));
        Self::set_last_head();
        Self::new_block();
        Self::finalize_block();
    }
    fn new_block() {
        use ::xcm_emulator::{
            Chain, HeadData, Network, Hooks, Encode, Parachain, TestExt,
        };
        let para_id = Self::para_id().into();
        Self::ext_wrapper(|| {
            let mut relay_block_number = N::relay_block_number();
            relay_block_number += 1;
            N::set_relay_block_number(relay_block_number);
            let mut block_number = <Self as Chain>::System::block_number();
            block_number += 1;
            let parent_head_data = ::xcm_emulator::LAST_HEAD
                .with(|b| {
                    b
                        .borrow_mut()
                        .get_mut(N::name())
                        .expect("network not initialized?")
                        .get(&para_id)
                        .expect("network not initialized?")
                        .clone()
                });
            <Self as Chain>::System::initialize(
                &block_number,
                &parent_head_data.hash(),
                &Default::default(),
            );
            <<Self as Parachain>::ParachainSystem as Hooks<
                ::xcm_emulator::BlockNumberFor<Self::Runtime>,
            >>::on_initialize(block_number);
            let _ = <Self as Parachain>::ParachainSystem::set_validation_data(
                <Self as Chain>::RuntimeOrigin::none(),
                N::hrmp_channel_parachain_inherent_data(
                    para_id,
                    relay_block_number,
                    parent_head_data,
                ),
            );
        });
    }
    fn finalize_block() {
        use ::xcm_emulator::{Chain, Encode, Hooks, Network, Parachain, TestExt};
        Self::ext_wrapper(|| {
            let block_number = <Self as Chain>::System::block_number();
            <Self as Parachain>::ParachainSystem::on_finalize(block_number);
        });
        Self::set_last_head();
    }
    fn set_last_head() {
        use ::xcm_emulator::{Chain, Encode, HeadData, Network, Parachain, TestExt};
        let para_id = Self::para_id().into();
        Self::ext_wrapper(|| {
            let created_header = <Self as Chain>::System::finalize();
            ::xcm_emulator::LAST_HEAD
                .with(|b| {
                    b
                        .borrow_mut()
                        .get_mut(N::name())
                        .expect("network not initialized?")
                        .insert(para_id, HeadData(created_header.encode()))
                });
        });
    }
}
pub trait PenpalAParaPallet {
    type System;
    type PolkadotXcm;
    type Assets;
    type ForeignAssets;
    type AssetConversion;
    type Balances;
}
impl<N: ::xcm_emulator::Network> PenpalAParaPallet for PenpalA<N> {
    type System = penpal_runtime::System;
    type PolkadotXcm = penpal_runtime::PolkadotXcm;
    type Assets = penpal_runtime::Assets;
    type ForeignAssets = penpal_runtime::ForeignAssets;
    type AssetConversion = penpal_runtime::AssetConversion;
    type Balances = penpal_runtime::Balances;
}
pub const LOCAL_EXT_PENPALA: ::std::thread::LocalKey<
    ::xcm_emulator::RefCell<::xcm_emulator::TestExternalities>,
> = {
    #[inline]
    fn __init() -> ::xcm_emulator::RefCell<::xcm_emulator::TestExternalities> {
        ::xcm_emulator::RefCell::new(
            ::xcm_emulator::TestExternalities::new(genesis(PARA_ID_A)),
        )
    }
    unsafe {
        use ::std::mem::needs_drop;
        use ::std::thread::LocalKey;
        use ::std::thread::local_impl::LazyStorage;
        LocalKey::new(const {
            if needs_drop::<
                ::xcm_emulator::RefCell<::xcm_emulator::TestExternalities>,
            >() {
                |init| {
                    #[thread_local]
                    static VAL: LazyStorage<
                        ::xcm_emulator::RefCell<::xcm_emulator::TestExternalities>,
                        (),
                    > = LazyStorage::new();
                    VAL.get_or_init(init, __init)
                }
            } else {
                |init| {
                    #[thread_local]
                    static VAL: LazyStorage<
                        ::xcm_emulator::RefCell<::xcm_emulator::TestExternalities>,
                        !,
                    > = LazyStorage::new();
                    VAL.get_or_init(init, __init)
                }
            }
        })
    }
};
#[allow(missing_copy_implementations)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
pub struct GLOBAL_EXT_PENPALA {
    __private_field: (),
}
#[doc(hidden)]
pub static GLOBAL_EXT_PENPALA: GLOBAL_EXT_PENPALA = GLOBAL_EXT_PENPALA {
    __private_field: (),
};
impl ::lazy_static::__Deref for GLOBAL_EXT_PENPALA {
    type Target = ::xcm_emulator::Mutex<
        ::xcm_emulator::RefCell<
            ::xcm_emulator::HashMap<String, ::xcm_emulator::TestExternalities>,
        >,
    >;
    fn deref(
        &self,
    ) -> &::xcm_emulator::Mutex<
        ::xcm_emulator::RefCell<
            ::xcm_emulator::HashMap<String, ::xcm_emulator::TestExternalities>,
        >,
    > {
        #[inline(always)]
        fn __static_ref_initialize() -> ::xcm_emulator::Mutex<
            ::xcm_emulator::RefCell<
                ::xcm_emulator::HashMap<String, ::xcm_emulator::TestExternalities>,
            >,
        > {
            ::xcm_emulator::Mutex::new(
                ::xcm_emulator::RefCell::new(::xcm_emulator::HashMap::new()),
            )
        }
        #[inline(always)]
        fn __stability() -> &'static ::xcm_emulator::Mutex<
            ::xcm_emulator::RefCell<
                ::xcm_emulator::HashMap<String, ::xcm_emulator::TestExternalities>,
            >,
        > {
            static LAZY: ::lazy_static::lazy::Lazy<
                ::xcm_emulator::Mutex<
                    ::xcm_emulator::RefCell<
                        ::xcm_emulator::HashMap<
                            String,
                            ::xcm_emulator::TestExternalities,
                        >,
                    >,
                >,
            > = ::lazy_static::lazy::Lazy::INIT;
            LAZY.get(__static_ref_initialize)
        }
        __stability()
    }
}
impl ::lazy_static::LazyStatic for GLOBAL_EXT_PENPALA {
    fn initialize(lazy: &Self) {
        let _ = &**lazy;
    }
}
impl<N: ::xcm_emulator::Network> ::xcm_emulator::TestExt for PenpalA<N> {
    fn build_new_ext(
        storage: ::xcm_emulator::Storage,
    ) -> ::xcm_emulator::TestExternalities {
        let mut ext = ::xcm_emulator::TestExternalities::new(storage);
        ext.execute_with(|| {
            #[allow(clippy::no_effect)]
            {
                penpal_runtime::AuraExt::on_initialize(1);
                let is = penpal_runtime::System::set_storage(
                    penpal_runtime::RuntimeOrigin::root(),
                    <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            (
                                PenpalRelayNetworkId::key().to_vec(),
                                NetworkId::Rococo.encode(),
                            ),
                        ]),
                    ),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            };
            ::xcm_emulator::sp_tracing::try_init_simple();
            let mut block_number = <Self as ::xcm_emulator::Chain>::System::block_number();
            block_number = std::cmp::max(1, block_number);
            <Self as ::xcm_emulator::Chain>::System::set_block_number(block_number);
        });
        ext
    }
    fn new_ext() -> ::xcm_emulator::TestExternalities {
        Self::build_new_ext(genesis(PARA_ID_A))
    }
    fn move_ext_out(id: &'static str) {
        use ::xcm_emulator::Deref;
        let local_ext = LOCAL_EXT_PENPALA.with(|v| { v.take() });
        let global_ext_guard = GLOBAL_EXT_PENPALA.lock().unwrap();
        global_ext_guard.deref().borrow_mut().insert(id.to_string(), local_ext);
    }
    fn move_ext_in(id: &'static str) {
        use ::xcm_emulator::Deref;
        let mut global_ext_unlocked = false;
        while !global_ext_unlocked {
            let global_ext_result = GLOBAL_EXT_PENPALA.try_lock();
            if let Ok(global_ext_guard) = global_ext_result {
                if !global_ext_guard.deref().borrow().contains_key(id) {
                    drop(global_ext_guard);
                } else {
                    global_ext_unlocked = true;
                }
            }
        }
        let mut global_ext_guard = GLOBAL_EXT_PENPALA.lock().unwrap();
        let global_ext = global_ext_guard.deref();
        LOCAL_EXT_PENPALA
            .with(|v| {
                v.replace(global_ext.take().remove(id).unwrap());
            });
    }
    fn reset_ext() {
        LOCAL_EXT_PENPALA
            .with(|v| *v.borrow_mut() = Self::build_new_ext(genesis(PARA_ID_A)));
    }
    fn execute_with<R>(execute: impl FnOnce() -> R) -> R {
        use ::xcm_emulator::{Chain, Get, Hooks, Network, Parachain, Encode};
        <N>::init();
        Self::new_block();
        let r = LOCAL_EXT_PENPALA.with(|v| v.borrow_mut().execute_with(execute));
        Self::finalize_block();
        let para_id = Self::para_id().into();
        LOCAL_EXT_PENPALA
            .with(|v| {
                v.borrow_mut()
                    .execute_with(|| {
                        let mock_header = ::xcm_emulator::HeaderT::new(
                            0,
                            Default::default(),
                            Default::default(),
                            Default::default(),
                            Default::default(),
                        );
                        let collation_info = <Self as Parachain>::ParachainSystem::collect_collation_info(
                            &mock_header,
                        );
                        let relay_block_number = <N>::relay_block_number();
                        for msg in collation_info.upward_messages.clone() {
                            <N>::send_upward_message(para_id, msg);
                        }
                        for msg in collation_info.horizontal_messages {
                            <N>::send_horizontal_messages(
                                msg.recipient.into(),
                                <[_]>::into_vec(
                                        #[rustc_box]
                                        ::alloc::boxed::Box::new([
                                            (para_id.into(), relay_block_number, msg.data),
                                        ]),
                                    )
                                    .into_iter(),
                            );
                        }
                        type NetworkBridge<N> = <N as ::xcm_emulator::Network>::Bridge;
                        let bridge_messages = <<NetworkBridge<
                            N,
                        > as ::xcm_emulator::Bridge>::Handler as ::xcm_emulator::BridgeMessageHandler>::get_source_outbound_messages();
                        for msg in bridge_messages {
                            <N>::send_bridged_messages(msg);
                        }
                        <Self as ::xcm_emulator::Chain>::events()
                            .iter()
                            .for_each(|event| {
                                {
                                    let lvl = ::log::Level::Debug;
                                    if lvl <= ::log::STATIC_MAX_LEVEL
                                        && lvl <= ::log::max_level()
                                    {
                                        ::log::__private_api::log(
                                            format_args!("{0:?}", event),
                                            lvl,
                                            &(
                                                "events::PenpalA",
                                                "penpal_emulated_chain",
                                                ::log::__private_api::loc(),
                                            ),
                                            (),
                                        );
                                    }
                                };
                            });
                        <Self as ::xcm_emulator::Chain>::System::reset_events();
                    })
            });
        <N>::process_messages();
        r
    }
    fn ext_wrapper<R>(func: impl FnOnce() -> R) -> R {
        LOCAL_EXT_PENPALA.with(|v| { v.borrow_mut().execute_with(|| { func() }) })
    }
}
impl<
    N,
    Origin,
    Destination,
    Hops,
    Args,
> ::xcm_emulator::CheckAssertion<Origin, Destination, Hops, Args> for PenpalA<N>
where
    N: ::xcm_emulator::Network,
    Origin: ::xcm_emulator::Chain + Clone,
    Destination: ::xcm_emulator::Chain + Clone,
    Origin::RuntimeOrigin: ::xcm_emulator::OriginTrait<
            AccountId = ::xcm_emulator::AccountIdOf<Origin::Runtime>,
        > + Clone,
    Destination::RuntimeOrigin: ::xcm_emulator::OriginTrait<
            AccountId = ::xcm_emulator::AccountIdOf<Destination::Runtime>,
        > + Clone,
    Hops: Clone,
    Args: Clone,
{
    fn check_assertion(test: ::xcm_emulator::Test<Origin, Destination, Hops, Args>) {
        use ::xcm_emulator::{Dispatchable, TestExt};
        let chain_name = std::any::type_name::<PenpalA<N>>();
        <PenpalA<
            N,
        >>::execute_with(|| {
            if let Some(dispatchable) = test.hops_dispatchable.get(chain_name) {
                let is = dispatchable(test.clone());
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            }
            if let Some(call) = test.hops_calls.get(chain_name) {
                let is = match call.clone().dispatch(test.signed_origin.clone()) {
                    Ok(_) => Ok(()),
                    Err(error_with_post_info) => Err(error_with_post_info.error),
                };
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            }
            if let Some(assertion) = test.hops_assertion.get(chain_name) {
                assertion(test);
            }
        });
    }
}
pub struct PenpalB<N>(::xcm_emulator::PhantomData<N>);
#[automatically_derived]
impl<N: ::core::clone::Clone> ::core::clone::Clone for PenpalB<N> {
    #[inline]
    fn clone(&self) -> PenpalB<N> {
        PenpalB(::core::clone::Clone::clone(&self.0))
    }
}
impl<N: ::xcm_emulator::Network> ::xcm_emulator::Chain for PenpalB<N> {
    type Runtime = penpal_runtime::Runtime;
    type RuntimeCall = penpal_runtime::RuntimeCall;
    type RuntimeOrigin = penpal_runtime::RuntimeOrigin;
    type RuntimeEvent = penpal_runtime::RuntimeEvent;
    type System = ::xcm_emulator::SystemPallet<Self::Runtime>;
    type OriginCaller = penpal_runtime::OriginCaller;
    type Network = N;
    fn account_data_of(
        account: ::xcm_emulator::AccountIdOf<Self::Runtime>,
    ) -> ::xcm_emulator::AccountData<::xcm_emulator::Balance> {
        <Self as ::xcm_emulator::TestExt>::ext_wrapper(|| {
            ::xcm_emulator::SystemPallet::<Self::Runtime>::account(account).data.into()
        })
    }
    fn events() -> Vec<<Self as ::xcm_emulator::Chain>::RuntimeEvent> {
        Self::System::events().iter().map(|record| record.event.clone()).collect()
    }
}
impl<N: ::xcm_emulator::Network> ::xcm_emulator::Parachain for PenpalB<N> {
    type XcmpMessageHandler = penpal_runtime::XcmpQueue;
    type LocationToAccountId = penpal_runtime::xcm_config::LocationToAccountId;
    type ParachainSystem = ::xcm_emulator::ParachainSystemPallet<
        <Self as ::xcm_emulator::Chain>::Runtime,
    >;
    type ParachainInfo = penpal_runtime::ParachainInfo;
    type MessageProcessor = ::xcm_emulator::DefaultParaMessageProcessor<
        PenpalB<N>,
        cumulus_primitives_core::AggregateMessageOrigin,
    >;
    fn init() {
        use ::xcm_emulator::{
            Chain, HeadData, Network, Hooks, Encode, Parachain, TestExt,
        };
        LOCAL_EXT_PENPALB
            .with(|v| *v.borrow_mut() = Self::build_new_ext(genesis(PARA_ID_B)));
        Self::set_last_head();
        Self::new_block();
        Self::finalize_block();
    }
    fn new_block() {
        use ::xcm_emulator::{
            Chain, HeadData, Network, Hooks, Encode, Parachain, TestExt,
        };
        let para_id = Self::para_id().into();
        Self::ext_wrapper(|| {
            let mut relay_block_number = N::relay_block_number();
            relay_block_number += 1;
            N::set_relay_block_number(relay_block_number);
            let mut block_number = <Self as Chain>::System::block_number();
            block_number += 1;
            let parent_head_data = ::xcm_emulator::LAST_HEAD
                .with(|b| {
                    b
                        .borrow_mut()
                        .get_mut(N::name())
                        .expect("network not initialized?")
                        .get(&para_id)
                        .expect("network not initialized?")
                        .clone()
                });
            <Self as Chain>::System::initialize(
                &block_number,
                &parent_head_data.hash(),
                &Default::default(),
            );
            <<Self as Parachain>::ParachainSystem as Hooks<
                ::xcm_emulator::BlockNumberFor<Self::Runtime>,
            >>::on_initialize(block_number);
            let _ = <Self as Parachain>::ParachainSystem::set_validation_data(
                <Self as Chain>::RuntimeOrigin::none(),
                N::hrmp_channel_parachain_inherent_data(
                    para_id,
                    relay_block_number,
                    parent_head_data,
                ),
            );
        });
    }
    fn finalize_block() {
        use ::xcm_emulator::{Chain, Encode, Hooks, Network, Parachain, TestExt};
        Self::ext_wrapper(|| {
            let block_number = <Self as Chain>::System::block_number();
            <Self as Parachain>::ParachainSystem::on_finalize(block_number);
        });
        Self::set_last_head();
    }
    fn set_last_head() {
        use ::xcm_emulator::{Chain, Encode, HeadData, Network, Parachain, TestExt};
        let para_id = Self::para_id().into();
        Self::ext_wrapper(|| {
            let created_header = <Self as Chain>::System::finalize();
            ::xcm_emulator::LAST_HEAD
                .with(|b| {
                    b
                        .borrow_mut()
                        .get_mut(N::name())
                        .expect("network not initialized?")
                        .insert(para_id, HeadData(created_header.encode()))
                });
        });
    }
}
pub trait PenpalBParaPallet {
    type PolkadotXcm;
    type Assets;
    type ForeignAssets;
    type AssetConversion;
    type Balances;
}
impl<N: ::xcm_emulator::Network> PenpalBParaPallet for PenpalB<N> {
    type PolkadotXcm = penpal_runtime::PolkadotXcm;
    type Assets = penpal_runtime::Assets;
    type ForeignAssets = penpal_runtime::ForeignAssets;
    type AssetConversion = penpal_runtime::AssetConversion;
    type Balances = penpal_runtime::Balances;
}
pub const LOCAL_EXT_PENPALB: ::std::thread::LocalKey<
    ::xcm_emulator::RefCell<::xcm_emulator::TestExternalities>,
> = {
    #[inline]
    fn __init() -> ::xcm_emulator::RefCell<::xcm_emulator::TestExternalities> {
        ::xcm_emulator::RefCell::new(
            ::xcm_emulator::TestExternalities::new(genesis(PARA_ID_B)),
        )
    }
    unsafe {
        use ::std::mem::needs_drop;
        use ::std::thread::LocalKey;
        use ::std::thread::local_impl::LazyStorage;
        LocalKey::new(const {
            if needs_drop::<
                ::xcm_emulator::RefCell<::xcm_emulator::TestExternalities>,
            >() {
                |init| {
                    #[thread_local]
                    static VAL: LazyStorage<
                        ::xcm_emulator::RefCell<::xcm_emulator::TestExternalities>,
                        (),
                    > = LazyStorage::new();
                    VAL.get_or_init(init, __init)
                }
            } else {
                |init| {
                    #[thread_local]
                    static VAL: LazyStorage<
                        ::xcm_emulator::RefCell<::xcm_emulator::TestExternalities>,
                        !,
                    > = LazyStorage::new();
                    VAL.get_or_init(init, __init)
                }
            }
        })
    }
};
#[allow(missing_copy_implementations)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
pub struct GLOBAL_EXT_PENPALB {
    __private_field: (),
}
#[doc(hidden)]
pub static GLOBAL_EXT_PENPALB: GLOBAL_EXT_PENPALB = GLOBAL_EXT_PENPALB {
    __private_field: (),
};
impl ::lazy_static::__Deref for GLOBAL_EXT_PENPALB {
    type Target = ::xcm_emulator::Mutex<
        ::xcm_emulator::RefCell<
            ::xcm_emulator::HashMap<String, ::xcm_emulator::TestExternalities>,
        >,
    >;
    fn deref(
        &self,
    ) -> &::xcm_emulator::Mutex<
        ::xcm_emulator::RefCell<
            ::xcm_emulator::HashMap<String, ::xcm_emulator::TestExternalities>,
        >,
    > {
        #[inline(always)]
        fn __static_ref_initialize() -> ::xcm_emulator::Mutex<
            ::xcm_emulator::RefCell<
                ::xcm_emulator::HashMap<String, ::xcm_emulator::TestExternalities>,
            >,
        > {
            ::xcm_emulator::Mutex::new(
                ::xcm_emulator::RefCell::new(::xcm_emulator::HashMap::new()),
            )
        }
        #[inline(always)]
        fn __stability() -> &'static ::xcm_emulator::Mutex<
            ::xcm_emulator::RefCell<
                ::xcm_emulator::HashMap<String, ::xcm_emulator::TestExternalities>,
            >,
        > {
            static LAZY: ::lazy_static::lazy::Lazy<
                ::xcm_emulator::Mutex<
                    ::xcm_emulator::RefCell<
                        ::xcm_emulator::HashMap<
                            String,
                            ::xcm_emulator::TestExternalities,
                        >,
                    >,
                >,
            > = ::lazy_static::lazy::Lazy::INIT;
            LAZY.get(__static_ref_initialize)
        }
        __stability()
    }
}
impl ::lazy_static::LazyStatic for GLOBAL_EXT_PENPALB {
    fn initialize(lazy: &Self) {
        let _ = &**lazy;
    }
}
impl<N: ::xcm_emulator::Network> ::xcm_emulator::TestExt for PenpalB<N> {
    fn build_new_ext(
        storage: ::xcm_emulator::Storage,
    ) -> ::xcm_emulator::TestExternalities {
        let mut ext = ::xcm_emulator::TestExternalities::new(storage);
        ext.execute_with(|| {
            #[allow(clippy::no_effect)]
            {
                penpal_runtime::AuraExt::on_initialize(1);
                let is = penpal_runtime::System::set_storage(
                    penpal_runtime::RuntimeOrigin::root(),
                    <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            (
                                PenpalRelayNetworkId::key().to_vec(),
                                NetworkId::Westend.encode(),
                            ),
                        ]),
                    ),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            };
            ::xcm_emulator::sp_tracing::try_init_simple();
            let mut block_number = <Self as ::xcm_emulator::Chain>::System::block_number();
            block_number = std::cmp::max(1, block_number);
            <Self as ::xcm_emulator::Chain>::System::set_block_number(block_number);
        });
        ext
    }
    fn new_ext() -> ::xcm_emulator::TestExternalities {
        Self::build_new_ext(genesis(PARA_ID_B))
    }
    fn move_ext_out(id: &'static str) {
        use ::xcm_emulator::Deref;
        let local_ext = LOCAL_EXT_PENPALB.with(|v| { v.take() });
        let global_ext_guard = GLOBAL_EXT_PENPALB.lock().unwrap();
        global_ext_guard.deref().borrow_mut().insert(id.to_string(), local_ext);
    }
    fn move_ext_in(id: &'static str) {
        use ::xcm_emulator::Deref;
        let mut global_ext_unlocked = false;
        while !global_ext_unlocked {
            let global_ext_result = GLOBAL_EXT_PENPALB.try_lock();
            if let Ok(global_ext_guard) = global_ext_result {
                if !global_ext_guard.deref().borrow().contains_key(id) {
                    drop(global_ext_guard);
                } else {
                    global_ext_unlocked = true;
                }
            }
        }
        let mut global_ext_guard = GLOBAL_EXT_PENPALB.lock().unwrap();
        let global_ext = global_ext_guard.deref();
        LOCAL_EXT_PENPALB
            .with(|v| {
                v.replace(global_ext.take().remove(id).unwrap());
            });
    }
    fn reset_ext() {
        LOCAL_EXT_PENPALB
            .with(|v| *v.borrow_mut() = Self::build_new_ext(genesis(PARA_ID_B)));
    }
    fn execute_with<R>(execute: impl FnOnce() -> R) -> R {
        use ::xcm_emulator::{Chain, Get, Hooks, Network, Parachain, Encode};
        <N>::init();
        Self::new_block();
        let r = LOCAL_EXT_PENPALB.with(|v| v.borrow_mut().execute_with(execute));
        Self::finalize_block();
        let para_id = Self::para_id().into();
        LOCAL_EXT_PENPALB
            .with(|v| {
                v.borrow_mut()
                    .execute_with(|| {
                        let mock_header = ::xcm_emulator::HeaderT::new(
                            0,
                            Default::default(),
                            Default::default(),
                            Default::default(),
                            Default::default(),
                        );
                        let collation_info = <Self as Parachain>::ParachainSystem::collect_collation_info(
                            &mock_header,
                        );
                        let relay_block_number = <N>::relay_block_number();
                        for msg in collation_info.upward_messages.clone() {
                            <N>::send_upward_message(para_id, msg);
                        }
                        for msg in collation_info.horizontal_messages {
                            <N>::send_horizontal_messages(
                                msg.recipient.into(),
                                <[_]>::into_vec(
                                        #[rustc_box]
                                        ::alloc::boxed::Box::new([
                                            (para_id.into(), relay_block_number, msg.data),
                                        ]),
                                    )
                                    .into_iter(),
                            );
                        }
                        type NetworkBridge<N> = <N as ::xcm_emulator::Network>::Bridge;
                        let bridge_messages = <<NetworkBridge<
                            N,
                        > as ::xcm_emulator::Bridge>::Handler as ::xcm_emulator::BridgeMessageHandler>::get_source_outbound_messages();
                        for msg in bridge_messages {
                            <N>::send_bridged_messages(msg);
                        }
                        <Self as ::xcm_emulator::Chain>::events()
                            .iter()
                            .for_each(|event| {
                                {
                                    let lvl = ::log::Level::Debug;
                                    if lvl <= ::log::STATIC_MAX_LEVEL
                                        && lvl <= ::log::max_level()
                                    {
                                        ::log::__private_api::log(
                                            format_args!("{0:?}", event),
                                            lvl,
                                            &(
                                                "events::PenpalB",
                                                "penpal_emulated_chain",
                                                ::log::__private_api::loc(),
                                            ),
                                            (),
                                        );
                                    }
                                };
                            });
                        <Self as ::xcm_emulator::Chain>::System::reset_events();
                    })
            });
        <N>::process_messages();
        r
    }
    fn ext_wrapper<R>(func: impl FnOnce() -> R) -> R {
        LOCAL_EXT_PENPALB.with(|v| { v.borrow_mut().execute_with(|| { func() }) })
    }
}
impl<
    N,
    Origin,
    Destination,
    Hops,
    Args,
> ::xcm_emulator::CheckAssertion<Origin, Destination, Hops, Args> for PenpalB<N>
where
    N: ::xcm_emulator::Network,
    Origin: ::xcm_emulator::Chain + Clone,
    Destination: ::xcm_emulator::Chain + Clone,
    Origin::RuntimeOrigin: ::xcm_emulator::OriginTrait<
            AccountId = ::xcm_emulator::AccountIdOf<Origin::Runtime>,
        > + Clone,
    Destination::RuntimeOrigin: ::xcm_emulator::OriginTrait<
            AccountId = ::xcm_emulator::AccountIdOf<Destination::Runtime>,
        > + Clone,
    Hops: Clone,
    Args: Clone,
{
    fn check_assertion(test: ::xcm_emulator::Test<Origin, Destination, Hops, Args>) {
        use ::xcm_emulator::{Dispatchable, TestExt};
        let chain_name = std::any::type_name::<PenpalB<N>>();
        <PenpalB<
            N,
        >>::execute_with(|| {
            if let Some(dispatchable) = test.hops_dispatchable.get(chain_name) {
                let is = dispatchable(test.clone());
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            }
            if let Some(call) = test.hops_calls.get(chain_name) {
                let is = match call.clone().dispatch(test.signed_origin.clone()) {
                    Ok(_) => Ok(()),
                    Err(error_with_post_info) => Err(error_with_post_info.error),
                };
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            }
            if let Some(assertion) = test.hops_assertion.get(chain_name) {
                assertion(test);
            }
        });
    }
}
impl<N: ::emulated_integration_tests_common::impls::Network> PenpalA<N> {
    /// Fund a set of accounts with a balance
    pub fn fund_accounts(
        accounts: Vec<
            (
                ::emulated_integration_tests_common::impls::AccountId,
                ::emulated_integration_tests_common::impls::Balance,
            ),
        >,
    ) {
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            for account in accounts {
                let who = account.0;
                let actual = <Self as PenpalAParaPallet>::Balances::free_balance(&who);
                let actual = actual
                    .saturating_add(
                        <Self as PenpalAParaPallet>::Balances::reserved_balance(&who),
                    );
                let is = <Self as PenpalAParaPallet>::Balances::force_set_balance(
                    <Self as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::root(),
                    who.into(),
                    actual.saturating_add(account.1),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            }
        });
    }
    /// Fund a sovereign account of sibling para.
    pub fn fund_para_sovereign(
        sibling_para_id: ::emulated_integration_tests_common::impls::ParaId,
        balance: ::emulated_integration_tests_common::impls::Balance,
    ) {
        let sibling_location = Self::sibling_location_of(sibling_para_id);
        let sovereign_account = Self::sovereign_account_id_of(sibling_location);
        Self::fund_accounts(
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([(sovereign_account.into(), balance)]),
            ),
        )
    }
    /// Return local sovereign account of `para_id` on other `network_id`
    pub fn sovereign_account_of_parachain_on_other_global_consensus(
        network_id: ::emulated_integration_tests_common::impls::NetworkId,
        para_id: ::emulated_integration_tests_common::impls::ParaId,
    ) -> ::emulated_integration_tests_common::impls::AccountId {
        let remote_location = ::emulated_integration_tests_common::impls::Location::new(
            2,
            [
                ::emulated_integration_tests_common::impls::Junction::GlobalConsensus(
                    network_id,
                ),
                ::emulated_integration_tests_common::impls::Junction::Parachain(
                    para_id.into(),
                ),
            ],
        );
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            Self::sovereign_account_id_of(remote_location)
        })
    }
}
impl<N: ::emulated_integration_tests_common::impls::Network> PenpalB<N> {
    /// Fund a set of accounts with a balance
    pub fn fund_accounts(
        accounts: Vec<
            (
                ::emulated_integration_tests_common::impls::AccountId,
                ::emulated_integration_tests_common::impls::Balance,
            ),
        >,
    ) {
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            for account in accounts {
                let who = account.0;
                let actual = <Self as PenpalBParaPallet>::Balances::free_balance(&who);
                let actual = actual
                    .saturating_add(
                        <Self as PenpalBParaPallet>::Balances::reserved_balance(&who),
                    );
                let is = <Self as PenpalBParaPallet>::Balances::force_set_balance(
                    <Self as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::root(),
                    who.into(),
                    actual.saturating_add(account.1),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            }
        });
    }
    /// Fund a sovereign account of sibling para.
    pub fn fund_para_sovereign(
        sibling_para_id: ::emulated_integration_tests_common::impls::ParaId,
        balance: ::emulated_integration_tests_common::impls::Balance,
    ) {
        let sibling_location = Self::sibling_location_of(sibling_para_id);
        let sovereign_account = Self::sovereign_account_id_of(sibling_location);
        Self::fund_accounts(
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([(sovereign_account.into(), balance)]),
            ),
        )
    }
    /// Return local sovereign account of `para_id` on other `network_id`
    pub fn sovereign_account_of_parachain_on_other_global_consensus(
        network_id: ::emulated_integration_tests_common::impls::NetworkId,
        para_id: ::emulated_integration_tests_common::impls::ParaId,
    ) -> ::emulated_integration_tests_common::impls::AccountId {
        let remote_location = ::emulated_integration_tests_common::impls::Location::new(
            2,
            [
                ::emulated_integration_tests_common::impls::Junction::GlobalConsensus(
                    network_id,
                ),
                ::emulated_integration_tests_common::impls::Junction::Parachain(
                    para_id.into(),
                ),
            ],
        );
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            Self::sovereign_account_id_of(remote_location)
        })
    }
}
type PenpalARuntimeEvent<N> = <PenpalA<
    N,
> as ::emulated_integration_tests_common::impls::Chain>::RuntimeEvent;
impl<N: ::emulated_integration_tests_common::impls::Network> PenpalA<N> {
    /// Asserts a dispatchable is completely executed and XCM sent
    pub fn assert_xcm_pallet_attempted_complete(
        expected_weight: Option<::emulated_integration_tests_common::impls::Weight>,
    ) {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalARuntimeEvent::<
                    N,
                >::PolkadotXcm(
                    ::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {
                        outcome: ::emulated_integration_tests_common::impls::Outcome::Complete {
                            used: weight,
                        },
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if !::emulated_integration_tests_common::impls::weight_within_threshold(
                        (
                            ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                            ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                        ),
                        expected_weight.unwrap_or(*weight),
                        *weight,
                    ) && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "weight",
                                        weight,
                                        "::emulated_integration_tests_common::impls::weight_within_threshold((::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,\n        ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD),\n    expected_weight.unwrap_or(*weight), *weight)",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= ::emulated_integration_tests_common::impls::weight_within_threshold(
                            (
                                ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                                ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                            ),
                            expected_weight.unwrap_or(*weight),
                            *weight,
                        );
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {\noutcome: ::emulated_integration_tests_common::impls::Outcome::Complete {\n    used: weight\n    } })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {\noutcome: ::emulated_integration_tests_common::impls::Outcome::Complete {\n    used: weight\n    } })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a dispatchable is incompletely executed and XCM sent
    pub fn assert_xcm_pallet_attempted_incomplete(
        expected_weight: Option<::emulated_integration_tests_common::impls::Weight>,
        expected_error: Option<::emulated_integration_tests_common::impls::XcmError>,
    ) {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalARuntimeEvent::<
                    N,
                >::PolkadotXcm(
                    ::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {
                        outcome: ::emulated_integration_tests_common::impls::Outcome::Incomplete {
                            used: weight,
                            error,
                        },
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if !::emulated_integration_tests_common::impls::weight_within_threshold(
                        (
                            ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                            ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                        ),
                        expected_weight.unwrap_or(*weight),
                        *weight,
                    ) && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "weight",
                                        weight,
                                        "::emulated_integration_tests_common::impls::weight_within_threshold((::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,\n        ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD),\n    expected_weight.unwrap_or(*weight), *weight)",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= ::emulated_integration_tests_common::impls::weight_within_threshold(
                            (
                                ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                                ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                            ),
                            expected_weight.unwrap_or(*weight),
                            *weight,
                        );
                    if !(*error == expected_error.unwrap_or((*error).into()).into())
                        && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "error",
                                        error,
                                        "*error == expected_error.unwrap_or((*error).into()).into()",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= *error == expected_error.unwrap_or((*error).into()).into();
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {\noutcome: ::emulated_integration_tests_common::impls::Outcome::Incomplete {\n    used: weight, error\n    } })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {\noutcome: ::emulated_integration_tests_common::impls::Outcome::Incomplete {\n    used: weight, error\n    } })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a dispatchable throws and error when trying to be sent
    pub fn assert_xcm_pallet_attempted_error(
        expected_error: Option<::emulated_integration_tests_common::impls::XcmError>,
    ) {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalARuntimeEvent::<
                    N,
                >::PolkadotXcm(
                    ::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {
                        outcome: ::emulated_integration_tests_common::impls::Outcome::Error {
                            error,
                        },
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if !(*error == expected_error.unwrap_or((*error).into()).into())
                        && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "error",
                                        error,
                                        "*error == expected_error.unwrap_or((*error).into()).into()",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= *error == expected_error.unwrap_or((*error).into()).into();
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {\noutcome: ::emulated_integration_tests_common::impls::Outcome::Error { error }\n})",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {\noutcome: ::emulated_integration_tests_common::impls::Outcome::Error { error }\n})",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a XCM message is sent
    pub fn assert_xcm_pallet_sent() {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalARuntimeEvent::<
                    N,
                >::PolkadotXcm(
                    ::emulated_integration_tests_common::impls::pallet_xcm::Event::Sent {
                        ..
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Sent {\n.. })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Sent {\n.. })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a XCM message is sent to Relay Chain
    pub fn assert_parachain_system_ump_sent() {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalARuntimeEvent::<
                    N,
                >::ParachainSystem(
                    ::emulated_integration_tests_common::impls::cumulus_pallet_parachain_system::Event::UpwardMessageSent {
                        ..
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::ParachainSystem(::emulated_integration_tests_common::impls::cumulus_pallet_parachain_system::Event::UpwardMessageSent {\n.. })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::ParachainSystem(::emulated_integration_tests_common::impls::cumulus_pallet_parachain_system::Event::UpwardMessageSent {\n.. })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a XCM from Relay Chain is completely executed
    pub fn assert_dmp_queue_complete(
        expected_weight: Option<::emulated_integration_tests_common::impls::Weight>,
    ) {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalARuntimeEvent::<
                    N,
                >::MessageQueue(
                    ::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {
                        success: true,
                        weight_used: weight,
                        ..
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if !::emulated_integration_tests_common::impls::weight_within_threshold(
                        (
                            ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                            ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                        ),
                        expected_weight.unwrap_or(*weight),
                        *weight,
                    ) && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "weight",
                                        weight,
                                        "::emulated_integration_tests_common::impls::weight_within_threshold((::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,\n        ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD),\n    expected_weight.unwrap_or(*weight), *weight)",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= ::emulated_integration_tests_common::impls::weight_within_threshold(
                            (
                                ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                                ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                            ),
                            expected_weight.unwrap_or(*weight),
                            *weight,
                        );
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {\nsuccess: true, weight_used: weight, .. })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {\nsuccess: true, weight_used: weight, .. })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a XCM from Relay Chain is incompletely executed
    pub fn assert_dmp_queue_incomplete(
        expected_weight: Option<::emulated_integration_tests_common::impls::Weight>,
    ) {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalARuntimeEvent::<
                    N,
                >::MessageQueue(
                    ::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {
                        success: false,
                        weight_used: weight,
                        ..
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if !::emulated_integration_tests_common::impls::weight_within_threshold(
                        (
                            ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                            ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                        ),
                        expected_weight.unwrap_or(*weight),
                        *weight,
                    ) && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "weight",
                                        weight,
                                        "::emulated_integration_tests_common::impls::weight_within_threshold((::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,\n        ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD),\n    expected_weight.unwrap_or(*weight), *weight)",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= ::emulated_integration_tests_common::impls::weight_within_threshold(
                            (
                                ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                                ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                            ),
                            expected_weight.unwrap_or(*weight),
                            *weight,
                        );
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {\nsuccess: false, weight_used: weight, .. })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {\nsuccess: false, weight_used: weight, .. })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a XCM from Relay Chain is executed with error
    pub fn assert_dmp_queue_error() {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalARuntimeEvent::<
                    N,
                >::MessageQueue(
                    ::emulated_integration_tests_common::impls::pallet_message_queue::Event::ProcessingFailed {
                        ..
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::ProcessingFailed {\n.. })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::ProcessingFailed {\n.. })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a XCM from another Parachain is completely executed
    pub fn assert_xcmp_queue_success(
        expected_weight: Option<::emulated_integration_tests_common::impls::Weight>,
    ) {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalARuntimeEvent::<
                    N,
                >::MessageQueue(
                    ::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {
                        success: true,
                        weight_used: weight,
                        ..
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if !::emulated_integration_tests_common::impls::weight_within_threshold(
                        (
                            ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                            ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                        ),
                        expected_weight.unwrap_or(*weight),
                        *weight,
                    ) && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "weight",
                                        weight,
                                        "::emulated_integration_tests_common::impls::weight_within_threshold((::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,\n        ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD),\n    expected_weight.unwrap_or(*weight), *weight)",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= ::emulated_integration_tests_common::impls::weight_within_threshold(
                            (
                                ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                                ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                            ),
                            expected_weight.unwrap_or(*weight),
                            *weight,
                        );
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {\nsuccess: true, weight_used: weight, .. })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalARuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {\nsuccess: true, weight_used: weight, .. })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
}
type PenpalBRuntimeEvent<N> = <PenpalB<
    N,
> as ::emulated_integration_tests_common::impls::Chain>::RuntimeEvent;
impl<N: ::emulated_integration_tests_common::impls::Network> PenpalB<N> {
    /// Asserts a dispatchable is completely executed and XCM sent
    pub fn assert_xcm_pallet_attempted_complete(
        expected_weight: Option<::emulated_integration_tests_common::impls::Weight>,
    ) {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalBRuntimeEvent::<
                    N,
                >::PolkadotXcm(
                    ::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {
                        outcome: ::emulated_integration_tests_common::impls::Outcome::Complete {
                            used: weight,
                        },
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if !::emulated_integration_tests_common::impls::weight_within_threshold(
                        (
                            ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                            ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                        ),
                        expected_weight.unwrap_or(*weight),
                        *weight,
                    ) && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "weight",
                                        weight,
                                        "::emulated_integration_tests_common::impls::weight_within_threshold((::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,\n        ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD),\n    expected_weight.unwrap_or(*weight), *weight)",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= ::emulated_integration_tests_common::impls::weight_within_threshold(
                            (
                                ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                                ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                            ),
                            expected_weight.unwrap_or(*weight),
                            *weight,
                        );
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {\noutcome: ::emulated_integration_tests_common::impls::Outcome::Complete {\n    used: weight\n    } })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {\noutcome: ::emulated_integration_tests_common::impls::Outcome::Complete {\n    used: weight\n    } })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a dispatchable is incompletely executed and XCM sent
    pub fn assert_xcm_pallet_attempted_incomplete(
        expected_weight: Option<::emulated_integration_tests_common::impls::Weight>,
        expected_error: Option<::emulated_integration_tests_common::impls::XcmError>,
    ) {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalBRuntimeEvent::<
                    N,
                >::PolkadotXcm(
                    ::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {
                        outcome: ::emulated_integration_tests_common::impls::Outcome::Incomplete {
                            used: weight,
                            error,
                        },
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if !::emulated_integration_tests_common::impls::weight_within_threshold(
                        (
                            ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                            ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                        ),
                        expected_weight.unwrap_or(*weight),
                        *weight,
                    ) && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "weight",
                                        weight,
                                        "::emulated_integration_tests_common::impls::weight_within_threshold((::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,\n        ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD),\n    expected_weight.unwrap_or(*weight), *weight)",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= ::emulated_integration_tests_common::impls::weight_within_threshold(
                            (
                                ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                                ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                            ),
                            expected_weight.unwrap_or(*weight),
                            *weight,
                        );
                    if !(*error == expected_error.unwrap_or((*error).into()).into())
                        && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "error",
                                        error,
                                        "*error == expected_error.unwrap_or((*error).into()).into()",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= *error == expected_error.unwrap_or((*error).into()).into();
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {\noutcome: ::emulated_integration_tests_common::impls::Outcome::Incomplete {\n    used: weight, error\n    } })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {\noutcome: ::emulated_integration_tests_common::impls::Outcome::Incomplete {\n    used: weight, error\n    } })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a dispatchable throws and error when trying to be sent
    pub fn assert_xcm_pallet_attempted_error(
        expected_error: Option<::emulated_integration_tests_common::impls::XcmError>,
    ) {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalBRuntimeEvent::<
                    N,
                >::PolkadotXcm(
                    ::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {
                        outcome: ::emulated_integration_tests_common::impls::Outcome::Error {
                            error,
                        },
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if !(*error == expected_error.unwrap_or((*error).into()).into())
                        && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "error",
                                        error,
                                        "*error == expected_error.unwrap_or((*error).into()).into()",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= *error == expected_error.unwrap_or((*error).into()).into();
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {\noutcome: ::emulated_integration_tests_common::impls::Outcome::Error { error }\n})",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Attempted {\noutcome: ::emulated_integration_tests_common::impls::Outcome::Error { error }\n})",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a XCM message is sent
    pub fn assert_xcm_pallet_sent() {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalBRuntimeEvent::<
                    N,
                >::PolkadotXcm(
                    ::emulated_integration_tests_common::impls::pallet_xcm::Event::Sent {
                        ..
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Sent {\n.. })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::PolkadotXcm(::emulated_integration_tests_common::impls::pallet_xcm::Event::Sent {\n.. })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a XCM message is sent to Relay Chain
    pub fn assert_parachain_system_ump_sent() {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalBRuntimeEvent::<
                    N,
                >::ParachainSystem(
                    ::emulated_integration_tests_common::impls::cumulus_pallet_parachain_system::Event::UpwardMessageSent {
                        ..
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::ParachainSystem(::emulated_integration_tests_common::impls::cumulus_pallet_parachain_system::Event::UpwardMessageSent {\n.. })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::ParachainSystem(::emulated_integration_tests_common::impls::cumulus_pallet_parachain_system::Event::UpwardMessageSent {\n.. })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a XCM from Relay Chain is completely executed
    pub fn assert_dmp_queue_complete(
        expected_weight: Option<::emulated_integration_tests_common::impls::Weight>,
    ) {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalBRuntimeEvent::<
                    N,
                >::MessageQueue(
                    ::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {
                        success: true,
                        weight_used: weight,
                        ..
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if !::emulated_integration_tests_common::impls::weight_within_threshold(
                        (
                            ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                            ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                        ),
                        expected_weight.unwrap_or(*weight),
                        *weight,
                    ) && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "weight",
                                        weight,
                                        "::emulated_integration_tests_common::impls::weight_within_threshold((::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,\n        ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD),\n    expected_weight.unwrap_or(*weight), *weight)",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= ::emulated_integration_tests_common::impls::weight_within_threshold(
                            (
                                ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                                ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                            ),
                            expected_weight.unwrap_or(*weight),
                            *weight,
                        );
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {\nsuccess: true, weight_used: weight, .. })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {\nsuccess: true, weight_used: weight, .. })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a XCM from Relay Chain is incompletely executed
    pub fn assert_dmp_queue_incomplete(
        expected_weight: Option<::emulated_integration_tests_common::impls::Weight>,
    ) {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalBRuntimeEvent::<
                    N,
                >::MessageQueue(
                    ::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {
                        success: false,
                        weight_used: weight,
                        ..
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if !::emulated_integration_tests_common::impls::weight_within_threshold(
                        (
                            ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                            ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                        ),
                        expected_weight.unwrap_or(*weight),
                        *weight,
                    ) && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "weight",
                                        weight,
                                        "::emulated_integration_tests_common::impls::weight_within_threshold((::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,\n        ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD),\n    expected_weight.unwrap_or(*weight), *weight)",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= ::emulated_integration_tests_common::impls::weight_within_threshold(
                            (
                                ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                                ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                            ),
                            expected_weight.unwrap_or(*weight),
                            *weight,
                        );
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {\nsuccess: false, weight_used: weight, .. })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {\nsuccess: false, weight_used: weight, .. })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a XCM from Relay Chain is executed with error
    pub fn assert_dmp_queue_error() {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalBRuntimeEvent::<
                    N,
                >::MessageQueue(
                    ::emulated_integration_tests_common::impls::pallet_message_queue::Event::ProcessingFailed {
                        ..
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::ProcessingFailed {\n.. })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::ProcessingFailed {\n.. })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
    /// Asserts a XCM from another Parachain is completely executed
    pub fn assert_xcmp_queue_success(
        expected_weight: Option<::emulated_integration_tests_common::impls::Weight>,
    ) {
        let mut message: Vec<String> = Vec::new();
        let mut events = <Self as ::xcm_emulator::Chain>::events();
        let mut event_received = false;
        let mut meet_conditions = true;
        let mut index_match = 0;
        let mut event_message: Vec<String> = Vec::new();
        for (index, event) in events.iter().enumerate() {
            meet_conditions = true;
            match event {
                PenpalBRuntimeEvent::<
                    N,
                >::MessageQueue(
                    ::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {
                        success: true,
                        weight_used: weight,
                        ..
                    },
                ) => {
                    event_received = true;
                    let mut conditions_message: Vec<String> = Vec::new();
                    if !::emulated_integration_tests_common::impls::weight_within_threshold(
                        (
                            ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                            ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                        ),
                        expected_weight.unwrap_or(*weight),
                        *weight,
                    ) && event_message.is_empty()
                    {
                        conditions_message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                        "weight",
                                        weight,
                                        "::emulated_integration_tests_common::impls::weight_within_threshold((::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,\n        ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD),\n    expected_weight.unwrap_or(*weight), *weight)",
                                    ),
                                );
                                res
                            });
                    }
                    meet_conditions
                        &= ::emulated_integration_tests_common::impls::weight_within_threshold(
                            (
                                ::emulated_integration_tests_common::impls::REF_TIME_THRESHOLD,
                                ::emulated_integration_tests_common::impls::PROOF_SIZE_THRESHOLD,
                            ),
                            expected_weight.unwrap_or(*weight),
                            *weight,
                        );
                    if event_received && meet_conditions {
                        index_match = index;
                        break;
                    } else {
                        event_message.extend(conditions_message);
                    }
                }
                _ => {}
            }
        }
        if event_received && !meet_conditions {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {\nsuccess: true, weight_used: weight, .. })",
                            event_message.concat(),
                        ),
                    );
                    res
                });
        } else if !event_received {
            message
                .push({
                    let res = ::alloc::fmt::format(
                        format_args!(
                            "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                            "Self",
                            "PenpalBRuntimeEvent::<N>::MessageQueue(::emulated_integration_tests_common::impls::pallet_message_queue::Event::Processed {\nsuccess: true, weight_used: weight, .. })",
                            <Self as ::xcm_emulator::Chain>::events(),
                        ),
                    );
                    res
                });
        } else {
            events.remove(index_match);
        }
        if !message.is_empty() {
            <Self as ::xcm_emulator::Chain>::events()
                .iter()
                .for_each(|event| {
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                format_args!("{0:?}", event),
                                lvl,
                                &(
                                    "events::Self",
                                    "penpal_emulated_chain",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    };
                });
            {
                #[cold]
                #[track_caller]
                #[inline(never)]
                #[rustc_const_panic_str]
                #[rustc_do_not_const_check]
                const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                    ::core::panicking::panic_display(arg)
                }
                panic_cold_display(&message.concat());
            }
        }
    }
}
impl<N: ::emulated_integration_tests_common::impls::Network> PenpalA<N> {
    /// Create assets using sudo `Assets::force_create()`
    pub fn force_create_asset(
        id: u32,
        owner: ::emulated_integration_tests_common::impls::AccountId,
        is_sufficient: bool,
        min_balance: u128,
        prefund_accounts: Vec<
            (::emulated_integration_tests_common::impls::AccountId, u128),
        >,
    ) {
        use ::emulated_integration_tests_common::impls::Inspect;
        let sudo_origin = <PenpalA<
            N,
        > as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::root();
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            let is = <Self as PenpalAParaPallet>::Assets::force_create(
                sudo_origin,
                id.clone().into(),
                owner.clone().into(),
                is_sufficient,
                min_balance,
            );
            match is {
                Ok(_) => {}
                _ => {
                    if !false {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("Expected Ok(_). Got {0:#?}", is),
                            );
                        }
                    }
                }
            };
            if !<Self as PenpalAParaPallet>::Assets::asset_exists(id.clone()) {
                ::core::panicking::panic(
                    "assertion failed: <Self as PenpalAParaPallet>::Assets::asset_exists(id.clone())",
                )
            }
            type RuntimeEvent<N> = <PenpalA<
                N,
            > as ::emulated_integration_tests_common::impls::Chain>::RuntimeEvent;
            let mut message: Vec<String> = Vec::new();
            let mut events = <Self as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::<
                        N,
                    >::Assets(
                        ::emulated_integration_tests_common::impls::pallet_assets::Event::ForceCreated {
                            asset_id,
                            ..
                        },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == id;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Self",
                                "RuntimeEvent::<N>::Assets(::emulated_integration_tests_common::impls::pallet_assets::Event::ForceCreated {\nasset_id, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Self",
                                "RuntimeEvent::<N>::Assets(::emulated_integration_tests_common::impls::pallet_assets::Event::ForceCreated {\nasset_id, .. })",
                                <Self as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <Self as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::Self",
                                        "penpal_emulated_chain",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        });
        for (beneficiary, amount) in prefund_accounts.into_iter() {
            let signed_origin = <PenpalA<
                N,
            > as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::signed(
                owner.clone(),
            );
            Self::mint_asset(signed_origin, id.clone(), beneficiary, amount);
        }
    }
    /// Mint assets making use of the assets pallet
    pub fn mint_asset(
        signed_origin: <Self as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin,
        id: u32,
        beneficiary: ::emulated_integration_tests_common::impls::AccountId,
        amount_to_mint: u128,
    ) {
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            let is = <Self as PenpalAParaPallet>::Assets::mint(
                signed_origin,
                id.clone().into(),
                beneficiary.clone().into(),
                amount_to_mint,
            );
            match is {
                Ok(_) => {}
                _ => {
                    if !false {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("Expected Ok(_). Got {0:#?}", is),
                            );
                        }
                    }
                }
            };
            type RuntimeEvent<N> = <PenpalA<
                N,
            > as ::emulated_integration_tests_common::impls::Chain>::RuntimeEvent;
            let mut message: Vec<String> = Vec::new();
            let mut events = <Self as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::<
                        N,
                    >::Assets(
                        ::emulated_integration_tests_common::impls::pallet_assets::Event::Issued {
                            asset_id,
                            owner,
                            amount,
                        },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == id;
                        if !(*owner == beneficiary.clone().into())
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == beneficiary.clone().into()",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == beneficiary.clone().into();
                        if !(*amount == amount_to_mint) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == amount_to_mint",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == amount_to_mint;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Self",
                                "RuntimeEvent::<N>::Assets(::emulated_integration_tests_common::impls::pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Self",
                                "RuntimeEvent::<N>::Assets(::emulated_integration_tests_common::impls::pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                <Self as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <Self as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::Self",
                                        "penpal_emulated_chain",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        });
    }
    /// Returns the encoded call for `create` from the assets pallet
    pub fn create_asset_call(
        asset_id: u32,
        min_balance: ::emulated_integration_tests_common::impls::Balance,
        admin: ::emulated_integration_tests_common::impls::AccountId,
    ) -> ::emulated_integration_tests_common::impls::DoubleEncoded<()> {
        use ::emulated_integration_tests_common::impls::{Chain, Encode};
        <Self as Chain>::RuntimeCall::Assets(::emulated_integration_tests_common::impls::pallet_assets::Call::<
                <Self as Chain>::Runtime,
                ::emulated_integration_tests_common::impls::pallet_assets::Instance1,
            >::create {
                id: asset_id.into(),
                min_balance,
                admin: admin.into(),
            })
            .encode()
            .into()
    }
}
impl<N: ::emulated_integration_tests_common::impls::Network> PenpalA<N> {
    /// Create foreign assets using sudo `ForeignAssets::force_create()`
    pub fn force_create_foreign_asset(
        id: xcm::latest::Location,
        owner: ::emulated_integration_tests_common::impls::AccountId,
        is_sufficient: bool,
        min_balance: u128,
        prefund_accounts: Vec<
            (::emulated_integration_tests_common::impls::AccountId, u128),
        >,
    ) {
        use ::emulated_integration_tests_common::impls::Inspect;
        let sudo_origin = <PenpalA<
            N,
        > as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::root();
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            let is = <Self as PenpalAParaPallet>::ForeignAssets::force_create(
                sudo_origin,
                id.clone(),
                owner.clone().into(),
                is_sufficient,
                min_balance,
            );
            match is {
                Ok(_) => {}
                _ => {
                    if !false {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("Expected Ok(_). Got {0:#?}", is),
                            );
                        }
                    }
                }
            };
            if !<Self as PenpalAParaPallet>::ForeignAssets::asset_exists(id.clone()) {
                ::core::panicking::panic(
                    "assertion failed: <Self as PenpalAParaPallet>::ForeignAssets::asset_exists(id.clone())",
                )
            }
            type RuntimeEvent<N> = <PenpalA<
                N,
            > as ::emulated_integration_tests_common::impls::Chain>::RuntimeEvent;
            let mut message: Vec<String> = Vec::new();
            let mut events = <Self as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::<
                        N,
                    >::ForeignAssets(
                        ::emulated_integration_tests_common::impls::pallet_assets::Event::ForceCreated {
                            asset_id,
                            ..
                        },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == id;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Self",
                                "RuntimeEvent::<N>::ForeignAssets(::emulated_integration_tests_common::impls::pallet_assets::Event::ForceCreated {\nasset_id, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Self",
                                "RuntimeEvent::<N>::ForeignAssets(::emulated_integration_tests_common::impls::pallet_assets::Event::ForceCreated {\nasset_id, .. })",
                                <Self as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <Self as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::Self",
                                        "penpal_emulated_chain",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        });
        for (beneficiary, amount) in prefund_accounts.into_iter() {
            let signed_origin = <PenpalA<
                N,
            > as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::signed(
                owner.clone(),
            );
            Self::mint_foreign_asset(signed_origin, id.clone(), beneficiary, amount);
        }
    }
    /// Mint assets making use of the ForeignAssets pallet-assets instance
    pub fn mint_foreign_asset(
        signed_origin: <Self as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin,
        id: xcm::latest::Location,
        beneficiary: ::emulated_integration_tests_common::impls::AccountId,
        amount_to_mint: u128,
    ) {
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            let is = <Self as PenpalAParaPallet>::ForeignAssets::mint(
                signed_origin,
                id.clone().into(),
                beneficiary.clone().into(),
                amount_to_mint,
            );
            match is {
                Ok(_) => {}
                _ => {
                    if !false {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("Expected Ok(_). Got {0:#?}", is),
                            );
                        }
                    }
                }
            };
            type RuntimeEvent<N> = <PenpalA<
                N,
            > as ::emulated_integration_tests_common::impls::Chain>::RuntimeEvent;
            let mut message: Vec<String> = Vec::new();
            let mut events = <Self as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::<
                        N,
                    >::ForeignAssets(
                        ::emulated_integration_tests_common::impls::pallet_assets::Event::Issued {
                            asset_id,
                            owner,
                            amount,
                        },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == id;
                        if !(*owner == beneficiary.clone().into())
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == beneficiary.clone().into()",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == beneficiary.clone().into();
                        if !(*amount == amount_to_mint) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == amount_to_mint",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == amount_to_mint;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Self",
                                "RuntimeEvent::<N>::ForeignAssets(::emulated_integration_tests_common::impls::pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Self",
                                "RuntimeEvent::<N>::ForeignAssets(::emulated_integration_tests_common::impls::pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                <Self as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <Self as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::Self",
                                        "penpal_emulated_chain",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        });
    }
    /// Returns the encoded call for `create` from the foreign assets pallet
    pub fn create_foreign_asset_call(
        asset_id: xcm::latest::Location,
        min_balance: ::emulated_integration_tests_common::impls::Balance,
        admin: ::emulated_integration_tests_common::impls::AccountId,
    ) -> ::emulated_integration_tests_common::impls::DoubleEncoded<()> {
        use ::emulated_integration_tests_common::impls::{Chain, Encode};
        <Self as Chain>::RuntimeCall::ForeignAssets(::emulated_integration_tests_common::impls::pallet_assets::Call::<
                <Self as Chain>::Runtime,
                ::emulated_integration_tests_common::impls::pallet_assets::Instance2,
            >::create {
                id: asset_id.into(),
                min_balance,
                admin: admin.into(),
            })
            .encode()
            .into()
    }
}
impl<N: ::emulated_integration_tests_common::impls::Network> PenpalB<N> {
    /// Create assets using sudo `Assets::force_create()`
    pub fn force_create_asset(
        id: u32,
        owner: ::emulated_integration_tests_common::impls::AccountId,
        is_sufficient: bool,
        min_balance: u128,
        prefund_accounts: Vec<
            (::emulated_integration_tests_common::impls::AccountId, u128),
        >,
    ) {
        use ::emulated_integration_tests_common::impls::Inspect;
        let sudo_origin = <PenpalB<
            N,
        > as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::root();
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            let is = <Self as PenpalBParaPallet>::Assets::force_create(
                sudo_origin,
                id.clone().into(),
                owner.clone().into(),
                is_sufficient,
                min_balance,
            );
            match is {
                Ok(_) => {}
                _ => {
                    if !false {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("Expected Ok(_). Got {0:#?}", is),
                            );
                        }
                    }
                }
            };
            if !<Self as PenpalBParaPallet>::Assets::asset_exists(id.clone()) {
                ::core::panicking::panic(
                    "assertion failed: <Self as PenpalBParaPallet>::Assets::asset_exists(id.clone())",
                )
            }
            type RuntimeEvent<N> = <PenpalB<
                N,
            > as ::emulated_integration_tests_common::impls::Chain>::RuntimeEvent;
            let mut message: Vec<String> = Vec::new();
            let mut events = <Self as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::<
                        N,
                    >::Assets(
                        ::emulated_integration_tests_common::impls::pallet_assets::Event::ForceCreated {
                            asset_id,
                            ..
                        },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == id;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Self",
                                "RuntimeEvent::<N>::Assets(::emulated_integration_tests_common::impls::pallet_assets::Event::ForceCreated {\nasset_id, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Self",
                                "RuntimeEvent::<N>::Assets(::emulated_integration_tests_common::impls::pallet_assets::Event::ForceCreated {\nasset_id, .. })",
                                <Self as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <Self as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::Self",
                                        "penpal_emulated_chain",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        });
        for (beneficiary, amount) in prefund_accounts.into_iter() {
            let signed_origin = <PenpalB<
                N,
            > as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::signed(
                owner.clone(),
            );
            Self::mint_asset(signed_origin, id.clone(), beneficiary, amount);
        }
    }
    /// Mint assets making use of the assets pallet
    pub fn mint_asset(
        signed_origin: <Self as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin,
        id: u32,
        beneficiary: ::emulated_integration_tests_common::impls::AccountId,
        amount_to_mint: u128,
    ) {
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            let is = <Self as PenpalBParaPallet>::Assets::mint(
                signed_origin,
                id.clone().into(),
                beneficiary.clone().into(),
                amount_to_mint,
            );
            match is {
                Ok(_) => {}
                _ => {
                    if !false {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("Expected Ok(_). Got {0:#?}", is),
                            );
                        }
                    }
                }
            };
            type RuntimeEvent<N> = <PenpalB<
                N,
            > as ::emulated_integration_tests_common::impls::Chain>::RuntimeEvent;
            let mut message: Vec<String> = Vec::new();
            let mut events = <Self as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::<
                        N,
                    >::Assets(
                        ::emulated_integration_tests_common::impls::pallet_assets::Event::Issued {
                            asset_id,
                            owner,
                            amount,
                        },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == id;
                        if !(*owner == beneficiary.clone().into())
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == beneficiary.clone().into()",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == beneficiary.clone().into();
                        if !(*amount == amount_to_mint) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == amount_to_mint",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == amount_to_mint;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Self",
                                "RuntimeEvent::<N>::Assets(::emulated_integration_tests_common::impls::pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Self",
                                "RuntimeEvent::<N>::Assets(::emulated_integration_tests_common::impls::pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                <Self as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <Self as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::Self",
                                        "penpal_emulated_chain",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        });
    }
    /// Returns the encoded call for `create` from the assets pallet
    pub fn create_asset_call(
        asset_id: u32,
        min_balance: ::emulated_integration_tests_common::impls::Balance,
        admin: ::emulated_integration_tests_common::impls::AccountId,
    ) -> ::emulated_integration_tests_common::impls::DoubleEncoded<()> {
        use ::emulated_integration_tests_common::impls::{Chain, Encode};
        <Self as Chain>::RuntimeCall::Assets(::emulated_integration_tests_common::impls::pallet_assets::Call::<
                <Self as Chain>::Runtime,
                ::emulated_integration_tests_common::impls::pallet_assets::Instance1,
            >::create {
                id: asset_id.into(),
                min_balance,
                admin: admin.into(),
            })
            .encode()
            .into()
    }
}
impl<N: ::emulated_integration_tests_common::impls::Network> PenpalB<N> {
    /// Create foreign assets using sudo `ForeignAssets::force_create()`
    pub fn force_create_foreign_asset(
        id: xcm::latest::Location,
        owner: ::emulated_integration_tests_common::impls::AccountId,
        is_sufficient: bool,
        min_balance: u128,
        prefund_accounts: Vec<
            (::emulated_integration_tests_common::impls::AccountId, u128),
        >,
    ) {
        use ::emulated_integration_tests_common::impls::Inspect;
        let sudo_origin = <PenpalB<
            N,
        > as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::root();
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            let is = <Self as PenpalBParaPallet>::ForeignAssets::force_create(
                sudo_origin,
                id.clone(),
                owner.clone().into(),
                is_sufficient,
                min_balance,
            );
            match is {
                Ok(_) => {}
                _ => {
                    if !false {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("Expected Ok(_). Got {0:#?}", is),
                            );
                        }
                    }
                }
            };
            if !<Self as PenpalBParaPallet>::ForeignAssets::asset_exists(id.clone()) {
                ::core::panicking::panic(
                    "assertion failed: <Self as PenpalBParaPallet>::ForeignAssets::asset_exists(id.clone())",
                )
            }
            type RuntimeEvent<N> = <PenpalB<
                N,
            > as ::emulated_integration_tests_common::impls::Chain>::RuntimeEvent;
            let mut message: Vec<String> = Vec::new();
            let mut events = <Self as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::<
                        N,
                    >::ForeignAssets(
                        ::emulated_integration_tests_common::impls::pallet_assets::Event::ForceCreated {
                            asset_id,
                            ..
                        },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == id;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Self",
                                "RuntimeEvent::<N>::ForeignAssets(::emulated_integration_tests_common::impls::pallet_assets::Event::ForceCreated {\nasset_id, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Self",
                                "RuntimeEvent::<N>::ForeignAssets(::emulated_integration_tests_common::impls::pallet_assets::Event::ForceCreated {\nasset_id, .. })",
                                <Self as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <Self as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::Self",
                                        "penpal_emulated_chain",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        });
        for (beneficiary, amount) in prefund_accounts.into_iter() {
            let signed_origin = <PenpalB<
                N,
            > as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::signed(
                owner.clone(),
            );
            Self::mint_foreign_asset(signed_origin, id.clone(), beneficiary, amount);
        }
    }
    /// Mint assets making use of the ForeignAssets pallet-assets instance
    pub fn mint_foreign_asset(
        signed_origin: <Self as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin,
        id: xcm::latest::Location,
        beneficiary: ::emulated_integration_tests_common::impls::AccountId,
        amount_to_mint: u128,
    ) {
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            let is = <Self as PenpalBParaPallet>::ForeignAssets::mint(
                signed_origin,
                id.clone().into(),
                beneficiary.clone().into(),
                amount_to_mint,
            );
            match is {
                Ok(_) => {}
                _ => {
                    if !false {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("Expected Ok(_). Got {0:#?}", is),
                            );
                        }
                    }
                }
            };
            type RuntimeEvent<N> = <PenpalB<
                N,
            > as ::emulated_integration_tests_common::impls::Chain>::RuntimeEvent;
            let mut message: Vec<String> = Vec::new();
            let mut events = <Self as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::<
                        N,
                    >::ForeignAssets(
                        ::emulated_integration_tests_common::impls::pallet_assets::Event::Issued {
                            asset_id,
                            owner,
                            amount,
                        },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == id;
                        if !(*owner == beneficiary.clone().into())
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == beneficiary.clone().into()",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == beneficiary.clone().into();
                        if !(*amount == amount_to_mint) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == amount_to_mint",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == amount_to_mint;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Self",
                                "RuntimeEvent::<N>::ForeignAssets(::emulated_integration_tests_common::impls::pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Self",
                                "RuntimeEvent::<N>::ForeignAssets(::emulated_integration_tests_common::impls::pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                <Self as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <Self as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::Self",
                                        "penpal_emulated_chain",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        });
    }
    /// Returns the encoded call for `create` from the foreign assets pallet
    pub fn create_foreign_asset_call(
        asset_id: xcm::latest::Location,
        min_balance: ::emulated_integration_tests_common::impls::Balance,
        admin: ::emulated_integration_tests_common::impls::AccountId,
    ) -> ::emulated_integration_tests_common::impls::DoubleEncoded<()> {
        use ::emulated_integration_tests_common::impls::{Chain, Encode};
        <Self as Chain>::RuntimeCall::ForeignAssets(::emulated_integration_tests_common::impls::pallet_assets::Call::<
                <Self as Chain>::Runtime,
                ::emulated_integration_tests_common::impls::pallet_assets::Instance2,
            >::create {
                id: asset_id.into(),
                min_balance,
                admin: admin.into(),
            })
            .encode()
            .into()
    }
}
impl<N: ::emulated_integration_tests_common::impls::Network> PenpalA<N> {
    /// Set XCM version for destination.
    pub fn force_xcm_version(
        dest: ::emulated_integration_tests_common::impls::Location,
        version: ::emulated_integration_tests_common::impls::XcmVersion,
    ) {
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            let is = <Self as PenpalAParaPallet>::PolkadotXcm::force_xcm_version(
                <Self as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::root(),
                Box::new(dest),
                version,
            );
            match is {
                Ok(_) => {}
                _ => {
                    if !false {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("Expected Ok(_). Got {0:#?}", is),
                            );
                        }
                    }
                }
            };
        });
    }
    /// Set default/safe XCM version for runtime.
    pub fn force_default_xcm_version(
        version: Option<::emulated_integration_tests_common::impls::XcmVersion>,
    ) {
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            let is = <Self as PenpalAParaPallet>::PolkadotXcm::force_default_xcm_version(
                <Self as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::root(),
                version,
            );
            match is {
                Ok(_) => {}
                _ => {
                    if !false {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("Expected Ok(_). Got {0:#?}", is),
                            );
                        }
                    }
                }
            };
        });
    }
}
impl<N: ::emulated_integration_tests_common::impls::Network> PenpalB<N> {
    /// Set XCM version for destination.
    pub fn force_xcm_version(
        dest: ::emulated_integration_tests_common::impls::Location,
        version: ::emulated_integration_tests_common::impls::XcmVersion,
    ) {
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            let is = <Self as PenpalBParaPallet>::PolkadotXcm::force_xcm_version(
                <Self as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::root(),
                Box::new(dest),
                version,
            );
            match is {
                Ok(_) => {}
                _ => {
                    if !false {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("Expected Ok(_). Got {0:#?}", is),
                            );
                        }
                    }
                }
            };
        });
    }
    /// Set default/safe XCM version for runtime.
    pub fn force_default_xcm_version(
        version: Option<::emulated_integration_tests_common::impls::XcmVersion>,
    ) {
        <Self as ::emulated_integration_tests_common::impls::TestExt>::execute_with(|| {
            let is = <Self as PenpalBParaPallet>::PolkadotXcm::force_default_xcm_version(
                <Self as ::emulated_integration_tests_common::impls::Chain>::RuntimeOrigin::root(),
                version,
            );
            match is {
                Ok(_) => {}
                _ => {
                    if !false {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("Expected Ok(_). Got {0:#?}", is),
                            );
                        }
                    }
                }
            };
        });
    }
}
