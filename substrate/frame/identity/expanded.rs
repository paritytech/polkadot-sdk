#![feature(prelude_import)]
//! # Identity Pallet
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ## Overview
//!
//! A federated naming system, allowing for multiple registrars to be added from a specified origin.
//! Registrars can set a fee to provide identity-verification service. Anyone can put forth a
//! proposed identity for a fixed deposit and ask for review by any number of registrars (paying
//! each of their fees). Registrar judgements are given as an `enum`, allowing for sophisticated,
//! multi-tier opinions.
//!
//! Some judgements are identified as *sticky*, which means they cannot be removed except by
//! complete removal of the identity, or by the registrar. Judgements are allowed to represent a
//! portion of funds that have been reserved for the registrar.
//!
//! A super-user can remove accounts and in doing so, slash the deposit.
//!
//! All accounts may also have a limited number of sub-accounts which may be specified by the owner;
//! by definition, these have equivalent ownership and each has an individual name.
//!
//! The number of registrars should be limited, and the deposit made sufficiently large, to ensure
//! no state-bloat attack is viable.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! #### For general users
//! * `set_identity` - Set the associated identity of an account; a small deposit is reserved if not
//!   already taken.
//! * `clear_identity` - Remove an account's associated identity; the deposit is returned.
//! * `request_judgement` - Request a judgement from a registrar, paying a fee.
//! * `cancel_request` - Cancel the previous request for a judgement.
//!
//! #### For general users with sub-identities
//! * `set_subs` - Set the sub-accounts of an identity.
//! * `add_sub` - Add a sub-identity to an identity.
//! * `remove_sub` - Remove a sub-identity of an identity.
//! * `rename_sub` - Rename a sub-identity of an identity.
//! * `quit_sub` - Remove a sub-identity of an identity (called by the sub-identity).
//!
//! #### For registrars
//! * `set_fee` - Set the fee required to be paid for a judgement to be given by the registrar.
//! * `set_fields` - Set the fields that a registrar cares about in their judgements.
//! * `provide_judgement` - Provide a judgement to an identity.
//!
//! #### For super-users
//! * `add_registrar` - Add a new registrar to the system.
//! * `kill_identity` - Forcibly remove the associated identity; the deposit is lost.
//!
//! [`Call`]: ./enum.Call.html
//! [`Config`]: ./trait.Config.html
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
mod types {
    use super::*;
    use codec::{Decode, Encode, MaxEncodedLen};
    use enumflags2::{bitflags, BitFlags};
    use frame_support::{
        traits::{ConstU32, Get},
        BoundedVec, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    };
    use scale_info::{
        build::{Fields, Variants},
        meta_type, Path, Type, TypeInfo, TypeParameter,
    };
    use sp_runtime::{traits::Zero, RuntimeDebug};
    use sp_std::{fmt::Debug, iter::once, ops::Add, prelude::*};
    /// Either underlying data blob if it is at most 32 bytes, or a hash of it. If the data is greater
    /// than 32-bytes then it will be truncated when encoding.
    ///
    /// Can also be `None`.
    pub enum Data {
        /// No data here.
        None,
        /// The data is stored directly.
        Raw(BoundedVec<u8, ConstU32<32>>),
        /// Only the Blake2 hash of the data is stored. The preimage of the hash may be retrieved
        /// through some hash-lookup service.
        BlakeTwo256([u8; 32]),
        /// Only the SHA2-256 hash of the data is stored. The preimage of the hash may be retrieved
        /// through some hash-lookup service.
        Sha256([u8; 32]),
        /// Only the Keccak-256 hash of the data is stored. The preimage of the hash may be retrieved
        /// through some hash-lookup service.
        Keccak256([u8; 32]),
        /// Only the SHA3-256 hash of the data is stored. The preimage of the hash may be retrieved
        /// through some hash-lookup service.
        ShaThree256([u8; 32]),
    }
    #[automatically_derived]
    impl ::core::clone::Clone for Data {
        #[inline]
        fn clone(&self) -> Data {
            match self {
                Data::None => Data::None,
                Data::Raw(__self_0) => Data::Raw(::core::clone::Clone::clone(__self_0)),
                Data::BlakeTwo256(__self_0) => {
                    Data::BlakeTwo256(::core::clone::Clone::clone(__self_0))
                }
                Data::Sha256(__self_0) => {
                    Data::Sha256(::core::clone::Clone::clone(__self_0))
                }
                Data::Keccak256(__self_0) => {
                    Data::Keccak256(::core::clone::Clone::clone(__self_0))
                }
                Data::ShaThree256(__self_0) => {
                    Data::ShaThree256(::core::clone::Clone::clone(__self_0))
                }
            }
        }
    }
    #[automatically_derived]
    impl ::core::marker::StructuralEq for Data {}
    #[automatically_derived]
    impl ::core::cmp::Eq for Data {
        #[inline]
        #[doc(hidden)]
        #[coverage(off)]
        fn assert_receiver_is_total_eq(&self) -> () {
            let _: ::core::cmp::AssertParamIsEq<BoundedVec<u8, ConstU32<32>>>;
            let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
            let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
            let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
            let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
        }
    }
    #[automatically_derived]
    impl ::core::marker::StructuralPartialEq for Data {}
    #[automatically_derived]
    impl ::core::cmp::PartialEq for Data {
        #[inline]
        fn eq(&self, other: &Data) -> bool {
            let __self_tag = ::core::intrinsics::discriminant_value(self);
            let __arg1_tag = ::core::intrinsics::discriminant_value(other);
            __self_tag == __arg1_tag
                && match (self, other) {
                    (Data::Raw(__self_0), Data::Raw(__arg1_0)) => *__self_0 == *__arg1_0,
                    (Data::BlakeTwo256(__self_0), Data::BlakeTwo256(__arg1_0)) => {
                        *__self_0 == *__arg1_0
                    }
                    (Data::Sha256(__self_0), Data::Sha256(__arg1_0)) => {
                        *__self_0 == *__arg1_0
                    }
                    (Data::Keccak256(__self_0), Data::Keccak256(__arg1_0)) => {
                        *__self_0 == *__arg1_0
                    }
                    (Data::ShaThree256(__self_0), Data::ShaThree256(__arg1_0)) => {
                        *__self_0 == *__arg1_0
                    }
                    _ => true,
                }
        }
    }
    impl core::fmt::Debug for Data {
        fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
            match self {
                Self::None => fmt.debug_tuple("Data::None").finish(),
                Self::Raw(ref a0) => fmt.debug_tuple("Data::Raw").field(a0).finish(),
                Self::BlakeTwo256(ref a0) => {
                    fmt.debug_tuple("Data::BlakeTwo256").field(a0).finish()
                }
                Self::Sha256(ref a0) => {
                    fmt.debug_tuple("Data::Sha256").field(a0).finish()
                }
                Self::Keccak256(ref a0) => {
                    fmt.debug_tuple("Data::Keccak256").field(a0).finish()
                }
                Self::ShaThree256(ref a0) => {
                    fmt.debug_tuple("Data::ShaThree256").field(a0).finish()
                }
                _ => Ok(()),
            }
        }
    }
    const _: () = {
        impl ::codec::MaxEncodedLen for Data {
            fn max_encoded_len() -> ::core::primitive::usize {
                0_usize
                    .max(0_usize)
                    .max(
                        0_usize
                            .saturating_add(
                                <BoundedVec<u8, ConstU32<32>>>::max_encoded_len(),
                            ),
                    )
                    .max(0_usize.saturating_add(<[u8; 32]>::max_encoded_len()))
                    .max(0_usize.saturating_add(<[u8; 32]>::max_encoded_len()))
                    .max(0_usize.saturating_add(<[u8; 32]>::max_encoded_len()))
                    .max(0_usize.saturating_add(<[u8; 32]>::max_encoded_len()))
                    .saturating_add(1)
            }
        }
    };
    impl Data {
        pub fn is_none(&self) -> bool {
            self == &Data::None
        }
    }
    impl Decode for Data {
        fn decode<I: codec::Input>(
            input: &mut I,
        ) -> sp_std::result::Result<Self, codec::Error> {
            let b = input.read_byte()?;
            Ok(
                match b {
                    0 => Data::None,
                    n @ 1..=33 => {
                        let mut r: BoundedVec<_, _> = ::alloc::vec::from_elem(
                                0u8,
                                n as usize - 1,
                            )
                            .try_into()
                            .expect("bound checked in match arm condition; qed");
                        input.read(&mut r[..])?;
                        Data::Raw(r)
                    }
                    34 => Data::BlakeTwo256(<[u8; 32]>::decode(input)?),
                    35 => Data::Sha256(<[u8; 32]>::decode(input)?),
                    36 => Data::Keccak256(<[u8; 32]>::decode(input)?),
                    37 => Data::ShaThree256(<[u8; 32]>::decode(input)?),
                    _ => return Err(codec::Error::from("invalid leading byte")),
                },
            )
        }
    }
    impl Encode for Data {
        fn encode(&self) -> Vec<u8> {
            match self {
                Data::None => ::alloc::vec::from_elem(0u8, 1),
                Data::Raw(ref x) => {
                    let l = x.len().min(32);
                    let mut r = ::alloc::vec::from_elem(l as u8 + 1, l + 1);
                    r[1..].copy_from_slice(&x[..l as usize]);
                    r
                }
                Data::BlakeTwo256(ref h) => once(34u8).chain(h.iter().cloned()).collect(),
                Data::Sha256(ref h) => once(35u8).chain(h.iter().cloned()).collect(),
                Data::Keccak256(ref h) => once(36u8).chain(h.iter().cloned()).collect(),
                Data::ShaThree256(ref h) => once(37u8).chain(h.iter().cloned()).collect(),
            }
        }
    }
    impl codec::EncodeLike for Data {}
    impl TypeInfo for Data {
        type Identity = Self;
        fn type_info() -> Type {
            let variants = Variants::new().variant("None", |v| v.index(0));
            let variants = variants
                .variant(
                    "Raw0",
                    |v| v.index(1).fields(Fields::unnamed().field(|f| f.ty::<[u8; 0]>())),
                )
                .variant(
                    "Raw1",
                    |v| v.index(2).fields(Fields::unnamed().field(|f| f.ty::<[u8; 1]>())),
                )
                .variant(
                    "Raw2",
                    |v| v.index(3).fields(Fields::unnamed().field(|f| f.ty::<[u8; 2]>())),
                )
                .variant(
                    "Raw3",
                    |v| v.index(4).fields(Fields::unnamed().field(|f| f.ty::<[u8; 3]>())),
                )
                .variant(
                    "Raw4",
                    |v| v.index(5).fields(Fields::unnamed().field(|f| f.ty::<[u8; 4]>())),
                )
                .variant(
                    "Raw5",
                    |v| v.index(6).fields(Fields::unnamed().field(|f| f.ty::<[u8; 5]>())),
                )
                .variant(
                    "Raw6",
                    |v| v.index(7).fields(Fields::unnamed().field(|f| f.ty::<[u8; 6]>())),
                )
                .variant(
                    "Raw7",
                    |v| v.index(8).fields(Fields::unnamed().field(|f| f.ty::<[u8; 7]>())),
                )
                .variant(
                    "Raw8",
                    |v| v.index(9).fields(Fields::unnamed().field(|f| f.ty::<[u8; 8]>())),
                )
                .variant(
                    "Raw9",
                    |v| {
                        v
                            .index(10)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 9]>()))
                    },
                )
                .variant(
                    "Raw10",
                    |v| {
                        v
                            .index(11)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 10]>()))
                    },
                )
                .variant(
                    "Raw11",
                    |v| {
                        v
                            .index(12)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 11]>()))
                    },
                )
                .variant(
                    "Raw12",
                    |v| {
                        v
                            .index(13)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 12]>()))
                    },
                )
                .variant(
                    "Raw13",
                    |v| {
                        v
                            .index(14)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 13]>()))
                    },
                )
                .variant(
                    "Raw14",
                    |v| {
                        v
                            .index(15)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 14]>()))
                    },
                )
                .variant(
                    "Raw15",
                    |v| {
                        v
                            .index(16)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 15]>()))
                    },
                )
                .variant(
                    "Raw16",
                    |v| {
                        v
                            .index(17)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 16]>()))
                    },
                )
                .variant(
                    "Raw17",
                    |v| {
                        v
                            .index(18)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 17]>()))
                    },
                )
                .variant(
                    "Raw18",
                    |v| {
                        v
                            .index(19)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 18]>()))
                    },
                )
                .variant(
                    "Raw19",
                    |v| {
                        v
                            .index(20)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 19]>()))
                    },
                )
                .variant(
                    "Raw20",
                    |v| {
                        v
                            .index(21)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 20]>()))
                    },
                )
                .variant(
                    "Raw21",
                    |v| {
                        v
                            .index(22)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 21]>()))
                    },
                )
                .variant(
                    "Raw22",
                    |v| {
                        v
                            .index(23)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 22]>()))
                    },
                )
                .variant(
                    "Raw23",
                    |v| {
                        v
                            .index(24)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 23]>()))
                    },
                )
                .variant(
                    "Raw24",
                    |v| {
                        v
                            .index(25)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 24]>()))
                    },
                )
                .variant(
                    "Raw25",
                    |v| {
                        v
                            .index(26)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 25]>()))
                    },
                )
                .variant(
                    "Raw26",
                    |v| {
                        v
                            .index(27)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 26]>()))
                    },
                )
                .variant(
                    "Raw27",
                    |v| {
                        v
                            .index(28)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 27]>()))
                    },
                )
                .variant(
                    "Raw28",
                    |v| {
                        v
                            .index(29)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 28]>()))
                    },
                )
                .variant(
                    "Raw29",
                    |v| {
                        v
                            .index(30)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 29]>()))
                    },
                )
                .variant(
                    "Raw30",
                    |v| {
                        v
                            .index(31)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 30]>()))
                    },
                )
                .variant(
                    "Raw31",
                    |v| {
                        v
                            .index(32)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 31]>()))
                    },
                )
                .variant(
                    "Raw32",
                    |v| {
                        v
                            .index(33)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 32]>()))
                    },
                );
            let variants = variants
                .variant(
                    "BlakeTwo256",
                    |v| {
                        v.index(34)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 32]>()))
                    },
                )
                .variant(
                    "Sha256",
                    |v| {
                        v.index(35)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 32]>()))
                    },
                )
                .variant(
                    "Keccak256",
                    |v| {
                        v.index(36)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 32]>()))
                    },
                )
                .variant(
                    "ShaThree256",
                    |v| {
                        v.index(37)
                            .fields(Fields::unnamed().field(|f| f.ty::<[u8; 32]>()))
                    },
                );
            Type::builder()
                .path(Path::new("Data", "pallet_identity::types"))
                .variant(variants)
        }
    }
    impl Default for Data {
        fn default() -> Self {
            Self::None
        }
    }
    /// An identifier for a single name registrar/identity verification service.
    pub type RegistrarIndex = u32;
    /// An attestation of a registrar over how accurate some `IdentityInfo` is in describing an account.
    ///
    /// NOTE: Registrars may pay little attention to some fields. Registrars may want to make clear
    /// which fields their attestation is relevant for by off-chain means.
    pub enum Judgement<
        Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq,
    > {
        /// The default value; no opinion is held.
        Unknown,
        /// No judgement is yet in place, but a deposit is reserved as payment for providing one.
        FeePaid(Balance),
        /// The data appears to be reasonably acceptable in terms of its accuracy, however no in depth
        /// checks (such as in-person meetings or formal KYC) have been conducted.
        Reasonable,
        /// The target is known directly by the registrar and the registrar can fully attest to the
        /// the data's accuracy.
        KnownGood,
        /// The data was once good but is currently out of date. There is no malicious intent in the
        /// inaccuracy. This judgement can be removed through updating the data.
        OutOfDate,
        /// The data is imprecise or of sufficiently low-quality to be problematic. It is not
        /// indicative of malicious intent. This judgement can be removed through updating the data.
        LowQuality,
        /// The data is erroneous. This may be indicative of malicious intent. This cannot be removed
        /// except by the registrar.
        Erroneous,
    }
    #[automatically_derived]
    impl<
        Balance: ::core::marker::Copy + Encode + Decode + MaxEncodedLen + Copy + Clone
            + Debug + Eq + PartialEq,
    > ::core::marker::Copy for Judgement<Balance> {}
    #[automatically_derived]
    impl<
        Balance: ::core::clone::Clone + Encode + Decode + MaxEncodedLen + Copy + Clone
            + Debug + Eq + PartialEq,
    > ::core::clone::Clone for Judgement<Balance> {
        #[inline]
        fn clone(&self) -> Judgement<Balance> {
            match self {
                Judgement::Unknown => Judgement::Unknown,
                Judgement::FeePaid(__self_0) => {
                    Judgement::FeePaid(::core::clone::Clone::clone(__self_0))
                }
                Judgement::Reasonable => Judgement::Reasonable,
                Judgement::KnownGood => Judgement::KnownGood,
                Judgement::OutOfDate => Judgement::OutOfDate,
                Judgement::LowQuality => Judgement::LowQuality,
                Judgement::Erroneous => Judgement::Erroneous,
            }
        }
    }
    #[allow(deprecated)]
    const _: () = {
        #[automatically_derived]
        impl<
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq,
        > ::codec::Encode for Judgement<Balance>
        where
            Balance: ::codec::Encode,
            Balance: ::codec::Encode,
        {
            fn size_hint(&self) -> usize {
                1_usize
                    + match *self {
                        Judgement::Unknown => 0_usize,
                        Judgement::FeePaid(ref aa) => {
                            0_usize.saturating_add(::codec::Encode::size_hint(aa))
                        }
                        Judgement::Reasonable => 0_usize,
                        Judgement::KnownGood => 0_usize,
                        Judgement::OutOfDate => 0_usize,
                        Judgement::LowQuality => 0_usize,
                        Judgement::Erroneous => 0_usize,
                        _ => 0_usize,
                    }
            }
            fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
                &self,
                __codec_dest_edqy: &mut __CodecOutputEdqy,
            ) {
                match *self {
                    Judgement::Unknown => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(0usize as ::core::primitive::u8);
                    }
                    Judgement::FeePaid(ref aa) => {
                        __codec_dest_edqy.push_byte(1usize as ::core::primitive::u8);
                        ::codec::Encode::encode_to(aa, __codec_dest_edqy);
                    }
                    Judgement::Reasonable => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(2usize as ::core::primitive::u8);
                    }
                    Judgement::KnownGood => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(3usize as ::core::primitive::u8);
                    }
                    Judgement::OutOfDate => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(4usize as ::core::primitive::u8);
                    }
                    Judgement::LowQuality => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(5usize as ::core::primitive::u8);
                    }
                    Judgement::Erroneous => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(6usize as ::core::primitive::u8);
                    }
                    _ => {}
                }
            }
        }
        #[automatically_derived]
        impl<
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq,
        > ::codec::EncodeLike for Judgement<Balance>
        where
            Balance: ::codec::Encode,
            Balance: ::codec::Encode,
        {}
    };
    #[allow(deprecated)]
    const _: () = {
        #[automatically_derived]
        impl<
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq,
        > ::codec::Decode for Judgement<Balance>
        where
            Balance: ::codec::Decode,
            Balance: ::codec::Decode,
        {
            fn decode<__CodecInputEdqy: ::codec::Input>(
                __codec_input_edqy: &mut __CodecInputEdqy,
            ) -> ::core::result::Result<Self, ::codec::Error> {
                match __codec_input_edqy
                    .read_byte()
                    .map_err(|e| {
                        e
                            .chain(
                                "Could not decode `Judgement`, failed to read variant byte",
                            )
                    })?
                {
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 0usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Judgement::<Balance>::Unknown)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 1usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(
                                Judgement::<
                                    Balance,
                                >::FeePaid({
                                    let __codec_res_edqy = <Balance as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Judgement::FeePaid.0`"),
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
                    __codec_x_edqy if __codec_x_edqy
                        == 2usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Judgement::<Balance>::Reasonable)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 3usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Judgement::<Balance>::KnownGood)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 4usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Judgement::<Balance>::OutOfDate)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 5usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Judgement::<Balance>::LowQuality)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 6usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Judgement::<Balance>::Erroneous)
                        })();
                    }
                    _ => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Err(
                                <_ as ::core::convert::Into<
                                    _,
                                >>::into(
                                    "Could not decode `Judgement`, variant doesn't exist",
                                ),
                            )
                        })();
                    }
                }
            }
        }
    };
    #[automatically_derived]
    impl<
        Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq,
    > ::core::marker::StructuralEq for Judgement<Balance> {}
    #[automatically_derived]
    impl<
        Balance: ::core::cmp::Eq + Encode + Decode + MaxEncodedLen + Copy + Clone + Debug
            + Eq + PartialEq,
    > ::core::cmp::Eq for Judgement<Balance> {
        #[inline]
        #[doc(hidden)]
        #[coverage(off)]
        fn assert_receiver_is_total_eq(&self) -> () {
            let _: ::core::cmp::AssertParamIsEq<Balance>;
        }
    }
    #[automatically_derived]
    impl<
        Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq,
    > ::core::marker::StructuralPartialEq for Judgement<Balance> {}
    #[automatically_derived]
    impl<
        Balance: ::core::cmp::PartialEq + Encode + Decode + MaxEncodedLen + Copy + Clone
            + Debug + Eq + PartialEq,
    > ::core::cmp::PartialEq for Judgement<Balance> {
        #[inline]
        fn eq(&self, other: &Judgement<Balance>) -> bool {
            let __self_tag = ::core::intrinsics::discriminant_value(self);
            let __arg1_tag = ::core::intrinsics::discriminant_value(other);
            __self_tag == __arg1_tag
                && match (self, other) {
                    (Judgement::FeePaid(__self_0), Judgement::FeePaid(__arg1_0)) => {
                        *__self_0 == *__arg1_0
                    }
                    _ => true,
                }
        }
    }
    impl<
        Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq,
    > core::fmt::Debug for Judgement<Balance>
    where
        Balance: core::fmt::Debug,
    {
        fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
            match self {
                Self::Unknown => fmt.debug_tuple("Judgement::Unknown").finish(),
                Self::FeePaid(ref a0) => {
                    fmt.debug_tuple("Judgement::FeePaid").field(a0).finish()
                }
                Self::Reasonable => fmt.debug_tuple("Judgement::Reasonable").finish(),
                Self::KnownGood => fmt.debug_tuple("Judgement::KnownGood").finish(),
                Self::OutOfDate => fmt.debug_tuple("Judgement::OutOfDate").finish(),
                Self::LowQuality => fmt.debug_tuple("Judgement::LowQuality").finish(),
                Self::Erroneous => fmt.debug_tuple("Judgement::Erroneous").finish(),
                _ => Ok(()),
            }
        }
    }
    const _: () = {
        impl<
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq,
        > ::codec::MaxEncodedLen for Judgement<Balance>
        where
            Balance: ::codec::MaxEncodedLen,
            Balance: ::codec::MaxEncodedLen,
        {
            fn max_encoded_len() -> ::core::primitive::usize {
                0_usize
                    .max(0_usize)
                    .max(0_usize.saturating_add(<Balance>::max_encoded_len()))
                    .max(0_usize)
                    .max(0_usize)
                    .max(0_usize)
                    .max(0_usize)
                    .max(0_usize)
                    .saturating_add(1)
            }
        }
    };
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        impl<
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq,
        > ::scale_info::TypeInfo for Judgement<Balance>
        where
            Balance: ::scale_info::TypeInfo + 'static,
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq + ::scale_info::TypeInfo + 'static,
        {
            type Identity = Self;
            fn type_info() -> ::scale_info::Type {
                ::scale_info::Type::builder()
                    .path(::scale_info::Path::new("Judgement", "pallet_identity::types"))
                    .type_params(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                ::scale_info::TypeParameter::new(
                                    "Balance",
                                    ::core::option::Option::Some(
                                        ::scale_info::meta_type::<Balance>(),
                                    ),
                                ),
                            ]),
                        ),
                    )
                    .docs(
                        &[
                            "An attestation of a registrar over how accurate some `IdentityInfo` is in describing an account.",
                            "",
                            "NOTE: Registrars may pay little attention to some fields. Registrars may want to make clear",
                            "which fields their attestation is relevant for by off-chain means.",
                        ],
                    )
                    .variant(
                        ::scale_info::build::Variants::new()
                            .variant(
                                "Unknown",
                                |v| {
                                    v
                                        .index(0usize as ::core::primitive::u8)
                                        .docs(&["The default value; no opinion is held."])
                                },
                            )
                            .variant(
                                "FeePaid",
                                |v| {
                                    v
                                        .index(1usize as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::unnamed()
                                                .field(|f| f.ty::<Balance>().type_name("Balance")),
                                        )
                                        .docs(
                                            &[
                                                "No judgement is yet in place, but a deposit is reserved as payment for providing one.",
                                            ],
                                        )
                                },
                            )
                            .variant(
                                "Reasonable",
                                |v| {
                                    v
                                        .index(2usize as ::core::primitive::u8)
                                        .docs(
                                            &[
                                                "The data appears to be reasonably acceptable in terms of its accuracy, however no in depth",
                                                "checks (such as in-person meetings or formal KYC) have been conducted.",
                                            ],
                                        )
                                },
                            )
                            .variant(
                                "KnownGood",
                                |v| {
                                    v
                                        .index(3usize as ::core::primitive::u8)
                                        .docs(
                                            &[
                                                "The target is known directly by the registrar and the registrar can fully attest to the",
                                                "the data's accuracy.",
                                            ],
                                        )
                                },
                            )
                            .variant(
                                "OutOfDate",
                                |v| {
                                    v
                                        .index(4usize as ::core::primitive::u8)
                                        .docs(
                                            &[
                                                "The data was once good but is currently out of date. There is no malicious intent in the",
                                                "inaccuracy. This judgement can be removed through updating the data.",
                                            ],
                                        )
                                },
                            )
                            .variant(
                                "LowQuality",
                                |v| {
                                    v
                                        .index(5usize as ::core::primitive::u8)
                                        .docs(
                                            &[
                                                "The data is imprecise or of sufficiently low-quality to be problematic. It is not",
                                                "indicative of malicious intent. This judgement can be removed through updating the data.",
                                            ],
                                        )
                                },
                            )
                            .variant(
                                "Erroneous",
                                |v| {
                                    v
                                        .index(6usize as ::core::primitive::u8)
                                        .docs(
                                            &[
                                                "The data is erroneous. This may be indicative of malicious intent. This cannot be removed",
                                                "except by the registrar.",
                                            ],
                                        )
                                },
                            ),
                    )
            }
        }
    };
    impl<
        Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq,
    > Judgement<Balance> {
        /// Returns `true` if this judgement is indicative of a deposit being currently held. This means
        /// it should not be cleared or replaced except by an operation which utilizes the deposit.
        pub(crate) fn has_deposit(&self) -> bool {
            match self {
                Judgement::FeePaid(_) => true,
                _ => false,
            }
        }
        /// Returns `true` if this judgement is one that should not be generally be replaced outside
        /// of specialized handlers. Examples include "malicious" judgements and deposit-holding
        /// judgements.
        pub(crate) fn is_sticky(&self) -> bool {
            match self {
                Judgement::FeePaid(_) | Judgement::Erroneous => true,
                _ => false,
            }
        }
    }
    /// The fields that we use to identify the owner of an account with. Each corresponds to a field
    /// in the `IdentityInfo` struct.
    #[repr(u64)]
    pub enum IdentityField {
        Display = 0b0000000000000000000000000000000000000000000000000000000000000001,
        Legal = 0b0000000000000000000000000000000000000000000000000000000000000010,
        Web = 0b0000000000000000000000000000000000000000000000000000000000000100,
        Riot = 0b0000000000000000000000000000000000000000000000000000000000001000,
        Email = 0b0000000000000000000000000000000000000000000000000000000000010000,
        PgpFingerprint = 0b0000000000000000000000000000000000000000000000000000000000100000,
        Image = 0b0000000000000000000000000000000000000000000000000000000001000000,
        Twitter = 0b0000000000000000000000000000000000000000000000000000000010000000,
    }
    #[automatically_derived]
    impl ::core::clone::Clone for IdentityField {
        #[inline]
        fn clone(&self) -> IdentityField {
            *self
        }
    }
    #[automatically_derived]
    impl ::core::marker::Copy for IdentityField {}
    #[automatically_derived]
    impl ::core::marker::StructuralPartialEq for IdentityField {}
    #[automatically_derived]
    impl ::core::cmp::PartialEq for IdentityField {
        #[inline]
        fn eq(&self, other: &IdentityField) -> bool {
            let __self_tag = ::core::intrinsics::discriminant_value(self);
            let __arg1_tag = ::core::intrinsics::discriminant_value(other);
            __self_tag == __arg1_tag
        }
    }
    #[automatically_derived]
    impl ::core::marker::StructuralEq for IdentityField {}
    #[automatically_derived]
    impl ::core::cmp::Eq for IdentityField {
        #[inline]
        #[doc(hidden)]
        #[coverage(off)]
        fn assert_receiver_is_total_eq(&self) -> () {}
    }
    impl core::fmt::Debug for IdentityField {
        fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
            match self {
                Self::Display => fmt.debug_tuple("IdentityField::Display").finish(),
                Self::Legal => fmt.debug_tuple("IdentityField::Legal").finish(),
                Self::Web => fmt.debug_tuple("IdentityField::Web").finish(),
                Self::Riot => fmt.debug_tuple("IdentityField::Riot").finish(),
                Self::Email => fmt.debug_tuple("IdentityField::Email").finish(),
                Self::PgpFingerprint => {
                    fmt.debug_tuple("IdentityField::PgpFingerprint").finish()
                }
                Self::Image => fmt.debug_tuple("IdentityField::Image").finish(),
                Self::Twitter => fmt.debug_tuple("IdentityField::Twitter").finish(),
                _ => Ok(()),
            }
        }
    }
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        impl ::scale_info::TypeInfo for IdentityField {
            type Identity = Self;
            fn type_info() -> ::scale_info::Type {
                ::scale_info::Type::builder()
                    .path(
                        ::scale_info::Path::new(
                            "IdentityField",
                            "pallet_identity::types",
                        ),
                    )
                    .type_params(::alloc::vec::Vec::new())
                    .docs(
                        &[
                            "The fields that we use to identify the owner of an account with. Each corresponds to a field",
                            "in the `IdentityInfo` struct.",
                        ],
                    )
                    .variant(
                        ::scale_info::build::Variants::new()
                            .variant(
                                "Display",
                                |v| {
                                    v
                                        .index(
                                            0b0000000000000000000000000000000000000000000000000000000000000001
                                                as ::core::primitive::u8,
                                        )
                                },
                            )
                            .variant(
                                "Legal",
                                |v| {
                                    v
                                        .index(
                                            0b0000000000000000000000000000000000000000000000000000000000000010
                                                as ::core::primitive::u8,
                                        )
                                },
                            )
                            .variant(
                                "Web",
                                |v| {
                                    v
                                        .index(
                                            0b0000000000000000000000000000000000000000000000000000000000000100
                                                as ::core::primitive::u8,
                                        )
                                },
                            )
                            .variant(
                                "Riot",
                                |v| {
                                    v
                                        .index(
                                            0b0000000000000000000000000000000000000000000000000000000000001000
                                                as ::core::primitive::u8,
                                        )
                                },
                            )
                            .variant(
                                "Email",
                                |v| {
                                    v
                                        .index(
                                            0b0000000000000000000000000000000000000000000000000000000000010000
                                                as ::core::primitive::u8,
                                        )
                                },
                            )
                            .variant(
                                "PgpFingerprint",
                                |v| {
                                    v
                                        .index(
                                            0b0000000000000000000000000000000000000000000000000000000000100000
                                                as ::core::primitive::u8,
                                        )
                                },
                            )
                            .variant(
                                "Image",
                                |v| {
                                    v
                                        .index(
                                            0b0000000000000000000000000000000000000000000000000000000001000000
                                                as ::core::primitive::u8,
                                        )
                                },
                            )
                            .variant(
                                "Twitter",
                                |v| {
                                    v
                                        .index(
                                            0b0000000000000000000000000000000000000000000000000000000010000000
                                                as ::core::primitive::u8,
                                        )
                                },
                            ),
                    )
            }
        }
    };
    impl ::enumflags2::_internal::core::ops::Not for IdentityField {
        type Output = ::enumflags2::BitFlags<Self>;
        #[inline(always)]
        fn not(self) -> Self::Output {
            use ::enumflags2::BitFlags;
            BitFlags::from_flag(self).not()
        }
    }
    impl ::enumflags2::_internal::core::ops::BitOr for IdentityField {
        type Output = ::enumflags2::BitFlags<Self>;
        #[inline(always)]
        fn bitor(self, other: Self) -> Self::Output {
            use ::enumflags2::BitFlags;
            BitFlags::from_flag(self) | other
        }
    }
    impl ::enumflags2::_internal::core::ops::BitAnd for IdentityField {
        type Output = ::enumflags2::BitFlags<Self>;
        #[inline(always)]
        fn bitand(self, other: Self) -> Self::Output {
            use ::enumflags2::BitFlags;
            BitFlags::from_flag(self) & other
        }
    }
    impl ::enumflags2::_internal::core::ops::BitXor for IdentityField {
        type Output = ::enumflags2::BitFlags<Self>;
        #[inline(always)]
        fn bitxor(self, other: Self) -> Self::Output {
            use ::enumflags2::BitFlags;
            BitFlags::from_flag(self) ^ other
        }
    }
    unsafe impl ::enumflags2::_internal::RawBitFlags for IdentityField {
        type Numeric = u64;
        const EMPTY: Self::Numeric = 0;
        const DEFAULT: Self::Numeric = 0;
        const ALL_BITS: Self::Numeric = 0 | (Self::Display as u64) | (Self::Legal as u64)
            | (Self::Web as u64) | (Self::Riot as u64) | (Self::Email as u64)
            | (Self::PgpFingerprint as u64) | (Self::Image as u64)
            | (Self::Twitter as u64);
        const BITFLAGS_TYPE_NAME: &'static str = "BitFlags<IdentityField>";
        fn bits(self) -> Self::Numeric {
            self as u64
        }
    }
    impl ::enumflags2::BitFlag for IdentityField {}
    /// Wrapper type for `BitFlags<IdentityField>` that implements `Codec`.
    pub struct IdentityFields(pub BitFlags<IdentityField>);
    #[automatically_derived]
    impl ::core::clone::Clone for IdentityFields {
        #[inline]
        fn clone(&self) -> IdentityFields {
            let _: ::core::clone::AssertParamIsClone<BitFlags<IdentityField>>;
            *self
        }
    }
    #[automatically_derived]
    impl ::core::marker::Copy for IdentityFields {}
    #[automatically_derived]
    impl ::core::marker::StructuralPartialEq for IdentityFields {}
    #[automatically_derived]
    impl ::core::cmp::PartialEq for IdentityFields {
        #[inline]
        fn eq(&self, other: &IdentityFields) -> bool {
            self.0 == other.0
        }
    }
    #[automatically_derived]
    impl ::core::default::Default for IdentityFields {
        #[inline]
        fn default() -> IdentityFields {
            IdentityFields(::core::default::Default::default())
        }
    }
    impl core::fmt::Debug for IdentityFields {
        fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
            fmt.debug_tuple("IdentityFields").field(&self.0).finish()
        }
    }
    impl MaxEncodedLen for IdentityFields {
        fn max_encoded_len() -> usize {
            u64::max_encoded_len()
        }
    }
    impl Eq for IdentityFields {}
    impl Encode for IdentityFields {
        fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
            self.0.bits().using_encoded(f)
        }
    }
    impl Decode for IdentityFields {
        fn decode<I: codec::Input>(
            input: &mut I,
        ) -> sp_std::result::Result<Self, codec::Error> {
            let field = u64::decode(input)?;
            Ok(
                Self(
                    <BitFlags<IdentityField>>::from_bits(field as u64)
                        .map_err(|_| "invalid value")?,
                ),
            )
        }
    }
    impl TypeInfo for IdentityFields {
        type Identity = Self;
        fn type_info() -> Type {
            Type::builder()
                .path(Path::new("BitFlags", "pallet_identity::types"))
                .type_params(
                    <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            TypeParameter::new("T", Some(meta_type::<IdentityField>())),
                        ]),
                    ),
                )
                .composite(
                    Fields::unnamed().field(|f| f.ty::<u64>().type_name("IdentityField")),
                )
        }
    }
    /// Information concerning the identity of the controller of an account.
    ///
    /// NOTE: This should be stored at the end of the storage item to facilitate the addition of extra
    /// fields in a backwards compatible way through a specialized `Decode` impl.
    #[codec(mel_bound())]
    #[scale_info(skip_type_params(FieldLimit))]
    pub struct IdentityInfo<FieldLimit: Get<u32>> {
        /// Additional fields of the identity that are not catered for with the struct's explicit
        /// fields.
        pub additional: BoundedVec<(Data, Data), FieldLimit>,
        /// A reasonable display name for the controller of the account. This should be whatever it is
        /// that it is typically known as and should not be confusable with other entities, given
        /// reasonable context.
        ///
        /// Stored as UTF-8.
        pub display: Data,
        /// The full legal name in the local jurisdiction of the entity. This might be a bit
        /// long-winded.
        ///
        /// Stored as UTF-8.
        pub legal: Data,
        /// A representative website held by the controller of the account.
        ///
        /// NOTE: `https://` is automatically prepended.
        ///
        /// Stored as UTF-8.
        pub web: Data,
        /// The Riot/Matrix handle held by the controller of the account.
        ///
        /// Stored as UTF-8.
        pub riot: Data,
        /// The email address of the controller of the account.
        ///
        /// Stored as UTF-8.
        pub email: Data,
        /// The PGP/GPG public key of the controller of the account.
        pub pgp_fingerprint: Option<[u8; 20]>,
        /// A graphic image representing the controller of the account. Should be a company,
        /// organization or project logo or a headshot in the case of a human.
        pub image: Data,
        /// The Twitter identity. The leading `@` character may be elided.
        pub twitter: Data,
    }
    const _: () = {
        impl<FieldLimit: Get<u32>> ::core::clone::Clone for IdentityInfo<FieldLimit> {
            fn clone(&self) -> Self {
                Self {
                    additional: ::core::clone::Clone::clone(&self.additional),
                    display: ::core::clone::Clone::clone(&self.display),
                    legal: ::core::clone::Clone::clone(&self.legal),
                    web: ::core::clone::Clone::clone(&self.web),
                    riot: ::core::clone::Clone::clone(&self.riot),
                    email: ::core::clone::Clone::clone(&self.email),
                    pgp_fingerprint: ::core::clone::Clone::clone(&self.pgp_fingerprint),
                    image: ::core::clone::Clone::clone(&self.image),
                    twitter: ::core::clone::Clone::clone(&self.twitter),
                }
            }
        }
    };
    #[allow(deprecated)]
    const _: () = {
        #[automatically_derived]
        impl<FieldLimit: Get<u32>> ::codec::Encode for IdentityInfo<FieldLimit>
        where
            BoundedVec<(Data, Data), FieldLimit>: ::codec::Encode,
            BoundedVec<(Data, Data), FieldLimit>: ::codec::Encode,
        {
            fn size_hint(&self) -> usize {
                0_usize
                    .saturating_add(::codec::Encode::size_hint(&self.additional))
                    .saturating_add(::codec::Encode::size_hint(&self.display))
                    .saturating_add(::codec::Encode::size_hint(&self.legal))
                    .saturating_add(::codec::Encode::size_hint(&self.web))
                    .saturating_add(::codec::Encode::size_hint(&self.riot))
                    .saturating_add(::codec::Encode::size_hint(&self.email))
                    .saturating_add(::codec::Encode::size_hint(&self.pgp_fingerprint))
                    .saturating_add(::codec::Encode::size_hint(&self.image))
                    .saturating_add(::codec::Encode::size_hint(&self.twitter))
            }
            fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
                &self,
                __codec_dest_edqy: &mut __CodecOutputEdqy,
            ) {
                ::codec::Encode::encode_to(&self.additional, __codec_dest_edqy);
                ::codec::Encode::encode_to(&self.display, __codec_dest_edqy);
                ::codec::Encode::encode_to(&self.legal, __codec_dest_edqy);
                ::codec::Encode::encode_to(&self.web, __codec_dest_edqy);
                ::codec::Encode::encode_to(&self.riot, __codec_dest_edqy);
                ::codec::Encode::encode_to(&self.email, __codec_dest_edqy);
                ::codec::Encode::encode_to(&self.pgp_fingerprint, __codec_dest_edqy);
                ::codec::Encode::encode_to(&self.image, __codec_dest_edqy);
                ::codec::Encode::encode_to(&self.twitter, __codec_dest_edqy);
            }
        }
        #[automatically_derived]
        impl<FieldLimit: Get<u32>> ::codec::EncodeLike for IdentityInfo<FieldLimit>
        where
            BoundedVec<(Data, Data), FieldLimit>: ::codec::Encode,
            BoundedVec<(Data, Data), FieldLimit>: ::codec::Encode,
        {}
    };
    #[allow(deprecated)]
    const _: () = {
        #[automatically_derived]
        impl<FieldLimit: Get<u32>> ::codec::Decode for IdentityInfo<FieldLimit>
        where
            BoundedVec<(Data, Data), FieldLimit>: ::codec::Decode,
            BoundedVec<(Data, Data), FieldLimit>: ::codec::Decode,
        {
            fn decode<__CodecInputEdqy: ::codec::Input>(
                __codec_input_edqy: &mut __CodecInputEdqy,
            ) -> ::core::result::Result<Self, ::codec::Error> {
                ::core::result::Result::Ok(IdentityInfo::<FieldLimit> {
                    additional: {
                        let __codec_res_edqy = <BoundedVec<
                            (Data, Data),
                            FieldLimit,
                        > as ::codec::Decode>::decode(__codec_input_edqy);
                        match __codec_res_edqy {
                            ::core::result::Result::Err(e) => {
                                return ::core::result::Result::Err(
                                    e.chain("Could not decode `IdentityInfo::additional`"),
                                );
                            }
                            ::core::result::Result::Ok(__codec_res_edqy) => {
                                __codec_res_edqy
                            }
                        }
                    },
                    display: {
                        let __codec_res_edqy = <Data as ::codec::Decode>::decode(
                            __codec_input_edqy,
                        );
                        match __codec_res_edqy {
                            ::core::result::Result::Err(e) => {
                                return ::core::result::Result::Err(
                                    e.chain("Could not decode `IdentityInfo::display`"),
                                );
                            }
                            ::core::result::Result::Ok(__codec_res_edqy) => {
                                __codec_res_edqy
                            }
                        }
                    },
                    legal: {
                        let __codec_res_edqy = <Data as ::codec::Decode>::decode(
                            __codec_input_edqy,
                        );
                        match __codec_res_edqy {
                            ::core::result::Result::Err(e) => {
                                return ::core::result::Result::Err(
                                    e.chain("Could not decode `IdentityInfo::legal`"),
                                );
                            }
                            ::core::result::Result::Ok(__codec_res_edqy) => {
                                __codec_res_edqy
                            }
                        }
                    },
                    web: {
                        let __codec_res_edqy = <Data as ::codec::Decode>::decode(
                            __codec_input_edqy,
                        );
                        match __codec_res_edqy {
                            ::core::result::Result::Err(e) => {
                                return ::core::result::Result::Err(
                                    e.chain("Could not decode `IdentityInfo::web`"),
                                );
                            }
                            ::core::result::Result::Ok(__codec_res_edqy) => {
                                __codec_res_edqy
                            }
                        }
                    },
                    riot: {
                        let __codec_res_edqy = <Data as ::codec::Decode>::decode(
                            __codec_input_edqy,
                        );
                        match __codec_res_edqy {
                            ::core::result::Result::Err(e) => {
                                return ::core::result::Result::Err(
                                    e.chain("Could not decode `IdentityInfo::riot`"),
                                );
                            }
                            ::core::result::Result::Ok(__codec_res_edqy) => {
                                __codec_res_edqy
                            }
                        }
                    },
                    email: {
                        let __codec_res_edqy = <Data as ::codec::Decode>::decode(
                            __codec_input_edqy,
                        );
                        match __codec_res_edqy {
                            ::core::result::Result::Err(e) => {
                                return ::core::result::Result::Err(
                                    e.chain("Could not decode `IdentityInfo::email`"),
                                );
                            }
                            ::core::result::Result::Ok(__codec_res_edqy) => {
                                __codec_res_edqy
                            }
                        }
                    },
                    pgp_fingerprint: {
                        let __codec_res_edqy = <Option<
                            [u8; 20],
                        > as ::codec::Decode>::decode(__codec_input_edqy);
                        match __codec_res_edqy {
                            ::core::result::Result::Err(e) => {
                                return ::core::result::Result::Err(
                                    e.chain("Could not decode `IdentityInfo::pgp_fingerprint`"),
                                );
                            }
                            ::core::result::Result::Ok(__codec_res_edqy) => {
                                __codec_res_edqy
                            }
                        }
                    },
                    image: {
                        let __codec_res_edqy = <Data as ::codec::Decode>::decode(
                            __codec_input_edqy,
                        );
                        match __codec_res_edqy {
                            ::core::result::Result::Err(e) => {
                                return ::core::result::Result::Err(
                                    e.chain("Could not decode `IdentityInfo::image`"),
                                );
                            }
                            ::core::result::Result::Ok(__codec_res_edqy) => {
                                __codec_res_edqy
                            }
                        }
                    },
                    twitter: {
                        let __codec_res_edqy = <Data as ::codec::Decode>::decode(
                            __codec_input_edqy,
                        );
                        match __codec_res_edqy {
                            ::core::result::Result::Err(e) => {
                                return ::core::result::Result::Err(
                                    e.chain("Could not decode `IdentityInfo::twitter`"),
                                );
                            }
                            ::core::result::Result::Ok(__codec_res_edqy) => {
                                __codec_res_edqy
                            }
                        }
                    },
                })
            }
        }
    };
    #[automatically_derived]
    impl<FieldLimit: Get<u32>> ::core::marker::StructuralEq
    for IdentityInfo<FieldLimit> {}
    #[automatically_derived]
    impl<FieldLimit: ::core::cmp::Eq + Get<u32>> ::core::cmp::Eq
    for IdentityInfo<FieldLimit> {
        #[inline]
        #[doc(hidden)]
        #[coverage(off)]
        fn assert_receiver_is_total_eq(&self) -> () {
            let _: ::core::cmp::AssertParamIsEq<BoundedVec<(Data, Data), FieldLimit>>;
            let _: ::core::cmp::AssertParamIsEq<Data>;
            let _: ::core::cmp::AssertParamIsEq<Option<[u8; 20]>>;
        }
    }
    const _: () = {
        impl<FieldLimit: Get<u32>> ::codec::MaxEncodedLen for IdentityInfo<FieldLimit> {
            fn max_encoded_len() -> ::core::primitive::usize {
                0_usize
                    .saturating_add(
                        <BoundedVec<(Data, Data), FieldLimit>>::max_encoded_len(),
                    )
                    .saturating_add(<Data>::max_encoded_len())
                    .saturating_add(<Data>::max_encoded_len())
                    .saturating_add(<Data>::max_encoded_len())
                    .saturating_add(<Data>::max_encoded_len())
                    .saturating_add(<Data>::max_encoded_len())
                    .saturating_add(<Option<[u8; 20]>>::max_encoded_len())
                    .saturating_add(<Data>::max_encoded_len())
                    .saturating_add(<Data>::max_encoded_len())
            }
        }
    };
    const _: () = {
        impl<FieldLimit: Get<u32>> ::core::cmp::PartialEq for IdentityInfo<FieldLimit> {
            fn eq(&self, other: &Self) -> bool {
                true && self.additional == other.additional
                    && self.display == other.display && self.legal == other.legal
                    && self.web == other.web && self.riot == other.riot
                    && self.email == other.email
                    && self.pgp_fingerprint == other.pgp_fingerprint
                    && self.image == other.image && self.twitter == other.twitter
            }
        }
    };
    const _: () = {
        impl<FieldLimit: Get<u32>> ::core::fmt::Debug for IdentityInfo<FieldLimit> {
            fn fmt(&self, fmt: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                fmt.debug_struct("IdentityInfo")
                    .field("additional", &self.additional)
                    .field("display", &self.display)
                    .field("legal", &self.legal)
                    .field("web", &self.web)
                    .field("riot", &self.riot)
                    .field("email", &self.email)
                    .field("pgp_fingerprint", &self.pgp_fingerprint)
                    .field("image", &self.image)
                    .field("twitter", &self.twitter)
                    .finish()
            }
        }
    };
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        impl<FieldLimit: Get<u32>> ::scale_info::TypeInfo for IdentityInfo<FieldLimit>
        where
            BoundedVec<(Data, Data), FieldLimit>: ::scale_info::TypeInfo + 'static,
            FieldLimit: Get<u32> + 'static,
        {
            type Identity = Self;
            fn type_info() -> ::scale_info::Type {
                ::scale_info::Type::builder()
                    .path(
                        ::scale_info::Path::new("IdentityInfo", "pallet_identity::types"),
                    )
                    .type_params(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                ::scale_info::TypeParameter::new(
                                    "FieldLimit",
                                    ::core::option::Option::None,
                                ),
                            ]),
                        ),
                    )
                    .docs(
                        &[
                            "Information concerning the identity of the controller of an account.",
                            "",
                            "NOTE: This should be stored at the end of the storage item to facilitate the addition of extra",
                            "fields in a backwards compatible way through a specialized `Decode` impl.",
                        ],
                    )
                    .composite(
                        ::scale_info::build::Fields::named()
                            .field(|f| {
                                f
                                    .ty::<BoundedVec<(Data, Data), FieldLimit>>()
                                    .name("additional")
                                    .type_name("BoundedVec<(Data, Data), FieldLimit>")
                                    .docs(
                                        &[
                                            "Additional fields of the identity that are not catered for with the struct's explicit",
                                            "fields.",
                                        ],
                                    )
                            })
                            .field(|f| {
                                f
                                    .ty::<Data>()
                                    .name("display")
                                    .type_name("Data")
                                    .docs(
                                        &[
                                            "A reasonable display name for the controller of the account. This should be whatever it is",
                                            "that it is typically known as and should not be confusable with other entities, given",
                                            "reasonable context.",
                                            "",
                                            "Stored as UTF-8.",
                                        ],
                                    )
                            })
                            .field(|f| {
                                f
                                    .ty::<Data>()
                                    .name("legal")
                                    .type_name("Data")
                                    .docs(
                                        &[
                                            "The full legal name in the local jurisdiction of the entity. This might be a bit",
                                            "long-winded.",
                                            "",
                                            "Stored as UTF-8.",
                                        ],
                                    )
                            })
                            .field(|f| {
                                f
                                    .ty::<Data>()
                                    .name("web")
                                    .type_name("Data")
                                    .docs(
                                        &[
                                            "A representative website held by the controller of the account.",
                                            "",
                                            "NOTE: `https://` is automatically prepended.",
                                            "",
                                            "Stored as UTF-8.",
                                        ],
                                    )
                            })
                            .field(|f| {
                                f
                                    .ty::<Data>()
                                    .name("riot")
                                    .type_name("Data")
                                    .docs(
                                        &[
                                            "The Riot/Matrix handle held by the controller of the account.",
                                            "",
                                            "Stored as UTF-8.",
                                        ],
                                    )
                            })
                            .field(|f| {
                                f
                                    .ty::<Data>()
                                    .name("email")
                                    .type_name("Data")
                                    .docs(
                                        &[
                                            "The email address of the controller of the account.",
                                            "",
                                            "Stored as UTF-8.",
                                        ],
                                    )
                            })
                            .field(|f| {
                                f
                                    .ty::<Option<[u8; 20]>>()
                                    .name("pgp_fingerprint")
                                    .type_name("Option<[u8; 20]>")
                                    .docs(
                                        &[
                                            "The PGP/GPG public key of the controller of the account.",
                                        ],
                                    )
                            })
                            .field(|f| {
                                f
                                    .ty::<Data>()
                                    .name("image")
                                    .type_name("Data")
                                    .docs(
                                        &[
                                            "A graphic image representing the controller of the account. Should be a company,",
                                            "organization or project logo or a headshot in the case of a human.",
                                        ],
                                    )
                            })
                            .field(|f| {
                                f
                                    .ty::<Data>()
                                    .name("twitter")
                                    .type_name("Data")
                                    .docs(
                                        &[
                                            "The Twitter identity. The leading `@` character may be elided.",
                                        ],
                                    )
                            }),
                    )
            }
        }
    };
    impl<FieldLimit: Get<u32>> IdentityInfo<FieldLimit> {
        pub(crate) fn fields(&self) -> IdentityFields {
            let mut res = <BitFlags<IdentityField>>::empty();
            if !self.display.is_none() {
                res.insert(IdentityField::Display);
            }
            if !self.legal.is_none() {
                res.insert(IdentityField::Legal);
            }
            if !self.web.is_none() {
                res.insert(IdentityField::Web);
            }
            if !self.riot.is_none() {
                res.insert(IdentityField::Riot);
            }
            if !self.email.is_none() {
                res.insert(IdentityField::Email);
            }
            if self.pgp_fingerprint.is_some() {
                res.insert(IdentityField::PgpFingerprint);
            }
            if !self.image.is_none() {
                res.insert(IdentityField::Image);
            }
            if !self.twitter.is_none() {
                res.insert(IdentityField::Twitter);
            }
            IdentityFields(res)
        }
    }
    /// Information concerning the identity of the controller of an account.
    ///
    /// NOTE: This is stored separately primarily to facilitate the addition of extra fields in a
    /// backwards compatible way through a specialized `Decode` impl.
    #[codec(mel_bound())]
    #[scale_info(skip_type_params(MaxJudgements, MaxAdditionalFields))]
    pub struct Registration<
        Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq,
        MaxJudgements: Get<u32>,
        MaxAdditionalFields: Get<u32>,
    > {
        /// Judgements from the registrars on this identity. Stored ordered by `RegistrarIndex`. There
        /// may be only a single judgement from each registrar.
        pub judgements: BoundedVec<(RegistrarIndex, Judgement<Balance>), MaxJudgements>,
        /// Amount held on deposit for this information.
        pub deposit: Balance,
        /// Information on the identity.
        pub info: IdentityInfo<MaxAdditionalFields>,
    }
    const _: () = {
        impl<
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq,
            MaxJudgements: Get<u32>,
            MaxAdditionalFields: Get<u32>,
        > ::core::clone::Clone
        for Registration<Balance, MaxJudgements, MaxAdditionalFields> {
            fn clone(&self) -> Self {
                Self {
                    judgements: ::core::clone::Clone::clone(&self.judgements),
                    deposit: ::core::clone::Clone::clone(&self.deposit),
                    info: ::core::clone::Clone::clone(&self.info),
                }
            }
        }
    };
    #[allow(deprecated)]
    const _: () = {
        #[automatically_derived]
        impl<
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq,
            MaxJudgements: Get<u32>,
            MaxAdditionalFields: Get<u32>,
        > ::codec::Encode for Registration<Balance, MaxJudgements, MaxAdditionalFields>
        where
            BoundedVec<
                (RegistrarIndex, Judgement<Balance>),
                MaxJudgements,
            >: ::codec::Encode,
            BoundedVec<
                (RegistrarIndex, Judgement<Balance>),
                MaxJudgements,
            >: ::codec::Encode,
            Balance: ::codec::Encode,
            Balance: ::codec::Encode,
            IdentityInfo<MaxAdditionalFields>: ::codec::Encode,
            IdentityInfo<MaxAdditionalFields>: ::codec::Encode,
        {
            fn size_hint(&self) -> usize {
                0_usize
                    .saturating_add(::codec::Encode::size_hint(&self.judgements))
                    .saturating_add(::codec::Encode::size_hint(&self.deposit))
                    .saturating_add(::codec::Encode::size_hint(&self.info))
            }
            fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
                &self,
                __codec_dest_edqy: &mut __CodecOutputEdqy,
            ) {
                ::codec::Encode::encode_to(&self.judgements, __codec_dest_edqy);
                ::codec::Encode::encode_to(&self.deposit, __codec_dest_edqy);
                ::codec::Encode::encode_to(&self.info, __codec_dest_edqy);
            }
        }
        #[automatically_derived]
        impl<
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq,
            MaxJudgements: Get<u32>,
            MaxAdditionalFields: Get<u32>,
        > ::codec::EncodeLike
        for Registration<Balance, MaxJudgements, MaxAdditionalFields>
        where
            BoundedVec<
                (RegistrarIndex, Judgement<Balance>),
                MaxJudgements,
            >: ::codec::Encode,
            BoundedVec<
                (RegistrarIndex, Judgement<Balance>),
                MaxJudgements,
            >: ::codec::Encode,
            Balance: ::codec::Encode,
            Balance: ::codec::Encode,
            IdentityInfo<MaxAdditionalFields>: ::codec::Encode,
            IdentityInfo<MaxAdditionalFields>: ::codec::Encode,
        {}
    };
    #[automatically_derived]
    impl<
        Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq,
        MaxJudgements: Get<u32>,
        MaxAdditionalFields: Get<u32>,
    > ::core::marker::StructuralEq
    for Registration<Balance, MaxJudgements, MaxAdditionalFields> {}
    #[automatically_derived]
    impl<
        Balance: ::core::cmp::Eq + Encode + Decode + MaxEncodedLen + Copy + Clone + Debug
            + Eq + PartialEq,
        MaxJudgements: ::core::cmp::Eq + Get<u32>,
        MaxAdditionalFields: ::core::cmp::Eq + Get<u32>,
    > ::core::cmp::Eq for Registration<Balance, MaxJudgements, MaxAdditionalFields> {
        #[inline]
        #[doc(hidden)]
        #[coverage(off)]
        fn assert_receiver_is_total_eq(&self) -> () {
            let _: ::core::cmp::AssertParamIsEq<
                BoundedVec<(RegistrarIndex, Judgement<Balance>), MaxJudgements>,
            >;
            let _: ::core::cmp::AssertParamIsEq<Balance>;
            let _: ::core::cmp::AssertParamIsEq<IdentityInfo<MaxAdditionalFields>>;
        }
    }
    const _: () = {
        impl<
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq,
            MaxJudgements: Get<u32>,
            MaxAdditionalFields: Get<u32>,
        > ::codec::MaxEncodedLen
        for Registration<Balance, MaxJudgements, MaxAdditionalFields> {
            fn max_encoded_len() -> ::core::primitive::usize {
                0_usize
                    .saturating_add(
                        <BoundedVec<
                            (RegistrarIndex, Judgement<Balance>),
                            MaxJudgements,
                        >>::max_encoded_len(),
                    )
                    .saturating_add(<Balance>::max_encoded_len())
                    .saturating_add(
                        <IdentityInfo<MaxAdditionalFields>>::max_encoded_len(),
                    )
            }
        }
    };
    const _: () = {
        impl<
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq,
            MaxJudgements: Get<u32>,
            MaxAdditionalFields: Get<u32>,
        > ::core::cmp::PartialEq
        for Registration<Balance, MaxJudgements, MaxAdditionalFields> {
            fn eq(&self, other: &Self) -> bool {
                true && self.judgements == other.judgements
                    && self.deposit == other.deposit && self.info == other.info
            }
        }
    };
    const _: () = {
        impl<
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq,
            MaxJudgements: Get<u32>,
            MaxAdditionalFields: Get<u32>,
        > ::core::fmt::Debug
        for Registration<Balance, MaxJudgements, MaxAdditionalFields> {
            fn fmt(&self, fmt: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                fmt.debug_struct("Registration")
                    .field("judgements", &self.judgements)
                    .field("deposit", &self.deposit)
                    .field("info", &self.info)
                    .finish()
            }
        }
    };
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        impl<
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq,
            MaxJudgements: Get<u32>,
            MaxAdditionalFields: Get<u32>,
        > ::scale_info::TypeInfo
        for Registration<Balance, MaxJudgements, MaxAdditionalFields>
        where
            BoundedVec<
                (RegistrarIndex, Judgement<Balance>),
                MaxJudgements,
            >: ::scale_info::TypeInfo + 'static,
            Balance: ::scale_info::TypeInfo + 'static,
            IdentityInfo<MaxAdditionalFields>: ::scale_info::TypeInfo + 'static,
            Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq
                + PartialEq + ::scale_info::TypeInfo + 'static,
            MaxJudgements: Get<u32> + 'static,
            MaxAdditionalFields: Get<u32> + 'static,
        {
            type Identity = Self;
            fn type_info() -> ::scale_info::Type {
                ::scale_info::Type::builder()
                    .path(
                        ::scale_info::Path::new("Registration", "pallet_identity::types"),
                    )
                    .type_params(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                ::scale_info::TypeParameter::new(
                                    "Balance",
                                    ::core::option::Option::Some(
                                        ::scale_info::meta_type::<Balance>(),
                                    ),
                                ),
                                ::scale_info::TypeParameter::new(
                                    "MaxJudgements",
                                    ::core::option::Option::None,
                                ),
                                ::scale_info::TypeParameter::new(
                                    "MaxAdditionalFields",
                                    ::core::option::Option::None,
                                ),
                            ]),
                        ),
                    )
                    .docs(
                        &[
                            "Information concerning the identity of the controller of an account.",
                            "",
                            "NOTE: This is stored separately primarily to facilitate the addition of extra fields in a",
                            "backwards compatible way through a specialized `Decode` impl.",
                        ],
                    )
                    .composite(
                        ::scale_info::build::Fields::named()
                            .field(|f| {
                                f
                                    .ty::<
                                        BoundedVec<
                                            (RegistrarIndex, Judgement<Balance>),
                                            MaxJudgements,
                                        >,
                                    >()
                                    .name("judgements")
                                    .type_name(
                                        "BoundedVec<(RegistrarIndex, Judgement<Balance>), MaxJudgements>",
                                    )
                                    .docs(
                                        &[
                                            "Judgements from the registrars on this identity. Stored ordered by `RegistrarIndex`. There",
                                            "may be only a single judgement from each registrar.",
                                        ],
                                    )
                            })
                            .field(|f| {
                                f
                                    .ty::<Balance>()
                                    .name("deposit")
                                    .type_name("Balance")
                                    .docs(&["Amount held on deposit for this information."])
                            })
                            .field(|f| {
                                f
                                    .ty::<IdentityInfo<MaxAdditionalFields>>()
                                    .name("info")
                                    .type_name("IdentityInfo<MaxAdditionalFields>")
                                    .docs(&["Information on the identity."])
                            }),
                    )
            }
        }
    };
    impl<
        Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq
            + Zero + Add,
        MaxJudgements: Get<u32>,
        MaxAdditionalFields: Get<u32>,
    > Registration<Balance, MaxJudgements, MaxAdditionalFields> {
        pub(crate) fn total_deposit(&self) -> Balance {
            self.deposit
                + self
                    .judgements
                    .iter()
                    .map(|(_, ref j)| {
                        if let Judgement::FeePaid(fee) = j { *fee } else { Zero::zero() }
                    })
                    .fold(Zero::zero(), |a, i| a + i)
        }
    }
    impl<
        Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq,
        MaxJudgements: Get<u32>,
        MaxAdditionalFields: Get<u32>,
    > Decode for Registration<Balance, MaxJudgements, MaxAdditionalFields> {
        fn decode<I: codec::Input>(
            input: &mut I,
        ) -> sp_std::result::Result<Self, codec::Error> {
            let (judgements, deposit, info) = Decode::decode(
                &mut AppendZerosInput::new(input),
            )?;
            Ok(Self { judgements, deposit, info })
        }
    }
    /// Information concerning a registrar.
    pub struct RegistrarInfo<
        Balance: Encode + Decode + Clone + Debug + Eq + PartialEq,
        AccountId: Encode + Decode + Clone + Debug + Eq + PartialEq,
    > {
        /// The account of the registrar.
        pub account: AccountId,
        /// Amount required to be given to the registrar for them to provide judgement.
        pub fee: Balance,
        /// Relevant fields for this registrar. Registrar judgements are limited to attestations on
        /// these fields.
        pub fields: IdentityFields,
    }
    #[automatically_derived]
    impl<
        Balance: ::core::clone::Clone + Encode + Decode + Clone + Debug + Eq + PartialEq,
        AccountId: ::core::clone::Clone + Encode + Decode + Clone + Debug + Eq
            + PartialEq,
    > ::core::clone::Clone for RegistrarInfo<Balance, AccountId> {
        #[inline]
        fn clone(&self) -> RegistrarInfo<Balance, AccountId> {
            RegistrarInfo {
                account: ::core::clone::Clone::clone(&self.account),
                fee: ::core::clone::Clone::clone(&self.fee),
                fields: ::core::clone::Clone::clone(&self.fields),
            }
        }
    }
    #[allow(deprecated)]
    const _: () = {
        #[automatically_derived]
        impl<
            Balance: Encode + Decode + Clone + Debug + Eq + PartialEq,
            AccountId: Encode + Decode + Clone + Debug + Eq + PartialEq,
        > ::codec::Encode for RegistrarInfo<Balance, AccountId>
        where
            AccountId: ::codec::Encode,
            AccountId: ::codec::Encode,
            Balance: ::codec::Encode,
            Balance: ::codec::Encode,
        {
            fn size_hint(&self) -> usize {
                0_usize
                    .saturating_add(::codec::Encode::size_hint(&self.account))
                    .saturating_add(::codec::Encode::size_hint(&self.fee))
                    .saturating_add(::codec::Encode::size_hint(&self.fields))
            }
            fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
                &self,
                __codec_dest_edqy: &mut __CodecOutputEdqy,
            ) {
                ::codec::Encode::encode_to(&self.account, __codec_dest_edqy);
                ::codec::Encode::encode_to(&self.fee, __codec_dest_edqy);
                ::codec::Encode::encode_to(&self.fields, __codec_dest_edqy);
            }
        }
        #[automatically_derived]
        impl<
            Balance: Encode + Decode + Clone + Debug + Eq + PartialEq,
            AccountId: Encode + Decode + Clone + Debug + Eq + PartialEq,
        > ::codec::EncodeLike for RegistrarInfo<Balance, AccountId>
        where
            AccountId: ::codec::Encode,
            AccountId: ::codec::Encode,
            Balance: ::codec::Encode,
            Balance: ::codec::Encode,
        {}
    };
    #[allow(deprecated)]
    const _: () = {
        #[automatically_derived]
        impl<
            Balance: Encode + Decode + Clone + Debug + Eq + PartialEq,
            AccountId: Encode + Decode + Clone + Debug + Eq + PartialEq,
        > ::codec::Decode for RegistrarInfo<Balance, AccountId>
        where
            AccountId: ::codec::Decode,
            AccountId: ::codec::Decode,
            Balance: ::codec::Decode,
            Balance: ::codec::Decode,
        {
            fn decode<__CodecInputEdqy: ::codec::Input>(
                __codec_input_edqy: &mut __CodecInputEdqy,
            ) -> ::core::result::Result<Self, ::codec::Error> {
                ::core::result::Result::Ok(RegistrarInfo::<Balance, AccountId> {
                    account: {
                        let __codec_res_edqy = <AccountId as ::codec::Decode>::decode(
                            __codec_input_edqy,
                        );
                        match __codec_res_edqy {
                            ::core::result::Result::Err(e) => {
                                return ::core::result::Result::Err(
                                    e.chain("Could not decode `RegistrarInfo::account`"),
                                );
                            }
                            ::core::result::Result::Ok(__codec_res_edqy) => {
                                __codec_res_edqy
                            }
                        }
                    },
                    fee: {
                        let __codec_res_edqy = <Balance as ::codec::Decode>::decode(
                            __codec_input_edqy,
                        );
                        match __codec_res_edqy {
                            ::core::result::Result::Err(e) => {
                                return ::core::result::Result::Err(
                                    e.chain("Could not decode `RegistrarInfo::fee`"),
                                );
                            }
                            ::core::result::Result::Ok(__codec_res_edqy) => {
                                __codec_res_edqy
                            }
                        }
                    },
                    fields: {
                        let __codec_res_edqy = <IdentityFields as ::codec::Decode>::decode(
                            __codec_input_edqy,
                        );
                        match __codec_res_edqy {
                            ::core::result::Result::Err(e) => {
                                return ::core::result::Result::Err(
                                    e.chain("Could not decode `RegistrarInfo::fields`"),
                                );
                            }
                            ::core::result::Result::Ok(__codec_res_edqy) => {
                                __codec_res_edqy
                            }
                        }
                    },
                })
            }
        }
    };
    #[automatically_derived]
    impl<
        Balance: Encode + Decode + Clone + Debug + Eq + PartialEq,
        AccountId: Encode + Decode + Clone + Debug + Eq + PartialEq,
    > ::core::marker::StructuralEq for RegistrarInfo<Balance, AccountId> {}
    #[automatically_derived]
    impl<
        Balance: ::core::cmp::Eq + Encode + Decode + Clone + Debug + Eq + PartialEq,
        AccountId: ::core::cmp::Eq + Encode + Decode + Clone + Debug + Eq + PartialEq,
    > ::core::cmp::Eq for RegistrarInfo<Balance, AccountId> {
        #[inline]
        #[doc(hidden)]
        #[coverage(off)]
        fn assert_receiver_is_total_eq(&self) -> () {
            let _: ::core::cmp::AssertParamIsEq<AccountId>;
            let _: ::core::cmp::AssertParamIsEq<Balance>;
            let _: ::core::cmp::AssertParamIsEq<IdentityFields>;
        }
    }
    #[automatically_derived]
    impl<
        Balance: Encode + Decode + Clone + Debug + Eq + PartialEq,
        AccountId: Encode + Decode + Clone + Debug + Eq + PartialEq,
    > ::core::marker::StructuralPartialEq for RegistrarInfo<Balance, AccountId> {}
    #[automatically_derived]
    impl<
        Balance: ::core::cmp::PartialEq + Encode + Decode + Clone + Debug + Eq
            + PartialEq,
        AccountId: ::core::cmp::PartialEq + Encode + Decode + Clone + Debug + Eq
            + PartialEq,
    > ::core::cmp::PartialEq for RegistrarInfo<Balance, AccountId> {
        #[inline]
        fn eq(&self, other: &RegistrarInfo<Balance, AccountId>) -> bool {
            self.account == other.account && self.fee == other.fee
                && self.fields == other.fields
        }
    }
    impl<
        Balance: Encode + Decode + Clone + Debug + Eq + PartialEq,
        AccountId: Encode + Decode + Clone + Debug + Eq + PartialEq,
    > core::fmt::Debug for RegistrarInfo<Balance, AccountId>
    where
        Balance: core::fmt::Debug,
        AccountId: core::fmt::Debug,
    {
        fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
            fmt.debug_struct("RegistrarInfo")
                .field("account", &self.account)
                .field("fee", &self.fee)
                .field("fields", &self.fields)
                .finish()
        }
    }
    const _: () = {
        impl<
            Balance: Encode + Decode + Clone + Debug + Eq + PartialEq,
            AccountId: Encode + Decode + Clone + Debug + Eq + PartialEq,
        > ::codec::MaxEncodedLen for RegistrarInfo<Balance, AccountId>
        where
            AccountId: ::codec::MaxEncodedLen,
            AccountId: ::codec::MaxEncodedLen,
            Balance: ::codec::MaxEncodedLen,
            Balance: ::codec::MaxEncodedLen,
        {
            fn max_encoded_len() -> ::core::primitive::usize {
                0_usize
                    .saturating_add(<AccountId>::max_encoded_len())
                    .saturating_add(<Balance>::max_encoded_len())
                    .saturating_add(<IdentityFields>::max_encoded_len())
            }
        }
    };
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        impl<
            Balance: Encode + Decode + Clone + Debug + Eq + PartialEq,
            AccountId: Encode + Decode + Clone + Debug + Eq + PartialEq,
        > ::scale_info::TypeInfo for RegistrarInfo<Balance, AccountId>
        where
            AccountId: ::scale_info::TypeInfo + 'static,
            Balance: ::scale_info::TypeInfo + 'static,
            Balance: Encode + Decode + Clone + Debug + Eq + PartialEq
                + ::scale_info::TypeInfo + 'static,
            AccountId: Encode + Decode + Clone + Debug + Eq + PartialEq
                + ::scale_info::TypeInfo + 'static,
        {
            type Identity = Self;
            fn type_info() -> ::scale_info::Type {
                ::scale_info::Type::builder()
                    .path(
                        ::scale_info::Path::new(
                            "RegistrarInfo",
                            "pallet_identity::types",
                        ),
                    )
                    .type_params(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                ::scale_info::TypeParameter::new(
                                    "Balance",
                                    ::core::option::Option::Some(
                                        ::scale_info::meta_type::<Balance>(),
                                    ),
                                ),
                                ::scale_info::TypeParameter::new(
                                    "AccountId",
                                    ::core::option::Option::Some(
                                        ::scale_info::meta_type::<AccountId>(),
                                    ),
                                ),
                            ]),
                        ),
                    )
                    .docs(&["Information concerning a registrar."])
                    .composite(
                        ::scale_info::build::Fields::named()
                            .field(|f| {
                                f
                                    .ty::<AccountId>()
                                    .name("account")
                                    .type_name("AccountId")
                                    .docs(&["The account of the registrar."])
                            })
                            .field(|f| {
                                f
                                    .ty::<Balance>()
                                    .name("fee")
                                    .type_name("Balance")
                                    .docs(
                                        &[
                                            "Amount required to be given to the registrar for them to provide judgement.",
                                        ],
                                    )
                            })
                            .field(|f| {
                                f
                                    .ty::<IdentityFields>()
                                    .name("fields")
                                    .type_name("IdentityFields")
                                    .docs(
                                        &[
                                            "Relevant fields for this registrar. Registrar judgements are limited to attestations on",
                                            "these fields.",
                                        ],
                                    )
                            }),
                    )
            }
        }
    };
}
pub mod weights {
    //! Autogenerated weights for pallet_identity
    //!
    //! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
    //! DATE: 2023-06-16, STEPS: `50`, REPEAT: `20`, LOW RANGE: `[]`, HIGH RANGE: `[]`
    //! WORST CASE MAP SIZE: `1000000`
    //! HOSTNAME: `runner-e8ezs4ez-project-145-concurrent-0`, CPU: `Intel(R) Xeon(R) CPU @ 2.60GHz`
    //! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: Some("dev"), DB CACHE: 1024
    #![allow(unused_parens)]
    #![allow(unused_imports)]
    #![allow(missing_docs)]
    use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
    use core::marker::PhantomData;
    /// Weight functions needed for pallet_identity.
    pub trait WeightInfo {
        fn add_registrar(r: u32) -> Weight;
        fn set_identity(r: u32, x: u32) -> Weight;
        fn set_subs_new(s: u32) -> Weight;
        fn set_subs_old(p: u32) -> Weight;
        fn clear_identity(r: u32, s: u32, x: u32) -> Weight;
        fn request_judgement(r: u32, x: u32) -> Weight;
        fn cancel_request(r: u32, x: u32) -> Weight;
        fn set_fee(r: u32) -> Weight;
        fn set_account_id(r: u32) -> Weight;
        fn set_fields(r: u32) -> Weight;
        fn provide_judgement(r: u32, x: u32) -> Weight;
        fn kill_identity(r: u32, s: u32, x: u32) -> Weight;
        fn add_sub(s: u32) -> Weight;
        fn rename_sub(s: u32) -> Weight;
        fn remove_sub(s: u32) -> Weight;
        fn quit_sub(s: u32) -> Weight;
    }
    /// Weights for pallet_identity using the Substrate node and recommended hardware.
    pub struct SubstrateWeight<T>(PhantomData<T>);
    impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
        /// Storage: Identity Registrars (r:1 w:1)
        /// Proof: Identity Registrars (max_values: Some(1), max_size: Some(1141), added: 1636, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 19]`.
        fn add_registrar(r: u32) -> Weight {
            Weight::from_parts(12_515_830, 2626)
                .saturating_add(Weight::from_parts(147_919, 0).saturating_mul(r.into()))
                .saturating_add(T::DbWeight::get().reads(1_u64))
                .saturating_add(T::DbWeight::get().writes(1_u64))
        }
        /// Storage: Identity IdentityOf (r:1 w:1)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 20]`.
        /// The range of component `x` is `[0, 100]`.
        fn set_identity(r: u32, x: u32) -> Weight {
            Weight::from_parts(31_329_634, 11003)
                .saturating_add(Weight::from_parts(203_570, 0).saturating_mul(r.into()))
                .saturating_add(Weight::from_parts(429_346, 0).saturating_mul(x.into()))
                .saturating_add(T::DbWeight::get().reads(1_u64))
                .saturating_add(T::DbWeight::get().writes(1_u64))
        }
        /// Storage: Identity IdentityOf (r:1 w:0)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:100 w:100)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// The range of component `s` is `[0, 100]`.
        fn set_subs_new(s: u32) -> Weight {
            Weight::from_parts(24_917_444, 11003)
                .saturating_add(
                    Weight::from_parts(3_279_868, 0).saturating_mul(s.into()),
                )
                .saturating_add(T::DbWeight::get().reads(2_u64))
                .saturating_add(
                    T::DbWeight::get().reads((1_u64).saturating_mul(s.into())),
                )
                .saturating_add(T::DbWeight::get().writes(1_u64))
                .saturating_add(
                    T::DbWeight::get().writes((1_u64).saturating_mul(s.into())),
                )
                .saturating_add(Weight::from_parts(0, 2589).saturating_mul(s.into()))
        }
        /// Storage: Identity IdentityOf (r:1 w:0)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:0 w:100)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// The range of component `p` is `[0, 100]`.
        fn set_subs_old(p: u32) -> Weight {
            Weight::from_parts(23_326_035, 11003)
                .saturating_add(
                    Weight::from_parts(1_439_873, 0).saturating_mul(p.into()),
                )
                .saturating_add(T::DbWeight::get().reads(2_u64))
                .saturating_add(T::DbWeight::get().writes(1_u64))
                .saturating_add(
                    T::DbWeight::get().writes((1_u64).saturating_mul(p.into())),
                )
        }
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// Storage: Identity IdentityOf (r:1 w:1)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:0 w:100)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 20]`.
        /// The range of component `s` is `[0, 100]`.
        /// The range of component `x` is `[0, 100]`.
        fn clear_identity(r: u32, s: u32, x: u32) -> Weight {
            Weight::from_parts(30_695_182, 11003)
                .saturating_add(Weight::from_parts(162_357, 0).saturating_mul(r.into()))
                .saturating_add(
                    Weight::from_parts(1_427_998, 0).saturating_mul(s.into()),
                )
                .saturating_add(Weight::from_parts(247_578, 0).saturating_mul(x.into()))
                .saturating_add(T::DbWeight::get().reads(2_u64))
                .saturating_add(T::DbWeight::get().writes(2_u64))
                .saturating_add(
                    T::DbWeight::get().writes((1_u64).saturating_mul(s.into())),
                )
        }
        /// Storage: Identity Registrars (r:1 w:0)
        /// Proof: Identity Registrars (max_values: Some(1), max_size: Some(1141), added: 1636, mode: MaxEncodedLen)
        /// Storage: Identity IdentityOf (r:1 w:1)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 20]`.
        /// The range of component `x` is `[0, 100]`.
        fn request_judgement(r: u32, x: u32) -> Weight {
            Weight::from_parts(32_207_018, 11003)
                .saturating_add(Weight::from_parts(249_156, 0).saturating_mul(r.into()))
                .saturating_add(Weight::from_parts(458_329, 0).saturating_mul(x.into()))
                .saturating_add(T::DbWeight::get().reads(2_u64))
                .saturating_add(T::DbWeight::get().writes(1_u64))
        }
        /// Storage: Identity IdentityOf (r:1 w:1)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 20]`.
        /// The range of component `x` is `[0, 100]`.
        fn cancel_request(r: u32, x: u32) -> Weight {
            Weight::from_parts(31_967_170, 11003)
                .saturating_add(Weight::from_parts(42_676, 0).saturating_mul(r.into()))
                .saturating_add(Weight::from_parts(446_213, 0).saturating_mul(x.into()))
                .saturating_add(T::DbWeight::get().reads(1_u64))
                .saturating_add(T::DbWeight::get().writes(1_u64))
        }
        /// Storage: Identity Registrars (r:1 w:1)
        /// Proof: Identity Registrars (max_values: Some(1), max_size: Some(1141), added: 1636, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 19]`.
        fn set_fee(r: u32) -> Weight {
            Weight::from_parts(7_932_950, 2626)
                .saturating_add(Weight::from_parts(132_653, 0).saturating_mul(r.into()))
                .saturating_add(T::DbWeight::get().reads(1_u64))
                .saturating_add(T::DbWeight::get().writes(1_u64))
        }
        /// Storage: Identity Registrars (r:1 w:1)
        /// Proof: Identity Registrars (max_values: Some(1), max_size: Some(1141), added: 1636, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 19]`.
        fn set_account_id(r: u32) -> Weight {
            Weight::from_parts(8_051_889, 2626)
                .saturating_add(Weight::from_parts(129_592, 0).saturating_mul(r.into()))
                .saturating_add(T::DbWeight::get().reads(1_u64))
                .saturating_add(T::DbWeight::get().writes(1_u64))
        }
        /// Storage: Identity Registrars (r:1 w:1)
        /// Proof: Identity Registrars (max_values: Some(1), max_size: Some(1141), added: 1636, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 19]`.
        fn set_fields(r: u32) -> Weight {
            Weight::from_parts(7_911_589, 2626)
                .saturating_add(Weight::from_parts(125_788, 0).saturating_mul(r.into()))
                .saturating_add(T::DbWeight::get().reads(1_u64))
                .saturating_add(T::DbWeight::get().writes(1_u64))
        }
        /// Storage: Identity Registrars (r:1 w:0)
        /// Proof: Identity Registrars (max_values: Some(1), max_size: Some(1141), added: 1636, mode: MaxEncodedLen)
        /// Storage: Identity IdentityOf (r:1 w:1)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 19]`.
        /// The range of component `x` is `[0, 100]`.
        fn provide_judgement(r: u32, x: u32) -> Weight {
            Weight::from_parts(17_817_684, 11003)
                .saturating_add(Weight::from_parts(406_251, 0).saturating_mul(r.into()))
                .saturating_add(Weight::from_parts(755_225, 0).saturating_mul(x.into()))
                .saturating_add(T::DbWeight::get().reads(2_u64))
                .saturating_add(T::DbWeight::get().writes(1_u64))
        }
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// Storage: Identity IdentityOf (r:1 w:1)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: System Account (r:1 w:1)
        /// Proof: System Account (max_values: None, max_size: Some(128), added: 2603, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:0 w:100)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 20]`.
        /// The range of component `s` is `[0, 100]`.
        /// The range of component `x` is `[0, 100]`.
        fn kill_identity(r: u32, s: u32, x: u32) -> Weight {
            Weight::from_parts(51_684_057, 11003)
                .saturating_add(Weight::from_parts(145_285, 0).saturating_mul(r.into()))
                .saturating_add(
                    Weight::from_parts(1_421_039, 0).saturating_mul(s.into()),
                )
                .saturating_add(Weight::from_parts(240_907, 0).saturating_mul(x.into()))
                .saturating_add(T::DbWeight::get().reads(3_u64))
                .saturating_add(T::DbWeight::get().writes(3_u64))
                .saturating_add(
                    T::DbWeight::get().writes((1_u64).saturating_mul(s.into())),
                )
        }
        /// Storage: Identity IdentityOf (r:1 w:0)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:1 w:1)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// The range of component `s` is `[0, 99]`.
        fn add_sub(s: u32) -> Weight {
            Weight::from_parts(34_214_998, 11003)
                .saturating_add(Weight::from_parts(114_551, 0).saturating_mul(s.into()))
                .saturating_add(T::DbWeight::get().reads(3_u64))
                .saturating_add(T::DbWeight::get().writes(2_u64))
        }
        /// Storage: Identity IdentityOf (r:1 w:0)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:1 w:1)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// The range of component `s` is `[1, 100]`.
        fn rename_sub(s: u32) -> Weight {
            Weight::from_parts(14_417_903, 11003)
                .saturating_add(Weight::from_parts(38_371, 0).saturating_mul(s.into()))
                .saturating_add(T::DbWeight::get().reads(2_u64))
                .saturating_add(T::DbWeight::get().writes(1_u64))
        }
        /// Storage: Identity IdentityOf (r:1 w:0)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:1 w:1)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// The range of component `s` is `[1, 100]`.
        fn remove_sub(s: u32) -> Weight {
            Weight::from_parts(36_208_941, 11003)
                .saturating_add(Weight::from_parts(105_805, 0).saturating_mul(s.into()))
                .saturating_add(T::DbWeight::get().reads(3_u64))
                .saturating_add(T::DbWeight::get().writes(2_u64))
        }
        /// Storage: Identity SuperOf (r:1 w:1)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// Storage: System Account (r:1 w:0)
        /// Proof: System Account (max_values: None, max_size: Some(128), added: 2603, mode: MaxEncodedLen)
        /// The range of component `s` is `[0, 99]`.
        fn quit_sub(s: u32) -> Weight {
            Weight::from_parts(26_407_731, 6723)
                .saturating_add(Weight::from_parts(101_112, 0).saturating_mul(s.into()))
                .saturating_add(T::DbWeight::get().reads(3_u64))
                .saturating_add(T::DbWeight::get().writes(2_u64))
        }
    }
    impl WeightInfo for () {
        /// Storage: Identity Registrars (r:1 w:1)
        /// Proof: Identity Registrars (max_values: Some(1), max_size: Some(1141), added: 1636, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 19]`.
        fn add_registrar(r: u32) -> Weight {
            Weight::from_parts(12_515_830, 2626)
                .saturating_add(Weight::from_parts(147_919, 0).saturating_mul(r.into()))
                .saturating_add(RocksDbWeight::get().reads(1_u64))
                .saturating_add(RocksDbWeight::get().writes(1_u64))
        }
        /// Storage: Identity IdentityOf (r:1 w:1)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 20]`.
        /// The range of component `x` is `[0, 100]`.
        fn set_identity(r: u32, x: u32) -> Weight {
            Weight::from_parts(31_329_634, 11003)
                .saturating_add(Weight::from_parts(203_570, 0).saturating_mul(r.into()))
                .saturating_add(Weight::from_parts(429_346, 0).saturating_mul(x.into()))
                .saturating_add(RocksDbWeight::get().reads(1_u64))
                .saturating_add(RocksDbWeight::get().writes(1_u64))
        }
        /// Storage: Identity IdentityOf (r:1 w:0)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:100 w:100)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// The range of component `s` is `[0, 100]`.
        fn set_subs_new(s: u32) -> Weight {
            Weight::from_parts(24_917_444, 11003)
                .saturating_add(
                    Weight::from_parts(3_279_868, 0).saturating_mul(s.into()),
                )
                .saturating_add(RocksDbWeight::get().reads(2_u64))
                .saturating_add(
                    RocksDbWeight::get().reads((1_u64).saturating_mul(s.into())),
                )
                .saturating_add(RocksDbWeight::get().writes(1_u64))
                .saturating_add(
                    RocksDbWeight::get().writes((1_u64).saturating_mul(s.into())),
                )
                .saturating_add(Weight::from_parts(0, 2589).saturating_mul(s.into()))
        }
        /// Storage: Identity IdentityOf (r:1 w:0)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:0 w:100)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// The range of component `p` is `[0, 100]`.
        fn set_subs_old(p: u32) -> Weight {
            Weight::from_parts(23_326_035, 11003)
                .saturating_add(
                    Weight::from_parts(1_439_873, 0).saturating_mul(p.into()),
                )
                .saturating_add(RocksDbWeight::get().reads(2_u64))
                .saturating_add(RocksDbWeight::get().writes(1_u64))
                .saturating_add(
                    RocksDbWeight::get().writes((1_u64).saturating_mul(p.into())),
                )
        }
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// Storage: Identity IdentityOf (r:1 w:1)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:0 w:100)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 20]`.
        /// The range of component `s` is `[0, 100]`.
        /// The range of component `x` is `[0, 100]`.
        fn clear_identity(r: u32, s: u32, x: u32) -> Weight {
            Weight::from_parts(30_695_182, 11003)
                .saturating_add(Weight::from_parts(162_357, 0).saturating_mul(r.into()))
                .saturating_add(
                    Weight::from_parts(1_427_998, 0).saturating_mul(s.into()),
                )
                .saturating_add(Weight::from_parts(247_578, 0).saturating_mul(x.into()))
                .saturating_add(RocksDbWeight::get().reads(2_u64))
                .saturating_add(RocksDbWeight::get().writes(2_u64))
                .saturating_add(
                    RocksDbWeight::get().writes((1_u64).saturating_mul(s.into())),
                )
        }
        /// Storage: Identity Registrars (r:1 w:0)
        /// Proof: Identity Registrars (max_values: Some(1), max_size: Some(1141), added: 1636, mode: MaxEncodedLen)
        /// Storage: Identity IdentityOf (r:1 w:1)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 20]`.
        /// The range of component `x` is `[0, 100]`.
        fn request_judgement(r: u32, x: u32) -> Weight {
            Weight::from_parts(32_207_018, 11003)
                .saturating_add(Weight::from_parts(249_156, 0).saturating_mul(r.into()))
                .saturating_add(Weight::from_parts(458_329, 0).saturating_mul(x.into()))
                .saturating_add(RocksDbWeight::get().reads(2_u64))
                .saturating_add(RocksDbWeight::get().writes(1_u64))
        }
        /// Storage: Identity IdentityOf (r:1 w:1)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 20]`.
        /// The range of component `x` is `[0, 100]`.
        fn cancel_request(r: u32, x: u32) -> Weight {
            Weight::from_parts(31_967_170, 11003)
                .saturating_add(Weight::from_parts(42_676, 0).saturating_mul(r.into()))
                .saturating_add(Weight::from_parts(446_213, 0).saturating_mul(x.into()))
                .saturating_add(RocksDbWeight::get().reads(1_u64))
                .saturating_add(RocksDbWeight::get().writes(1_u64))
        }
        /// Storage: Identity Registrars (r:1 w:1)
        /// Proof: Identity Registrars (max_values: Some(1), max_size: Some(1141), added: 1636, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 19]`.
        fn set_fee(r: u32) -> Weight {
            Weight::from_parts(7_932_950, 2626)
                .saturating_add(Weight::from_parts(132_653, 0).saturating_mul(r.into()))
                .saturating_add(RocksDbWeight::get().reads(1_u64))
                .saturating_add(RocksDbWeight::get().writes(1_u64))
        }
        /// Storage: Identity Registrars (r:1 w:1)
        /// Proof: Identity Registrars (max_values: Some(1), max_size: Some(1141), added: 1636, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 19]`.
        fn set_account_id(r: u32) -> Weight {
            Weight::from_parts(8_051_889, 2626)
                .saturating_add(Weight::from_parts(129_592, 0).saturating_mul(r.into()))
                .saturating_add(RocksDbWeight::get().reads(1_u64))
                .saturating_add(RocksDbWeight::get().writes(1_u64))
        }
        /// Storage: Identity Registrars (r:1 w:1)
        /// Proof: Identity Registrars (max_values: Some(1), max_size: Some(1141), added: 1636, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 19]`.
        fn set_fields(r: u32) -> Weight {
            Weight::from_parts(7_911_589, 2626)
                .saturating_add(Weight::from_parts(125_788, 0).saturating_mul(r.into()))
                .saturating_add(RocksDbWeight::get().reads(1_u64))
                .saturating_add(RocksDbWeight::get().writes(1_u64))
        }
        /// Storage: Identity Registrars (r:1 w:0)
        /// Proof: Identity Registrars (max_values: Some(1), max_size: Some(1141), added: 1636, mode: MaxEncodedLen)
        /// Storage: Identity IdentityOf (r:1 w:1)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 19]`.
        /// The range of component `x` is `[0, 100]`.
        fn provide_judgement(r: u32, x: u32) -> Weight {
            Weight::from_parts(17_817_684, 11003)
                .saturating_add(Weight::from_parts(406_251, 0).saturating_mul(r.into()))
                .saturating_add(Weight::from_parts(755_225, 0).saturating_mul(x.into()))
                .saturating_add(RocksDbWeight::get().reads(2_u64))
                .saturating_add(RocksDbWeight::get().writes(1_u64))
        }
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// Storage: Identity IdentityOf (r:1 w:1)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: System Account (r:1 w:1)
        /// Proof: System Account (max_values: None, max_size: Some(128), added: 2603, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:0 w:100)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// The range of component `r` is `[1, 20]`.
        /// The range of component `s` is `[0, 100]`.
        /// The range of component `x` is `[0, 100]`.
        fn kill_identity(r: u32, s: u32, x: u32) -> Weight {
            Weight::from_parts(51_684_057, 11003)
                .saturating_add(Weight::from_parts(145_285, 0).saturating_mul(r.into()))
                .saturating_add(
                    Weight::from_parts(1_421_039, 0).saturating_mul(s.into()),
                )
                .saturating_add(Weight::from_parts(240_907, 0).saturating_mul(x.into()))
                .saturating_add(RocksDbWeight::get().reads(3_u64))
                .saturating_add(RocksDbWeight::get().writes(3_u64))
                .saturating_add(
                    RocksDbWeight::get().writes((1_u64).saturating_mul(s.into())),
                )
        }
        /// Storage: Identity IdentityOf (r:1 w:0)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:1 w:1)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// The range of component `s` is `[0, 99]`.
        fn add_sub(s: u32) -> Weight {
            Weight::from_parts(34_214_998, 11003)
                .saturating_add(Weight::from_parts(114_551, 0).saturating_mul(s.into()))
                .saturating_add(RocksDbWeight::get().reads(3_u64))
                .saturating_add(RocksDbWeight::get().writes(2_u64))
        }
        /// Storage: Identity IdentityOf (r:1 w:0)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:1 w:1)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// The range of component `s` is `[1, 100]`.
        fn rename_sub(s: u32) -> Weight {
            Weight::from_parts(14_417_903, 11003)
                .saturating_add(Weight::from_parts(38_371, 0).saturating_mul(s.into()))
                .saturating_add(RocksDbWeight::get().reads(2_u64))
                .saturating_add(RocksDbWeight::get().writes(1_u64))
        }
        /// Storage: Identity IdentityOf (r:1 w:0)
        /// Proof: Identity IdentityOf (max_values: None, max_size: Some(7538), added: 10013, mode: MaxEncodedLen)
        /// Storage: Identity SuperOf (r:1 w:1)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// The range of component `s` is `[1, 100]`.
        fn remove_sub(s: u32) -> Weight {
            Weight::from_parts(36_208_941, 11003)
                .saturating_add(Weight::from_parts(105_805, 0).saturating_mul(s.into()))
                .saturating_add(RocksDbWeight::get().reads(3_u64))
                .saturating_add(RocksDbWeight::get().writes(2_u64))
        }
        /// Storage: Identity SuperOf (r:1 w:1)
        /// Proof: Identity SuperOf (max_values: None, max_size: Some(114), added: 2589, mode: MaxEncodedLen)
        /// Storage: Identity SubsOf (r:1 w:1)
        /// Proof: Identity SubsOf (max_values: None, max_size: Some(3258), added: 5733, mode: MaxEncodedLen)
        /// Storage: System Account (r:1 w:0)
        /// Proof: System Account (max_values: None, max_size: Some(128), added: 2603, mode: MaxEncodedLen)
        /// The range of component `s` is `[0, 99]`.
        fn quit_sub(s: u32) -> Weight {
            Weight::from_parts(26_407_731, 6723)
                .saturating_add(Weight::from_parts(101_112, 0).saturating_mul(s.into()))
                .saturating_add(RocksDbWeight::get().reads(3_u64))
                .saturating_add(RocksDbWeight::get().writes(2_u64))
        }
    }
}
use frame_support::traits::{BalanceStatus, Currency, OnUnbalanced, ReservableCurrency};
use sp_runtime::traits::{AppendZerosInput, Hash, Saturating, StaticLookup, Zero};
use sp_std::prelude::*;
pub use weights::WeightInfo;
pub use pallet::*;
pub use types::{
    Data, IdentityField, IdentityFields, IdentityInfo, Judgement, RegistrarIndex,
    RegistrarInfo, Registration,
};
type BalanceOf<T> = <<T as Config>::Currency as Currency<
    <T as frame_system::Config>::AccountId,
>>::Balance;
type NegativeImbalanceOf<T> = <<T as Config>::Currency as Currency<
    <T as frame_system::Config>::AccountId,
>>::NegativeImbalance;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
/**The `pallet` module in each FRAME pallet hosts the most important items needed
to construct this pallet.

The main components of this pallet are:
- [`Pallet`], which implements all of the dispatchable extrinsics of the pallet, among
other public functions.
	- The subset of the functions that are dispatchable can be identified either in the
	[`dispatchables`] module or in the [`Call`] enum.
- [`storage_types`], which contains the list of all types that are representing a
storage item. Otherwise, all storage items are listed among [*Type Definitions*](#types).
- [`Config`], which contains the configuration trait of this pallet.
- [`Event`] and [`Error`], which are listed among the [*Enums*](#enums).
		*/
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    /**
Configuration trait of this pallet.

The main purpose of this trait is to act as an interface between this pallet and the runtime in
which it is embedded in. A type, function, or constant in this trait is essentially left to be
configured by the runtime that includes this pallet.

Consequently, a runtime that wants to include this pallet must implement this trait.*/
    pub trait Config: frame_system::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        /// The currency trait.
        type Currency: ReservableCurrency<Self::AccountId>;
        /// The amount held on deposit for a registered identity
        type BasicDeposit: Get<BalanceOf<Self>>;
        /// The amount held on deposit per additional field for a registered identity.
        type FieldDeposit: Get<BalanceOf<Self>>;
        /// The amount held on deposit for a registered subaccount. This should account for the fact
        /// that one storage item's value will increase by the size of an account ID, and there will
        /// be another trie item whose value is the size of an account ID plus 32 bytes.
        type SubAccountDeposit: Get<BalanceOf<Self>>;
        /// The maximum number of sub-accounts allowed per identified account.
        type MaxSubAccounts: Get<u32>;
        /// Maximum number of additional fields that may be stored in an ID. Needed to bound the I/O
        /// required to access an identity, but can be pretty high.
        type MaxAdditionalFields: Get<u32>;
        /// Maxmimum number of registrars allowed in the system. Needed to bound the complexity
        /// of, e.g., updating judgements.
        type MaxRegistrars: Get<u32>;
        /// What to do with slashed funds.
        type Slashed: OnUnbalanced<NegativeImbalanceOf<Self>>;
        /// The origin which may forcibly set or remove a name. Root can always do this.
        type ForceOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        /// The origin which may add or remove registrars. Root can always do this.
        type RegistrarOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;
    }
    /**
				The `Pallet` struct, the main type that implements traits and standalone
				functions within the pallet.
			*/
    pub struct Pallet<T>(frame_support::__private::sp_std::marker::PhantomData<(T)>);
    const _: () = {
        impl<T> ::core::clone::Clone for Pallet<T> {
            fn clone(&self) -> Self {
                Self(::core::clone::Clone::clone(&self.0))
            }
        }
    };
    const _: () = {
        impl<T> ::core::cmp::Eq for Pallet<T> {}
    };
    const _: () = {
        impl<T> ::core::cmp::PartialEq for Pallet<T> {
            fn eq(&self, other: &Self) -> bool {
                true && self.0 == other.0
            }
        }
    };
    const _: () = {
        impl<T> ::core::fmt::Debug for Pallet<T> {
            fn fmt(&self, fmt: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                fmt.debug_tuple("Pallet").field(&self.0).finish()
            }
        }
    };
    /// Information that is pertinent to identify the entity behind an account.
    ///
    /// TWOX-NOTE: OK ― `AccountId` is a secure hash.
    #[allow(type_alias_bounds)]
    ///
    ///Storage type is [`StorageMap`] with key type `T :: AccountId` and value type `Registration < BalanceOf < T >, T :: MaxRegistrars, T :: MaxAdditionalFields >`.
    pub(super) type IdentityOf<T: Config> = StorageMap<
        _GeneratedPrefixForStorageIdentityOf<T>,
        Twox64Concat,
        T::AccountId,
        Registration<BalanceOf<T>, T::MaxRegistrars, T::MaxAdditionalFields>,
        OptionQuery,
    >;
    /// The super-identity of an alternative "sub" identity together with its name, within that
    /// context. If the account is not some other account's sub-identity, then just `None`.
    #[allow(type_alias_bounds)]
    ///
    ///Storage type is [`StorageMap`] with key type `T :: AccountId` and value type `(T :: AccountId, Data)`.
    pub(super) type SuperOf<T: Config> = StorageMap<
        _GeneratedPrefixForStorageSuperOf<T>,
        Blake2_128Concat,
        T::AccountId,
        (T::AccountId, Data),
        OptionQuery,
    >;
    /// Alternative "sub" identities of this account.
    ///
    /// The first item is the deposit, the second is a vector of the accounts.
    ///
    /// TWOX-NOTE: OK ― `AccountId` is a secure hash.
    #[allow(type_alias_bounds)]
    ///
    ///Storage type is [`StorageMap`] with key type `T :: AccountId` and value type `(BalanceOf < T >, BoundedVec < T :: AccountId, T :: MaxSubAccounts >)`.
    pub(super) type SubsOf<T: Config> = StorageMap<
        _GeneratedPrefixForStorageSubsOf<T>,
        Twox64Concat,
        T::AccountId,
        (BalanceOf<T>, BoundedVec<T::AccountId, T::MaxSubAccounts>),
        ValueQuery,
    >;
    /// The set of registrars. Not expected to get very big as can only be added through a
    /// special origin (likely a council motion).
    ///
    /// The index into this can be cast to `RegistrarIndex` to get a valid value.
    #[allow(type_alias_bounds)]
    ///
    /**Storage type is [`StorageValue`] with value type `BoundedVec < Option < RegistrarInfo < BalanceOf < T >, T :: AccountId > >, T
:: MaxRegistrars >`.*/
    pub(super) type Registrars<T: Config> = StorageValue<
        _GeneratedPrefixForStorageRegistrars<T>,
        BoundedVec<Option<RegistrarInfo<BalanceOf<T>, T::AccountId>>, T::MaxRegistrars>,
        ValueQuery,
    >;
    #[scale_info(skip_type_params(T), capture_docs = "always")]
    ///The `Error` enum of this pallet.
    pub enum Error<T> {
        #[doc(hidden)]
        #[codec(skip)]
        __Ignore(
            frame_support::__private::sp_std::marker::PhantomData<(T)>,
            frame_support::Never,
        ),
        /// Too many subs-accounts.
        TooManySubAccounts,
        /// Account isn't found.
        NotFound,
        /// Account isn't named.
        NotNamed,
        /// Empty index.
        EmptyIndex,
        /// Fee is changed.
        FeeChanged,
        /// No identity found.
        NoIdentity,
        /// Sticky judgement.
        StickyJudgement,
        /// Judgement given.
        JudgementGiven,
        /// Invalid judgement.
        InvalidJudgement,
        /// The index is invalid.
        InvalidIndex,
        /// The target is invalid.
        InvalidTarget,
        /// Too many additional fields.
        TooManyFields,
        /// Maximum amount of registrars reached. Cannot add any more.
        TooManyRegistrars,
        /// Account ID is already named.
        AlreadyClaimed,
        /// Sender is not a sub-account.
        NotSub,
        /// Sub-account isn't owned by sender.
        NotOwned,
        /// The provided judgement was for a different identity.
        JudgementForDifferentIdentity,
        /// Error that occurs when there is an issue paying for judgement.
        JudgementPaymentFailed,
    }
    #[allow(deprecated)]
    const _: () = {
        #[automatically_derived]
        impl<T> ::codec::Encode for Error<T> {
            fn size_hint(&self) -> usize {
                1_usize
                    + match *self {
                        Error::TooManySubAccounts => 0_usize,
                        Error::NotFound => 0_usize,
                        Error::NotNamed => 0_usize,
                        Error::EmptyIndex => 0_usize,
                        Error::FeeChanged => 0_usize,
                        Error::NoIdentity => 0_usize,
                        Error::StickyJudgement => 0_usize,
                        Error::JudgementGiven => 0_usize,
                        Error::InvalidJudgement => 0_usize,
                        Error::InvalidIndex => 0_usize,
                        Error::InvalidTarget => 0_usize,
                        Error::TooManyFields => 0_usize,
                        Error::TooManyRegistrars => 0_usize,
                        Error::AlreadyClaimed => 0_usize,
                        Error::NotSub => 0_usize,
                        Error::NotOwned => 0_usize,
                        Error::JudgementForDifferentIdentity => 0_usize,
                        Error::JudgementPaymentFailed => 0_usize,
                        _ => 0_usize,
                    }
            }
            fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
                &self,
                __codec_dest_edqy: &mut __CodecOutputEdqy,
            ) {
                match *self {
                    Error::TooManySubAccounts => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(0usize as ::core::primitive::u8);
                    }
                    Error::NotFound => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(1usize as ::core::primitive::u8);
                    }
                    Error::NotNamed => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(2usize as ::core::primitive::u8);
                    }
                    Error::EmptyIndex => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(3usize as ::core::primitive::u8);
                    }
                    Error::FeeChanged => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(4usize as ::core::primitive::u8);
                    }
                    Error::NoIdentity => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(5usize as ::core::primitive::u8);
                    }
                    Error::StickyJudgement => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(6usize as ::core::primitive::u8);
                    }
                    Error::JudgementGiven => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(7usize as ::core::primitive::u8);
                    }
                    Error::InvalidJudgement => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(8usize as ::core::primitive::u8);
                    }
                    Error::InvalidIndex => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(9usize as ::core::primitive::u8);
                    }
                    Error::InvalidTarget => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(10usize as ::core::primitive::u8);
                    }
                    Error::TooManyFields => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(11usize as ::core::primitive::u8);
                    }
                    Error::TooManyRegistrars => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(12usize as ::core::primitive::u8);
                    }
                    Error::AlreadyClaimed => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(13usize as ::core::primitive::u8);
                    }
                    Error::NotSub => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(14usize as ::core::primitive::u8);
                    }
                    Error::NotOwned => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(15usize as ::core::primitive::u8);
                    }
                    Error::JudgementForDifferentIdentity => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(16usize as ::core::primitive::u8);
                    }
                    Error::JudgementPaymentFailed => {
                        #[allow(clippy::unnecessary_cast)]
                        __codec_dest_edqy.push_byte(17usize as ::core::primitive::u8);
                    }
                    _ => {}
                }
            }
        }
        #[automatically_derived]
        impl<T> ::codec::EncodeLike for Error<T> {}
    };
    #[allow(deprecated)]
    const _: () = {
        #[automatically_derived]
        impl<T> ::codec::Decode for Error<T> {
            fn decode<__CodecInputEdqy: ::codec::Input>(
                __codec_input_edqy: &mut __CodecInputEdqy,
            ) -> ::core::result::Result<Self, ::codec::Error> {
                match __codec_input_edqy
                    .read_byte()
                    .map_err(|e| {
                        e.chain("Could not decode `Error`, failed to read variant byte")
                    })?
                {
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 0usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::TooManySubAccounts)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 1usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::NotFound)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 2usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::NotNamed)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 3usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::EmptyIndex)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 4usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::FeeChanged)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 5usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::NoIdentity)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 6usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::StickyJudgement)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 7usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::JudgementGiven)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 8usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::InvalidJudgement)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 9usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::InvalidIndex)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 10usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::InvalidTarget)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 11usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::TooManyFields)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 12usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::TooManyRegistrars)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 13usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::AlreadyClaimed)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 14usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::NotSub)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 15usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Error::<T>::NotOwned)
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 16usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(
                                Error::<T>::JudgementForDifferentIdentity,
                            )
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 17usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(
                                Error::<T>::JudgementPaymentFailed,
                            )
                        })();
                    }
                    _ => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Err(
                                <_ as ::core::convert::Into<
                                    _,
                                >>::into("Could not decode `Error`, variant doesn't exist"),
                            )
                        })();
                    }
                }
            }
        }
    };
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        impl<T> ::scale_info::TypeInfo for Error<T>
        where
            frame_support::__private::sp_std::marker::PhantomData<
                (T),
            >: ::scale_info::TypeInfo + 'static,
            T: 'static,
        {
            type Identity = Self;
            fn type_info() -> ::scale_info::Type {
                ::scale_info::Type::builder()
                    .path(::scale_info::Path::new("Error", "pallet_identity::pallet"))
                    .type_params(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                ::scale_info::TypeParameter::new(
                                    "T",
                                    ::core::option::Option::None,
                                ),
                            ]),
                        ),
                    )
                    .docs_always(&["The `Error` enum of this pallet."])
                    .variant(
                        ::scale_info::build::Variants::new()
                            .variant(
                                "TooManySubAccounts",
                                |v| {
                                    v
                                        .index(0usize as ::core::primitive::u8)
                                        .docs_always(&["Too many subs-accounts."])
                                },
                            )
                            .variant(
                                "NotFound",
                                |v| {
                                    v
                                        .index(1usize as ::core::primitive::u8)
                                        .docs_always(&["Account isn't found."])
                                },
                            )
                            .variant(
                                "NotNamed",
                                |v| {
                                    v
                                        .index(2usize as ::core::primitive::u8)
                                        .docs_always(&["Account isn't named."])
                                },
                            )
                            .variant(
                                "EmptyIndex",
                                |v| {
                                    v
                                        .index(3usize as ::core::primitive::u8)
                                        .docs_always(&["Empty index."])
                                },
                            )
                            .variant(
                                "FeeChanged",
                                |v| {
                                    v
                                        .index(4usize as ::core::primitive::u8)
                                        .docs_always(&["Fee is changed."])
                                },
                            )
                            .variant(
                                "NoIdentity",
                                |v| {
                                    v
                                        .index(5usize as ::core::primitive::u8)
                                        .docs_always(&["No identity found."])
                                },
                            )
                            .variant(
                                "StickyJudgement",
                                |v| {
                                    v
                                        .index(6usize as ::core::primitive::u8)
                                        .docs_always(&["Sticky judgement."])
                                },
                            )
                            .variant(
                                "JudgementGiven",
                                |v| {
                                    v
                                        .index(7usize as ::core::primitive::u8)
                                        .docs_always(&["Judgement given."])
                                },
                            )
                            .variant(
                                "InvalidJudgement",
                                |v| {
                                    v
                                        .index(8usize as ::core::primitive::u8)
                                        .docs_always(&["Invalid judgement."])
                                },
                            )
                            .variant(
                                "InvalidIndex",
                                |v| {
                                    v
                                        .index(9usize as ::core::primitive::u8)
                                        .docs_always(&["The index is invalid."])
                                },
                            )
                            .variant(
                                "InvalidTarget",
                                |v| {
                                    v
                                        .index(10usize as ::core::primitive::u8)
                                        .docs_always(&["The target is invalid."])
                                },
                            )
                            .variant(
                                "TooManyFields",
                                |v| {
                                    v
                                        .index(11usize as ::core::primitive::u8)
                                        .docs_always(&["Too many additional fields."])
                                },
                            )
                            .variant(
                                "TooManyRegistrars",
                                |v| {
                                    v
                                        .index(12usize as ::core::primitive::u8)
                                        .docs_always(
                                            &[
                                                "Maximum amount of registrars reached. Cannot add any more.",
                                            ],
                                        )
                                },
                            )
                            .variant(
                                "AlreadyClaimed",
                                |v| {
                                    v
                                        .index(13usize as ::core::primitive::u8)
                                        .docs_always(&["Account ID is already named."])
                                },
                            )
                            .variant(
                                "NotSub",
                                |v| {
                                    v
                                        .index(14usize as ::core::primitive::u8)
                                        .docs_always(&["Sender is not a sub-account."])
                                },
                            )
                            .variant(
                                "NotOwned",
                                |v| {
                                    v
                                        .index(15usize as ::core::primitive::u8)
                                        .docs_always(&["Sub-account isn't owned by sender."])
                                },
                            )
                            .variant(
                                "JudgementForDifferentIdentity",
                                |v| {
                                    v
                                        .index(16usize as ::core::primitive::u8)
                                        .docs_always(
                                            &["The provided judgement was for a different identity."],
                                        )
                                },
                            )
                            .variant(
                                "JudgementPaymentFailed",
                                |v| {
                                    v
                                        .index(17usize as ::core::primitive::u8)
                                        .docs_always(
                                            &[
                                                "Error that occurs when there is an issue paying for judgement.",
                                            ],
                                        )
                                },
                            ),
                    )
            }
        }
    };
    const _: () = {
        impl<T> frame_support::traits::PalletError for Error<T> {
            const MAX_ENCODED_SIZE: usize = 1;
        }
    };
    ///The `Event` enum of this pallet
    #[scale_info(skip_type_params(T), capture_docs = "always")]
    pub enum Event<T: Config> {
        /// A name was set or reset (which will remove all judgements).
        IdentitySet { who: T::AccountId },
        /// A name was cleared, and the given balance returned.
        IdentityCleared { who: T::AccountId, deposit: BalanceOf<T> },
        /// A name was removed and the given balance slashed.
        IdentityKilled { who: T::AccountId, deposit: BalanceOf<T> },
        /// A judgement was asked from a registrar.
        JudgementRequested { who: T::AccountId, registrar_index: RegistrarIndex },
        /// A judgement request was retracted.
        JudgementUnrequested { who: T::AccountId, registrar_index: RegistrarIndex },
        /// A judgement was given by a registrar.
        JudgementGiven { target: T::AccountId, registrar_index: RegistrarIndex },
        /// A registrar was added.
        RegistrarAdded { registrar_index: RegistrarIndex },
        /// A sub-identity was added to an identity and the deposit paid.
        SubIdentityAdded {
            sub: T::AccountId,
            main: T::AccountId,
            deposit: BalanceOf<T>,
        },
        /// A sub-identity was removed from an identity and the deposit freed.
        SubIdentityRemoved {
            sub: T::AccountId,
            main: T::AccountId,
            deposit: BalanceOf<T>,
        },
        /// A sub-identity was cleared, and the given deposit repatriated from the
        /// main identity account to the sub-identity account.
        SubIdentityRevoked {
            sub: T::AccountId,
            main: T::AccountId,
            deposit: BalanceOf<T>,
        },
        #[doc(hidden)]
        #[codec(skip)]
        __Ignore(
            frame_support::__private::sp_std::marker::PhantomData<(T)>,
            frame_support::Never,
        ),
    }
    const _: () = {
        impl<T: Config> ::core::clone::Clone for Event<T> {
            fn clone(&self) -> Self {
                match self {
                    Self::IdentitySet { ref who } => {
                        Self::IdentitySet {
                            who: ::core::clone::Clone::clone(who),
                        }
                    }
                    Self::IdentityCleared { ref who, ref deposit } => {
                        Self::IdentityCleared {
                            who: ::core::clone::Clone::clone(who),
                            deposit: ::core::clone::Clone::clone(deposit),
                        }
                    }
                    Self::IdentityKilled { ref who, ref deposit } => {
                        Self::IdentityKilled {
                            who: ::core::clone::Clone::clone(who),
                            deposit: ::core::clone::Clone::clone(deposit),
                        }
                    }
                    Self::JudgementRequested { ref who, ref registrar_index } => {
                        Self::JudgementRequested {
                            who: ::core::clone::Clone::clone(who),
                            registrar_index: ::core::clone::Clone::clone(registrar_index),
                        }
                    }
                    Self::JudgementUnrequested { ref who, ref registrar_index } => {
                        Self::JudgementUnrequested {
                            who: ::core::clone::Clone::clone(who),
                            registrar_index: ::core::clone::Clone::clone(registrar_index),
                        }
                    }
                    Self::JudgementGiven { ref target, ref registrar_index } => {
                        Self::JudgementGiven {
                            target: ::core::clone::Clone::clone(target),
                            registrar_index: ::core::clone::Clone::clone(registrar_index),
                        }
                    }
                    Self::RegistrarAdded { ref registrar_index } => {
                        Self::RegistrarAdded {
                            registrar_index: ::core::clone::Clone::clone(registrar_index),
                        }
                    }
                    Self::SubIdentityAdded { ref sub, ref main, ref deposit } => {
                        Self::SubIdentityAdded {
                            sub: ::core::clone::Clone::clone(sub),
                            main: ::core::clone::Clone::clone(main),
                            deposit: ::core::clone::Clone::clone(deposit),
                        }
                    }
                    Self::SubIdentityRemoved { ref sub, ref main, ref deposit } => {
                        Self::SubIdentityRemoved {
                            sub: ::core::clone::Clone::clone(sub),
                            main: ::core::clone::Clone::clone(main),
                            deposit: ::core::clone::Clone::clone(deposit),
                        }
                    }
                    Self::SubIdentityRevoked { ref sub, ref main, ref deposit } => {
                        Self::SubIdentityRevoked {
                            sub: ::core::clone::Clone::clone(sub),
                            main: ::core::clone::Clone::clone(main),
                            deposit: ::core::clone::Clone::clone(deposit),
                        }
                    }
                    Self::__Ignore(ref _0, ref _1) => {
                        Self::__Ignore(
                            ::core::clone::Clone::clone(_0),
                            ::core::clone::Clone::clone(_1),
                        )
                    }
                }
            }
        }
    };
    const _: () = {
        impl<T: Config> ::core::cmp::Eq for Event<T> {}
    };
    const _: () = {
        impl<T: Config> ::core::cmp::PartialEq for Event<T> {
            fn eq(&self, other: &Self) -> bool {
                match (self, other) {
                    (Self::IdentitySet { who }, Self::IdentitySet { who: _0 }) => {
                        true && who == _0
                    }
                    (
                        Self::IdentityCleared { who, deposit },
                        Self::IdentityCleared { who: _0, deposit: _1 },
                    ) => true && who == _0 && deposit == _1,
                    (
                        Self::IdentityKilled { who, deposit },
                        Self::IdentityKilled { who: _0, deposit: _1 },
                    ) => true && who == _0 && deposit == _1,
                    (
                        Self::JudgementRequested { who, registrar_index },
                        Self::JudgementRequested { who: _0, registrar_index: _1 },
                    ) => true && who == _0 && registrar_index == _1,
                    (
                        Self::JudgementUnrequested { who, registrar_index },
                        Self::JudgementUnrequested { who: _0, registrar_index: _1 },
                    ) => true && who == _0 && registrar_index == _1,
                    (
                        Self::JudgementGiven { target, registrar_index },
                        Self::JudgementGiven { target: _0, registrar_index: _1 },
                    ) => true && target == _0 && registrar_index == _1,
                    (
                        Self::RegistrarAdded { registrar_index },
                        Self::RegistrarAdded { registrar_index: _0 },
                    ) => true && registrar_index == _0,
                    (
                        Self::SubIdentityAdded { sub, main, deposit },
                        Self::SubIdentityAdded { sub: _0, main: _1, deposit: _2 },
                    ) => true && sub == _0 && main == _1 && deposit == _2,
                    (
                        Self::SubIdentityRemoved { sub, main, deposit },
                        Self::SubIdentityRemoved { sub: _0, main: _1, deposit: _2 },
                    ) => true && sub == _0 && main == _1 && deposit == _2,
                    (
                        Self::SubIdentityRevoked { sub, main, deposit },
                        Self::SubIdentityRevoked { sub: _0, main: _1, deposit: _2 },
                    ) => true && sub == _0 && main == _1 && deposit == _2,
                    (Self::__Ignore(_0, _1), Self::__Ignore(_0_other, _1_other)) => {
                        true && _0 == _0_other && _1 == _1_other
                    }
                    (Self::IdentitySet { .. }, Self::IdentityCleared { .. }) => false,
                    (Self::IdentitySet { .. }, Self::IdentityKilled { .. }) => false,
                    (Self::IdentitySet { .. }, Self::JudgementRequested { .. }) => false,
                    (Self::IdentitySet { .. }, Self::JudgementUnrequested { .. }) => {
                        false
                    }
                    (Self::IdentitySet { .. }, Self::JudgementGiven { .. }) => false,
                    (Self::IdentitySet { .. }, Self::RegistrarAdded { .. }) => false,
                    (Self::IdentitySet { .. }, Self::SubIdentityAdded { .. }) => false,
                    (Self::IdentitySet { .. }, Self::SubIdentityRemoved { .. }) => false,
                    (Self::IdentitySet { .. }, Self::SubIdentityRevoked { .. }) => false,
                    (Self::IdentitySet { .. }, Self::__Ignore { .. }) => false,
                    (Self::IdentityCleared { .. }, Self::IdentitySet { .. }) => false,
                    (Self::IdentityCleared { .. }, Self::IdentityKilled { .. }) => false,
                    (Self::IdentityCleared { .. }, Self::JudgementRequested { .. }) => {
                        false
                    }
                    (Self::IdentityCleared { .. }, Self::JudgementUnrequested { .. }) => {
                        false
                    }
                    (Self::IdentityCleared { .. }, Self::JudgementGiven { .. }) => false,
                    (Self::IdentityCleared { .. }, Self::RegistrarAdded { .. }) => false,
                    (Self::IdentityCleared { .. }, Self::SubIdentityAdded { .. }) => {
                        false
                    }
                    (Self::IdentityCleared { .. }, Self::SubIdentityRemoved { .. }) => {
                        false
                    }
                    (Self::IdentityCleared { .. }, Self::SubIdentityRevoked { .. }) => {
                        false
                    }
                    (Self::IdentityCleared { .. }, Self::__Ignore { .. }) => false,
                    (Self::IdentityKilled { .. }, Self::IdentitySet { .. }) => false,
                    (Self::IdentityKilled { .. }, Self::IdentityCleared { .. }) => false,
                    (Self::IdentityKilled { .. }, Self::JudgementRequested { .. }) => {
                        false
                    }
                    (Self::IdentityKilled { .. }, Self::JudgementUnrequested { .. }) => {
                        false
                    }
                    (Self::IdentityKilled { .. }, Self::JudgementGiven { .. }) => false,
                    (Self::IdentityKilled { .. }, Self::RegistrarAdded { .. }) => false,
                    (Self::IdentityKilled { .. }, Self::SubIdentityAdded { .. }) => false,
                    (Self::IdentityKilled { .. }, Self::SubIdentityRemoved { .. }) => {
                        false
                    }
                    (Self::IdentityKilled { .. }, Self::SubIdentityRevoked { .. }) => {
                        false
                    }
                    (Self::IdentityKilled { .. }, Self::__Ignore { .. }) => false,
                    (Self::JudgementRequested { .. }, Self::IdentitySet { .. }) => false,
                    (Self::JudgementRequested { .. }, Self::IdentityCleared { .. }) => {
                        false
                    }
                    (Self::JudgementRequested { .. }, Self::IdentityKilled { .. }) => {
                        false
                    }
                    (
                        Self::JudgementRequested { .. },
                        Self::JudgementUnrequested { .. },
                    ) => false,
                    (Self::JudgementRequested { .. }, Self::JudgementGiven { .. }) => {
                        false
                    }
                    (Self::JudgementRequested { .. }, Self::RegistrarAdded { .. }) => {
                        false
                    }
                    (Self::JudgementRequested { .. }, Self::SubIdentityAdded { .. }) => {
                        false
                    }
                    (
                        Self::JudgementRequested { .. },
                        Self::SubIdentityRemoved { .. },
                    ) => false,
                    (
                        Self::JudgementRequested { .. },
                        Self::SubIdentityRevoked { .. },
                    ) => false,
                    (Self::JudgementRequested { .. }, Self::__Ignore { .. }) => false,
                    (Self::JudgementUnrequested { .. }, Self::IdentitySet { .. }) => {
                        false
                    }
                    (Self::JudgementUnrequested { .. }, Self::IdentityCleared { .. }) => {
                        false
                    }
                    (Self::JudgementUnrequested { .. }, Self::IdentityKilled { .. }) => {
                        false
                    }
                    (
                        Self::JudgementUnrequested { .. },
                        Self::JudgementRequested { .. },
                    ) => false,
                    (Self::JudgementUnrequested { .. }, Self::JudgementGiven { .. }) => {
                        false
                    }
                    (Self::JudgementUnrequested { .. }, Self::RegistrarAdded { .. }) => {
                        false
                    }
                    (
                        Self::JudgementUnrequested { .. },
                        Self::SubIdentityAdded { .. },
                    ) => false,
                    (
                        Self::JudgementUnrequested { .. },
                        Self::SubIdentityRemoved { .. },
                    ) => false,
                    (
                        Self::JudgementUnrequested { .. },
                        Self::SubIdentityRevoked { .. },
                    ) => false,
                    (Self::JudgementUnrequested { .. }, Self::__Ignore { .. }) => false,
                    (Self::JudgementGiven { .. }, Self::IdentitySet { .. }) => false,
                    (Self::JudgementGiven { .. }, Self::IdentityCleared { .. }) => false,
                    (Self::JudgementGiven { .. }, Self::IdentityKilled { .. }) => false,
                    (Self::JudgementGiven { .. }, Self::JudgementRequested { .. }) => {
                        false
                    }
                    (Self::JudgementGiven { .. }, Self::JudgementUnrequested { .. }) => {
                        false
                    }
                    (Self::JudgementGiven { .. }, Self::RegistrarAdded { .. }) => false,
                    (Self::JudgementGiven { .. }, Self::SubIdentityAdded { .. }) => false,
                    (Self::JudgementGiven { .. }, Self::SubIdentityRemoved { .. }) => {
                        false
                    }
                    (Self::JudgementGiven { .. }, Self::SubIdentityRevoked { .. }) => {
                        false
                    }
                    (Self::JudgementGiven { .. }, Self::__Ignore { .. }) => false,
                    (Self::RegistrarAdded { .. }, Self::IdentitySet { .. }) => false,
                    (Self::RegistrarAdded { .. }, Self::IdentityCleared { .. }) => false,
                    (Self::RegistrarAdded { .. }, Self::IdentityKilled { .. }) => false,
                    (Self::RegistrarAdded { .. }, Self::JudgementRequested { .. }) => {
                        false
                    }
                    (Self::RegistrarAdded { .. }, Self::JudgementUnrequested { .. }) => {
                        false
                    }
                    (Self::RegistrarAdded { .. }, Self::JudgementGiven { .. }) => false,
                    (Self::RegistrarAdded { .. }, Self::SubIdentityAdded { .. }) => false,
                    (Self::RegistrarAdded { .. }, Self::SubIdentityRemoved { .. }) => {
                        false
                    }
                    (Self::RegistrarAdded { .. }, Self::SubIdentityRevoked { .. }) => {
                        false
                    }
                    (Self::RegistrarAdded { .. }, Self::__Ignore { .. }) => false,
                    (Self::SubIdentityAdded { .. }, Self::IdentitySet { .. }) => false,
                    (Self::SubIdentityAdded { .. }, Self::IdentityCleared { .. }) => {
                        false
                    }
                    (Self::SubIdentityAdded { .. }, Self::IdentityKilled { .. }) => false,
                    (Self::SubIdentityAdded { .. }, Self::JudgementRequested { .. }) => {
                        false
                    }
                    (
                        Self::SubIdentityAdded { .. },
                        Self::JudgementUnrequested { .. },
                    ) => false,
                    (Self::SubIdentityAdded { .. }, Self::JudgementGiven { .. }) => false,
                    (Self::SubIdentityAdded { .. }, Self::RegistrarAdded { .. }) => false,
                    (Self::SubIdentityAdded { .. }, Self::SubIdentityRemoved { .. }) => {
                        false
                    }
                    (Self::SubIdentityAdded { .. }, Self::SubIdentityRevoked { .. }) => {
                        false
                    }
                    (Self::SubIdentityAdded { .. }, Self::__Ignore { .. }) => false,
                    (Self::SubIdentityRemoved { .. }, Self::IdentitySet { .. }) => false,
                    (Self::SubIdentityRemoved { .. }, Self::IdentityCleared { .. }) => {
                        false
                    }
                    (Self::SubIdentityRemoved { .. }, Self::IdentityKilled { .. }) => {
                        false
                    }
                    (
                        Self::SubIdentityRemoved { .. },
                        Self::JudgementRequested { .. },
                    ) => false,
                    (
                        Self::SubIdentityRemoved { .. },
                        Self::JudgementUnrequested { .. },
                    ) => false,
                    (Self::SubIdentityRemoved { .. }, Self::JudgementGiven { .. }) => {
                        false
                    }
                    (Self::SubIdentityRemoved { .. }, Self::RegistrarAdded { .. }) => {
                        false
                    }
                    (Self::SubIdentityRemoved { .. }, Self::SubIdentityAdded { .. }) => {
                        false
                    }
                    (
                        Self::SubIdentityRemoved { .. },
                        Self::SubIdentityRevoked { .. },
                    ) => false,
                    (Self::SubIdentityRemoved { .. }, Self::__Ignore { .. }) => false,
                    (Self::SubIdentityRevoked { .. }, Self::IdentitySet { .. }) => false,
                    (Self::SubIdentityRevoked { .. }, Self::IdentityCleared { .. }) => {
                        false
                    }
                    (Self::SubIdentityRevoked { .. }, Self::IdentityKilled { .. }) => {
                        false
                    }
                    (
                        Self::SubIdentityRevoked { .. },
                        Self::JudgementRequested { .. },
                    ) => false,
                    (
                        Self::SubIdentityRevoked { .. },
                        Self::JudgementUnrequested { .. },
                    ) => false,
                    (Self::SubIdentityRevoked { .. }, Self::JudgementGiven { .. }) => {
                        false
                    }
                    (Self::SubIdentityRevoked { .. }, Self::RegistrarAdded { .. }) => {
                        false
                    }
                    (Self::SubIdentityRevoked { .. }, Self::SubIdentityAdded { .. }) => {
                        false
                    }
                    (
                        Self::SubIdentityRevoked { .. },
                        Self::SubIdentityRemoved { .. },
                    ) => false,
                    (Self::SubIdentityRevoked { .. }, Self::__Ignore { .. }) => false,
                    (Self::__Ignore { .. }, Self::IdentitySet { .. }) => false,
                    (Self::__Ignore { .. }, Self::IdentityCleared { .. }) => false,
                    (Self::__Ignore { .. }, Self::IdentityKilled { .. }) => false,
                    (Self::__Ignore { .. }, Self::JudgementRequested { .. }) => false,
                    (Self::__Ignore { .. }, Self::JudgementUnrequested { .. }) => false,
                    (Self::__Ignore { .. }, Self::JudgementGiven { .. }) => false,
                    (Self::__Ignore { .. }, Self::RegistrarAdded { .. }) => false,
                    (Self::__Ignore { .. }, Self::SubIdentityAdded { .. }) => false,
                    (Self::__Ignore { .. }, Self::SubIdentityRemoved { .. }) => false,
                    (Self::__Ignore { .. }, Self::SubIdentityRevoked { .. }) => false,
                }
            }
        }
    };
    const _: () = {
        impl<T: Config> ::core::fmt::Debug for Event<T> {
            fn fmt(&self, fmt: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                match *self {
                    Self::IdentitySet { ref who } => {
                        fmt.debug_struct("Event::IdentitySet")
                            .field("who", &who)
                            .finish()
                    }
                    Self::IdentityCleared { ref who, ref deposit } => {
                        fmt.debug_struct("Event::IdentityCleared")
                            .field("who", &who)
                            .field("deposit", &deposit)
                            .finish()
                    }
                    Self::IdentityKilled { ref who, ref deposit } => {
                        fmt.debug_struct("Event::IdentityKilled")
                            .field("who", &who)
                            .field("deposit", &deposit)
                            .finish()
                    }
                    Self::JudgementRequested { ref who, ref registrar_index } => {
                        fmt.debug_struct("Event::JudgementRequested")
                            .field("who", &who)
                            .field("registrar_index", &registrar_index)
                            .finish()
                    }
                    Self::JudgementUnrequested { ref who, ref registrar_index } => {
                        fmt.debug_struct("Event::JudgementUnrequested")
                            .field("who", &who)
                            .field("registrar_index", &registrar_index)
                            .finish()
                    }
                    Self::JudgementGiven { ref target, ref registrar_index } => {
                        fmt.debug_struct("Event::JudgementGiven")
                            .field("target", &target)
                            .field("registrar_index", &registrar_index)
                            .finish()
                    }
                    Self::RegistrarAdded { ref registrar_index } => {
                        fmt.debug_struct("Event::RegistrarAdded")
                            .field("registrar_index", &registrar_index)
                            .finish()
                    }
                    Self::SubIdentityAdded { ref sub, ref main, ref deposit } => {
                        fmt.debug_struct("Event::SubIdentityAdded")
                            .field("sub", &sub)
                            .field("main", &main)
                            .field("deposit", &deposit)
                            .finish()
                    }
                    Self::SubIdentityRemoved { ref sub, ref main, ref deposit } => {
                        fmt.debug_struct("Event::SubIdentityRemoved")
                            .field("sub", &sub)
                            .field("main", &main)
                            .field("deposit", &deposit)
                            .finish()
                    }
                    Self::SubIdentityRevoked { ref sub, ref main, ref deposit } => {
                        fmt.debug_struct("Event::SubIdentityRevoked")
                            .field("sub", &sub)
                            .field("main", &main)
                            .field("deposit", &deposit)
                            .finish()
                    }
                    Self::__Ignore(ref _0, ref _1) => {
                        fmt.debug_tuple("Event::__Ignore").field(&_0).field(&_1).finish()
                    }
                }
            }
        }
    };
    #[allow(deprecated)]
    const _: () = {
        #[automatically_derived]
        impl<T: Config> ::codec::Encode for Event<T>
        where
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
        {
            fn size_hint(&self) -> usize {
                1_usize
                    + match *self {
                        Event::IdentitySet { ref who } => {
                            0_usize.saturating_add(::codec::Encode::size_hint(who))
                        }
                        Event::IdentityCleared { ref who, ref deposit } => {
                            0_usize
                                .saturating_add(::codec::Encode::size_hint(who))
                                .saturating_add(::codec::Encode::size_hint(deposit))
                        }
                        Event::IdentityKilled { ref who, ref deposit } => {
                            0_usize
                                .saturating_add(::codec::Encode::size_hint(who))
                                .saturating_add(::codec::Encode::size_hint(deposit))
                        }
                        Event::JudgementRequested { ref who, ref registrar_index } => {
                            0_usize
                                .saturating_add(::codec::Encode::size_hint(who))
                                .saturating_add(::codec::Encode::size_hint(registrar_index))
                        }
                        Event::JudgementUnrequested { ref who, ref registrar_index } => {
                            0_usize
                                .saturating_add(::codec::Encode::size_hint(who))
                                .saturating_add(::codec::Encode::size_hint(registrar_index))
                        }
                        Event::JudgementGiven { ref target, ref registrar_index } => {
                            0_usize
                                .saturating_add(::codec::Encode::size_hint(target))
                                .saturating_add(::codec::Encode::size_hint(registrar_index))
                        }
                        Event::RegistrarAdded { ref registrar_index } => {
                            0_usize
                                .saturating_add(::codec::Encode::size_hint(registrar_index))
                        }
                        Event::SubIdentityAdded { ref sub, ref main, ref deposit } => {
                            0_usize
                                .saturating_add(::codec::Encode::size_hint(sub))
                                .saturating_add(::codec::Encode::size_hint(main))
                                .saturating_add(::codec::Encode::size_hint(deposit))
                        }
                        Event::SubIdentityRemoved { ref sub, ref main, ref deposit } => {
                            0_usize
                                .saturating_add(::codec::Encode::size_hint(sub))
                                .saturating_add(::codec::Encode::size_hint(main))
                                .saturating_add(::codec::Encode::size_hint(deposit))
                        }
                        Event::SubIdentityRevoked { ref sub, ref main, ref deposit } => {
                            0_usize
                                .saturating_add(::codec::Encode::size_hint(sub))
                                .saturating_add(::codec::Encode::size_hint(main))
                                .saturating_add(::codec::Encode::size_hint(deposit))
                        }
                        _ => 0_usize,
                    }
            }
            fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
                &self,
                __codec_dest_edqy: &mut __CodecOutputEdqy,
            ) {
                match *self {
                    Event::IdentitySet { ref who } => {
                        __codec_dest_edqy.push_byte(0usize as ::core::primitive::u8);
                        ::codec::Encode::encode_to(who, __codec_dest_edqy);
                    }
                    Event::IdentityCleared { ref who, ref deposit } => {
                        __codec_dest_edqy.push_byte(1usize as ::core::primitive::u8);
                        ::codec::Encode::encode_to(who, __codec_dest_edqy);
                        ::codec::Encode::encode_to(deposit, __codec_dest_edqy);
                    }
                    Event::IdentityKilled { ref who, ref deposit } => {
                        __codec_dest_edqy.push_byte(2usize as ::core::primitive::u8);
                        ::codec::Encode::encode_to(who, __codec_dest_edqy);
                        ::codec::Encode::encode_to(deposit, __codec_dest_edqy);
                    }
                    Event::JudgementRequested { ref who, ref registrar_index } => {
                        __codec_dest_edqy.push_byte(3usize as ::core::primitive::u8);
                        ::codec::Encode::encode_to(who, __codec_dest_edqy);
                        ::codec::Encode::encode_to(registrar_index, __codec_dest_edqy);
                    }
                    Event::JudgementUnrequested { ref who, ref registrar_index } => {
                        __codec_dest_edqy.push_byte(4usize as ::core::primitive::u8);
                        ::codec::Encode::encode_to(who, __codec_dest_edqy);
                        ::codec::Encode::encode_to(registrar_index, __codec_dest_edqy);
                    }
                    Event::JudgementGiven { ref target, ref registrar_index } => {
                        __codec_dest_edqy.push_byte(5usize as ::core::primitive::u8);
                        ::codec::Encode::encode_to(target, __codec_dest_edqy);
                        ::codec::Encode::encode_to(registrar_index, __codec_dest_edqy);
                    }
                    Event::RegistrarAdded { ref registrar_index } => {
                        __codec_dest_edqy.push_byte(6usize as ::core::primitive::u8);
                        ::codec::Encode::encode_to(registrar_index, __codec_dest_edqy);
                    }
                    Event::SubIdentityAdded { ref sub, ref main, ref deposit } => {
                        __codec_dest_edqy.push_byte(7usize as ::core::primitive::u8);
                        ::codec::Encode::encode_to(sub, __codec_dest_edqy);
                        ::codec::Encode::encode_to(main, __codec_dest_edqy);
                        ::codec::Encode::encode_to(deposit, __codec_dest_edqy);
                    }
                    Event::SubIdentityRemoved { ref sub, ref main, ref deposit } => {
                        __codec_dest_edqy.push_byte(8usize as ::core::primitive::u8);
                        ::codec::Encode::encode_to(sub, __codec_dest_edqy);
                        ::codec::Encode::encode_to(main, __codec_dest_edqy);
                        ::codec::Encode::encode_to(deposit, __codec_dest_edqy);
                    }
                    Event::SubIdentityRevoked { ref sub, ref main, ref deposit } => {
                        __codec_dest_edqy.push_byte(9usize as ::core::primitive::u8);
                        ::codec::Encode::encode_to(sub, __codec_dest_edqy);
                        ::codec::Encode::encode_to(main, __codec_dest_edqy);
                        ::codec::Encode::encode_to(deposit, __codec_dest_edqy);
                    }
                    _ => {}
                }
            }
        }
        #[automatically_derived]
        impl<T: Config> ::codec::EncodeLike for Event<T>
        where
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            T::AccountId: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
            BalanceOf<T>: ::codec::Encode,
        {}
    };
    #[allow(deprecated)]
    const _: () = {
        #[automatically_derived]
        impl<T: Config> ::codec::Decode for Event<T>
        where
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            BalanceOf<T>: ::codec::Decode,
            BalanceOf<T>: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            BalanceOf<T>: ::codec::Decode,
            BalanceOf<T>: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            BalanceOf<T>: ::codec::Decode,
            BalanceOf<T>: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            BalanceOf<T>: ::codec::Decode,
            BalanceOf<T>: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            T::AccountId: ::codec::Decode,
            BalanceOf<T>: ::codec::Decode,
            BalanceOf<T>: ::codec::Decode,
        {
            fn decode<__CodecInputEdqy: ::codec::Input>(
                __codec_input_edqy: &mut __CodecInputEdqy,
            ) -> ::core::result::Result<Self, ::codec::Error> {
                match __codec_input_edqy
                    .read_byte()
                    .map_err(|e| {
                        e.chain("Could not decode `Event`, failed to read variant byte")
                    })?
                {
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 0usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Event::<T>::IdentitySet {
                                who: {
                                    let __codec_res_edqy = <T::AccountId as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Event::IdentitySet::who`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 1usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Event::<T>::IdentityCleared {
                                who: {
                                    let __codec_res_edqy = <T::AccountId as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Event::IdentityCleared::who`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                deposit: {
                                    let __codec_res_edqy = <BalanceOf<
                                        T,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain("Could not decode `Event::IdentityCleared::deposit`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 2usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Event::<T>::IdentityKilled {
                                who: {
                                    let __codec_res_edqy = <T::AccountId as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Event::IdentityKilled::who`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                deposit: {
                                    let __codec_res_edqy = <BalanceOf<
                                        T,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Event::IdentityKilled::deposit`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 3usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Event::<T>::JudgementRequested {
                                who: {
                                    let __codec_res_edqy = <T::AccountId as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Event::JudgementRequested::who`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                registrar_index: {
                                    let __codec_res_edqy = <RegistrarIndex as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Event::JudgementRequested::registrar_index`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 4usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Event::<T>::JudgementUnrequested {
                                who: {
                                    let __codec_res_edqy = <T::AccountId as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Event::JudgementUnrequested::who`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                registrar_index: {
                                    let __codec_res_edqy = <RegistrarIndex as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Event::JudgementUnrequested::registrar_index`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 5usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Event::<T>::JudgementGiven {
                                target: {
                                    let __codec_res_edqy = <T::AccountId as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Event::JudgementGiven::target`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                registrar_index: {
                                    let __codec_res_edqy = <RegistrarIndex as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Event::JudgementGiven::registrar_index`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 6usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Event::<T>::RegistrarAdded {
                                registrar_index: {
                                    let __codec_res_edqy = <RegistrarIndex as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Event::RegistrarAdded::registrar_index`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 7usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Event::<T>::SubIdentityAdded {
                                sub: {
                                    let __codec_res_edqy = <T::AccountId as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Event::SubIdentityAdded::sub`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                main: {
                                    let __codec_res_edqy = <T::AccountId as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Event::SubIdentityAdded::main`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                deposit: {
                                    let __codec_res_edqy = <BalanceOf<
                                        T,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Event::SubIdentityAdded::deposit`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 8usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Event::<T>::SubIdentityRemoved {
                                sub: {
                                    let __codec_res_edqy = <T::AccountId as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Event::SubIdentityRemoved::sub`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                main: {
                                    let __codec_res_edqy = <T::AccountId as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain("Could not decode `Event::SubIdentityRemoved::main`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                deposit: {
                                    let __codec_res_edqy = <BalanceOf<
                                        T,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Event::SubIdentityRemoved::deposit`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 9usize as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Event::<T>::SubIdentityRevoked {
                                sub: {
                                    let __codec_res_edqy = <T::AccountId as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Event::SubIdentityRevoked::sub`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                main: {
                                    let __codec_res_edqy = <T::AccountId as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain("Could not decode `Event::SubIdentityRevoked::main`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                deposit: {
                                    let __codec_res_edqy = <BalanceOf<
                                        T,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Event::SubIdentityRevoked::deposit`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    _ => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Err(
                                <_ as ::core::convert::Into<
                                    _,
                                >>::into("Could not decode `Event`, variant doesn't exist"),
                            )
                        })();
                    }
                }
            }
        }
    };
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        impl<T: Config> ::scale_info::TypeInfo for Event<T>
        where
            T::AccountId: ::scale_info::TypeInfo + 'static,
            T::AccountId: ::scale_info::TypeInfo + 'static,
            BalanceOf<T>: ::scale_info::TypeInfo + 'static,
            T::AccountId: ::scale_info::TypeInfo + 'static,
            BalanceOf<T>: ::scale_info::TypeInfo + 'static,
            T::AccountId: ::scale_info::TypeInfo + 'static,
            T::AccountId: ::scale_info::TypeInfo + 'static,
            T::AccountId: ::scale_info::TypeInfo + 'static,
            T::AccountId: ::scale_info::TypeInfo + 'static,
            T::AccountId: ::scale_info::TypeInfo + 'static,
            BalanceOf<T>: ::scale_info::TypeInfo + 'static,
            T::AccountId: ::scale_info::TypeInfo + 'static,
            T::AccountId: ::scale_info::TypeInfo + 'static,
            BalanceOf<T>: ::scale_info::TypeInfo + 'static,
            T::AccountId: ::scale_info::TypeInfo + 'static,
            T::AccountId: ::scale_info::TypeInfo + 'static,
            BalanceOf<T>: ::scale_info::TypeInfo + 'static,
            frame_support::__private::sp_std::marker::PhantomData<
                (T),
            >: ::scale_info::TypeInfo + 'static,
            T: Config + 'static,
        {
            type Identity = Self;
            fn type_info() -> ::scale_info::Type {
                ::scale_info::Type::builder()
                    .path(::scale_info::Path::new("Event", "pallet_identity::pallet"))
                    .type_params(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                ::scale_info::TypeParameter::new(
                                    "T",
                                    ::core::option::Option::None,
                                ),
                            ]),
                        ),
                    )
                    .docs_always(&["The `Event` enum of this pallet"])
                    .variant(
                        ::scale_info::build::Variants::new()
                            .variant(
                                "IdentitySet",
                                |v| {
                                    v
                                        .index(0usize as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f.ty::<T::AccountId>().name("who").type_name("T::AccountId")
                                                }),
                                        )
                                        .docs_always(
                                            &[
                                                "A name was set or reset (which will remove all judgements).",
                                            ],
                                        )
                                },
                            )
                            .variant(
                                "IdentityCleared",
                                |v| {
                                    v
                                        .index(1usize as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f.ty::<T::AccountId>().name("who").type_name("T::AccountId")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<BalanceOf<T>>()
                                                        .name("deposit")
                                                        .type_name("BalanceOf<T>")
                                                }),
                                        )
                                        .docs_always(
                                            &["A name was cleared, and the given balance returned."],
                                        )
                                },
                            )
                            .variant(
                                "IdentityKilled",
                                |v| {
                                    v
                                        .index(2usize as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f.ty::<T::AccountId>().name("who").type_name("T::AccountId")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<BalanceOf<T>>()
                                                        .name("deposit")
                                                        .type_name("BalanceOf<T>")
                                                }),
                                        )
                                        .docs_always(
                                            &["A name was removed and the given balance slashed."],
                                        )
                                },
                            )
                            .variant(
                                "JudgementRequested",
                                |v| {
                                    v
                                        .index(3usize as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f.ty::<T::AccountId>().name("who").type_name("T::AccountId")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<RegistrarIndex>()
                                                        .name("registrar_index")
                                                        .type_name("RegistrarIndex")
                                                }),
                                        )
                                        .docs_always(&["A judgement was asked from a registrar."])
                                },
                            )
                            .variant(
                                "JudgementUnrequested",
                                |v| {
                                    v
                                        .index(4usize as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f.ty::<T::AccountId>().name("who").type_name("T::AccountId")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<RegistrarIndex>()
                                                        .name("registrar_index")
                                                        .type_name("RegistrarIndex")
                                                }),
                                        )
                                        .docs_always(&["A judgement request was retracted."])
                                },
                            )
                            .variant(
                                "JudgementGiven",
                                |v| {
                                    v
                                        .index(5usize as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .ty::<T::AccountId>()
                                                        .name("target")
                                                        .type_name("T::AccountId")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<RegistrarIndex>()
                                                        .name("registrar_index")
                                                        .type_name("RegistrarIndex")
                                                }),
                                        )
                                        .docs_always(&["A judgement was given by a registrar."])
                                },
                            )
                            .variant(
                                "RegistrarAdded",
                                |v| {
                                    v
                                        .index(6usize as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .ty::<RegistrarIndex>()
                                                        .name("registrar_index")
                                                        .type_name("RegistrarIndex")
                                                }),
                                        )
                                        .docs_always(&["A registrar was added."])
                                },
                            )
                            .variant(
                                "SubIdentityAdded",
                                |v| {
                                    v
                                        .index(7usize as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f.ty::<T::AccountId>().name("sub").type_name("T::AccountId")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<T::AccountId>()
                                                        .name("main")
                                                        .type_name("T::AccountId")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<BalanceOf<T>>()
                                                        .name("deposit")
                                                        .type_name("BalanceOf<T>")
                                                }),
                                        )
                                        .docs_always(
                                            &[
                                                "A sub-identity was added to an identity and the deposit paid.",
                                            ],
                                        )
                                },
                            )
                            .variant(
                                "SubIdentityRemoved",
                                |v| {
                                    v
                                        .index(8usize as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f.ty::<T::AccountId>().name("sub").type_name("T::AccountId")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<T::AccountId>()
                                                        .name("main")
                                                        .type_name("T::AccountId")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<BalanceOf<T>>()
                                                        .name("deposit")
                                                        .type_name("BalanceOf<T>")
                                                }),
                                        )
                                        .docs_always(
                                            &[
                                                "A sub-identity was removed from an identity and the deposit freed.",
                                            ],
                                        )
                                },
                            )
                            .variant(
                                "SubIdentityRevoked",
                                |v| {
                                    v
                                        .index(9usize as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f.ty::<T::AccountId>().name("sub").type_name("T::AccountId")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<T::AccountId>()
                                                        .name("main")
                                                        .type_name("T::AccountId")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<BalanceOf<T>>()
                                                        .name("deposit")
                                                        .type_name("BalanceOf<T>")
                                                }),
                                        )
                                        .docs_always(
                                            &[
                                                "A sub-identity was cleared, and the given deposit repatriated from the",
                                                "main identity account to the sub-identity account.",
                                            ],
                                        )
                                },
                            ),
                    )
            }
        }
    };
    /// Identity pallet declaration.
    impl<T: Config> Pallet<T> {
        /// Add a registrar to the system.
        ///
        /// The dispatch origin for this call must be `T::RegistrarOrigin`.
        ///
        /// - `account`: the account of the registrar.
        ///
        /// Emits `RegistrarAdded` if successful.
        ///
        /// ## Complexity
        /// - `O(R)` where `R` registrar-count (governance-bounded and code-bounded).
        pub fn add_registrar(
            origin: OriginFor<T>,
            account: AccountIdLookupOf<T>,
        ) -> DispatchResultWithPostInfo {
            frame_support::storage::with_storage_layer(|| {
                T::RegistrarOrigin::ensure_origin(origin)?;
                let account = T::Lookup::lookup(account)?;
                let (i, registrar_count) = <Registrars<
                    T,
                >>::try_mutate(|
                    registrars,
                | -> Result<(RegistrarIndex, usize), DispatchError> {
                    registrars
                        .try_push(
                            Some(RegistrarInfo {
                                account,
                                fee: Zero::zero(),
                                fields: Default::default(),
                            }),
                        )
                        .map_err(|_| Error::<T>::TooManyRegistrars)?;
                    Ok(((registrars.len() - 1) as RegistrarIndex, registrars.len()))
                })?;
                Self::deposit_event(Event::RegistrarAdded {
                    registrar_index: i,
                });
                Ok(Some(T::WeightInfo::add_registrar(registrar_count as u32)).into())
            })
        }
        /// Set an account's identity information and reserve the appropriate deposit.
        ///
        /// If the account already has identity information, the deposit is taken as part payment
        /// for the new deposit.
        ///
        /// The dispatch origin for this call must be _Signed_.
        ///
        /// - `info`: The identity information.
        ///
        /// Emits `IdentitySet` if successful.
        ///
        /// ## Complexity
        /// - `O(X + X' + R)`
        ///   - where `X` additional-field-count (deposit-bounded and code-bounded)
        ///   - where `R` judgements-count (registrar-count-bounded)
        pub fn set_identity(
            origin: OriginFor<T>,
            info: Box<IdentityInfo<T::MaxAdditionalFields>>,
        ) -> DispatchResultWithPostInfo {
            frame_support::storage::with_storage_layer(|| {
                let sender = ensure_signed(origin)?;
                let extra_fields = info.additional.len() as u32;
                {
                    if !(extra_fields <= T::MaxAdditionalFields::get()) {
                        { return Err(Error::<T>::TooManyFields.into()) };
                    }
                };
                let fd = <BalanceOf<T>>::from(extra_fields) * T::FieldDeposit::get();
                let mut id = match <IdentityOf<T>>::get(&sender) {
                    Some(mut id) => {
                        id.judgements.retain(|j| j.1.is_sticky());
                        id.info = *info;
                        id
                    }
                    None => {
                        Registration {
                            info: *info,
                            judgements: BoundedVec::default(),
                            deposit: Zero::zero(),
                        }
                    }
                };
                let old_deposit = id.deposit;
                id.deposit = T::BasicDeposit::get() + fd;
                if id.deposit > old_deposit {
                    T::Currency::reserve(&sender, id.deposit - old_deposit)?;
                }
                if old_deposit > id.deposit {
                    let err_amount = T::Currency::unreserve(
                        &sender,
                        old_deposit - id.deposit,
                    );
                    if true {
                        if !err_amount.is_zero() {
                            ::core::panicking::panic(
                                "assertion failed: err_amount.is_zero()",
                            )
                        }
                    }
                }
                let judgements = id.judgements.len();
                <IdentityOf<T>>::insert(&sender, id);
                Self::deposit_event(Event::IdentitySet { who: sender });
                Ok(
                    Some(T::WeightInfo::set_identity(judgements as u32, extra_fields))
                        .into(),
                )
            })
        }
        /// Set the sub-accounts of the sender.
        ///
        /// Payment: Any aggregate balance reserved by previous `set_subs` calls will be returned
        /// and an amount `SubAccountDeposit` will be reserved for each item in `subs`.
        ///
        /// The dispatch origin for this call must be _Signed_ and the sender must have a registered
        /// identity.
        ///
        /// - `subs`: The identity's (new) sub-accounts.
        ///
        /// ## Complexity
        /// - `O(P + S)`
        ///   - where `P` old-subs-count (hard- and deposit-bounded).
        ///   - where `S` subs-count (hard- and deposit-bounded).
        pub fn set_subs(
            origin: OriginFor<T>,
            subs: Vec<(T::AccountId, Data)>,
        ) -> DispatchResultWithPostInfo {
            frame_support::storage::with_storage_layer(|| {
                let sender = ensure_signed(origin)?;
                {
                    if !<IdentityOf<T>>::contains_key(&sender) {
                        { return Err(Error::<T>::NotFound.into()) };
                    }
                };
                {
                    if !(subs.len() <= T::MaxSubAccounts::get() as usize) {
                        { return Err(Error::<T>::TooManySubAccounts.into()) };
                    }
                };
                let (old_deposit, old_ids) = <SubsOf<T>>::get(&sender);
                let new_deposit = T::SubAccountDeposit::get()
                    * <BalanceOf<T>>::from(subs.len() as u32);
                let not_other_sub = subs
                    .iter()
                    .filter_map(|i| SuperOf::<T>::get(&i.0))
                    .all(|i| i.0 == sender);
                {
                    if !not_other_sub {
                        { return Err(Error::<T>::AlreadyClaimed.into()) };
                    }
                };
                if old_deposit < new_deposit {
                    T::Currency::reserve(&sender, new_deposit - old_deposit)?;
                } else if old_deposit > new_deposit {
                    let err_amount = T::Currency::unreserve(
                        &sender,
                        old_deposit - new_deposit,
                    );
                    if true {
                        if !err_amount.is_zero() {
                            ::core::panicking::panic(
                                "assertion failed: err_amount.is_zero()",
                            )
                        }
                    }
                }
                for s in old_ids.iter() {
                    <SuperOf<T>>::remove(s);
                }
                let mut ids = BoundedVec::<T::AccountId, T::MaxSubAccounts>::default();
                for (id, name) in subs {
                    <SuperOf<T>>::insert(&id, (sender.clone(), name));
                    ids.try_push(id)
                        .expect("subs length is less than T::MaxSubAccounts; qed");
                }
                let new_subs = ids.len();
                if ids.is_empty() {
                    <SubsOf<T>>::remove(&sender);
                } else {
                    <SubsOf<T>>::insert(&sender, (new_deposit, ids));
                }
                Ok(
                    Some(
                            T::WeightInfo::set_subs_old(old_ids.len() as u32)
                                .saturating_add(
                                    T::WeightInfo::set_subs_new(new_subs as u32),
                                ),
                        )
                        .into(),
                )
            })
        }
        /// Clear an account's identity info and all sub-accounts and return all deposits.
        ///
        /// Payment: All reserved balances on the account are returned.
        ///
        /// The dispatch origin for this call must be _Signed_ and the sender must have a registered
        /// identity.
        ///
        /// Emits `IdentityCleared` if successful.
        ///
        /// ## Complexity
        /// - `O(R + S + X)`
        ///   - where `R` registrar-count (governance-bounded).
        ///   - where `S` subs-count (hard- and deposit-bounded).
        ///   - where `X` additional-field-count (deposit-bounded and code-bounded).
        pub fn clear_identity(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            frame_support::storage::with_storage_layer(|| {
                let sender = ensure_signed(origin)?;
                let (subs_deposit, sub_ids) = <SubsOf<T>>::take(&sender);
                let id = <IdentityOf<T>>::take(&sender).ok_or(Error::<T>::NotNamed)?;
                let deposit = id.total_deposit() + subs_deposit;
                for sub in sub_ids.iter() {
                    <SuperOf<T>>::remove(sub);
                }
                let err_amount = T::Currency::unreserve(&sender, deposit);
                if true {
                    if !err_amount.is_zero() {
                        ::core::panicking::panic(
                            "assertion failed: err_amount.is_zero()",
                        )
                    }
                }
                Self::deposit_event(Event::IdentityCleared {
                    who: sender,
                    deposit,
                });
                Ok(
                    Some(
                            T::WeightInfo::clear_identity(
                                id.judgements.len() as u32,
                                sub_ids.len() as u32,
                                id.info.additional.len() as u32,
                            ),
                        )
                        .into(),
                )
            })
        }
        /// Request a judgement from a registrar.
        ///
        /// Payment: At most `max_fee` will be reserved for payment to the registrar if judgement
        /// given.
        ///
        /// The dispatch origin for this call must be _Signed_ and the sender must have a
        /// registered identity.
        ///
        /// - `reg_index`: The index of the registrar whose judgement is requested.
        /// - `max_fee`: The maximum fee that may be paid. This should just be auto-populated as:
        ///
        /// ```nocompile
        /// Self::registrars().get(reg_index).unwrap().fee
        /// ```
        ///
        /// Emits `JudgementRequested` if successful.
        ///
        /// ## Complexity
        /// - `O(R + X)`.
        ///   - where `R` registrar-count (governance-bounded).
        ///   - where `X` additional-field-count (deposit-bounded and code-bounded).
        pub fn request_judgement(
            origin: OriginFor<T>,
            reg_index: RegistrarIndex,
            max_fee: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            frame_support::storage::with_storage_layer(|| {
                let sender = ensure_signed(origin)?;
                let registrars = <Registrars<T>>::get();
                let registrar = registrars
                    .get(reg_index as usize)
                    .and_then(Option::as_ref)
                    .ok_or(Error::<T>::EmptyIndex)?;
                {
                    if !(max_fee >= registrar.fee) {
                        { return Err(Error::<T>::FeeChanged.into()) };
                    }
                };
                let mut id = <IdentityOf<T>>::get(&sender)
                    .ok_or(Error::<T>::NoIdentity)?;
                let item = (reg_index, Judgement::FeePaid(registrar.fee));
                match id.judgements.binary_search_by_key(&reg_index, |x| x.0) {
                    Ok(i) => {
                        if id.judgements[i].1.is_sticky() {
                            return Err(Error::<T>::StickyJudgement.into())
                        } else {
                            id.judgements[i] = item
                        }
                    }
                    Err(i) => {
                        id.judgements
                            .try_insert(i, item)
                            .map_err(|_| Error::<T>::TooManyRegistrars)?
                    }
                }
                T::Currency::reserve(&sender, registrar.fee)?;
                let judgements = id.judgements.len();
                let extra_fields = id.info.additional.len();
                <IdentityOf<T>>::insert(&sender, id);
                Self::deposit_event(Event::JudgementRequested {
                    who: sender,
                    registrar_index: reg_index,
                });
                Ok(
                    Some(
                            T::WeightInfo::request_judgement(
                                judgements as u32,
                                extra_fields as u32,
                            ),
                        )
                        .into(),
                )
            })
        }
        /// Cancel a previous request.
        ///
        /// Payment: A previously reserved deposit is returned on success.
        ///
        /// The dispatch origin for this call must be _Signed_ and the sender must have a
        /// registered identity.
        ///
        /// - `reg_index`: The index of the registrar whose judgement is no longer requested.
        ///
        /// Emits `JudgementUnrequested` if successful.
        ///
        /// ## Complexity
        /// - `O(R + X)`.
        ///   - where `R` registrar-count (governance-bounded).
        ///   - where `X` additional-field-count (deposit-bounded and code-bounded).
        pub fn cancel_request(
            origin: OriginFor<T>,
            reg_index: RegistrarIndex,
        ) -> DispatchResultWithPostInfo {
            frame_support::storage::with_storage_layer(|| {
                let sender = ensure_signed(origin)?;
                let mut id = <IdentityOf<T>>::get(&sender)
                    .ok_or(Error::<T>::NoIdentity)?;
                let pos = id
                    .judgements
                    .binary_search_by_key(&reg_index, |x| x.0)
                    .map_err(|_| Error::<T>::NotFound)?;
                let fee = if let Judgement::FeePaid(fee) = id.judgements.remove(pos).1 {
                    fee
                } else {
                    return Err(Error::<T>::JudgementGiven.into())
                };
                let err_amount = T::Currency::unreserve(&sender, fee);
                if true {
                    if !err_amount.is_zero() {
                        ::core::panicking::panic(
                            "assertion failed: err_amount.is_zero()",
                        )
                    }
                }
                let judgements = id.judgements.len();
                let extra_fields = id.info.additional.len();
                <IdentityOf<T>>::insert(&sender, id);
                Self::deposit_event(Event::JudgementUnrequested {
                    who: sender,
                    registrar_index: reg_index,
                });
                Ok(
                    Some(
                            T::WeightInfo::cancel_request(
                                judgements as u32,
                                extra_fields as u32,
                            ),
                        )
                        .into(),
                )
            })
        }
        /// Set the fee required for a judgement to be requested from a registrar.
        ///
        /// The dispatch origin for this call must be _Signed_ and the sender must be the account
        /// of the registrar whose index is `index`.
        ///
        /// - `index`: the index of the registrar whose fee is to be set.
        /// - `fee`: the new fee.
        ///
        /// ## Complexity
        /// - `O(R)`.
        ///   - where `R` registrar-count (governance-bounded).
        pub fn set_fee(
            origin: OriginFor<T>,
            index: RegistrarIndex,
            fee: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            frame_support::storage::with_storage_layer(|| {
                let who = ensure_signed(origin)?;
                let registrars = <Registrars<
                    T,
                >>::mutate(|rs| -> Result<usize, DispatchError> {
                    rs.get_mut(index as usize)
                        .and_then(|x| x.as_mut())
                        .and_then(|r| {
                            if r.account == who {
                                r.fee = fee;
                                Some(())
                            } else {
                                None
                            }
                        })
                        .ok_or_else(|| DispatchError::from(Error::<T>::InvalidIndex))?;
                    Ok(rs.len())
                })?;
                Ok(Some(T::WeightInfo::set_fee(registrars as u32)).into())
            })
        }
        /// Change the account associated with a registrar.
        ///
        /// The dispatch origin for this call must be _Signed_ and the sender must be the account
        /// of the registrar whose index is `index`.
        ///
        /// - `index`: the index of the registrar whose fee is to be set.
        /// - `new`: the new account ID.
        ///
        /// ## Complexity
        /// - `O(R)`.
        ///   - where `R` registrar-count (governance-bounded).
        pub fn set_account_id(
            origin: OriginFor<T>,
            index: RegistrarIndex,
            new: AccountIdLookupOf<T>,
        ) -> DispatchResultWithPostInfo {
            frame_support::storage::with_storage_layer(|| {
                let who = ensure_signed(origin)?;
                let new = T::Lookup::lookup(new)?;
                let registrars = <Registrars<
                    T,
                >>::mutate(|rs| -> Result<usize, DispatchError> {
                    rs.get_mut(index as usize)
                        .and_then(|x| x.as_mut())
                        .and_then(|r| {
                            if r.account == who {
                                r.account = new;
                                Some(())
                            } else {
                                None
                            }
                        })
                        .ok_or_else(|| DispatchError::from(Error::<T>::InvalidIndex))?;
                    Ok(rs.len())
                })?;
                Ok(Some(T::WeightInfo::set_account_id(registrars as u32)).into())
            })
        }
        /// Set the field information for a registrar.
        ///
        /// The dispatch origin for this call must be _Signed_ and the sender must be the account
        /// of the registrar whose index is `index`.
        ///
        /// - `index`: the index of the registrar whose fee is to be set.
        /// - `fields`: the fields that the registrar concerns themselves with.
        ///
        /// ## Complexity
        /// - `O(R)`.
        ///   - where `R` registrar-count (governance-bounded).
        pub fn set_fields(
            origin: OriginFor<T>,
            index: RegistrarIndex,
            fields: IdentityFields,
        ) -> DispatchResultWithPostInfo {
            frame_support::storage::with_storage_layer(|| {
                let who = ensure_signed(origin)?;
                let registrars = <Registrars<
                    T,
                >>::mutate(|rs| -> Result<usize, DispatchError> {
                    rs.get_mut(index as usize)
                        .and_then(|x| x.as_mut())
                        .and_then(|r| {
                            if r.account == who {
                                r.fields = fields;
                                Some(())
                            } else {
                                None
                            }
                        })
                        .ok_or_else(|| DispatchError::from(Error::<T>::InvalidIndex))?;
                    Ok(rs.len())
                })?;
                Ok(Some(T::WeightInfo::set_fields(registrars as u32)).into())
            })
        }
        /// Provide a judgement for an account's identity.
        ///
        /// The dispatch origin for this call must be _Signed_ and the sender must be the account
        /// of the registrar whose index is `reg_index`.
        ///
        /// - `reg_index`: the index of the registrar whose judgement is being made.
        /// - `target`: the account whose identity the judgement is upon. This must be an account
        ///   with a registered identity.
        /// - `judgement`: the judgement of the registrar of index `reg_index` about `target`.
        /// - `identity`: The hash of the [`IdentityInfo`] for that the judgement is provided.
        ///
        /// Emits `JudgementGiven` if successful.
        ///
        /// ## Complexity
        /// - `O(R + X)`.
        ///   - where `R` registrar-count (governance-bounded).
        ///   - where `X` additional-field-count (deposit-bounded and code-bounded).
        pub fn provide_judgement(
            origin: OriginFor<T>,
            reg_index: RegistrarIndex,
            target: AccountIdLookupOf<T>,
            judgement: Judgement<BalanceOf<T>>,
            identity: T::Hash,
        ) -> DispatchResultWithPostInfo {
            frame_support::storage::with_storage_layer(|| {
                let sender = ensure_signed(origin)?;
                let target = T::Lookup::lookup(target)?;
                {
                    if !!judgement.has_deposit() {
                        { return Err(Error::<T>::InvalidJudgement.into()) };
                    }
                };
                <Registrars<T>>::get()
                    .get(reg_index as usize)
                    .and_then(Option::as_ref)
                    .filter(|r| r.account == sender)
                    .ok_or(Error::<T>::InvalidIndex)?;
                let mut id = <IdentityOf<T>>::get(&target)
                    .ok_or(Error::<T>::InvalidTarget)?;
                if T::Hashing::hash_of(&id.info) != identity {
                    return Err(Error::<T>::JudgementForDifferentIdentity.into());
                }
                let item = (reg_index, judgement);
                match id.judgements.binary_search_by_key(&reg_index, |x| x.0) {
                    Ok(position) => {
                        if let Judgement::FeePaid(fee) = id.judgements[position].1 {
                            T::Currency::repatriate_reserved(
                                    &target,
                                    &sender,
                                    fee,
                                    BalanceStatus::Free,
                                )
                                .map_err(|_| Error::<T>::JudgementPaymentFailed)?;
                        }
                        id.judgements[position] = item;
                    }
                    Err(position) => {
                        id.judgements
                            .try_insert(position, item)
                            .map_err(|_| Error::<T>::TooManyRegistrars)?
                    }
                }
                let judgements = id.judgements.len();
                let extra_fields = id.info.additional.len();
                <IdentityOf<T>>::insert(&target, id);
                Self::deposit_event(Event::JudgementGiven {
                    target,
                    registrar_index: reg_index,
                });
                Ok(
                    Some(
                            T::WeightInfo::provide_judgement(
                                judgements as u32,
                                extra_fields as u32,
                            ),
                        )
                        .into(),
                )
            })
        }
        /// Remove an account's identity and sub-account information and slash the deposits.
        ///
        /// Payment: Reserved balances from `set_subs` and `set_identity` are slashed and handled by
        /// `Slash`. Verification request deposits are not returned; they should be cancelled
        /// manually using `cancel_request`.
        ///
        /// The dispatch origin for this call must match `T::ForceOrigin`.
        ///
        /// - `target`: the account whose identity the judgement is upon. This must be an account
        ///   with a registered identity.
        ///
        /// Emits `IdentityKilled` if successful.
        ///
        /// ## Complexity
        /// - `O(R + S + X)`
        ///   - where `R` registrar-count (governance-bounded).
        ///   - where `S` subs-count (hard- and deposit-bounded).
        ///   - where `X` additional-field-count (deposit-bounded and code-bounded).
        pub fn kill_identity(
            origin: OriginFor<T>,
            target: AccountIdLookupOf<T>,
        ) -> DispatchResultWithPostInfo {
            frame_support::storage::with_storage_layer(|| {
                T::ForceOrigin::ensure_origin(origin)?;
                let target = T::Lookup::lookup(target)?;
                let (subs_deposit, sub_ids) = <SubsOf<T>>::take(&target);
                let id = <IdentityOf<T>>::take(&target).ok_or(Error::<T>::NotNamed)?;
                let deposit = id.total_deposit() + subs_deposit;
                for sub in sub_ids.iter() {
                    <SuperOf<T>>::remove(sub);
                }
                T::Slashed::on_unbalanced(
                    T::Currency::slash_reserved(&target, deposit).0,
                );
                Self::deposit_event(Event::IdentityKilled {
                    who: target,
                    deposit,
                });
                Ok(
                    Some(
                            T::WeightInfo::kill_identity(
                                id.judgements.len() as u32,
                                sub_ids.len() as u32,
                                id.info.additional.len() as u32,
                            ),
                        )
                        .into(),
                )
            })
        }
        /// Add the given account to the sender's subs.
        ///
        /// Payment: Balance reserved by a previous `set_subs` call for one sub will be repatriated
        /// to the sender.
        ///
        /// The dispatch origin for this call must be _Signed_ and the sender must have a registered
        /// sub identity of `sub`.
        pub fn add_sub(
            origin: OriginFor<T>,
            sub: AccountIdLookupOf<T>,
            data: Data,
        ) -> DispatchResult {
            frame_support::storage::with_storage_layer(|| {
                let sender = ensure_signed(origin)?;
                let sub = T::Lookup::lookup(sub)?;
                {
                    if !IdentityOf::<T>::contains_key(&sender) {
                        { return Err(Error::<T>::NoIdentity.into()) };
                    }
                };
                {
                    if !!SuperOf::<T>::contains_key(&sub) {
                        { return Err(Error::<T>::AlreadyClaimed.into()) };
                    }
                };
                SubsOf::<
                    T,
                >::try_mutate(
                    &sender,
                    |(ref mut subs_deposit, ref mut sub_ids)| {
                        {
                            if !(sub_ids.len() < T::MaxSubAccounts::get() as usize) {
                                { return Err(Error::<T>::TooManySubAccounts.into()) };
                            }
                        };
                        let deposit = T::SubAccountDeposit::get();
                        T::Currency::reserve(&sender, deposit)?;
                        SuperOf::<T>::insert(&sub, (sender.clone(), data));
                        sub_ids
                            .try_push(sub.clone())
                            .expect("sub ids length checked above; qed");
                        *subs_deposit = subs_deposit.saturating_add(deposit);
                        Self::deposit_event(Event::SubIdentityAdded {
                            sub,
                            main: sender.clone(),
                            deposit,
                        });
                        Ok(())
                    },
                )
            })
        }
        /// Alter the associated name of the given sub-account.
        ///
        /// The dispatch origin for this call must be _Signed_ and the sender must have a registered
        /// sub identity of `sub`.
        pub fn rename_sub(
            origin: OriginFor<T>,
            sub: AccountIdLookupOf<T>,
            data: Data,
        ) -> DispatchResult {
            frame_support::storage::with_storage_layer(|| {
                let sender = ensure_signed(origin)?;
                let sub = T::Lookup::lookup(sub)?;
                {
                    if !IdentityOf::<T>::contains_key(&sender) {
                        { return Err(Error::<T>::NoIdentity.into()) };
                    }
                };
                {
                    if !SuperOf::<T>::get(&sub).map_or(false, |x| x.0 == sender) {
                        { return Err(Error::<T>::NotOwned.into()) };
                    }
                };
                SuperOf::<T>::insert(&sub, (sender, data));
                Ok(())
            })
        }
        /// Remove the given account from the sender's subs.
        ///
        /// Payment: Balance reserved by a previous `set_subs` call for one sub will be repatriated
        /// to the sender.
        ///
        /// The dispatch origin for this call must be _Signed_ and the sender must have a registered
        /// sub identity of `sub`.
        pub fn remove_sub(
            origin: OriginFor<T>,
            sub: AccountIdLookupOf<T>,
        ) -> DispatchResult {
            frame_support::storage::with_storage_layer(|| {
                let sender = ensure_signed(origin)?;
                {
                    if !IdentityOf::<T>::contains_key(&sender) {
                        { return Err(Error::<T>::NoIdentity.into()) };
                    }
                };
                let sub = T::Lookup::lookup(sub)?;
                let (sup, _) = SuperOf::<T>::get(&sub).ok_or(Error::<T>::NotSub)?;
                {
                    if !(sup == sender) {
                        { return Err(Error::<T>::NotOwned.into()) };
                    }
                };
                SuperOf::<T>::remove(&sub);
                SubsOf::<
                    T,
                >::mutate(
                    &sup,
                    |(ref mut subs_deposit, ref mut sub_ids)| {
                        sub_ids.retain(|x| x != &sub);
                        let deposit = T::SubAccountDeposit::get().min(*subs_deposit);
                        *subs_deposit -= deposit;
                        let err_amount = T::Currency::unreserve(&sender, deposit);
                        if true {
                            if !err_amount.is_zero() {
                                ::core::panicking::panic(
                                    "assertion failed: err_amount.is_zero()",
                                )
                            }
                        }
                        Self::deposit_event(Event::SubIdentityRemoved {
                            sub,
                            main: sender,
                            deposit,
                        });
                    },
                );
                Ok(())
            })
        }
        /// Remove the sender as a sub-account.
        ///
        /// Payment: Balance reserved by a previous `set_subs` call for one sub will be repatriated
        /// to the sender (*not* the original depositor).
        ///
        /// The dispatch origin for this call must be _Signed_ and the sender must have a registered
        /// super-identity.
        ///
        /// NOTE: This should not normally be used, but is provided in the case that the non-
        /// controller of an account is maliciously registered as a sub-account.
        pub fn quit_sub(origin: OriginFor<T>) -> DispatchResult {
            frame_support::storage::with_storage_layer(|| {
                let sender = ensure_signed(origin)?;
                let (sup, _) = SuperOf::<T>::take(&sender).ok_or(Error::<T>::NotSub)?;
                SubsOf::<
                    T,
                >::mutate(
                    &sup,
                    |(ref mut subs_deposit, ref mut sub_ids)| {
                        sub_ids.retain(|x| x != &sender);
                        let deposit = T::SubAccountDeposit::get().min(*subs_deposit);
                        *subs_deposit -= deposit;
                        let _ = T::Currency::repatriate_reserved(
                            &sup,
                            &sender,
                            deposit,
                            BalanceStatus::Free,
                        );
                        Self::deposit_event(Event::SubIdentityRevoked {
                            sub: sender,
                            main: sup.clone(),
                            deposit,
                        });
                    },
                );
                Ok(())
            })
        }
    }
    impl<T: Config> Pallet<T> {
        #[doc(hidden)]
        pub fn pallet_documentation_metadata() -> frame_support::__private::sp_std::vec::Vec<
            &'static str,
        > {
            ::alloc::vec::Vec::new()
        }
    }
    impl<T: Config> Pallet<T> {
        #[doc(hidden)]
        pub fn pallet_constants_metadata() -> frame_support::__private::sp_std::vec::Vec<
            frame_support::__private::metadata_ir::PalletConstantMetadataIR,
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    {
                        frame_support::__private::metadata_ir::PalletConstantMetadataIR {
                            name: "BasicDeposit",
                            ty: frame_support::__private::scale_info::meta_type::<
                                BalanceOf<T>,
                            >(),
                            value: {
                                let value = <<T as Config>::BasicDeposit as frame_support::traits::Get<
                                    BalanceOf<T>,
                                >>::get();
                                frame_support::__private::codec::Encode::encode(&value)
                            },
                            docs: <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    " The amount held on deposit for a registered identity",
                                ]),
                            ),
                        }
                    },
                    {
                        frame_support::__private::metadata_ir::PalletConstantMetadataIR {
                            name: "FieldDeposit",
                            ty: frame_support::__private::scale_info::meta_type::<
                                BalanceOf<T>,
                            >(),
                            value: {
                                let value = <<T as Config>::FieldDeposit as frame_support::traits::Get<
                                    BalanceOf<T>,
                                >>::get();
                                frame_support::__private::codec::Encode::encode(&value)
                            },
                            docs: <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    " The amount held on deposit per additional field for a registered identity.",
                                ]),
                            ),
                        }
                    },
                    {
                        frame_support::__private::metadata_ir::PalletConstantMetadataIR {
                            name: "SubAccountDeposit",
                            ty: frame_support::__private::scale_info::meta_type::<
                                BalanceOf<T>,
                            >(),
                            value: {
                                let value = <<T as Config>::SubAccountDeposit as frame_support::traits::Get<
                                    BalanceOf<T>,
                                >>::get();
                                frame_support::__private::codec::Encode::encode(&value)
                            },
                            docs: <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    " The amount held on deposit for a registered subaccount. This should account for the fact",
                                    " that one storage item\'s value will increase by the size of an account ID, and there will",
                                    " be another trie item whose value is the size of an account ID plus 32 bytes.",
                                ]),
                            ),
                        }
                    },
                    {
                        frame_support::__private::metadata_ir::PalletConstantMetadataIR {
                            name: "MaxSubAccounts",
                            ty: frame_support::__private::scale_info::meta_type::<u32>(),
                            value: {
                                let value = <<T as Config>::MaxSubAccounts as frame_support::traits::Get<
                                    u32,
                                >>::get();
                                frame_support::__private::codec::Encode::encode(&value)
                            },
                            docs: <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    " The maximum number of sub-accounts allowed per identified account.",
                                ]),
                            ),
                        }
                    },
                    {
                        frame_support::__private::metadata_ir::PalletConstantMetadataIR {
                            name: "MaxAdditionalFields",
                            ty: frame_support::__private::scale_info::meta_type::<u32>(),
                            value: {
                                let value = <<T as Config>::MaxAdditionalFields as frame_support::traits::Get<
                                    u32,
                                >>::get();
                                frame_support::__private::codec::Encode::encode(&value)
                            },
                            docs: <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    " Maximum number of additional fields that may be stored in an ID. Needed to bound the I/O",
                                    " required to access an identity, but can be pretty high.",
                                ]),
                            ),
                        }
                    },
                    {
                        frame_support::__private::metadata_ir::PalletConstantMetadataIR {
                            name: "MaxRegistrars",
                            ty: frame_support::__private::scale_info::meta_type::<u32>(),
                            value: {
                                let value = <<T as Config>::MaxRegistrars as frame_support::traits::Get<
                                    u32,
                                >>::get();
                                frame_support::__private::codec::Encode::encode(&value)
                            },
                            docs: <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    " Maxmimum number of registrars allowed in the system. Needed to bound the complexity",
                                    " of, e.g., updating judgements.",
                                ]),
                            ),
                        }
                    },
                ]),
            )
        }
    }
    impl<T: Config> Pallet<T> {
        #[doc(hidden)]
        pub fn error_metadata() -> Option<
            frame_support::__private::metadata_ir::PalletErrorMetadataIR,
        > {
            Some(frame_support::__private::metadata_ir::PalletErrorMetadataIR {
                ty: frame_support::__private::scale_info::meta_type::<Error<T>>(),
            })
        }
    }
    /// Type alias to `Pallet`, to be used by `construct_runtime`.
    ///
    /// Generated by `pallet` attribute macro.
    #[deprecated(note = "use `Pallet` instead")]
    #[allow(dead_code)]
    pub type Module<T> = Pallet<T>;
    impl<T: Config> frame_support::traits::GetStorageVersion for Pallet<T> {
        type CurrentStorageVersion = frame_support::traits::NoStorageVersionSet;
        fn current_storage_version() -> Self::CurrentStorageVersion {
            core::default::Default::default()
        }
        fn on_chain_storage_version() -> frame_support::traits::StorageVersion {
            frame_support::traits::StorageVersion::get::<Self>()
        }
    }
    impl<T: Config> frame_support::traits::OnGenesis for Pallet<T> {
        fn on_genesis() {
            let storage_version: frame_support::traits::StorageVersion = core::default::Default::default();
            storage_version.put::<Self>();
        }
    }
    impl<T: Config> frame_support::traits::PalletInfoAccess for Pallet<T> {
        fn index() -> usize {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::index::<
                Self,
            >()
                .expect(
                    "Pallet is part of the runtime because pallet `Config` trait is \
						implemented by the runtime",
                )
        }
        fn name() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<
                Self,
            >()
                .expect(
                    "Pallet is part of the runtime because pallet `Config` trait is \
						implemented by the runtime",
                )
        }
        fn name_hash() -> [u8; 16] {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name_hash::<
                Self,
            >()
                .expect(
                    "Pallet is part of the runtime because pallet `Config` trait is \
						implemented by the runtime",
                )
        }
        fn module_name() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::module_name::<
                Self,
            >()
                .expect(
                    "Pallet is part of the runtime because pallet `Config` trait is \
						implemented by the runtime",
                )
        }
        fn crate_version() -> frame_support::traits::CrateVersion {
            frame_support::traits::CrateVersion {
                major: 4u16,
                minor: 0u8,
                patch: 0u8,
            }
        }
    }
    impl<T: Config> frame_support::traits::PalletsInfoAccess for Pallet<T> {
        fn count() -> usize {
            1
        }
        fn infos() -> frame_support::__private::sp_std::vec::Vec<
            frame_support::traits::PalletInfoData,
        > {
            use frame_support::traits::PalletInfoAccess;
            let item = frame_support::traits::PalletInfoData {
                index: Self::index(),
                name: Self::name(),
                module_name: Self::module_name(),
                crate_version: Self::crate_version(),
            };
            <[_]>::into_vec(#[rustc_box] ::alloc::boxed::Box::new([item]))
        }
    }
    impl<T: Config> frame_support::traits::StorageInfoTrait for Pallet<T> {
        fn storage_info() -> frame_support::__private::sp_std::vec::Vec<
            frame_support::traits::StorageInfo,
        > {
            #[allow(unused_mut)]
            let mut res = ::alloc::vec::Vec::new();
            {
                let mut storage_info = <IdentityOf<
                    T,
                > as frame_support::traits::StorageInfoTrait>::storage_info();
                res.append(&mut storage_info);
            }
            {
                let mut storage_info = <SuperOf<
                    T,
                > as frame_support::traits::StorageInfoTrait>::storage_info();
                res.append(&mut storage_info);
            }
            {
                let mut storage_info = <SubsOf<
                    T,
                > as frame_support::traits::StorageInfoTrait>::storage_info();
                res.append(&mut storage_info);
            }
            {
                let mut storage_info = <Registrars<
                    T,
                > as frame_support::traits::StorageInfoTrait>::storage_info();
                res.append(&mut storage_info);
            }
            res
        }
    }
    use frame_support::traits::{
        StorageInfoTrait, TrackedStorageKey, WhitelistedStorageKeys,
    };
    impl<T: Config> WhitelistedStorageKeys for Pallet<T> {
        fn whitelisted_storage_keys() -> frame_support::__private::sp_std::vec::Vec<
            TrackedStorageKey,
        > {
            use frame_support::__private::sp_std::vec;
            ::alloc::vec::Vec::new()
        }
    }
    mod warnings {}
    #[doc(hidden)]
    pub mod __substrate_call_check {
        #[doc(hidden)]
        pub use __is_call_part_defined_0 as is_call_part_defined;
    }
    /// Identity pallet declaration.
    #[codec(encode_bound())]
    #[codec(decode_bound())]
    #[scale_info(skip_type_params(T), capture_docs = "always")]
    #[allow(non_camel_case_types)]
    pub enum Call<T: Config> {
        #[doc(hidden)]
        #[codec(skip)]
        __Ignore(
            frame_support::__private::sp_std::marker::PhantomData<(T,)>,
            frame_support::Never,
        ),
        ///See [`Pallet::add_registrar`].
        #[codec(index = 0u8)]
        add_registrar { #[allow(missing_docs)] account: AccountIdLookupOf<T> },
        ///See [`Pallet::set_identity`].
        #[codec(index = 1u8)]
        set_identity {
            #[allow(missing_docs)]
            info: Box<IdentityInfo<T::MaxAdditionalFields>>,
        },
        ///See [`Pallet::set_subs`].
        #[codec(index = 2u8)]
        set_subs { #[allow(missing_docs)] subs: Vec<(T::AccountId, Data)> },
        ///See [`Pallet::clear_identity`].
        #[codec(index = 3u8)]
        clear_identity {},
        ///See [`Pallet::request_judgement`].
        #[codec(index = 4u8)]
        request_judgement {
            #[allow(missing_docs)]
            #[codec(compact)]
            reg_index: RegistrarIndex,
            #[allow(missing_docs)]
            #[codec(compact)]
            max_fee: BalanceOf<T>,
        },
        ///See [`Pallet::cancel_request`].
        #[codec(index = 5u8)]
        cancel_request { #[allow(missing_docs)] reg_index: RegistrarIndex },
        ///See [`Pallet::set_fee`].
        #[codec(index = 6u8)]
        set_fee {
            #[allow(missing_docs)]
            #[codec(compact)]
            index: RegistrarIndex,
            #[allow(missing_docs)]
            #[codec(compact)]
            fee: BalanceOf<T>,
        },
        ///See [`Pallet::set_account_id`].
        #[codec(index = 7u8)]
        set_account_id {
            #[allow(missing_docs)]
            #[codec(compact)]
            index: RegistrarIndex,
            #[allow(missing_docs)]
            new: AccountIdLookupOf<T>,
        },
        ///See [`Pallet::set_fields`].
        #[codec(index = 8u8)]
        set_fields {
            #[allow(missing_docs)]
            #[codec(compact)]
            index: RegistrarIndex,
            #[allow(missing_docs)]
            fields: IdentityFields,
        },
        ///See [`Pallet::provide_judgement`].
        #[codec(index = 9u8)]
        provide_judgement {
            #[allow(missing_docs)]
            #[codec(compact)]
            reg_index: RegistrarIndex,
            #[allow(missing_docs)]
            target: AccountIdLookupOf<T>,
            #[allow(missing_docs)]
            judgement: Judgement<BalanceOf<T>>,
            #[allow(missing_docs)]
            identity: T::Hash,
        },
        ///See [`Pallet::kill_identity`].
        #[codec(index = 10u8)]
        kill_identity { #[allow(missing_docs)] target: AccountIdLookupOf<T> },
        ///See [`Pallet::add_sub`].
        #[codec(index = 11u8)]
        add_sub {
            #[allow(missing_docs)]
            sub: AccountIdLookupOf<T>,
            #[allow(missing_docs)]
            data: Data,
        },
        ///See [`Pallet::rename_sub`].
        #[codec(index = 12u8)]
        rename_sub {
            #[allow(missing_docs)]
            sub: AccountIdLookupOf<T>,
            #[allow(missing_docs)]
            data: Data,
        },
        ///See [`Pallet::remove_sub`].
        #[codec(index = 13u8)]
        remove_sub { #[allow(missing_docs)] sub: AccountIdLookupOf<T> },
        ///See [`Pallet::quit_sub`].
        #[codec(index = 14u8)]
        quit_sub {},
    }
    const _: () = {
        impl<T: Config> ::core::fmt::Debug for Call<T> {
            fn fmt(&self, fmt: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                match *self {
                    Self::__Ignore(ref _0, ref _1) => {
                        fmt.debug_tuple("Call::__Ignore").field(&_0).field(&_1).finish()
                    }
                    Self::add_registrar { ref account } => {
                        fmt.debug_struct("Call::add_registrar")
                            .field("account", &account)
                            .finish()
                    }
                    Self::set_identity { ref info } => {
                        fmt.debug_struct("Call::set_identity")
                            .field("info", &info)
                            .finish()
                    }
                    Self::set_subs { ref subs } => {
                        fmt.debug_struct("Call::set_subs").field("subs", &subs).finish()
                    }
                    Self::clear_identity {} => {
                        fmt.debug_struct("Call::clear_identity").finish()
                    }
                    Self::request_judgement { ref reg_index, ref max_fee } => {
                        fmt.debug_struct("Call::request_judgement")
                            .field("reg_index", &reg_index)
                            .field("max_fee", &max_fee)
                            .finish()
                    }
                    Self::cancel_request { ref reg_index } => {
                        fmt.debug_struct("Call::cancel_request")
                            .field("reg_index", &reg_index)
                            .finish()
                    }
                    Self::set_fee { ref index, ref fee } => {
                        fmt.debug_struct("Call::set_fee")
                            .field("index", &index)
                            .field("fee", &fee)
                            .finish()
                    }
                    Self::set_account_id { ref index, ref new } => {
                        fmt.debug_struct("Call::set_account_id")
                            .field("index", &index)
                            .field("new", &new)
                            .finish()
                    }
                    Self::set_fields { ref index, ref fields } => {
                        fmt.debug_struct("Call::set_fields")
                            .field("index", &index)
                            .field("fields", &fields)
                            .finish()
                    }
                    Self::provide_judgement {
                        ref reg_index,
                        ref target,
                        ref judgement,
                        ref identity,
                    } => {
                        fmt.debug_struct("Call::provide_judgement")
                            .field("reg_index", &reg_index)
                            .field("target", &target)
                            .field("judgement", &judgement)
                            .field("identity", &identity)
                            .finish()
                    }
                    Self::kill_identity { ref target } => {
                        fmt.debug_struct("Call::kill_identity")
                            .field("target", &target)
                            .finish()
                    }
                    Self::add_sub { ref sub, ref data } => {
                        fmt.debug_struct("Call::add_sub")
                            .field("sub", &sub)
                            .field("data", &data)
                            .finish()
                    }
                    Self::rename_sub { ref sub, ref data } => {
                        fmt.debug_struct("Call::rename_sub")
                            .field("sub", &sub)
                            .field("data", &data)
                            .finish()
                    }
                    Self::remove_sub { ref sub } => {
                        fmt.debug_struct("Call::remove_sub").field("sub", &sub).finish()
                    }
                    Self::quit_sub {} => fmt.debug_struct("Call::quit_sub").finish(),
                }
            }
        }
    };
    const _: () = {
        impl<T: Config> ::core::clone::Clone for Call<T> {
            fn clone(&self) -> Self {
                match self {
                    Self::__Ignore(ref _0, ref _1) => {
                        Self::__Ignore(
                            ::core::clone::Clone::clone(_0),
                            ::core::clone::Clone::clone(_1),
                        )
                    }
                    Self::add_registrar { ref account } => {
                        Self::add_registrar {
                            account: ::core::clone::Clone::clone(account),
                        }
                    }
                    Self::set_identity { ref info } => {
                        Self::set_identity {
                            info: ::core::clone::Clone::clone(info),
                        }
                    }
                    Self::set_subs { ref subs } => {
                        Self::set_subs {
                            subs: ::core::clone::Clone::clone(subs),
                        }
                    }
                    Self::clear_identity {} => Self::clear_identity {},
                    Self::request_judgement { ref reg_index, ref max_fee } => {
                        Self::request_judgement {
                            reg_index: ::core::clone::Clone::clone(reg_index),
                            max_fee: ::core::clone::Clone::clone(max_fee),
                        }
                    }
                    Self::cancel_request { ref reg_index } => {
                        Self::cancel_request {
                            reg_index: ::core::clone::Clone::clone(reg_index),
                        }
                    }
                    Self::set_fee { ref index, ref fee } => {
                        Self::set_fee {
                            index: ::core::clone::Clone::clone(index),
                            fee: ::core::clone::Clone::clone(fee),
                        }
                    }
                    Self::set_account_id { ref index, ref new } => {
                        Self::set_account_id {
                            index: ::core::clone::Clone::clone(index),
                            new: ::core::clone::Clone::clone(new),
                        }
                    }
                    Self::set_fields { ref index, ref fields } => {
                        Self::set_fields {
                            index: ::core::clone::Clone::clone(index),
                            fields: ::core::clone::Clone::clone(fields),
                        }
                    }
                    Self::provide_judgement {
                        ref reg_index,
                        ref target,
                        ref judgement,
                        ref identity,
                    } => {
                        Self::provide_judgement {
                            reg_index: ::core::clone::Clone::clone(reg_index),
                            target: ::core::clone::Clone::clone(target),
                            judgement: ::core::clone::Clone::clone(judgement),
                            identity: ::core::clone::Clone::clone(identity),
                        }
                    }
                    Self::kill_identity { ref target } => {
                        Self::kill_identity {
                            target: ::core::clone::Clone::clone(target),
                        }
                    }
                    Self::add_sub { ref sub, ref data } => {
                        Self::add_sub {
                            sub: ::core::clone::Clone::clone(sub),
                            data: ::core::clone::Clone::clone(data),
                        }
                    }
                    Self::rename_sub { ref sub, ref data } => {
                        Self::rename_sub {
                            sub: ::core::clone::Clone::clone(sub),
                            data: ::core::clone::Clone::clone(data),
                        }
                    }
                    Self::remove_sub { ref sub } => {
                        Self::remove_sub {
                            sub: ::core::clone::Clone::clone(sub),
                        }
                    }
                    Self::quit_sub {} => Self::quit_sub {},
                }
            }
        }
    };
    const _: () = {
        impl<T: Config> ::core::cmp::Eq for Call<T> {}
    };
    const _: () = {
        impl<T: Config> ::core::cmp::PartialEq for Call<T> {
            fn eq(&self, other: &Self) -> bool {
                match (self, other) {
                    (Self::__Ignore(_0, _1), Self::__Ignore(_0_other, _1_other)) => {
                        true && _0 == _0_other && _1 == _1_other
                    }
                    (
                        Self::add_registrar { account },
                        Self::add_registrar { account: _0 },
                    ) => true && account == _0,
                    (Self::set_identity { info }, Self::set_identity { info: _0 }) => {
                        true && info == _0
                    }
                    (Self::set_subs { subs }, Self::set_subs { subs: _0 }) => {
                        true && subs == _0
                    }
                    (Self::clear_identity {}, Self::clear_identity {}) => true,
                    (
                        Self::request_judgement { reg_index, max_fee },
                        Self::request_judgement { reg_index: _0, max_fee: _1 },
                    ) => true && reg_index == _0 && max_fee == _1,
                    (
                        Self::cancel_request { reg_index },
                        Self::cancel_request { reg_index: _0 },
                    ) => true && reg_index == _0,
                    (
                        Self::set_fee { index, fee },
                        Self::set_fee { index: _0, fee: _1 },
                    ) => true && index == _0 && fee == _1,
                    (
                        Self::set_account_id { index, new },
                        Self::set_account_id { index: _0, new: _1 },
                    ) => true && index == _0 && new == _1,
                    (
                        Self::set_fields { index, fields },
                        Self::set_fields { index: _0, fields: _1 },
                    ) => true && index == _0 && fields == _1,
                    (
                        Self::provide_judgement {
                            reg_index,
                            target,
                            judgement,
                            identity,
                        },
                        Self::provide_judgement {
                            reg_index: _0,
                            target: _1,
                            judgement: _2,
                            identity: _3,
                        },
                    ) => {
                        true && reg_index == _0 && target == _1 && judgement == _2
                            && identity == _3
                    }
                    (
                        Self::kill_identity { target },
                        Self::kill_identity { target: _0 },
                    ) => true && target == _0,
                    (
                        Self::add_sub { sub, data },
                        Self::add_sub { sub: _0, data: _1 },
                    ) => true && sub == _0 && data == _1,
                    (
                        Self::rename_sub { sub, data },
                        Self::rename_sub { sub: _0, data: _1 },
                    ) => true && sub == _0 && data == _1,
                    (Self::remove_sub { sub }, Self::remove_sub { sub: _0 }) => {
                        true && sub == _0
                    }
                    (Self::quit_sub {}, Self::quit_sub {}) => true,
                    (Self::__Ignore { .. }, Self::add_registrar { .. }) => false,
                    (Self::__Ignore { .. }, Self::set_identity { .. }) => false,
                    (Self::__Ignore { .. }, Self::set_subs { .. }) => false,
                    (Self::__Ignore { .. }, Self::clear_identity { .. }) => false,
                    (Self::__Ignore { .. }, Self::request_judgement { .. }) => false,
                    (Self::__Ignore { .. }, Self::cancel_request { .. }) => false,
                    (Self::__Ignore { .. }, Self::set_fee { .. }) => false,
                    (Self::__Ignore { .. }, Self::set_account_id { .. }) => false,
                    (Self::__Ignore { .. }, Self::set_fields { .. }) => false,
                    (Self::__Ignore { .. }, Self::provide_judgement { .. }) => false,
                    (Self::__Ignore { .. }, Self::kill_identity { .. }) => false,
                    (Self::__Ignore { .. }, Self::add_sub { .. }) => false,
                    (Self::__Ignore { .. }, Self::rename_sub { .. }) => false,
                    (Self::__Ignore { .. }, Self::remove_sub { .. }) => false,
                    (Self::__Ignore { .. }, Self::quit_sub { .. }) => false,
                    (Self::add_registrar { .. }, Self::__Ignore { .. }) => false,
                    (Self::add_registrar { .. }, Self::set_identity { .. }) => false,
                    (Self::add_registrar { .. }, Self::set_subs { .. }) => false,
                    (Self::add_registrar { .. }, Self::clear_identity { .. }) => false,
                    (Self::add_registrar { .. }, Self::request_judgement { .. }) => false,
                    (Self::add_registrar { .. }, Self::cancel_request { .. }) => false,
                    (Self::add_registrar { .. }, Self::set_fee { .. }) => false,
                    (Self::add_registrar { .. }, Self::set_account_id { .. }) => false,
                    (Self::add_registrar { .. }, Self::set_fields { .. }) => false,
                    (Self::add_registrar { .. }, Self::provide_judgement { .. }) => false,
                    (Self::add_registrar { .. }, Self::kill_identity { .. }) => false,
                    (Self::add_registrar { .. }, Self::add_sub { .. }) => false,
                    (Self::add_registrar { .. }, Self::rename_sub { .. }) => false,
                    (Self::add_registrar { .. }, Self::remove_sub { .. }) => false,
                    (Self::add_registrar { .. }, Self::quit_sub { .. }) => false,
                    (Self::set_identity { .. }, Self::__Ignore { .. }) => false,
                    (Self::set_identity { .. }, Self::add_registrar { .. }) => false,
                    (Self::set_identity { .. }, Self::set_subs { .. }) => false,
                    (Self::set_identity { .. }, Self::clear_identity { .. }) => false,
                    (Self::set_identity { .. }, Self::request_judgement { .. }) => false,
                    (Self::set_identity { .. }, Self::cancel_request { .. }) => false,
                    (Self::set_identity { .. }, Self::set_fee { .. }) => false,
                    (Self::set_identity { .. }, Self::set_account_id { .. }) => false,
                    (Self::set_identity { .. }, Self::set_fields { .. }) => false,
                    (Self::set_identity { .. }, Self::provide_judgement { .. }) => false,
                    (Self::set_identity { .. }, Self::kill_identity { .. }) => false,
                    (Self::set_identity { .. }, Self::add_sub { .. }) => false,
                    (Self::set_identity { .. }, Self::rename_sub { .. }) => false,
                    (Self::set_identity { .. }, Self::remove_sub { .. }) => false,
                    (Self::set_identity { .. }, Self::quit_sub { .. }) => false,
                    (Self::set_subs { .. }, Self::__Ignore { .. }) => false,
                    (Self::set_subs { .. }, Self::add_registrar { .. }) => false,
                    (Self::set_subs { .. }, Self::set_identity { .. }) => false,
                    (Self::set_subs { .. }, Self::clear_identity { .. }) => false,
                    (Self::set_subs { .. }, Self::request_judgement { .. }) => false,
                    (Self::set_subs { .. }, Self::cancel_request { .. }) => false,
                    (Self::set_subs { .. }, Self::set_fee { .. }) => false,
                    (Self::set_subs { .. }, Self::set_account_id { .. }) => false,
                    (Self::set_subs { .. }, Self::set_fields { .. }) => false,
                    (Self::set_subs { .. }, Self::provide_judgement { .. }) => false,
                    (Self::set_subs { .. }, Self::kill_identity { .. }) => false,
                    (Self::set_subs { .. }, Self::add_sub { .. }) => false,
                    (Self::set_subs { .. }, Self::rename_sub { .. }) => false,
                    (Self::set_subs { .. }, Self::remove_sub { .. }) => false,
                    (Self::set_subs { .. }, Self::quit_sub { .. }) => false,
                    (Self::clear_identity { .. }, Self::__Ignore { .. }) => false,
                    (Self::clear_identity { .. }, Self::add_registrar { .. }) => false,
                    (Self::clear_identity { .. }, Self::set_identity { .. }) => false,
                    (Self::clear_identity { .. }, Self::set_subs { .. }) => false,
                    (Self::clear_identity { .. }, Self::request_judgement { .. }) => {
                        false
                    }
                    (Self::clear_identity { .. }, Self::cancel_request { .. }) => false,
                    (Self::clear_identity { .. }, Self::set_fee { .. }) => false,
                    (Self::clear_identity { .. }, Self::set_account_id { .. }) => false,
                    (Self::clear_identity { .. }, Self::set_fields { .. }) => false,
                    (Self::clear_identity { .. }, Self::provide_judgement { .. }) => {
                        false
                    }
                    (Self::clear_identity { .. }, Self::kill_identity { .. }) => false,
                    (Self::clear_identity { .. }, Self::add_sub { .. }) => false,
                    (Self::clear_identity { .. }, Self::rename_sub { .. }) => false,
                    (Self::clear_identity { .. }, Self::remove_sub { .. }) => false,
                    (Self::clear_identity { .. }, Self::quit_sub { .. }) => false,
                    (Self::request_judgement { .. }, Self::__Ignore { .. }) => false,
                    (Self::request_judgement { .. }, Self::add_registrar { .. }) => false,
                    (Self::request_judgement { .. }, Self::set_identity { .. }) => false,
                    (Self::request_judgement { .. }, Self::set_subs { .. }) => false,
                    (Self::request_judgement { .. }, Self::clear_identity { .. }) => {
                        false
                    }
                    (Self::request_judgement { .. }, Self::cancel_request { .. }) => {
                        false
                    }
                    (Self::request_judgement { .. }, Self::set_fee { .. }) => false,
                    (Self::request_judgement { .. }, Self::set_account_id { .. }) => {
                        false
                    }
                    (Self::request_judgement { .. }, Self::set_fields { .. }) => false,
                    (Self::request_judgement { .. }, Self::provide_judgement { .. }) => {
                        false
                    }
                    (Self::request_judgement { .. }, Self::kill_identity { .. }) => false,
                    (Self::request_judgement { .. }, Self::add_sub { .. }) => false,
                    (Self::request_judgement { .. }, Self::rename_sub { .. }) => false,
                    (Self::request_judgement { .. }, Self::remove_sub { .. }) => false,
                    (Self::request_judgement { .. }, Self::quit_sub { .. }) => false,
                    (Self::cancel_request { .. }, Self::__Ignore { .. }) => false,
                    (Self::cancel_request { .. }, Self::add_registrar { .. }) => false,
                    (Self::cancel_request { .. }, Self::set_identity { .. }) => false,
                    (Self::cancel_request { .. }, Self::set_subs { .. }) => false,
                    (Self::cancel_request { .. }, Self::clear_identity { .. }) => false,
                    (Self::cancel_request { .. }, Self::request_judgement { .. }) => {
                        false
                    }
                    (Self::cancel_request { .. }, Self::set_fee { .. }) => false,
                    (Self::cancel_request { .. }, Self::set_account_id { .. }) => false,
                    (Self::cancel_request { .. }, Self::set_fields { .. }) => false,
                    (Self::cancel_request { .. }, Self::provide_judgement { .. }) => {
                        false
                    }
                    (Self::cancel_request { .. }, Self::kill_identity { .. }) => false,
                    (Self::cancel_request { .. }, Self::add_sub { .. }) => false,
                    (Self::cancel_request { .. }, Self::rename_sub { .. }) => false,
                    (Self::cancel_request { .. }, Self::remove_sub { .. }) => false,
                    (Self::cancel_request { .. }, Self::quit_sub { .. }) => false,
                    (Self::set_fee { .. }, Self::__Ignore { .. }) => false,
                    (Self::set_fee { .. }, Self::add_registrar { .. }) => false,
                    (Self::set_fee { .. }, Self::set_identity { .. }) => false,
                    (Self::set_fee { .. }, Self::set_subs { .. }) => false,
                    (Self::set_fee { .. }, Self::clear_identity { .. }) => false,
                    (Self::set_fee { .. }, Self::request_judgement { .. }) => false,
                    (Self::set_fee { .. }, Self::cancel_request { .. }) => false,
                    (Self::set_fee { .. }, Self::set_account_id { .. }) => false,
                    (Self::set_fee { .. }, Self::set_fields { .. }) => false,
                    (Self::set_fee { .. }, Self::provide_judgement { .. }) => false,
                    (Self::set_fee { .. }, Self::kill_identity { .. }) => false,
                    (Self::set_fee { .. }, Self::add_sub { .. }) => false,
                    (Self::set_fee { .. }, Self::rename_sub { .. }) => false,
                    (Self::set_fee { .. }, Self::remove_sub { .. }) => false,
                    (Self::set_fee { .. }, Self::quit_sub { .. }) => false,
                    (Self::set_account_id { .. }, Self::__Ignore { .. }) => false,
                    (Self::set_account_id { .. }, Self::add_registrar { .. }) => false,
                    (Self::set_account_id { .. }, Self::set_identity { .. }) => false,
                    (Self::set_account_id { .. }, Self::set_subs { .. }) => false,
                    (Self::set_account_id { .. }, Self::clear_identity { .. }) => false,
                    (Self::set_account_id { .. }, Self::request_judgement { .. }) => {
                        false
                    }
                    (Self::set_account_id { .. }, Self::cancel_request { .. }) => false,
                    (Self::set_account_id { .. }, Self::set_fee { .. }) => false,
                    (Self::set_account_id { .. }, Self::set_fields { .. }) => false,
                    (Self::set_account_id { .. }, Self::provide_judgement { .. }) => {
                        false
                    }
                    (Self::set_account_id { .. }, Self::kill_identity { .. }) => false,
                    (Self::set_account_id { .. }, Self::add_sub { .. }) => false,
                    (Self::set_account_id { .. }, Self::rename_sub { .. }) => false,
                    (Self::set_account_id { .. }, Self::remove_sub { .. }) => false,
                    (Self::set_account_id { .. }, Self::quit_sub { .. }) => false,
                    (Self::set_fields { .. }, Self::__Ignore { .. }) => false,
                    (Self::set_fields { .. }, Self::add_registrar { .. }) => false,
                    (Self::set_fields { .. }, Self::set_identity { .. }) => false,
                    (Self::set_fields { .. }, Self::set_subs { .. }) => false,
                    (Self::set_fields { .. }, Self::clear_identity { .. }) => false,
                    (Self::set_fields { .. }, Self::request_judgement { .. }) => false,
                    (Self::set_fields { .. }, Self::cancel_request { .. }) => false,
                    (Self::set_fields { .. }, Self::set_fee { .. }) => false,
                    (Self::set_fields { .. }, Self::set_account_id { .. }) => false,
                    (Self::set_fields { .. }, Self::provide_judgement { .. }) => false,
                    (Self::set_fields { .. }, Self::kill_identity { .. }) => false,
                    (Self::set_fields { .. }, Self::add_sub { .. }) => false,
                    (Self::set_fields { .. }, Self::rename_sub { .. }) => false,
                    (Self::set_fields { .. }, Self::remove_sub { .. }) => false,
                    (Self::set_fields { .. }, Self::quit_sub { .. }) => false,
                    (Self::provide_judgement { .. }, Self::__Ignore { .. }) => false,
                    (Self::provide_judgement { .. }, Self::add_registrar { .. }) => false,
                    (Self::provide_judgement { .. }, Self::set_identity { .. }) => false,
                    (Self::provide_judgement { .. }, Self::set_subs { .. }) => false,
                    (Self::provide_judgement { .. }, Self::clear_identity { .. }) => {
                        false
                    }
                    (Self::provide_judgement { .. }, Self::request_judgement { .. }) => {
                        false
                    }
                    (Self::provide_judgement { .. }, Self::cancel_request { .. }) => {
                        false
                    }
                    (Self::provide_judgement { .. }, Self::set_fee { .. }) => false,
                    (Self::provide_judgement { .. }, Self::set_account_id { .. }) => {
                        false
                    }
                    (Self::provide_judgement { .. }, Self::set_fields { .. }) => false,
                    (Self::provide_judgement { .. }, Self::kill_identity { .. }) => false,
                    (Self::provide_judgement { .. }, Self::add_sub { .. }) => false,
                    (Self::provide_judgement { .. }, Self::rename_sub { .. }) => false,
                    (Self::provide_judgement { .. }, Self::remove_sub { .. }) => false,
                    (Self::provide_judgement { .. }, Self::quit_sub { .. }) => false,
                    (Self::kill_identity { .. }, Self::__Ignore { .. }) => false,
                    (Self::kill_identity { .. }, Self::add_registrar { .. }) => false,
                    (Self::kill_identity { .. }, Self::set_identity { .. }) => false,
                    (Self::kill_identity { .. }, Self::set_subs { .. }) => false,
                    (Self::kill_identity { .. }, Self::clear_identity { .. }) => false,
                    (Self::kill_identity { .. }, Self::request_judgement { .. }) => false,
                    (Self::kill_identity { .. }, Self::cancel_request { .. }) => false,
                    (Self::kill_identity { .. }, Self::set_fee { .. }) => false,
                    (Self::kill_identity { .. }, Self::set_account_id { .. }) => false,
                    (Self::kill_identity { .. }, Self::set_fields { .. }) => false,
                    (Self::kill_identity { .. }, Self::provide_judgement { .. }) => false,
                    (Self::kill_identity { .. }, Self::add_sub { .. }) => false,
                    (Self::kill_identity { .. }, Self::rename_sub { .. }) => false,
                    (Self::kill_identity { .. }, Self::remove_sub { .. }) => false,
                    (Self::kill_identity { .. }, Self::quit_sub { .. }) => false,
                    (Self::add_sub { .. }, Self::__Ignore { .. }) => false,
                    (Self::add_sub { .. }, Self::add_registrar { .. }) => false,
                    (Self::add_sub { .. }, Self::set_identity { .. }) => false,
                    (Self::add_sub { .. }, Self::set_subs { .. }) => false,
                    (Self::add_sub { .. }, Self::clear_identity { .. }) => false,
                    (Self::add_sub { .. }, Self::request_judgement { .. }) => false,
                    (Self::add_sub { .. }, Self::cancel_request { .. }) => false,
                    (Self::add_sub { .. }, Self::set_fee { .. }) => false,
                    (Self::add_sub { .. }, Self::set_account_id { .. }) => false,
                    (Self::add_sub { .. }, Self::set_fields { .. }) => false,
                    (Self::add_sub { .. }, Self::provide_judgement { .. }) => false,
                    (Self::add_sub { .. }, Self::kill_identity { .. }) => false,
                    (Self::add_sub { .. }, Self::rename_sub { .. }) => false,
                    (Self::add_sub { .. }, Self::remove_sub { .. }) => false,
                    (Self::add_sub { .. }, Self::quit_sub { .. }) => false,
                    (Self::rename_sub { .. }, Self::__Ignore { .. }) => false,
                    (Self::rename_sub { .. }, Self::add_registrar { .. }) => false,
                    (Self::rename_sub { .. }, Self::set_identity { .. }) => false,
                    (Self::rename_sub { .. }, Self::set_subs { .. }) => false,
                    (Self::rename_sub { .. }, Self::clear_identity { .. }) => false,
                    (Self::rename_sub { .. }, Self::request_judgement { .. }) => false,
                    (Self::rename_sub { .. }, Self::cancel_request { .. }) => false,
                    (Self::rename_sub { .. }, Self::set_fee { .. }) => false,
                    (Self::rename_sub { .. }, Self::set_account_id { .. }) => false,
                    (Self::rename_sub { .. }, Self::set_fields { .. }) => false,
                    (Self::rename_sub { .. }, Self::provide_judgement { .. }) => false,
                    (Self::rename_sub { .. }, Self::kill_identity { .. }) => false,
                    (Self::rename_sub { .. }, Self::add_sub { .. }) => false,
                    (Self::rename_sub { .. }, Self::remove_sub { .. }) => false,
                    (Self::rename_sub { .. }, Self::quit_sub { .. }) => false,
                    (Self::remove_sub { .. }, Self::__Ignore { .. }) => false,
                    (Self::remove_sub { .. }, Self::add_registrar { .. }) => false,
                    (Self::remove_sub { .. }, Self::set_identity { .. }) => false,
                    (Self::remove_sub { .. }, Self::set_subs { .. }) => false,
                    (Self::remove_sub { .. }, Self::clear_identity { .. }) => false,
                    (Self::remove_sub { .. }, Self::request_judgement { .. }) => false,
                    (Self::remove_sub { .. }, Self::cancel_request { .. }) => false,
                    (Self::remove_sub { .. }, Self::set_fee { .. }) => false,
                    (Self::remove_sub { .. }, Self::set_account_id { .. }) => false,
                    (Self::remove_sub { .. }, Self::set_fields { .. }) => false,
                    (Self::remove_sub { .. }, Self::provide_judgement { .. }) => false,
                    (Self::remove_sub { .. }, Self::kill_identity { .. }) => false,
                    (Self::remove_sub { .. }, Self::add_sub { .. }) => false,
                    (Self::remove_sub { .. }, Self::rename_sub { .. }) => false,
                    (Self::remove_sub { .. }, Self::quit_sub { .. }) => false,
                    (Self::quit_sub { .. }, Self::__Ignore { .. }) => false,
                    (Self::quit_sub { .. }, Self::add_registrar { .. }) => false,
                    (Self::quit_sub { .. }, Self::set_identity { .. }) => false,
                    (Self::quit_sub { .. }, Self::set_subs { .. }) => false,
                    (Self::quit_sub { .. }, Self::clear_identity { .. }) => false,
                    (Self::quit_sub { .. }, Self::request_judgement { .. }) => false,
                    (Self::quit_sub { .. }, Self::cancel_request { .. }) => false,
                    (Self::quit_sub { .. }, Self::set_fee { .. }) => false,
                    (Self::quit_sub { .. }, Self::set_account_id { .. }) => false,
                    (Self::quit_sub { .. }, Self::set_fields { .. }) => false,
                    (Self::quit_sub { .. }, Self::provide_judgement { .. }) => false,
                    (Self::quit_sub { .. }, Self::kill_identity { .. }) => false,
                    (Self::quit_sub { .. }, Self::add_sub { .. }) => false,
                    (Self::quit_sub { .. }, Self::rename_sub { .. }) => false,
                    (Self::quit_sub { .. }, Self::remove_sub { .. }) => false,
                }
            }
        }
    };
    #[allow(deprecated)]
    const _: () = {
        #[allow(non_camel_case_types)]
        #[automatically_derived]
        impl<T: Config> ::codec::Encode for Call<T> {
            fn size_hint(&self) -> usize {
                1_usize
                    + match *self {
                        Call::add_registrar { ref account } => {
                            0_usize.saturating_add(::codec::Encode::size_hint(account))
                        }
                        Call::set_identity { ref info } => {
                            0_usize.saturating_add(::codec::Encode::size_hint(info))
                        }
                        Call::set_subs { ref subs } => {
                            0_usize.saturating_add(::codec::Encode::size_hint(subs))
                        }
                        Call::clear_identity {} => 0_usize,
                        Call::request_judgement { ref reg_index, ref max_fee } => {
                            0_usize
                                .saturating_add(
                                    ::codec::Encode::size_hint(
                                        &<<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                            '_,
                                            RegistrarIndex,
                                        >>::RefType::from(reg_index),
                                    ),
                                )
                                .saturating_add(
                                    ::codec::Encode::size_hint(
                                        &<<BalanceOf<
                                            T,
                                        > as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                            '_,
                                            BalanceOf<T>,
                                        >>::RefType::from(max_fee),
                                    ),
                                )
                        }
                        Call::cancel_request { ref reg_index } => {
                            0_usize.saturating_add(::codec::Encode::size_hint(reg_index))
                        }
                        Call::set_fee { ref index, ref fee } => {
                            0_usize
                                .saturating_add(
                                    ::codec::Encode::size_hint(
                                        &<<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                            '_,
                                            RegistrarIndex,
                                        >>::RefType::from(index),
                                    ),
                                )
                                .saturating_add(
                                    ::codec::Encode::size_hint(
                                        &<<BalanceOf<
                                            T,
                                        > as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                            '_,
                                            BalanceOf<T>,
                                        >>::RefType::from(fee),
                                    ),
                                )
                        }
                        Call::set_account_id { ref index, ref new } => {
                            0_usize
                                .saturating_add(
                                    ::codec::Encode::size_hint(
                                        &<<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                            '_,
                                            RegistrarIndex,
                                        >>::RefType::from(index),
                                    ),
                                )
                                .saturating_add(::codec::Encode::size_hint(new))
                        }
                        Call::set_fields { ref index, ref fields } => {
                            0_usize
                                .saturating_add(
                                    ::codec::Encode::size_hint(
                                        &<<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                            '_,
                                            RegistrarIndex,
                                        >>::RefType::from(index),
                                    ),
                                )
                                .saturating_add(::codec::Encode::size_hint(fields))
                        }
                        Call::provide_judgement {
                            ref reg_index,
                            ref target,
                            ref judgement,
                            ref identity,
                        } => {
                            0_usize
                                .saturating_add(
                                    ::codec::Encode::size_hint(
                                        &<<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                            '_,
                                            RegistrarIndex,
                                        >>::RefType::from(reg_index),
                                    ),
                                )
                                .saturating_add(::codec::Encode::size_hint(target))
                                .saturating_add(::codec::Encode::size_hint(judgement))
                                .saturating_add(::codec::Encode::size_hint(identity))
                        }
                        Call::kill_identity { ref target } => {
                            0_usize.saturating_add(::codec::Encode::size_hint(target))
                        }
                        Call::add_sub { ref sub, ref data } => {
                            0_usize
                                .saturating_add(::codec::Encode::size_hint(sub))
                                .saturating_add(::codec::Encode::size_hint(data))
                        }
                        Call::rename_sub { ref sub, ref data } => {
                            0_usize
                                .saturating_add(::codec::Encode::size_hint(sub))
                                .saturating_add(::codec::Encode::size_hint(data))
                        }
                        Call::remove_sub { ref sub } => {
                            0_usize.saturating_add(::codec::Encode::size_hint(sub))
                        }
                        Call::quit_sub {} => 0_usize,
                        _ => 0_usize,
                    }
            }
            fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
                &self,
                __codec_dest_edqy: &mut __CodecOutputEdqy,
            ) {
                match *self {
                    Call::add_registrar { ref account } => {
                        __codec_dest_edqy.push_byte(0u8 as ::core::primitive::u8);
                        ::codec::Encode::encode_to(account, __codec_dest_edqy);
                    }
                    Call::set_identity { ref info } => {
                        __codec_dest_edqy.push_byte(1u8 as ::core::primitive::u8);
                        ::codec::Encode::encode_to(info, __codec_dest_edqy);
                    }
                    Call::set_subs { ref subs } => {
                        __codec_dest_edqy.push_byte(2u8 as ::core::primitive::u8);
                        ::codec::Encode::encode_to(subs, __codec_dest_edqy);
                    }
                    Call::clear_identity {} => {
                        __codec_dest_edqy.push_byte(3u8 as ::core::primitive::u8);
                    }
                    Call::request_judgement { ref reg_index, ref max_fee } => {
                        __codec_dest_edqy.push_byte(4u8 as ::core::primitive::u8);
                        {
                            ::codec::Encode::encode_to(
                                &<<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                    '_,
                                    RegistrarIndex,
                                >>::RefType::from(reg_index),
                                __codec_dest_edqy,
                            );
                        }
                        {
                            ::codec::Encode::encode_to(
                                &<<BalanceOf<
                                    T,
                                > as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                    '_,
                                    BalanceOf<T>,
                                >>::RefType::from(max_fee),
                                __codec_dest_edqy,
                            );
                        }
                    }
                    Call::cancel_request { ref reg_index } => {
                        __codec_dest_edqy.push_byte(5u8 as ::core::primitive::u8);
                        ::codec::Encode::encode_to(reg_index, __codec_dest_edqy);
                    }
                    Call::set_fee { ref index, ref fee } => {
                        __codec_dest_edqy.push_byte(6u8 as ::core::primitive::u8);
                        {
                            ::codec::Encode::encode_to(
                                &<<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                    '_,
                                    RegistrarIndex,
                                >>::RefType::from(index),
                                __codec_dest_edqy,
                            );
                        }
                        {
                            ::codec::Encode::encode_to(
                                &<<BalanceOf<
                                    T,
                                > as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                    '_,
                                    BalanceOf<T>,
                                >>::RefType::from(fee),
                                __codec_dest_edqy,
                            );
                        }
                    }
                    Call::set_account_id { ref index, ref new } => {
                        __codec_dest_edqy.push_byte(7u8 as ::core::primitive::u8);
                        {
                            ::codec::Encode::encode_to(
                                &<<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                    '_,
                                    RegistrarIndex,
                                >>::RefType::from(index),
                                __codec_dest_edqy,
                            );
                        }
                        ::codec::Encode::encode_to(new, __codec_dest_edqy);
                    }
                    Call::set_fields { ref index, ref fields } => {
                        __codec_dest_edqy.push_byte(8u8 as ::core::primitive::u8);
                        {
                            ::codec::Encode::encode_to(
                                &<<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                    '_,
                                    RegistrarIndex,
                                >>::RefType::from(index),
                                __codec_dest_edqy,
                            );
                        }
                        ::codec::Encode::encode_to(fields, __codec_dest_edqy);
                    }
                    Call::provide_judgement {
                        ref reg_index,
                        ref target,
                        ref judgement,
                        ref identity,
                    } => {
                        __codec_dest_edqy.push_byte(9u8 as ::core::primitive::u8);
                        {
                            ::codec::Encode::encode_to(
                                &<<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::EncodeAsRef<
                                    '_,
                                    RegistrarIndex,
                                >>::RefType::from(reg_index),
                                __codec_dest_edqy,
                            );
                        }
                        ::codec::Encode::encode_to(target, __codec_dest_edqy);
                        ::codec::Encode::encode_to(judgement, __codec_dest_edqy);
                        ::codec::Encode::encode_to(identity, __codec_dest_edqy);
                    }
                    Call::kill_identity { ref target } => {
                        __codec_dest_edqy.push_byte(10u8 as ::core::primitive::u8);
                        ::codec::Encode::encode_to(target, __codec_dest_edqy);
                    }
                    Call::add_sub { ref sub, ref data } => {
                        __codec_dest_edqy.push_byte(11u8 as ::core::primitive::u8);
                        ::codec::Encode::encode_to(sub, __codec_dest_edqy);
                        ::codec::Encode::encode_to(data, __codec_dest_edqy);
                    }
                    Call::rename_sub { ref sub, ref data } => {
                        __codec_dest_edqy.push_byte(12u8 as ::core::primitive::u8);
                        ::codec::Encode::encode_to(sub, __codec_dest_edqy);
                        ::codec::Encode::encode_to(data, __codec_dest_edqy);
                    }
                    Call::remove_sub { ref sub } => {
                        __codec_dest_edqy.push_byte(13u8 as ::core::primitive::u8);
                        ::codec::Encode::encode_to(sub, __codec_dest_edqy);
                    }
                    Call::quit_sub {} => {
                        __codec_dest_edqy.push_byte(14u8 as ::core::primitive::u8);
                    }
                    _ => {}
                }
            }
        }
        #[automatically_derived]
        impl<T: Config> ::codec::EncodeLike for Call<T> {}
    };
    #[allow(deprecated)]
    const _: () = {
        #[allow(non_camel_case_types)]
        #[automatically_derived]
        impl<T: Config> ::codec::Decode for Call<T> {
            fn decode<__CodecInputEdqy: ::codec::Input>(
                __codec_input_edqy: &mut __CodecInputEdqy,
            ) -> ::core::result::Result<Self, ::codec::Error> {
                match __codec_input_edqy
                    .read_byte()
                    .map_err(|e| {
                        e.chain("Could not decode `Call`, failed to read variant byte")
                    })?
                {
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy == 0u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::add_registrar {
                                account: {
                                    let __codec_res_edqy = <AccountIdLookupOf<
                                        T,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::add_registrar::account`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy == 1u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::set_identity {
                                info: {
                                    let __codec_res_edqy = <Box<
                                        IdentityInfo<T::MaxAdditionalFields>,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::set_identity::info`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy == 2u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::set_subs {
                                subs: {
                                    let __codec_res_edqy = <Vec<
                                        (T::AccountId, Data),
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::set_subs::subs`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy == 3u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::clear_identity {})
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy == 4u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::request_judgement {
                                reg_index: {
                                    let __codec_res_edqy = <<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Call::request_judgement::reg_index`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy.into()
                                        }
                                    }
                                },
                                max_fee: {
                                    let __codec_res_edqy = <<BalanceOf<
                                        T,
                                    > as ::codec::HasCompact>::Type as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Call::request_judgement::max_fee`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy.into()
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy == 5u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::cancel_request {
                                reg_index: {
                                    let __codec_res_edqy = <RegistrarIndex as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain("Could not decode `Call::cancel_request::reg_index`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy == 6u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::set_fee {
                                index: {
                                    let __codec_res_edqy = <<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::set_fee::index`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy.into()
                                        }
                                    }
                                },
                                fee: {
                                    let __codec_res_edqy = <<BalanceOf<
                                        T,
                                    > as ::codec::HasCompact>::Type as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::set_fee::fee`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy.into()
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy == 7u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::set_account_id {
                                index: {
                                    let __codec_res_edqy = <<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::set_account_id::index`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy.into()
                                        }
                                    }
                                },
                                new: {
                                    let __codec_res_edqy = <AccountIdLookupOf<
                                        T,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::set_account_id::new`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy == 8u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::set_fields {
                                index: {
                                    let __codec_res_edqy = <<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::set_fields::index`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy.into()
                                        }
                                    }
                                },
                                fields: {
                                    let __codec_res_edqy = <IdentityFields as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::set_fields::fields`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy == 9u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::provide_judgement {
                                reg_index: {
                                    let __codec_res_edqy = <<RegistrarIndex as ::codec::HasCompact>::Type as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Call::provide_judgement::reg_index`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy.into()
                                        }
                                    }
                                },
                                target: {
                                    let __codec_res_edqy = <AccountIdLookupOf<
                                        T,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain("Could not decode `Call::provide_judgement::target`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                judgement: {
                                    let __codec_res_edqy = <Judgement<
                                        BalanceOf<T>,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Call::provide_judgement::judgement`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                identity: {
                                    let __codec_res_edqy = <T::Hash as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Call::provide_judgement::identity`",
                                                    ),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 10u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::kill_identity {
                                target: {
                                    let __codec_res_edqy = <AccountIdLookupOf<
                                        T,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::kill_identity::target`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 11u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::add_sub {
                                sub: {
                                    let __codec_res_edqy = <AccountIdLookupOf<
                                        T,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::add_sub::sub`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                data: {
                                    let __codec_res_edqy = <Data as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::add_sub::data`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 12u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::rename_sub {
                                sub: {
                                    let __codec_res_edqy = <AccountIdLookupOf<
                                        T,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::rename_sub::sub`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                                data: {
                                    let __codec_res_edqy = <Data as ::codec::Decode>::decode(
                                        __codec_input_edqy,
                                    );
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::rename_sub::data`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 13u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::remove_sub {
                                sub: {
                                    let __codec_res_edqy = <AccountIdLookupOf<
                                        T,
                                    > as ::codec::Decode>::decode(__codec_input_edqy);
                                    match __codec_res_edqy {
                                        ::core::result::Result::Err(e) => {
                                            return ::core::result::Result::Err(
                                                e.chain("Could not decode `Call::remove_sub::sub`"),
                                            );
                                        }
                                        ::core::result::Result::Ok(__codec_res_edqy) => {
                                            __codec_res_edqy
                                        }
                                    }
                                },
                            })
                        })();
                    }
                    #[allow(clippy::unnecessary_cast)]
                    __codec_x_edqy if __codec_x_edqy
                        == 14u8 as ::core::primitive::u8 => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Ok(Call::<T>::quit_sub {})
                        })();
                    }
                    _ => {
                        #[allow(clippy::redundant_closure_call)]
                        return (move || {
                            ::core::result::Result::Err(
                                <_ as ::core::convert::Into<
                                    _,
                                >>::into("Could not decode `Call`, variant doesn't exist"),
                            )
                        })();
                    }
                }
            }
        }
    };
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        impl<T: Config> ::scale_info::TypeInfo for Call<T>
        where
            frame_support::__private::sp_std::marker::PhantomData<
                (T,),
            >: ::scale_info::TypeInfo + 'static,
            AccountIdLookupOf<T>: ::scale_info::TypeInfo + 'static,
            Box<IdentityInfo<T::MaxAdditionalFields>>: ::scale_info::TypeInfo + 'static,
            Vec<(T::AccountId, Data)>: ::scale_info::TypeInfo + 'static,
            BalanceOf<T>: ::scale_info::scale::HasCompact,
            BalanceOf<T>: ::scale_info::scale::HasCompact,
            AccountIdLookupOf<T>: ::scale_info::TypeInfo + 'static,
            AccountIdLookupOf<T>: ::scale_info::TypeInfo + 'static,
            Judgement<BalanceOf<T>>: ::scale_info::TypeInfo + 'static,
            T::Hash: ::scale_info::TypeInfo + 'static,
            AccountIdLookupOf<T>: ::scale_info::TypeInfo + 'static,
            AccountIdLookupOf<T>: ::scale_info::TypeInfo + 'static,
            AccountIdLookupOf<T>: ::scale_info::TypeInfo + 'static,
            AccountIdLookupOf<T>: ::scale_info::TypeInfo + 'static,
            T: Config + 'static,
        {
            type Identity = Self;
            fn type_info() -> ::scale_info::Type {
                ::scale_info::Type::builder()
                    .path(::scale_info::Path::new("Call", "pallet_identity::pallet"))
                    .type_params(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                ::scale_info::TypeParameter::new(
                                    "T",
                                    ::core::option::Option::None,
                                ),
                            ]),
                        ),
                    )
                    .docs_always(&["Identity pallet declaration."])
                    .variant(
                        ::scale_info::build::Variants::new()
                            .variant(
                                "add_registrar",
                                |v| {
                                    v
                                        .index(0u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .ty::<AccountIdLookupOf<T>>()
                                                        .name("account")
                                                        .type_name("AccountIdLookupOf<T>")
                                                }),
                                        )
                                        .docs_always(&["See [`Pallet::add_registrar`]."])
                                },
                            )
                            .variant(
                                "set_identity",
                                |v| {
                                    v
                                        .index(1u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .ty::<Box<IdentityInfo<T::MaxAdditionalFields>>>()
                                                        .name("info")
                                                        .type_name("Box<IdentityInfo<T::MaxAdditionalFields>>")
                                                }),
                                        )
                                        .docs_always(&["See [`Pallet::set_identity`]."])
                                },
                            )
                            .variant(
                                "set_subs",
                                |v| {
                                    v
                                        .index(2u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .ty::<Vec<(T::AccountId, Data)>>()
                                                        .name("subs")
                                                        .type_name("Vec<(T::AccountId, Data)>")
                                                }),
                                        )
                                        .docs_always(&["See [`Pallet::set_subs`]."])
                                },
                            )
                            .variant(
                                "clear_identity",
                                |v| {
                                    v
                                        .index(3u8 as ::core::primitive::u8)
                                        .fields(::scale_info::build::Fields::named())
                                        .docs_always(&["See [`Pallet::clear_identity`]."])
                                },
                            )
                            .variant(
                                "request_judgement",
                                |v| {
                                    v
                                        .index(4u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .compact::<RegistrarIndex>()
                                                        .name("reg_index")
                                                        .type_name("RegistrarIndex")
                                                })
                                                .field(|f| {
                                                    f
                                                        .compact::<BalanceOf<T>>()
                                                        .name("max_fee")
                                                        .type_name("BalanceOf<T>")
                                                }),
                                        )
                                        .docs_always(&["See [`Pallet::request_judgement`]."])
                                },
                            )
                            .variant(
                                "cancel_request",
                                |v| {
                                    v
                                        .index(5u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .ty::<RegistrarIndex>()
                                                        .name("reg_index")
                                                        .type_name("RegistrarIndex")
                                                }),
                                        )
                                        .docs_always(&["See [`Pallet::cancel_request`]."])
                                },
                            )
                            .variant(
                                "set_fee",
                                |v| {
                                    v
                                        .index(6u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .compact::<RegistrarIndex>()
                                                        .name("index")
                                                        .type_name("RegistrarIndex")
                                                })
                                                .field(|f| {
                                                    f
                                                        .compact::<BalanceOf<T>>()
                                                        .name("fee")
                                                        .type_name("BalanceOf<T>")
                                                }),
                                        )
                                        .docs_always(&["See [`Pallet::set_fee`]."])
                                },
                            )
                            .variant(
                                "set_account_id",
                                |v| {
                                    v
                                        .index(7u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .compact::<RegistrarIndex>()
                                                        .name("index")
                                                        .type_name("RegistrarIndex")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<AccountIdLookupOf<T>>()
                                                        .name("new")
                                                        .type_name("AccountIdLookupOf<T>")
                                                }),
                                        )
                                        .docs_always(&["See [`Pallet::set_account_id`]."])
                                },
                            )
                            .variant(
                                "set_fields",
                                |v| {
                                    v
                                        .index(8u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .compact::<RegistrarIndex>()
                                                        .name("index")
                                                        .type_name("RegistrarIndex")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<IdentityFields>()
                                                        .name("fields")
                                                        .type_name("IdentityFields")
                                                }),
                                        )
                                        .docs_always(&["See [`Pallet::set_fields`]."])
                                },
                            )
                            .variant(
                                "provide_judgement",
                                |v| {
                                    v
                                        .index(9u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .compact::<RegistrarIndex>()
                                                        .name("reg_index")
                                                        .type_name("RegistrarIndex")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<AccountIdLookupOf<T>>()
                                                        .name("target")
                                                        .type_name("AccountIdLookupOf<T>")
                                                })
                                                .field(|f| {
                                                    f
                                                        .ty::<Judgement<BalanceOf<T>>>()
                                                        .name("judgement")
                                                        .type_name("Judgement<BalanceOf<T>>")
                                                })
                                                .field(|f| {
                                                    f.ty::<T::Hash>().name("identity").type_name("T::Hash")
                                                }),
                                        )
                                        .docs_always(&["See [`Pallet::provide_judgement`]."])
                                },
                            )
                            .variant(
                                "kill_identity",
                                |v| {
                                    v
                                        .index(10u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .ty::<AccountIdLookupOf<T>>()
                                                        .name("target")
                                                        .type_name("AccountIdLookupOf<T>")
                                                }),
                                        )
                                        .docs_always(&["See [`Pallet::kill_identity`]."])
                                },
                            )
                            .variant(
                                "add_sub",
                                |v| {
                                    v
                                        .index(11u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .ty::<AccountIdLookupOf<T>>()
                                                        .name("sub")
                                                        .type_name("AccountIdLookupOf<T>")
                                                })
                                                .field(|f| f.ty::<Data>().name("data").type_name("Data")),
                                        )
                                        .docs_always(&["See [`Pallet::add_sub`]."])
                                },
                            )
                            .variant(
                                "rename_sub",
                                |v| {
                                    v
                                        .index(12u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .ty::<AccountIdLookupOf<T>>()
                                                        .name("sub")
                                                        .type_name("AccountIdLookupOf<T>")
                                                })
                                                .field(|f| f.ty::<Data>().name("data").type_name("Data")),
                                        )
                                        .docs_always(&["See [`Pallet::rename_sub`]."])
                                },
                            )
                            .variant(
                                "remove_sub",
                                |v| {
                                    v
                                        .index(13u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .ty::<AccountIdLookupOf<T>>()
                                                        .name("sub")
                                                        .type_name("AccountIdLookupOf<T>")
                                                }),
                                        )
                                        .docs_always(&["See [`Pallet::remove_sub`]."])
                                },
                            )
                            .variant(
                                "quit_sub",
                                |v| {
                                    v
                                        .index(14u8 as ::core::primitive::u8)
                                        .fields(::scale_info::build::Fields::named())
                                        .docs_always(&["See [`Pallet::quit_sub`]."])
                                },
                            ),
                    )
            }
        }
    };
    impl<T: Config> Call<T> {
        ///Create a call with the variant `add_registrar`.
        pub fn new_call_variant_add_registrar(account: AccountIdLookupOf<T>) -> Self {
            Self::add_registrar { account }
        }
        ///Create a call with the variant `set_identity`.
        pub fn new_call_variant_set_identity(
            info: Box<IdentityInfo<T::MaxAdditionalFields>>,
        ) -> Self {
            Self::set_identity { info }
        }
        ///Create a call with the variant `set_subs`.
        pub fn new_call_variant_set_subs(subs: Vec<(T::AccountId, Data)>) -> Self {
            Self::set_subs { subs }
        }
        ///Create a call with the variant `clear_identity`.
        pub fn new_call_variant_clear_identity() -> Self {
            Self::clear_identity {}
        }
        ///Create a call with the variant `request_judgement`.
        pub fn new_call_variant_request_judgement(
            reg_index: RegistrarIndex,
            max_fee: BalanceOf<T>,
        ) -> Self {
            Self::request_judgement {
                reg_index,
                max_fee,
            }
        }
        ///Create a call with the variant `cancel_request`.
        pub fn new_call_variant_cancel_request(reg_index: RegistrarIndex) -> Self {
            Self::cancel_request { reg_index }
        }
        ///Create a call with the variant `set_fee`.
        pub fn new_call_variant_set_fee(
            index: RegistrarIndex,
            fee: BalanceOf<T>,
        ) -> Self {
            Self::set_fee { index, fee }
        }
        ///Create a call with the variant `set_account_id`.
        pub fn new_call_variant_set_account_id(
            index: RegistrarIndex,
            new: AccountIdLookupOf<T>,
        ) -> Self {
            Self::set_account_id { index, new }
        }
        ///Create a call with the variant `set_fields`.
        pub fn new_call_variant_set_fields(
            index: RegistrarIndex,
            fields: IdentityFields,
        ) -> Self {
            Self::set_fields { index, fields }
        }
        ///Create a call with the variant `provide_judgement`.
        pub fn new_call_variant_provide_judgement(
            reg_index: RegistrarIndex,
            target: AccountIdLookupOf<T>,
            judgement: Judgement<BalanceOf<T>>,
            identity: T::Hash,
        ) -> Self {
            Self::provide_judgement {
                reg_index,
                target,
                judgement,
                identity,
            }
        }
        ///Create a call with the variant `kill_identity`.
        pub fn new_call_variant_kill_identity(target: AccountIdLookupOf<T>) -> Self {
            Self::kill_identity { target }
        }
        ///Create a call with the variant `add_sub`.
        pub fn new_call_variant_add_sub(sub: AccountIdLookupOf<T>, data: Data) -> Self {
            Self::add_sub { sub, data }
        }
        ///Create a call with the variant `rename_sub`.
        pub fn new_call_variant_rename_sub(
            sub: AccountIdLookupOf<T>,
            data: Data,
        ) -> Self {
            Self::rename_sub { sub, data }
        }
        ///Create a call with the variant `remove_sub`.
        pub fn new_call_variant_remove_sub(sub: AccountIdLookupOf<T>) -> Self {
            Self::remove_sub { sub }
        }
        ///Create a call with the variant `quit_sub`.
        pub fn new_call_variant_quit_sub() -> Self {
            Self::quit_sub {}
        }
    }
    impl<T: Config> frame_support::dispatch::GetDispatchInfo for Call<T> {
        fn get_dispatch_info(&self) -> frame_support::dispatch::DispatchInfo {
            match *self {
                Self::add_registrar { ref account } => {
                    let __pallet_base_weight = T::WeightInfo::add_registrar(
                        T::MaxRegistrars::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (&AccountIdLookupOf<T>,),
                    >>::weigh_data(&__pallet_base_weight, (account,));
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (&AccountIdLookupOf<T>,),
                    >>::classify_dispatch(&__pallet_base_weight, (account,));
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (&AccountIdLookupOf<T>,),
                    >>::pays_fee(&__pallet_base_weight, (account,));
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::set_identity { ref info } => {
                    let __pallet_base_weight = T::WeightInfo::set_identity(
                        T::MaxRegistrars::get(),
                        T::MaxAdditionalFields::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (&Box<IdentityInfo<T::MaxAdditionalFields>>,),
                    >>::weigh_data(&__pallet_base_weight, (info,));
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (&Box<IdentityInfo<T::MaxAdditionalFields>>,),
                    >>::classify_dispatch(&__pallet_base_weight, (info,));
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (&Box<IdentityInfo<T::MaxAdditionalFields>>,),
                    >>::pays_fee(&__pallet_base_weight, (info,));
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::set_subs { ref subs } => {
                    let __pallet_base_weight = T::WeightInfo::set_subs_old(
                            T::MaxSubAccounts::get(),
                        )
                        .saturating_add(T::WeightInfo::set_subs_new(subs.len() as u32));
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (&Vec<(T::AccountId, Data)>,),
                    >>::weigh_data(&__pallet_base_weight, (subs,));
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (&Vec<(T::AccountId, Data)>,),
                    >>::classify_dispatch(&__pallet_base_weight, (subs,));
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (&Vec<(T::AccountId, Data)>,),
                    >>::pays_fee(&__pallet_base_weight, (subs,));
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::clear_identity {} => {
                    let __pallet_base_weight = T::WeightInfo::clear_identity(
                        T::MaxRegistrars::get(),
                        T::MaxSubAccounts::get(),
                        T::MaxAdditionalFields::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (),
                    >>::weigh_data(&__pallet_base_weight, ());
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (),
                    >>::classify_dispatch(&__pallet_base_weight, ());
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (),
                    >>::pays_fee(&__pallet_base_weight, ());
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::request_judgement { ref reg_index, ref max_fee } => {
                    let __pallet_base_weight = T::WeightInfo::request_judgement(
                        T::MaxRegistrars::get(),
                        T::MaxAdditionalFields::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (&RegistrarIndex, &BalanceOf<T>),
                    >>::weigh_data(&__pallet_base_weight, (reg_index, max_fee));
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (&RegistrarIndex, &BalanceOf<T>),
                    >>::classify_dispatch(&__pallet_base_weight, (reg_index, max_fee));
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (&RegistrarIndex, &BalanceOf<T>),
                    >>::pays_fee(&__pallet_base_weight, (reg_index, max_fee));
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::cancel_request { ref reg_index } => {
                    let __pallet_base_weight = T::WeightInfo::cancel_request(
                        T::MaxRegistrars::get(),
                        T::MaxAdditionalFields::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (&RegistrarIndex,),
                    >>::weigh_data(&__pallet_base_weight, (reg_index,));
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (&RegistrarIndex,),
                    >>::classify_dispatch(&__pallet_base_weight, (reg_index,));
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (&RegistrarIndex,),
                    >>::pays_fee(&__pallet_base_weight, (reg_index,));
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::set_fee { ref index, ref fee } => {
                    let __pallet_base_weight = T::WeightInfo::set_fee(
                        T::MaxRegistrars::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (&RegistrarIndex, &BalanceOf<T>),
                    >>::weigh_data(&__pallet_base_weight, (index, fee));
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (&RegistrarIndex, &BalanceOf<T>),
                    >>::classify_dispatch(&__pallet_base_weight, (index, fee));
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (&RegistrarIndex, &BalanceOf<T>),
                    >>::pays_fee(&__pallet_base_weight, (index, fee));
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::set_account_id { ref index, ref new } => {
                    let __pallet_base_weight = T::WeightInfo::set_account_id(
                        T::MaxRegistrars::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (&RegistrarIndex, &AccountIdLookupOf<T>),
                    >>::weigh_data(&__pallet_base_weight, (index, new));
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (&RegistrarIndex, &AccountIdLookupOf<T>),
                    >>::classify_dispatch(&__pallet_base_weight, (index, new));
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (&RegistrarIndex, &AccountIdLookupOf<T>),
                    >>::pays_fee(&__pallet_base_weight, (index, new));
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::set_fields { ref index, ref fields } => {
                    let __pallet_base_weight = T::WeightInfo::set_fields(
                        T::MaxRegistrars::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (&RegistrarIndex, &IdentityFields),
                    >>::weigh_data(&__pallet_base_weight, (index, fields));
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (&RegistrarIndex, &IdentityFields),
                    >>::classify_dispatch(&__pallet_base_weight, (index, fields));
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (&RegistrarIndex, &IdentityFields),
                    >>::pays_fee(&__pallet_base_weight, (index, fields));
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::provide_judgement {
                    ref reg_index,
                    ref target,
                    ref judgement,
                    ref identity,
                } => {
                    let __pallet_base_weight = T::WeightInfo::provide_judgement(
                        T::MaxRegistrars::get(),
                        T::MaxAdditionalFields::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (
                            &RegistrarIndex,
                            &AccountIdLookupOf<T>,
                            &Judgement<BalanceOf<T>>,
                            &T::Hash,
                        ),
                    >>::weigh_data(
                        &__pallet_base_weight,
                        (reg_index, target, judgement, identity),
                    );
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (
                            &RegistrarIndex,
                            &AccountIdLookupOf<T>,
                            &Judgement<BalanceOf<T>>,
                            &T::Hash,
                        ),
                    >>::classify_dispatch(
                        &__pallet_base_weight,
                        (reg_index, target, judgement, identity),
                    );
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (
                            &RegistrarIndex,
                            &AccountIdLookupOf<T>,
                            &Judgement<BalanceOf<T>>,
                            &T::Hash,
                        ),
                    >>::pays_fee(
                        &__pallet_base_weight,
                        (reg_index, target, judgement, identity),
                    );
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::kill_identity { ref target } => {
                    let __pallet_base_weight = T::WeightInfo::kill_identity(
                        T::MaxRegistrars::get(),
                        T::MaxSubAccounts::get(),
                        T::MaxAdditionalFields::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (&AccountIdLookupOf<T>,),
                    >>::weigh_data(&__pallet_base_weight, (target,));
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (&AccountIdLookupOf<T>,),
                    >>::classify_dispatch(&__pallet_base_weight, (target,));
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (&AccountIdLookupOf<T>,),
                    >>::pays_fee(&__pallet_base_weight, (target,));
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::add_sub { ref sub, ref data } => {
                    let __pallet_base_weight = T::WeightInfo::add_sub(
                        T::MaxSubAccounts::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (&AccountIdLookupOf<T>, &Data),
                    >>::weigh_data(&__pallet_base_weight, (sub, data));
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (&AccountIdLookupOf<T>, &Data),
                    >>::classify_dispatch(&__pallet_base_weight, (sub, data));
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (&AccountIdLookupOf<T>, &Data),
                    >>::pays_fee(&__pallet_base_weight, (sub, data));
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::rename_sub { ref sub, ref data } => {
                    let __pallet_base_weight = T::WeightInfo::rename_sub(
                        T::MaxSubAccounts::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (&AccountIdLookupOf<T>, &Data),
                    >>::weigh_data(&__pallet_base_weight, (sub, data));
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (&AccountIdLookupOf<T>, &Data),
                    >>::classify_dispatch(&__pallet_base_weight, (sub, data));
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (&AccountIdLookupOf<T>, &Data),
                    >>::pays_fee(&__pallet_base_weight, (sub, data));
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::remove_sub { ref sub } => {
                    let __pallet_base_weight = T::WeightInfo::remove_sub(
                        T::MaxSubAccounts::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (&AccountIdLookupOf<T>,),
                    >>::weigh_data(&__pallet_base_weight, (sub,));
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (&AccountIdLookupOf<T>,),
                    >>::classify_dispatch(&__pallet_base_weight, (sub,));
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (&AccountIdLookupOf<T>,),
                    >>::pays_fee(&__pallet_base_weight, (sub,));
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::quit_sub {} => {
                    let __pallet_base_weight = T::WeightInfo::quit_sub(
                        T::MaxSubAccounts::get(),
                    );
                    let __pallet_weight = <dyn frame_support::dispatch::WeighData<
                        (),
                    >>::weigh_data(&__pallet_base_weight, ());
                    let __pallet_class = <dyn frame_support::dispatch::ClassifyDispatch<
                        (),
                    >>::classify_dispatch(&__pallet_base_weight, ());
                    let __pallet_pays_fee = <dyn frame_support::dispatch::PaysFee<
                        (),
                    >>::pays_fee(&__pallet_base_weight, ());
                    frame_support::dispatch::DispatchInfo {
                        weight: __pallet_weight,
                        class: __pallet_class,
                        pays_fee: __pallet_pays_fee,
                    }
                }
                Self::__Ignore(_, _) => {
                    ::core::panicking::panic_fmt(
                        format_args!(
                            "internal error: entered unreachable code: {0}",
                            format_args!("__Ignore cannot be used"),
                        ),
                    );
                }
            }
        }
    }
    impl<T: Config> frame_support::traits::GetCallName for Call<T> {
        fn get_call_name(&self) -> &'static str {
            match *self {
                Self::add_registrar { .. } => "add_registrar",
                Self::set_identity { .. } => "set_identity",
                Self::set_subs { .. } => "set_subs",
                Self::clear_identity { .. } => "clear_identity",
                Self::request_judgement { .. } => "request_judgement",
                Self::cancel_request { .. } => "cancel_request",
                Self::set_fee { .. } => "set_fee",
                Self::set_account_id { .. } => "set_account_id",
                Self::set_fields { .. } => "set_fields",
                Self::provide_judgement { .. } => "provide_judgement",
                Self::kill_identity { .. } => "kill_identity",
                Self::add_sub { .. } => "add_sub",
                Self::rename_sub { .. } => "rename_sub",
                Self::remove_sub { .. } => "remove_sub",
                Self::quit_sub { .. } => "quit_sub",
                Self::__Ignore(_, _) => {
                    ::core::panicking::panic_fmt(
                        format_args!(
                            "internal error: entered unreachable code: {0}",
                            format_args!("__PhantomItem cannot be used."),
                        ),
                    );
                }
            }
        }
        fn get_call_names() -> &'static [&'static str] {
            &[
                "add_registrar",
                "set_identity",
                "set_subs",
                "clear_identity",
                "request_judgement",
                "cancel_request",
                "set_fee",
                "set_account_id",
                "set_fields",
                "provide_judgement",
                "kill_identity",
                "add_sub",
                "rename_sub",
                "remove_sub",
                "quit_sub",
            ]
        }
    }
    impl<T: Config> frame_support::traits::GetCallIndex for Call<T> {
        fn get_call_index(&self) -> u8 {
            match *self {
                Self::add_registrar { .. } => 0u8,
                Self::set_identity { .. } => 1u8,
                Self::set_subs { .. } => 2u8,
                Self::clear_identity { .. } => 3u8,
                Self::request_judgement { .. } => 4u8,
                Self::cancel_request { .. } => 5u8,
                Self::set_fee { .. } => 6u8,
                Self::set_account_id { .. } => 7u8,
                Self::set_fields { .. } => 8u8,
                Self::provide_judgement { .. } => 9u8,
                Self::kill_identity { .. } => 10u8,
                Self::add_sub { .. } => 11u8,
                Self::rename_sub { .. } => 12u8,
                Self::remove_sub { .. } => 13u8,
                Self::quit_sub { .. } => 14u8,
                Self::__Ignore(_, _) => {
                    ::core::panicking::panic_fmt(
                        format_args!(
                            "internal error: entered unreachable code: {0}",
                            format_args!("__PhantomItem cannot be used."),
                        ),
                    );
                }
            }
        }
        fn get_call_indices() -> &'static [u8] {
            &[
                0u8,
                1u8,
                2u8,
                3u8,
                4u8,
                5u8,
                6u8,
                7u8,
                8u8,
                9u8,
                10u8,
                11u8,
                12u8,
                13u8,
                14u8,
            ]
        }
    }
    impl<T: Config> frame_support::traits::UnfilteredDispatchable for Call<T> {
        type RuntimeOrigin = frame_system::pallet_prelude::OriginFor<T>;
        fn dispatch_bypass_filter(
            self,
            origin: Self::RuntimeOrigin,
        ) -> frame_support::dispatch::DispatchResultWithPostInfo {
            frame_support::dispatch_context::run_in_context(|| {
                match self {
                    Self::add_registrar { account } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "add_registrar",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::add_registrar(origin, account)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::set_identity { info } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "set_identity",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::set_identity(origin, info)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::set_subs { subs } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "set_subs",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::set_subs(origin, subs)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::clear_identity {} => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "clear_identity",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::clear_identity(origin)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::request_judgement { reg_index, max_fee } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "request_judgement",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::request_judgement(origin, reg_index, max_fee)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::cancel_request { reg_index } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "cancel_request",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::cancel_request(origin, reg_index)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::set_fee { index, fee } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "set_fee",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::set_fee(origin, index, fee)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::set_account_id { index, new } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "set_account_id",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::set_account_id(origin, index, new)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::set_fields { index, fields } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "set_fields",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::set_fields(origin, index, fields)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::provide_judgement {
                        reg_index,
                        target,
                        judgement,
                        identity,
                    } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "provide_judgement",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<
                            T,
                        >>::provide_judgement(
                                origin,
                                reg_index,
                                target,
                                judgement,
                                identity,
                            )
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::kill_identity { target } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "kill_identity",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::kill_identity(origin, target)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::add_sub { sub, data } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "add_sub",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::add_sub(origin, sub, data)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::rename_sub { sub, data } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "rename_sub",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::rename_sub(origin, sub, data)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::remove_sub { sub } => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "remove_sub",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::remove_sub(origin, sub)
                            .map(Into::into)
                            .map_err(Into::into)
                    }
                    Self::quit_sub {} => {
                        let __within_span__ = {
                            use ::tracing::__macro_support::Callsite as _;
                            static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                                static META: ::tracing::Metadata<'static> = {
                                    ::tracing_core::metadata::Metadata::new(
                                        "quit_sub",
                                        "pallet_identity::pallet",
                                        ::tracing::Level::TRACE,
                                        Some("substrate/frame/identity/src/lib.rs"),
                                        Some(99u32),
                                        Some("pallet_identity::pallet"),
                                        ::tracing_core::field::FieldSet::new(
                                            &[],
                                            ::tracing_core::callsite::Identifier(&CALLSITE),
                                        ),
                                        ::tracing::metadata::Kind::SPAN,
                                    )
                                };
                                ::tracing::callsite::DefaultCallsite::new(&META)
                            };
                            let mut interest = ::tracing::subscriber::Interest::never();
                            if ::tracing::Level::TRACE
                                <= ::tracing::level_filters::STATIC_MAX_LEVEL
                                && ::tracing::Level::TRACE
                                    <= ::tracing::level_filters::LevelFilter::current()
                                && {
                                    interest = CALLSITE.interest();
                                    !interest.is_never()
                                }
                                && ::tracing::__macro_support::__is_enabled(
                                    CALLSITE.metadata(),
                                    interest,
                                )
                            {
                                let meta = CALLSITE.metadata();
                                ::tracing::Span::new(
                                    meta,
                                    &{ meta.fields().value_set(&[]) },
                                )
                            } else {
                                let span = ::tracing::__macro_support::__disabled_span(
                                    CALLSITE.metadata(),
                                );
                                {};
                                span
                            }
                        };
                        let __tracing_guard__ = __within_span__.enter();
                        <Pallet<T>>::quit_sub(origin).map(Into::into).map_err(Into::into)
                    }
                    Self::__Ignore(_, _) => {
                        let _ = origin;
                        {
                            ::core::panicking::panic_fmt(
                                format_args!(
                                    "internal error: entered unreachable code: {0}",
                                    format_args!("__PhantomItem cannot be used."),
                                ),
                            );
                        };
                    }
                }
            })
        }
    }
    impl<T: Config> frame_support::dispatch::Callable<T> for Pallet<T> {
        type RuntimeCall = Call<T>;
    }
    impl<T: Config> Pallet<T> {
        #[doc(hidden)]
        pub fn call_functions() -> frame_support::__private::metadata_ir::PalletCallMetadataIR {
            frame_support::__private::scale_info::meta_type::<Call<T>>().into()
        }
    }
    impl<T: Config> frame_support::__private::sp_std::fmt::Debug for Error<T> {
        fn fmt(
            &self,
            f: &mut frame_support::__private::sp_std::fmt::Formatter<'_>,
        ) -> frame_support::__private::sp_std::fmt::Result {
            f.write_str(self.as_str())
        }
    }
    impl<T: Config> Error<T> {
        #[doc(hidden)]
        pub fn as_str(&self) -> &'static str {
            match &self {
                Self::__Ignore(_, _) => {
                    ::core::panicking::panic_fmt(
                        format_args!(
                            "internal error: entered unreachable code: {0}",
                            format_args!("`__Ignore` can never be constructed"),
                        ),
                    );
                }
                Self::TooManySubAccounts => "TooManySubAccounts",
                Self::NotFound => "NotFound",
                Self::NotNamed => "NotNamed",
                Self::EmptyIndex => "EmptyIndex",
                Self::FeeChanged => "FeeChanged",
                Self::NoIdentity => "NoIdentity",
                Self::StickyJudgement => "StickyJudgement",
                Self::JudgementGiven => "JudgementGiven",
                Self::InvalidJudgement => "InvalidJudgement",
                Self::InvalidIndex => "InvalidIndex",
                Self::InvalidTarget => "InvalidTarget",
                Self::TooManyFields => "TooManyFields",
                Self::TooManyRegistrars => "TooManyRegistrars",
                Self::AlreadyClaimed => "AlreadyClaimed",
                Self::NotSub => "NotSub",
                Self::NotOwned => "NotOwned",
                Self::JudgementForDifferentIdentity => "JudgementForDifferentIdentity",
                Self::JudgementPaymentFailed => "JudgementPaymentFailed",
            }
        }
    }
    impl<T: Config> From<Error<T>> for &'static str {
        fn from(err: Error<T>) -> &'static str {
            err.as_str()
        }
    }
    impl<T: Config> From<Error<T>> for frame_support::sp_runtime::DispatchError {
        fn from(err: Error<T>) -> Self {
            use frame_support::__private::codec::Encode;
            let index = <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::index::<
                Pallet<T>,
            >()
                .expect("Every active module has an index in the runtime; qed") as u8;
            let mut encoded = err.encode();
            encoded.resize(frame_support::MAX_MODULE_ERROR_ENCODED_SIZE, 0);
            frame_support::sp_runtime::DispatchError::Module(frame_support::sp_runtime::ModuleError {
                index,
                error: TryInto::try_into(encoded)
                    .expect(
                        "encoded error is resized to be equal to the maximum encoded error size; qed",
                    ),
                message: Some(err.as_str()),
            })
        }
    }
    pub use __tt_error_token_1 as tt_error_token;
    #[doc(hidden)]
    pub mod __substrate_event_check {
        #[doc(hidden)]
        pub use __is_event_part_defined_2 as is_event_part_defined;
    }
    impl<T: Config> Pallet<T> {
        pub(super) fn deposit_event(event: Event<T>) {
            let event = <<T as Config>::RuntimeEvent as From<Event<T>>>::from(event);
            let event = <<T as Config>::RuntimeEvent as Into<
                <T as frame_system::Config>::RuntimeEvent,
            >>::into(event);
            <frame_system::Pallet<T>>::deposit_event(event)
        }
    }
    impl<T: Config> From<Event<T>> for () {
        fn from(_: Event<T>) {}
    }
    impl<T: Config> Pallet<T> {
        #[doc(hidden)]
        pub fn storage_metadata() -> frame_support::__private::metadata_ir::PalletStorageMetadataIR {
            frame_support::__private::metadata_ir::PalletStorageMetadataIR {
                prefix: <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<
                    Pallet<T>,
                >()
                    .expect(
                        "No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                    ),
                entries: {
                    #[allow(unused_mut)]
                    let mut entries = ::alloc::vec::Vec::new();
                    {
                        <IdentityOf<
                            T,
                        > as frame_support::storage::StorageEntryMetadataBuilder>::build_metadata(
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    " Information that is pertinent to identify the entity behind an account.",
                                    "",
                                    " TWOX-NOTE: OK ― `AccountId` is a secure hash.",
                                ]),
                            ),
                            &mut entries,
                        );
                    }
                    {
                        <SuperOf<
                            T,
                        > as frame_support::storage::StorageEntryMetadataBuilder>::build_metadata(
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    " The super-identity of an alternative \"sub\" identity together with its name, within that",
                                    " context. If the account is not some other account\'s sub-identity, then just `None`.",
                                ]),
                            ),
                            &mut entries,
                        );
                    }
                    {
                        <SubsOf<
                            T,
                        > as frame_support::storage::StorageEntryMetadataBuilder>::build_metadata(
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    " Alternative \"sub\" identities of this account.",
                                    "",
                                    " The first item is the deposit, the second is a vector of the accounts.",
                                    "",
                                    " TWOX-NOTE: OK ― `AccountId` is a secure hash.",
                                ]),
                            ),
                            &mut entries,
                        );
                    }
                    {
                        <Registrars<
                            T,
                        > as frame_support::storage::StorageEntryMetadataBuilder>::build_metadata(
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    " The set of registrars. Not expected to get very big as can only be added through a",
                                    " special origin (likely a council motion).",
                                    "",
                                    " The index into this can be cast to `RegistrarIndex` to get a valid value.",
                                ]),
                            ),
                            &mut entries,
                        );
                    }
                    entries
                },
            }
        }
    }
    impl<T: Config> Pallet<T> {
        ///An auto-generated getter for `IdentityOf`.
        pub fn identity<KArg>(
            k: KArg,
        ) -> Option<Registration<BalanceOf<T>, T::MaxRegistrars, T::MaxAdditionalFields>>
        where
            KArg: frame_support::__private::codec::EncodeLike<T::AccountId>,
        {
            <IdentityOf<
                T,
            > as frame_support::storage::StorageMap<
                T::AccountId,
                Registration<BalanceOf<T>, T::MaxRegistrars, T::MaxAdditionalFields>,
            >>::get(k)
        }
    }
    impl<T: Config> Pallet<T> {
        ///An auto-generated getter for `SuperOf`.
        pub fn super_of<KArg>(k: KArg) -> Option<(T::AccountId, Data)>
        where
            KArg: frame_support::__private::codec::EncodeLike<T::AccountId>,
        {
            <SuperOf<
                T,
            > as frame_support::storage::StorageMap<
                T::AccountId,
                (T::AccountId, Data),
            >>::get(k)
        }
    }
    impl<T: Config> Pallet<T> {
        ///An auto-generated getter for `SubsOf`.
        pub fn subs_of<KArg>(
            k: KArg,
        ) -> (BalanceOf<T>, BoundedVec<T::AccountId, T::MaxSubAccounts>)
        where
            KArg: frame_support::__private::codec::EncodeLike<T::AccountId>,
        {
            <SubsOf<
                T,
            > as frame_support::storage::StorageMap<
                T::AccountId,
                (BalanceOf<T>, BoundedVec<T::AccountId, T::MaxSubAccounts>),
            >>::get(k)
        }
    }
    impl<T: Config> Pallet<T> {
        ///An auto-generated getter for `Registrars`.
        pub fn registrars() -> BoundedVec<
            Option<RegistrarInfo<BalanceOf<T>, T::AccountId>>,
            T::MaxRegistrars,
        > {
            <Registrars<
                T,
            > as frame_support::storage::StorageValue<
                BoundedVec<
                    Option<RegistrarInfo<BalanceOf<T>, T::AccountId>>,
                    T::MaxRegistrars,
                >,
            >>::get()
        }
    }
    #[doc(hidden)]
    pub(super) struct _GeneratedPrefixForStorageIdentityOf<T>(
        core::marker::PhantomData<(T,)>,
    );
    impl<T: Config> frame_support::traits::StorageInstance
    for _GeneratedPrefixForStorageIdentityOf<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<
                Pallet<T>,
            >()
                .expect(
                    "No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                )
        }
        fn pallet_prefix_hash() -> [u8; 16] {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name_hash::<
                Pallet<T>,
            >()
                .expect(
                    "No name_hash found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                )
        }
        const STORAGE_PREFIX: &'static str = "IdentityOf";
        fn storage_prefix_hash() -> [u8; 16] {
            [
                205u8,
                127u8,
                55u8,
                49u8,
                124u8,
                210u8,
                11u8,
                97u8,
                233u8,
                189u8,
                70u8,
                250u8,
                184u8,
                112u8,
                71u8,
                20u8,
            ]
        }
    }
    #[doc(hidden)]
    pub(super) struct _GeneratedPrefixForStorageSuperOf<T>(
        core::marker::PhantomData<(T,)>,
    );
    impl<T: Config> frame_support::traits::StorageInstance
    for _GeneratedPrefixForStorageSuperOf<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<
                Pallet<T>,
            >()
                .expect(
                    "No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                )
        }
        fn pallet_prefix_hash() -> [u8; 16] {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name_hash::<
                Pallet<T>,
            >()
                .expect(
                    "No name_hash found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                )
        }
        const STORAGE_PREFIX: &'static str = "SuperOf";
        fn storage_prefix_hash() -> [u8; 16] {
            [
                67u8,
                169u8,
                83u8,
                172u8,
                8u8,
                46u8,
                8u8,
                182u8,
                82u8,
                124u8,
                226u8,
                98u8,
                219u8,
                212u8,
                171u8,
                242u8,
            ]
        }
    }
    #[doc(hidden)]
    pub(super) struct _GeneratedPrefixForStorageSubsOf<T>(
        core::marker::PhantomData<(T,)>,
    );
    impl<T: Config> frame_support::traits::StorageInstance
    for _GeneratedPrefixForStorageSubsOf<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<
                Pallet<T>,
            >()
                .expect(
                    "No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                )
        }
        fn pallet_prefix_hash() -> [u8; 16] {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name_hash::<
                Pallet<T>,
            >()
                .expect(
                    "No name_hash found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                )
        }
        const STORAGE_PREFIX: &'static str = "SubsOf";
        fn storage_prefix_hash() -> [u8; 16] {
            [
                110u8,
                229u8,
                160u8,
                176u8,
                158u8,
                126u8,
                154u8,
                150u8,
                33u8,
                157u8,
                214u8,
                111u8,
                15u8,
                116u8,
                195u8,
                126u8,
            ]
        }
    }
    #[doc(hidden)]
    pub(super) struct _GeneratedPrefixForStorageRegistrars<T>(
        core::marker::PhantomData<(T,)>,
    );
    impl<T: Config> frame_support::traits::StorageInstance
    for _GeneratedPrefixForStorageRegistrars<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<
                Pallet<T>,
            >()
                .expect(
                    "No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                )
        }
        fn pallet_prefix_hash() -> [u8; 16] {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name_hash::<
                Pallet<T>,
            >()
                .expect(
                    "No name_hash found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                )
        }
        const STORAGE_PREFIX: &'static str = "Registrars";
        fn storage_prefix_hash() -> [u8; 16] {
            [
                31u8,
                127u8,
                63u8,
                62u8,
                177u8,
                194u8,
                166u8,
                153u8,
                120u8,
                218u8,
                153u8,
                141u8,
                25u8,
                247u8,
                78u8,
                197u8,
            ]
        }
    }
    #[doc(hidden)]
    pub mod __substrate_inherent_check {
        #[doc(hidden)]
        pub use __is_inherent_part_defined_3 as is_inherent_part_defined;
    }
    /// Hidden instance generated to be internally used when module is used without
    /// instance.
    #[doc(hidden)]
    pub type __InherentHiddenInstance = ();
    impl<
        T: Config,
    > frame_support::traits::Hooks<frame_system::pallet_prelude::BlockNumberFor<T>>
    for Pallet<T> {}
    impl<
        T: Config,
    > frame_support::traits::OnFinalize<frame_system::pallet_prelude::BlockNumberFor<T>>
    for Pallet<T> {
        fn on_finalize(n: frame_system::pallet_prelude::BlockNumberFor<T>) {
            let __within_span__ = {
                use ::tracing::__macro_support::Callsite as _;
                static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                    static META: ::tracing::Metadata<'static> = {
                        ::tracing_core::metadata::Metadata::new(
                            "on_finalize",
                            "pallet_identity::pallet",
                            ::tracing::Level::TRACE,
                            Some("substrate/frame/identity/src/lib.rs"),
                            Some(99u32),
                            Some("pallet_identity::pallet"),
                            ::tracing_core::field::FieldSet::new(
                                &[],
                                ::tracing_core::callsite::Identifier(&CALLSITE),
                            ),
                            ::tracing::metadata::Kind::SPAN,
                        )
                    };
                    ::tracing::callsite::DefaultCallsite::new(&META)
                };
                let mut interest = ::tracing::subscriber::Interest::never();
                if ::tracing::Level::TRACE <= ::tracing::level_filters::STATIC_MAX_LEVEL
                    && ::tracing::Level::TRACE
                        <= ::tracing::level_filters::LevelFilter::current()
                    && {
                        interest = CALLSITE.interest();
                        !interest.is_never()
                    }
                    && ::tracing::__macro_support::__is_enabled(
                        CALLSITE.metadata(),
                        interest,
                    )
                {
                    let meta = CALLSITE.metadata();
                    ::tracing::Span::new(meta, &{ meta.fields().value_set(&[]) })
                } else {
                    let span = ::tracing::__macro_support::__disabled_span(
                        CALLSITE.metadata(),
                    );
                    {};
                    span
                }
            };
            let __tracing_guard__ = __within_span__.enter();
            <Self as frame_support::traits::Hooks<
                frame_system::pallet_prelude::BlockNumberFor<T>,
            >>::on_finalize(n)
        }
    }
    impl<
        T: Config,
    > frame_support::traits::OnIdle<frame_system::pallet_prelude::BlockNumberFor<T>>
    for Pallet<T> {
        fn on_idle(
            n: frame_system::pallet_prelude::BlockNumberFor<T>,
            remaining_weight: frame_support::weights::Weight,
        ) -> frame_support::weights::Weight {
            <Self as frame_support::traits::Hooks<
                frame_system::pallet_prelude::BlockNumberFor<T>,
            >>::on_idle(n, remaining_weight)
        }
    }
    impl<
        T: Config,
    > frame_support::traits::OnInitialize<
        frame_system::pallet_prelude::BlockNumberFor<T>,
    > for Pallet<T> {
        fn on_initialize(
            n: frame_system::pallet_prelude::BlockNumberFor<T>,
        ) -> frame_support::weights::Weight {
            let __within_span__ = {
                use ::tracing::__macro_support::Callsite as _;
                static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                    static META: ::tracing::Metadata<'static> = {
                        ::tracing_core::metadata::Metadata::new(
                            "on_initialize",
                            "pallet_identity::pallet",
                            ::tracing::Level::TRACE,
                            Some("substrate/frame/identity/src/lib.rs"),
                            Some(99u32),
                            Some("pallet_identity::pallet"),
                            ::tracing_core::field::FieldSet::new(
                                &[],
                                ::tracing_core::callsite::Identifier(&CALLSITE),
                            ),
                            ::tracing::metadata::Kind::SPAN,
                        )
                    };
                    ::tracing::callsite::DefaultCallsite::new(&META)
                };
                let mut interest = ::tracing::subscriber::Interest::never();
                if ::tracing::Level::TRACE <= ::tracing::level_filters::STATIC_MAX_LEVEL
                    && ::tracing::Level::TRACE
                        <= ::tracing::level_filters::LevelFilter::current()
                    && {
                        interest = CALLSITE.interest();
                        !interest.is_never()
                    }
                    && ::tracing::__macro_support::__is_enabled(
                        CALLSITE.metadata(),
                        interest,
                    )
                {
                    let meta = CALLSITE.metadata();
                    ::tracing::Span::new(meta, &{ meta.fields().value_set(&[]) })
                } else {
                    let span = ::tracing::__macro_support::__disabled_span(
                        CALLSITE.metadata(),
                    );
                    {};
                    span
                }
            };
            let __tracing_guard__ = __within_span__.enter();
            <Self as frame_support::traits::Hooks<
                frame_system::pallet_prelude::BlockNumberFor<T>,
            >>::on_initialize(n)
        }
    }
    impl<T: Config> frame_support::traits::OnRuntimeUpgrade for Pallet<T> {
        fn on_runtime_upgrade() -> frame_support::weights::Weight {
            let __within_span__ = {
                use ::tracing::__macro_support::Callsite as _;
                static CALLSITE: ::tracing::callsite::DefaultCallsite = {
                    static META: ::tracing::Metadata<'static> = {
                        ::tracing_core::metadata::Metadata::new(
                            "on_runtime_update",
                            "pallet_identity::pallet",
                            ::tracing::Level::TRACE,
                            Some("substrate/frame/identity/src/lib.rs"),
                            Some(99u32),
                            Some("pallet_identity::pallet"),
                            ::tracing_core::field::FieldSet::new(
                                &[],
                                ::tracing_core::callsite::Identifier(&CALLSITE),
                            ),
                            ::tracing::metadata::Kind::SPAN,
                        )
                    };
                    ::tracing::callsite::DefaultCallsite::new(&META)
                };
                let mut interest = ::tracing::subscriber::Interest::never();
                if ::tracing::Level::TRACE <= ::tracing::level_filters::STATIC_MAX_LEVEL
                    && ::tracing::Level::TRACE
                        <= ::tracing::level_filters::LevelFilter::current()
                    && {
                        interest = CALLSITE.interest();
                        !interest.is_never()
                    }
                    && ::tracing::__macro_support::__is_enabled(
                        CALLSITE.metadata(),
                        interest,
                    )
                {
                    let meta = CALLSITE.metadata();
                    ::tracing::Span::new(meta, &{ meta.fields().value_set(&[]) })
                } else {
                    let span = ::tracing::__macro_support::__disabled_span(
                        CALLSITE.metadata(),
                    );
                    {};
                    span
                }
            };
            let __tracing_guard__ = __within_span__.enter();
            let pallet_name = <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<
                Self,
            >()
                .unwrap_or("<unknown pallet name>");
            {
                let lvl = ::log::Level::Debug;
                if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                    ::log::__private_api::log(
                        format_args!("✅ no migration for {0}", pallet_name),
                        lvl,
                        &(
                            frame_support::LOG_TARGET,
                            "pallet_identity::pallet",
                            "substrate/frame/identity/src/lib.rs",
                        ),
                        99u32,
                        ::log::__private_api::Option::None,
                    );
                }
            };
            <Self as frame_support::traits::Hooks<
                frame_system::pallet_prelude::BlockNumberFor<T>,
            >>::on_runtime_upgrade()
        }
    }
    impl<
        T: Config,
    > frame_support::traits::OffchainWorker<
        frame_system::pallet_prelude::BlockNumberFor<T>,
    > for Pallet<T> {
        fn offchain_worker(n: frame_system::pallet_prelude::BlockNumberFor<T>) {
            <Self as frame_support::traits::Hooks<
                frame_system::pallet_prelude::BlockNumberFor<T>,
            >>::offchain_worker(n)
        }
    }
    impl<T: Config> frame_support::traits::IntegrityTest for Pallet<T> {
        fn integrity_test() {
            frame_support::__private::sp_io::TestExternalities::default()
                .execute_with(|| {
                    <Self as frame_support::traits::Hooks<
                        frame_system::pallet_prelude::BlockNumberFor<T>,
                    >>::integrity_test()
                });
        }
    }
    #[doc(hidden)]
    pub mod __substrate_genesis_config_check {
        #[doc(hidden)]
        pub use __is_genesis_config_defined_4 as is_genesis_config_defined;
        #[doc(hidden)]
        pub use __is_std_enabled_for_genesis_4 as is_std_enabled_for_genesis;
    }
    #[doc(hidden)]
    pub mod __substrate_origin_check {
        #[doc(hidden)]
        pub use __is_origin_part_defined_5 as is_origin_part_defined;
    }
    #[doc(hidden)]
    pub mod __substrate_validate_unsigned_check {
        #[doc(hidden)]
        pub use __is_validate_unsigned_part_defined_6 as is_validate_unsigned_part_defined;
    }
    pub use __tt_default_parts_7 as tt_default_parts;
    pub use __tt_extra_parts_7 as tt_extra_parts;
}
impl<T: Config> Pallet<T> {
    /// Get the subs of an account.
    pub fn subs(who: &T::AccountId) -> Vec<(T::AccountId, Data)> {
        SubsOf::<T>::get(who)
            .1
            .into_iter()
            .filter_map(|a| SuperOf::<T>::get(&a).map(|x| (a, x.1)))
            .collect()
    }
    /// Check if the account has corresponding identity information by the identity field.
    pub fn has_identity(who: &T::AccountId, fields: u64) -> bool {
        IdentityOf::<T>::get(who)
            .map_or(
                false,
                |registration| (registration.info.fields().0.bits() & fields) == fields,
            )
    }
}
