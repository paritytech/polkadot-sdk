#![feature(prelude_import)]
//! # Template Pallet
//!
//! A pallet with minimal functionality to help developers understand the essential components of
//! writing a FRAME pallet. It is typically used in beginner tutorials or in Polkadot SDK template
//! as a starting point for creating a new pallet and **not meant to be used in production**.
//!
//! ## Overview
//!
//! This template pallet contains basic examples of:
//! - declaring a storage item that stores a single block-number
//! - declaring and using events
//! - declaring and using errors
//! - a dispatchable function that allows a user to set a new value to storage and emits an event
//!   upon success
//! - another dispatchable function that causes a custom error to be thrown
//!
//! Each pallet section is annotated with an attribute using the `#[pallet::...]` procedural macro.
//! This macro generates the necessary code for a pallet to be aggregated into a FRAME runtime.
//!
//! To get started with pallet development, consider using this tutorial:
//!
//! <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html>
//!
//! And reading the main documentation of the `frame` crate:
//!
//! <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/polkadot_sdk/frame_runtime/index.html>
//!
//! And looking at the frame [`kitchen-sink`](https://paritytech.github.io/polkadot-sdk/master/pallet_example_kitchensink/index.html)
//! pallet, a showcase of all pallet macros.
//!
//! ### Pallet Sections
//!
//! The pallet sections in this template are:
//!
//! - A **configuration trait** that defines the types and parameters which the pallet depends on
//!   (denoted by the `#[pallet::config]` attribute). See: [`Config`].
//! - A **means to store pallet-specific data** (denoted by the `#[pallet::storage]` attribute).
//!   See: [`storage_types`].
//! - A **declaration of the events** this pallet emits (denoted by the `#[pallet::event]`
//!   attribute). See: [`Event`].
//! - A **declaration of the errors** that this pallet can throw (denoted by the `#[pallet::error]`
//!   attribute). See: [`Error`].
//! - A **set of dispatchable functions** that define the pallet's functionality (denoted by the
//!   `#[pallet::call]` attribute). See: [`dispatchables`].
//!
//! Run `cargo doc --package pallet-template --open` to view this pallet's documentation.

pub use pallet::*;
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
	use frame::prelude::*;
	/**
	Configuration trait of this pallet.

	The main purpose of this trait is to act as an interface between this pallet and the runtime in
	which it is embedded in. A type, function, or constant in this trait is essentially left to be
	configured by the runtime that includes this pallet.

	Consequently, a runtime that wants to include this pallet must implement this trait.*/
	/// Configure the pallet by specifying the parameters and types on which it depends.
	pub trait Config:
		frame_system::Config + frame::deps::frame_system::Config<RuntimeEvent: From<Event<Self>>>
	{
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// A type representing the weights required by the dispatchables of this pallet.
		type WeightInfo;
	}
	/**
		The `Pallet` struct, the main type that implements traits and standalone
		functions within the pallet.
	*/
	pub struct Pallet<T>(core::marker::PhantomData<(T)>);
	const _: () = {
		#[automatically_derived]
		#[allow(deprecated)]
		impl<T> ::core::clone::Clone for Pallet<T> {
			fn clone(&self) -> Self {
				Self(::core::clone::Clone::clone(&self.0))
			}
		}
	};
	#[allow(deprecated)]
	const _: () = {
		impl<T> ::core::cmp::Eq for Pallet<T> {}
	};
	const _: () = {
		#[automatically_derived]
		#[allow(deprecated)]
		impl<T> ::core::cmp::PartialEq for Pallet<T> {
			fn eq(&self, other: &Self) -> bool {
				true && self.0 == other.0
			}
		}
	};
	const _: () = {
		#[automatically_derived]
		#[allow(deprecated)]
		impl<T> ::core::fmt::Debug for Pallet<T> {
			fn fmt(&self, fmt: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				fmt.debug_tuple("Pallet").field(&self.0).finish()
			}
		}
	};
	/// A struct to store a single block-number. Has all the right derives to store it in storage.
	/// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/frame_storage_derives/index.html>
	#[scale_info(skip_type_params(T))]
	pub struct CompositeStruct<T: Config> {
		/// A block number.
		pub(crate) block_number: BlockNumberFor<T>,
	}
	#[allow(deprecated)]
	const _: () = {
		#[automatically_derived]
		impl<T: Config> ::codec::Encode for CompositeStruct<T>
		where
			BlockNumberFor<T>: ::codec::Encode,
			BlockNumberFor<T>: ::codec::Encode,
		{
			fn size_hint(&self) -> usize {
				::codec::Encode::size_hint(&&self.block_number)
			}
			fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
				&self,
				__codec_dest_edqy: &mut __CodecOutputEdqy,
			) {
				::codec::Encode::encode_to(&&self.block_number, __codec_dest_edqy)
			}
			fn encode(&self) -> ::codec::alloc::vec::Vec<::core::primitive::u8> {
				::codec::Encode::encode(&&self.block_number)
			}
			fn using_encoded<
				__CodecOutputReturn,
				__CodecUsingEncodedCallback: ::core::ops::FnOnce(&[::core::primitive::u8]) -> __CodecOutputReturn,
			>(
				&self,
				f: __CodecUsingEncodedCallback,
			) -> __CodecOutputReturn {
				::codec::Encode::using_encoded(&&self.block_number, f)
			}
		}
		#[automatically_derived]
		impl<T: Config> ::codec::EncodeLike for CompositeStruct<T>
		where
			BlockNumberFor<T>: ::codec::Encode,
			BlockNumberFor<T>: ::codec::Encode,
		{
		}
	};
	#[allow(deprecated)]
	const _: () = {
		#[automatically_derived]
		impl<T: Config> ::codec::Decode for CompositeStruct<T>
		where
			BlockNumberFor<T>: ::codec::Decode,
			BlockNumberFor<T>: ::codec::Decode,
		{
			fn decode<__CodecInputEdqy: ::codec::Input>(
				__codec_input_edqy: &mut __CodecInputEdqy,
			) -> ::core::result::Result<Self, ::codec::Error> {
				::core::result::Result::Ok(CompositeStruct::<T> {
					block_number: {
						let __codec_res_edqy =
							<BlockNumberFor<T> as ::codec::Decode>::decode(__codec_input_edqy);
						match __codec_res_edqy {
							::core::result::Result::Err(e) => {
								return ::core::result::Result::Err(
									e.chain("Could not decode `CompositeStruct::block_number`"),
								);
							},
							::core::result::Result::Ok(__codec_res_edqy) => __codec_res_edqy,
						}
					},
				})
			}
		}
	};
	const _: () = {
		#[automatically_derived]
		impl<T: Config> ::codec::MaxEncodedLen for CompositeStruct<T>
		where
			BlockNumberFor<T>: ::codec::MaxEncodedLen,
			BlockNumberFor<T>: ::codec::MaxEncodedLen,
		{
			fn max_encoded_len() -> ::core::primitive::usize {
				0_usize
					.saturating_add(<BlockNumberFor<T> as ::codec::MaxEncodedLen>::max_encoded_len())
			}
		}
	};
	#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
	const _: () = {
		impl<T: Config> ::scale_info::TypeInfo for CompositeStruct<T>
		where
			BlockNumberFor<T>: ::scale_info::TypeInfo + 'static,
			T: Config + 'static,
		{
			type Identity = Self;
			fn type_info() -> ::scale_info::Type {
				::scale_info::Type::builder()
                    .path(
                        ::scale_info::Path::new_with_replace(
                            "CompositeStruct",
                            "pallet_parachain_template::pallet",
                            &[],
                        ),
                    )
                    .type_params(
                        <[_]>::into_vec(
                            ::alloc::boxed::box_new([
                                ::scale_info::TypeParameter::new(
                                    "T",
                                    ::core::option::Option::None,
                                ),
                            ]),
                        ),
                    )
                    .docs(
                        &[
                            "A struct to store a single block-number. Has all the right derives to store it in storage.",
                            "<https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/frame_storage_derives/index.html>",
                        ],
                    )
                    .composite(
                        ::scale_info::build::Fields::named()
                            .field(|f| {
                                f
                                    .ty::<BlockNumberFor<T>>()
                                    .name("block_number")
                                    .type_name("BlockNumberFor<T>")
                                    .docs(&["A block number."])
                            }),
                    )
			}
		}
	};
	const _: () = {
		#[automatically_derived]
		#[allow(deprecated)]
		impl<T: Config> ::core::clone::Clone for CompositeStruct<T> {
			fn clone(&self) -> Self {
				Self { block_number: ::core::clone::Clone::clone(&self.block_number) }
			}
		}
	};
	const _: () = {
		#[automatically_derived]
		#[allow(deprecated)]
		impl<T: Config> ::core::cmp::PartialEq for CompositeStruct<T> {
			fn eq(&self, other: &Self) -> bool {
				true && self.block_number == other.block_number
			}
		}
	};
	const _: () = {
		#[automatically_derived]
		#[allow(deprecated)]
		impl<T: Config> ::core::default::Default for CompositeStruct<T> {
			fn default() -> Self {
				Self { block_number: ::core::default::Default::default() }
			}
		}
	};
	/// The pallet's storage items.
	/// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html#storage>
	/// <https://paritytech.github.io/polkadot-sdk/master/frame_support/pallet_macros/attr.storage.html>
	#[allow(type_alias_bounds)]
	///
	///Storage type is [`StorageValue`] with value type `CompositeStruct < T >`.
	pub type Something<T: Config> =
		StorageValue<_GeneratedPrefixForStorageSomething<T>, CompositeStruct<T>>;
	/// Pallets use events to inform users when important changes are made.
	/// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html#event-and-error>
	#[scale_info(skip_type_params(T), capture_docs = "always")]
	pub enum Event<T: Config> {
		/// We usually use passive tense for events.
		SomethingStored { block_number: BlockNumberFor<T>, who: T::AccountId },
		#[doc(hidden)]
		#[codec(skip)]
		__Ignore(::core::marker::PhantomData<(T)>, frame::deps::frame_support::Never),
	}
	const _: () = {
		#[automatically_derived]
		#[allow(deprecated)]
		impl<T: Config> ::core::clone::Clone for Event<T> {
			fn clone(&self) -> Self {
				match self {
					Self::SomethingStored { ref block_number, ref who } => Self::SomethingStored {
						block_number: ::core::clone::Clone::clone(block_number),
						who: ::core::clone::Clone::clone(who),
					},
					Self::__Ignore(ref _0, ref _1) => Self::__Ignore(
						::core::clone::Clone::clone(_0),
						::core::clone::Clone::clone(_1),
					),
				}
			}
		}
	};
	#[allow(deprecated)]
	const _: () = {
		impl<T: Config> ::core::cmp::Eq for Event<T> {}
	};
	const _: () = {
		#[automatically_derived]
		#[allow(deprecated)]
		impl<T: Config> ::core::cmp::PartialEq for Event<T> {
			fn eq(&self, other: &Self) -> bool {
				match (self, other) {
					(
						Self::SomethingStored { block_number, who },
						Self::SomethingStored { block_number: _0, who: _1 },
					) => true && block_number == _0 && who == _1,
					(Self::__Ignore(_0, _1), Self::__Ignore(_0_other, _1_other)) => {
						true && _0 == _0_other && _1 == _1_other
					},
					(Self::SomethingStored { .. }, Self::__Ignore { .. }) => false,
					(Self::__Ignore { .. }, Self::SomethingStored { .. }) => false,
				}
			}
		}
	};
	const _: () = {
		#[automatically_derived]
		#[allow(deprecated)]
		impl<T: Config> ::core::fmt::Debug for Event<T> {
			fn fmt(&self, fmt: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				match *self {
					Self::SomethingStored { ref block_number, ref who } => fmt
						.debug_struct("Event::SomethingStored")
						.field("block_number", &block_number)
						.field("who", &who)
						.finish(),
					Self::__Ignore(ref _0, ref _1) => {
						fmt.debug_tuple("Event::__Ignore").field(&_0).field(&_1).finish()
					},
				}
			}
		}
	};
	#[allow(deprecated)]
	const _: () = {
		#[automatically_derived]
		impl<T: Config> ::codec::Encode for Event<T>
		where
			BlockNumberFor<T>: ::codec::Encode,
			BlockNumberFor<T>: ::codec::Encode,
			T::AccountId: ::codec::Encode,
			T::AccountId: ::codec::Encode,
		{
			fn size_hint(&self) -> usize {
				1_usize
					+ match *self {
						Event::SomethingStored { ref block_number, ref who } => 0_usize
							.saturating_add(::codec::Encode::size_hint(block_number))
							.saturating_add(::codec::Encode::size_hint(who)),
						_ => 0_usize,
					}
			}
			fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
				&self,
				__codec_dest_edqy: &mut __CodecOutputEdqy,
			) {
				#[automatically_derived]
				const _: () = {
					#[allow(clippy::unnecessary_cast)]
					#[allow(clippy::cast_possible_truncation)]
					const indices: [(usize, &'static str); 1usize] =
						[((0usize) as ::core::primitive::usize, "SomethingStored")];
					const fn search_for_invalid_index(
						array: &[(usize, &'static str); 1usize],
					) -> (bool, usize) {
						let mut i = 0;
						while i < 1usize {
							if array[i].0 > 255 {
								return (true, i);
							}
							i += 1;
						}
						(false, 0)
					}
					const INVALID_INDEX: (bool, usize) = search_for_invalid_index(&indices);
					if INVALID_INDEX.0 {
						let msg = ::const_format::pmr::__AssertStr {
							x: {
								use ::const_format::__cf_osRcTFl4A;
								({
									#[doc(hidden)]
									#[allow(unused_mut, non_snake_case)]
									const CONCATP_NHPMWYD3NJA: &[__cf_osRcTFl4A::pmr::PArgument] = {
										let fmt = __cf_osRcTFl4A::pmr::FormattingFlags::NEW;
										&[
											__cf_osRcTFl4A::pmr::PConvWrapper("Found variant `")
												.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												indices[INVALID_INDEX.1].1,
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												"` with invalid index: `",
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												indices[INVALID_INDEX.1].0,
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												"`. Max supported index is 255.",
											)
											.to_pargument_display(fmt),
										]
									};
									{
										#[doc(hidden)]
										const ARR_LEN: usize =
											::const_format::pmr::PArgument::calc_len(
												CONCATP_NHPMWYD3NJA,
											);
										#[doc(hidden)]
										const CONCAT_ARR: &::const_format::pmr::LenAndArray<
											[u8; ARR_LEN],
										> = &::const_format::pmr::__priv_concatenate(
											CONCATP_NHPMWYD3NJA,
										);
										#[doc(hidden)]
										#[allow(clippy::transmute_ptr_to_ptr)]
										const CONCAT_STR: &str = unsafe {
											let slice = ::const_format::pmr::transmute::<
												&[u8; ARR_LEN],
												&[u8; CONCAT_ARR.len],
											>(&CONCAT_ARR.array);
											{
												let bytes: &'static [::const_format::pmr::u8] =
													slice;
												let string: &'static ::const_format::pmr::str = {
													::const_format::__hidden_utils::PtrToRef {
														ptr: bytes
															as *const [::const_format::pmr::u8]
															as *const str,
													}
													.reff
												};
												string
											}
										};
										CONCAT_STR
									}
								})
							},
						}
						.x;
						{
							#[cold]
							#[track_caller]
							#[inline(never)]
							#[rustc_const_panic_str]
							#[rustc_do_not_const_check]
							const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
								::core::panicking::panic_display(arg)
							}
							panic_cold_display(&msg);
						};
					}
					const fn duplicate_info(
						array: &[(usize, &'static str); 1usize],
					) -> (bool, usize, usize) {
						let len = 1usize;
						let mut i = 0usize;
						while i < len {
							let mut j = i + 1;
							while j < len {
								if array[i].0 == array[j].0 {
									return (true, i, j);
								}
								j += 1;
							}
							i += 1;
						}
						(false, 0, 0)
					}
					const DUP_INFO: (bool, usize, usize) = duplicate_info(&indices);
					if DUP_INFO.0 {
						let msg = ::const_format::pmr::__AssertStr {
                            x: {
                                use ::const_format::__cf_osRcTFl4A;
                                ({
                                    #[doc(hidden)]
                                    #[allow(unused_mut, non_snake_case)]
                                    const CONCATP_NHPMWYD3NJA: &[__cf_osRcTFl4A::pmr::PArgument] = {
                                        let fmt = __cf_osRcTFl4A::pmr::FormattingFlags::NEW;
                                        &[
                                            __cf_osRcTFl4A::pmr::PConvWrapper(
                                                    "Found variants that have duplicate indexes. Both `",
                                                )
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.1].1)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper("` and `")
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.2].1)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper("` have the index `")
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.1].0)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(
                                                    "`. Use different indexes for each variant.",
                                                )
                                                .to_pargument_display(fmt),
                                        ]
                                    };
                                    {
                                        #[doc(hidden)]
                                        const ARR_LEN: usize = ::const_format::pmr::PArgument::calc_len(
                                            CONCATP_NHPMWYD3NJA,
                                        );
                                        #[doc(hidden)]
                                        const CONCAT_ARR: &::const_format::pmr::LenAndArray<
                                            [u8; ARR_LEN],
                                        > = &::const_format::pmr::__priv_concatenate(
                                            CONCATP_NHPMWYD3NJA,
                                        );
                                        #[doc(hidden)]
                                        #[allow(clippy::transmute_ptr_to_ptr)]
                                        const CONCAT_STR: &str = unsafe {
                                            let slice = ::const_format::pmr::transmute::<
                                                &[u8; ARR_LEN],
                                                &[u8; CONCAT_ARR.len],
                                            >(&CONCAT_ARR.array);
                                            {
                                                let bytes: &'static [::const_format::pmr::u8] = slice;
                                                let string: &'static ::const_format::pmr::str = {
                                                    ::const_format::__hidden_utils::PtrToRef {
                                                        ptr: bytes as *const [::const_format::pmr::u8] as *const str,
                                                    }
                                                        .reff
                                                };
                                                string
                                            }
                                        };
                                        CONCAT_STR
                                    }
                                })
                            },
                        }
                            .x;
						{
							#[cold]
							#[track_caller]
							#[inline(never)]
							#[rustc_const_panic_str]
							#[rustc_do_not_const_check]
							const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
								::core::panicking::panic_display(arg)
							}
							panic_cold_display(&msg);
						};
					}
				};
				match *self {
					Event::SomethingStored { ref block_number, ref who } => {
						#[allow(clippy::unnecessary_cast)]
						__codec_dest_edqy.push_byte((0usize) as ::core::primitive::u8);
						::codec::Encode::encode_to(block_number, __codec_dest_edqy);
						::codec::Encode::encode_to(who, __codec_dest_edqy);
					},
					_ => {},
				}
			}
		}
		#[automatically_derived]
		impl<T: Config> ::codec::EncodeLike for Event<T>
		where
			BlockNumberFor<T>: ::codec::Encode,
			BlockNumberFor<T>: ::codec::Encode,
			T::AccountId: ::codec::Encode,
			T::AccountId: ::codec::Encode,
		{
		}
	};
	#[allow(deprecated)]
	const _: () = {
		#[automatically_derived]
		impl<T: Config> ::codec::Decode for Event<T>
		where
			BlockNumberFor<T>: ::codec::Decode,
			BlockNumberFor<T>: ::codec::Decode,
			T::AccountId: ::codec::Decode,
			T::AccountId: ::codec::Decode,
		{
			fn decode<__CodecInputEdqy: ::codec::Input>(
				__codec_input_edqy: &mut __CodecInputEdqy,
			) -> ::core::result::Result<Self, ::codec::Error> {
				#[automatically_derived]
				const _: () = {
					#[allow(clippy::unnecessary_cast)]
					#[allow(clippy::cast_possible_truncation)]
					const indices: [(usize, &'static str); 1usize] =
						[((0usize) as ::core::primitive::usize, "SomethingStored")];
					const fn search_for_invalid_index(
						array: &[(usize, &'static str); 1usize],
					) -> (bool, usize) {
						let mut i = 0;
						while i < 1usize {
							if array[i].0 > 255 {
								return (true, i);
							}
							i += 1;
						}
						(false, 0)
					}
					const INVALID_INDEX: (bool, usize) = search_for_invalid_index(&indices);
					if INVALID_INDEX.0 {
						let msg = ::const_format::pmr::__AssertStr {
							x: {
								use ::const_format::__cf_osRcTFl4A;
								({
									#[doc(hidden)]
									#[allow(unused_mut, non_snake_case)]
									const CONCATP_NHPMWYD3NJA: &[__cf_osRcTFl4A::pmr::PArgument] = {
										let fmt = __cf_osRcTFl4A::pmr::FormattingFlags::NEW;
										&[
											__cf_osRcTFl4A::pmr::PConvWrapper("Found variant `")
												.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												indices[INVALID_INDEX.1].1,
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												"` with invalid index: `",
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												indices[INVALID_INDEX.1].0,
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												"`. Max supported index is 255.",
											)
											.to_pargument_display(fmt),
										]
									};
									{
										#[doc(hidden)]
										const ARR_LEN: usize =
											::const_format::pmr::PArgument::calc_len(
												CONCATP_NHPMWYD3NJA,
											);
										#[doc(hidden)]
										const CONCAT_ARR: &::const_format::pmr::LenAndArray<
											[u8; ARR_LEN],
										> = &::const_format::pmr::__priv_concatenate(
											CONCATP_NHPMWYD3NJA,
										);
										#[doc(hidden)]
										#[allow(clippy::transmute_ptr_to_ptr)]
										const CONCAT_STR: &str = unsafe {
											let slice = ::const_format::pmr::transmute::<
												&[u8; ARR_LEN],
												&[u8; CONCAT_ARR.len],
											>(&CONCAT_ARR.array);
											{
												let bytes: &'static [::const_format::pmr::u8] =
													slice;
												let string: &'static ::const_format::pmr::str = {
													::const_format::__hidden_utils::PtrToRef {
														ptr: bytes
															as *const [::const_format::pmr::u8]
															as *const str,
													}
													.reff
												};
												string
											}
										};
										CONCAT_STR
									}
								})
							},
						}
						.x;
						{
							#[cold]
							#[track_caller]
							#[inline(never)]
							#[rustc_const_panic_str]
							#[rustc_do_not_const_check]
							const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
								::core::panicking::panic_display(arg)
							}
							panic_cold_display(&msg);
						};
					}
					const fn duplicate_info(
						array: &[(usize, &'static str); 1usize],
					) -> (bool, usize, usize) {
						let len = 1usize;
						let mut i = 0usize;
						while i < len {
							let mut j = i + 1;
							while j < len {
								if array[i].0 == array[j].0 {
									return (true, i, j);
								}
								j += 1;
							}
							i += 1;
						}
						(false, 0, 0)
					}
					const DUP_INFO: (bool, usize, usize) = duplicate_info(&indices);
					if DUP_INFO.0 {
						let msg = ::const_format::pmr::__AssertStr {
                            x: {
                                use ::const_format::__cf_osRcTFl4A;
                                ({
                                    #[doc(hidden)]
                                    #[allow(unused_mut, non_snake_case)]
                                    const CONCATP_NHPMWYD3NJA: &[__cf_osRcTFl4A::pmr::PArgument] = {
                                        let fmt = __cf_osRcTFl4A::pmr::FormattingFlags::NEW;
                                        &[
                                            __cf_osRcTFl4A::pmr::PConvWrapper(
                                                    "Found variants that have duplicate indexes. Both `",
                                                )
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.1].1)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper("` and `")
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.2].1)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper("` have the index `")
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.1].0)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(
                                                    "`. Use different indexes for each variant.",
                                                )
                                                .to_pargument_display(fmt),
                                        ]
                                    };
                                    {
                                        #[doc(hidden)]
                                        const ARR_LEN: usize = ::const_format::pmr::PArgument::calc_len(
                                            CONCATP_NHPMWYD3NJA,
                                        );
                                        #[doc(hidden)]
                                        const CONCAT_ARR: &::const_format::pmr::LenAndArray<
                                            [u8; ARR_LEN],
                                        > = &::const_format::pmr::__priv_concatenate(
                                            CONCATP_NHPMWYD3NJA,
                                        );
                                        #[doc(hidden)]
                                        #[allow(clippy::transmute_ptr_to_ptr)]
                                        const CONCAT_STR: &str = unsafe {
                                            let slice = ::const_format::pmr::transmute::<
                                                &[u8; ARR_LEN],
                                                &[u8; CONCAT_ARR.len],
                                            >(&CONCAT_ARR.array);
                                            {
                                                let bytes: &'static [::const_format::pmr::u8] = slice;
                                                let string: &'static ::const_format::pmr::str = {
                                                    ::const_format::__hidden_utils::PtrToRef {
                                                        ptr: bytes as *const [::const_format::pmr::u8] as *const str,
                                                    }
                                                        .reff
                                                };
                                                string
                                            }
                                        };
                                        CONCAT_STR
                                    }
                                })
                            },
                        }
                            .x;
						{
							#[cold]
							#[track_caller]
							#[inline(never)]
							#[rustc_const_panic_str]
							#[rustc_do_not_const_check]
							const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
								::core::panicking::panic_display(arg)
							}
							panic_cold_display(&msg);
						};
					}
				};
				match __codec_input_edqy
					.read_byte()
					.map_err(|e| e.chain("Could not decode `Event`, failed to read variant byte"))?
				{
					#[allow(clippy::unnecessary_cast)]
					#[allow(clippy::cast_possible_truncation)]
					__codec_x_edqy if __codec_x_edqy == (0usize) as ::core::primitive::u8 => {
						#[allow(clippy::redundant_closure_call)]
						return (move || {
							::core::result::Result::Ok(Event::SomethingStored::<T> {
								block_number: {
									let __codec_res_edqy =
										<BlockNumberFor<T> as ::codec::Decode>::decode(
											__codec_input_edqy,
										);
									match __codec_res_edqy {
										::core::result::Result::Err(e) => {
											return ::core::result::Result::Err(
                                                e
                                                    .chain(
                                                        "Could not decode `Event::SomethingStored::block_number`",
                                                    ),
                                            );
										},
										::core::result::Result::Ok(__codec_res_edqy) => {
											__codec_res_edqy
										},
									}
								},
								who: {
									let __codec_res_edqy =
										<T::AccountId as ::codec::Decode>::decode(
											__codec_input_edqy,
										);
									match __codec_res_edqy {
										::core::result::Result::Err(e) => {
											return ::core::result::Result::Err(e.chain(
												"Could not decode `Event::SomethingStored::who`",
											));
										},
										::core::result::Result::Ok(__codec_res_edqy) => {
											__codec_res_edqy
										},
									}
								},
							})
						})();
					},
					_ => {
						#[allow(clippy::redundant_closure_call)]
						return (move || {
							::core::result::Result::Err(<_ as ::core::convert::Into<_>>::into(
								"Could not decode `Event`, variant doesn't exist",
							))
						})();
					},
				}
			}
		}
	};
	#[allow(deprecated)]
	const _: () = {
		fn check_struct<T: Config>()
		where
			BlockNumberFor<T>: ::codec::DecodeWithMemTracking,
			BlockNumberFor<T>: ::codec::DecodeWithMemTracking,
			T::AccountId: ::codec::DecodeWithMemTracking,
			T::AccountId: ::codec::DecodeWithMemTracking,
		{
			fn check_field<T: ::codec::DecodeWithMemTracking>() {}
			check_field::<BlockNumberFor<T>>();
			check_field::<T::AccountId>();
		}
		#[automatically_derived]
		impl<T: Config> ::codec::DecodeWithMemTracking for Event<T>
		where
			BlockNumberFor<T>: ::codec::DecodeWithMemTracking,
			BlockNumberFor<T>: ::codec::DecodeWithMemTracking,
			T::AccountId: ::codec::DecodeWithMemTracking,
			T::AccountId: ::codec::DecodeWithMemTracking,
		{
		}
	};
	#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
	const _: () = {
		impl<T: Config> ::scale_info::TypeInfo for Event<T>
		where
			BlockNumberFor<T>: ::scale_info::TypeInfo + 'static,
			T::AccountId: ::scale_info::TypeInfo + 'static,
			::core::marker::PhantomData<(T)>: ::scale_info::TypeInfo + 'static,
			T: Config + 'static,
		{
			type Identity = Self;
			fn type_info() -> ::scale_info::Type {
				::scale_info::Type::builder()
                    .path(
                        ::scale_info::Path::new_with_replace(
                            "Event",
                            "pallet_parachain_template::pallet",
                            &[],
                        ),
                    )
                    .type_params(
                        <[_]>::into_vec(
                            ::alloc::boxed::box_new([
                                ::scale_info::TypeParameter::new(
                                    "T",
                                    ::core::option::Option::None,
                                ),
                            ]),
                        ),
                    )
                    .docs_always(
                        &[
                            "Pallets use events to inform users when important changes are made.",
                            "<https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html#event-and-error>",
                        ],
                    )
                    .variant(
                        ::scale_info::build::Variants::new()
                            .variant(
                                "SomethingStored",
                                |v| {
                                    v
                                        .index(0usize as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| {
                                                    f
                                                        .ty::<BlockNumberFor<T>>()
                                                        .name("block_number")
                                                        .type_name("BlockNumberFor<T>")
                                                })
                                                .field(|f| {
                                                    f.ty::<T::AccountId>().name("who").type_name("T::AccountId")
                                                }),
                                        )
                                        .docs_always(&["We usually use passive tense for events."])
                                },
                            ),
                    )
			}
		}
	};
	/// Errors inform users that something went wrong.
	/// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html#event-and-error>
	#[scale_info(skip_type_params(T), capture_docs = "always")]
	pub enum Error<T> {
		#[doc(hidden)]
		#[codec(skip)]
		__Ignore(core::marker::PhantomData<(T)>, frame::deps::frame_support::Never),
		/// Error names should be descriptive.
		NoneValue,
		/// Errors should have helpful documentation associated with them.
		StorageOverflow,
	}
	#[allow(deprecated)]
	const _: () = {
		#[automatically_derived]
		impl<T> ::codec::Encode for Error<T> {
			fn size_hint(&self) -> usize {
				1_usize
					+ match *self {
						Error::NoneValue => 0_usize,
						Error::StorageOverflow => 0_usize,
						_ => 0_usize,
					}
			}
			fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
				&self,
				__codec_dest_edqy: &mut __CodecOutputEdqy,
			) {
				#[automatically_derived]
				const _: () = {
					#[allow(clippy::unnecessary_cast)]
					#[allow(clippy::cast_possible_truncation)]
					const indices: [(usize, &'static str); 2usize] = [
						((0usize) as ::core::primitive::usize, "NoneValue"),
						((1usize) as ::core::primitive::usize, "StorageOverflow"),
					];
					const fn search_for_invalid_index(
						array: &[(usize, &'static str); 2usize],
					) -> (bool, usize) {
						let mut i = 0;
						while i < 2usize {
							if array[i].0 > 255 {
								return (true, i);
							}
							i += 1;
						}
						(false, 0)
					}
					const INVALID_INDEX: (bool, usize) = search_for_invalid_index(&indices);
					if INVALID_INDEX.0 {
						let msg = ::const_format::pmr::__AssertStr {
							x: {
								use ::const_format::__cf_osRcTFl4A;
								({
									#[doc(hidden)]
									#[allow(unused_mut, non_snake_case)]
									const CONCATP_NHPMWYD3NJA: &[__cf_osRcTFl4A::pmr::PArgument] = {
										let fmt = __cf_osRcTFl4A::pmr::FormattingFlags::NEW;
										&[
											__cf_osRcTFl4A::pmr::PConvWrapper("Found variant `")
												.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												indices[INVALID_INDEX.1].1,
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												"` with invalid index: `",
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												indices[INVALID_INDEX.1].0,
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												"`. Max supported index is 255.",
											)
											.to_pargument_display(fmt),
										]
									};
									{
										#[doc(hidden)]
										const ARR_LEN: usize =
											::const_format::pmr::PArgument::calc_len(
												CONCATP_NHPMWYD3NJA,
											);
										#[doc(hidden)]
										const CONCAT_ARR: &::const_format::pmr::LenAndArray<
											[u8; ARR_LEN],
										> = &::const_format::pmr::__priv_concatenate(
											CONCATP_NHPMWYD3NJA,
										);
										#[doc(hidden)]
										#[allow(clippy::transmute_ptr_to_ptr)]
										const CONCAT_STR: &str = unsafe {
											let slice = ::const_format::pmr::transmute::<
												&[u8; ARR_LEN],
												&[u8; CONCAT_ARR.len],
											>(&CONCAT_ARR.array);
											{
												let bytes: &'static [::const_format::pmr::u8] =
													slice;
												let string: &'static ::const_format::pmr::str = {
													::const_format::__hidden_utils::PtrToRef {
														ptr: bytes
															as *const [::const_format::pmr::u8]
															as *const str,
													}
													.reff
												};
												string
											}
										};
										CONCAT_STR
									}
								})
							},
						}
						.x;
						{
							#[cold]
							#[track_caller]
							#[inline(never)]
							#[rustc_const_panic_str]
							#[rustc_do_not_const_check]
							const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
								::core::panicking::panic_display(arg)
							}
							panic_cold_display(&msg);
						};
					}
					const fn duplicate_info(
						array: &[(usize, &'static str); 2usize],
					) -> (bool, usize, usize) {
						let len = 2usize;
						let mut i = 0usize;
						while i < len {
							let mut j = i + 1;
							while j < len {
								if array[i].0 == array[j].0 {
									return (true, i, j);
								}
								j += 1;
							}
							i += 1;
						}
						(false, 0, 0)
					}
					const DUP_INFO: (bool, usize, usize) = duplicate_info(&indices);
					if DUP_INFO.0 {
						let msg = ::const_format::pmr::__AssertStr {
                            x: {
                                use ::const_format::__cf_osRcTFl4A;
                                ({
                                    #[doc(hidden)]
                                    #[allow(unused_mut, non_snake_case)]
                                    const CONCATP_NHPMWYD3NJA: &[__cf_osRcTFl4A::pmr::PArgument] = {
                                        let fmt = __cf_osRcTFl4A::pmr::FormattingFlags::NEW;
                                        &[
                                            __cf_osRcTFl4A::pmr::PConvWrapper(
                                                    "Found variants that have duplicate indexes. Both `",
                                                )
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.1].1)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper("` and `")
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.2].1)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper("` have the index `")
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.1].0)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(
                                                    "`. Use different indexes for each variant.",
                                                )
                                                .to_pargument_display(fmt),
                                        ]
                                    };
                                    {
                                        #[doc(hidden)]
                                        const ARR_LEN: usize = ::const_format::pmr::PArgument::calc_len(
                                            CONCATP_NHPMWYD3NJA,
                                        );
                                        #[doc(hidden)]
                                        const CONCAT_ARR: &::const_format::pmr::LenAndArray<
                                            [u8; ARR_LEN],
                                        > = &::const_format::pmr::__priv_concatenate(
                                            CONCATP_NHPMWYD3NJA,
                                        );
                                        #[doc(hidden)]
                                        #[allow(clippy::transmute_ptr_to_ptr)]
                                        const CONCAT_STR: &str = unsafe {
                                            let slice = ::const_format::pmr::transmute::<
                                                &[u8; ARR_LEN],
                                                &[u8; CONCAT_ARR.len],
                                            >(&CONCAT_ARR.array);
                                            {
                                                let bytes: &'static [::const_format::pmr::u8] = slice;
                                                let string: &'static ::const_format::pmr::str = {
                                                    ::const_format::__hidden_utils::PtrToRef {
                                                        ptr: bytes as *const [::const_format::pmr::u8] as *const str,
                                                    }
                                                        .reff
                                                };
                                                string
                                            }
                                        };
                                        CONCAT_STR
                                    }
                                })
                            },
                        }
                            .x;
						{
							#[cold]
							#[track_caller]
							#[inline(never)]
							#[rustc_const_panic_str]
							#[rustc_do_not_const_check]
							const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
								::core::panicking::panic_display(arg)
							}
							panic_cold_display(&msg);
						};
					}
				};
				match *self {
					Error::NoneValue => {
						#[allow(clippy::unnecessary_cast)]
						#[allow(clippy::cast_possible_truncation)]
						__codec_dest_edqy.push_byte((0usize) as ::core::primitive::u8);
					},
					Error::StorageOverflow => {
						#[allow(clippy::unnecessary_cast)]
						#[allow(clippy::cast_possible_truncation)]
						__codec_dest_edqy.push_byte((1usize) as ::core::primitive::u8);
					},
					_ => {},
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
				#[automatically_derived]
				const _: () = {
					#[allow(clippy::unnecessary_cast)]
					#[allow(clippy::cast_possible_truncation)]
					const indices: [(usize, &'static str); 2usize] = [
						((0usize) as ::core::primitive::usize, "NoneValue"),
						((1usize) as ::core::primitive::usize, "StorageOverflow"),
					];
					const fn search_for_invalid_index(
						array: &[(usize, &'static str); 2usize],
					) -> (bool, usize) {
						let mut i = 0;
						while i < 2usize {
							if array[i].0 > 255 {
								return (true, i);
							}
							i += 1;
						}
						(false, 0)
					}
					const INVALID_INDEX: (bool, usize) = search_for_invalid_index(&indices);
					if INVALID_INDEX.0 {
						let msg = ::const_format::pmr::__AssertStr {
							x: {
								use ::const_format::__cf_osRcTFl4A;
								({
									#[doc(hidden)]
									#[allow(unused_mut, non_snake_case)]
									const CONCATP_NHPMWYD3NJA: &[__cf_osRcTFl4A::pmr::PArgument] = {
										let fmt = __cf_osRcTFl4A::pmr::FormattingFlags::NEW;
										&[
											__cf_osRcTFl4A::pmr::PConvWrapper("Found variant `")
												.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												indices[INVALID_INDEX.1].1,
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												"` with invalid index: `",
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												indices[INVALID_INDEX.1].0,
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												"`. Max supported index is 255.",
											)
											.to_pargument_display(fmt),
										]
									};
									{
										#[doc(hidden)]
										const ARR_LEN: usize =
											::const_format::pmr::PArgument::calc_len(
												CONCATP_NHPMWYD3NJA,
											);
										#[doc(hidden)]
										const CONCAT_ARR: &::const_format::pmr::LenAndArray<
											[u8; ARR_LEN],
										> = &::const_format::pmr::__priv_concatenate(
											CONCATP_NHPMWYD3NJA,
										);
										#[doc(hidden)]
										#[allow(clippy::transmute_ptr_to_ptr)]
										const CONCAT_STR: &str = unsafe {
											let slice = ::const_format::pmr::transmute::<
												&[u8; ARR_LEN],
												&[u8; CONCAT_ARR.len],
											>(&CONCAT_ARR.array);
											{
												let bytes: &'static [::const_format::pmr::u8] =
													slice;
												let string: &'static ::const_format::pmr::str = {
													::const_format::__hidden_utils::PtrToRef {
														ptr: bytes
															as *const [::const_format::pmr::u8]
															as *const str,
													}
													.reff
												};
												string
											}
										};
										CONCAT_STR
									}
								})
							},
						}
						.x;
						{
							#[cold]
							#[track_caller]
							#[inline(never)]
							#[rustc_const_panic_str]
							#[rustc_do_not_const_check]
							const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
								::core::panicking::panic_display(arg)
							}
							panic_cold_display(&msg);
						};
					}
					const fn duplicate_info(
						array: &[(usize, &'static str); 2usize],
					) -> (bool, usize, usize) {
						let len = 2usize;
						let mut i = 0usize;
						while i < len {
							let mut j = i + 1;
							while j < len {
								if array[i].0 == array[j].0 {
									return (true, i, j);
								}
								j += 1;
							}
							i += 1;
						}
						(false, 0, 0)
					}
					const DUP_INFO: (bool, usize, usize) = duplicate_info(&indices);
					if DUP_INFO.0 {
						let msg = ::const_format::pmr::__AssertStr {
                            x: {
                                use ::const_format::__cf_osRcTFl4A;
                                ({
                                    #[doc(hidden)]
                                    #[allow(unused_mut, non_snake_case)]
                                    const CONCATP_NHPMWYD3NJA: &[__cf_osRcTFl4A::pmr::PArgument] = {
                                        let fmt = __cf_osRcTFl4A::pmr::FormattingFlags::NEW;
                                        &[
                                            __cf_osRcTFl4A::pmr::PConvWrapper(
                                                    "Found variants that have duplicate indexes. Both `",
                                                )
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.1].1)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper("` and `")
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.2].1)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper("` have the index `")
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.1].0)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(
                                                    "`. Use different indexes for each variant.",
                                                )
                                                .to_pargument_display(fmt),
                                        ]
                                    };
                                    {
                                        #[doc(hidden)]
                                        const ARR_LEN: usize = ::const_format::pmr::PArgument::calc_len(
                                            CONCATP_NHPMWYD3NJA,
                                        );
                                        #[doc(hidden)]
                                        const CONCAT_ARR: &::const_format::pmr::LenAndArray<
                                            [u8; ARR_LEN],
                                        > = &::const_format::pmr::__priv_concatenate(
                                            CONCATP_NHPMWYD3NJA,
                                        );
                                        #[doc(hidden)]
                                        #[allow(clippy::transmute_ptr_to_ptr)]
                                        const CONCAT_STR: &str = unsafe {
                                            let slice = ::const_format::pmr::transmute::<
                                                &[u8; ARR_LEN],
                                                &[u8; CONCAT_ARR.len],
                                            >(&CONCAT_ARR.array);
                                            {
                                                let bytes: &'static [::const_format::pmr::u8] = slice;
                                                let string: &'static ::const_format::pmr::str = {
                                                    ::const_format::__hidden_utils::PtrToRef {
                                                        ptr: bytes as *const [::const_format::pmr::u8] as *const str,
                                                    }
                                                        .reff
                                                };
                                                string
                                            }
                                        };
                                        CONCAT_STR
                                    }
                                })
                            },
                        }
                            .x;
						{
							#[cold]
							#[track_caller]
							#[inline(never)]
							#[rustc_const_panic_str]
							#[rustc_do_not_const_check]
							const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
								::core::panicking::panic_display(arg)
							}
							panic_cold_display(&msg);
						};
					}
				};
				match __codec_input_edqy
					.read_byte()
					.map_err(|e| e.chain("Could not decode `Error`, failed to read variant byte"))?
				{
					#[allow(clippy::unnecessary_cast)]
					#[allow(clippy::cast_possible_truncation)]
					__codec_x_edqy if __codec_x_edqy == (0usize) as ::core::primitive::u8 => {
						#[allow(clippy::redundant_closure_call)]
						return (move || ::core::result::Result::Ok(Error::NoneValue::<T>))();
					},
					#[allow(clippy::unnecessary_cast)]
					#[allow(clippy::cast_possible_truncation)]
					__codec_x_edqy if __codec_x_edqy == (1usize) as ::core::primitive::u8 => {
						#[allow(clippy::redundant_closure_call)]
						return (move || ::core::result::Result::Ok(Error::StorageOverflow::<T>))();
					},
					_ => {
						#[allow(clippy::redundant_closure_call)]
						return (move || {
							::core::result::Result::Err(<_ as ::core::convert::Into<_>>::into(
								"Could not decode `Error`, variant doesn't exist",
							))
						})();
					},
				}
			}
		}
	};
	#[allow(deprecated)]
	const _: () = {
		fn check_struct<T>() {
			fn check_field<T: ::codec::DecodeWithMemTracking>() {}
		}
		#[automatically_derived]
		impl<T> ::codec::DecodeWithMemTracking for Error<T> {}
	};
	#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
	const _: () = {
		impl<T> ::scale_info::TypeInfo for Error<T>
		where
			core::marker::PhantomData<(T)>: ::scale_info::TypeInfo + 'static,
			T: 'static,
		{
			type Identity = Self;
			fn type_info() -> ::scale_info::Type {
				::scale_info::Type::builder()
                    .path(
                        ::scale_info::Path::new_with_replace(
                            "Error",
                            "pallet_parachain_template::pallet",
                            &[],
                        ),
                    )
                    .type_params(
                        <[_]>::into_vec(
                            ::alloc::boxed::box_new([
                                ::scale_info::TypeParameter::new(
                                    "T",
                                    ::core::option::Option::None,
                                ),
                            ]),
                        ),
                    )
                    .docs_always(
                        &[
                            "Errors inform users that something went wrong.",
                            "<https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html#event-and-error>",
                        ],
                    )
                    .variant(
                        ::scale_info::build::Variants::new()
                            .variant(
                                "NoneValue",
                                |v| {
                                    v
                                        .index(0usize as ::core::primitive::u8)
                                        .docs_always(&["Error names should be descriptive."])
                                },
                            )
                            .variant(
                                "StorageOverflow",
                                |v| {
                                    v
                                        .index(1usize as ::core::primitive::u8)
                                        .docs_always(
                                            &[
                                                "Errors should have helpful documentation associated with them.",
                                            ],
                                        )
                                },
                            ),
                    )
			}
		}
	};
	#[allow(deprecated)]
	const _: () = {
		impl<T> frame::deps::frame_support::traits::PalletError for Error<T> {
			const MAX_ENCODED_SIZE: usize = 1;
		}
	};
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}
	/// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	/// These functions materialize as "extrinsics", which are often compared to transactions.
	/// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	/// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html#dispatchables>
	impl<T: Config> Pallet<T> {
		/// An example dispatchable that takes a singles value as a parameter, writes the value to
		/// storage and emits an event. This function must be dispatched by a signed extrinsic.
		pub fn do_something(origin: OriginFor<T>, bn: u32) -> DispatchResultWithPostInfo {
			frame::deps::frame_support::storage::with_storage_layer::<
				frame::deps::frame_support::dispatch::PostDispatchInfo,
				frame::deps::frame_support::dispatch::DispatchErrorWithPostInfo,
				_,
			>(|| {
				let who = ensure_signed(origin)?;
				let block_number: BlockNumberFor<T> = bn.into();
				<Something<T>>::put(CompositeStruct { block_number });
				Self::deposit_event(Event::SomethingStored { block_number, who });
				Ok(().into())
			})
		}
		/// An example dispatchable that may throw a custom error.
		pub fn cause_error(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			frame::deps::frame_support::storage::with_storage_layer::<
				frame::deps::frame_support::dispatch::PostDispatchInfo,
				frame::deps::frame_support::dispatch::DispatchErrorWithPostInfo,
				_,
			>(|| {
				let _who = ensure_signed(origin)?;
				match <Something<T>>::get() {
					None => Err(Error::<T>::NoneValue)?,
					Some(mut old) => {
						old.block_number = old
							.block_number
							.checked_add(&One::one())
							.ok_or(Error::<T>::StorageOverflow)?;
						<Something<T>>::put(old);
						Ok(().into())
					},
				}
			})
		}
	}
	impl<T: Config> Pallet<T> {
		#[doc(hidden)]
		pub fn pallet_documentation_metadata(
		) -> frame::deps::frame_support::__private::Vec<&'static str> {
			::alloc::vec::Vec::new()
		}
	}
	impl<T: Config> Pallet<T> {
		#[doc(hidden)]
		pub fn pallet_constants_metadata() -> frame::deps::frame_support::__private::Vec<
			frame::deps::frame_support::__private::metadata_ir::PalletConstantMetadataIR,
		> {
			::alloc::vec::Vec::new()
		}
	}
	impl<T: Config> Pallet<T> {
		#[doc(hidden)]
		#[allow(deprecated)]
		pub fn error_metadata(
		) -> Option<frame::deps::frame_support::__private::metadata_ir::PalletErrorMetadataIR> {
			Some(<Error<T>>::error_metadata())
		}
	}
	/// Type alias to `Pallet`, to be used by `construct_runtime`.
	///
	/// Generated by `pallet` attribute macro.
	#[deprecated(note = "use `Pallet` instead")]
	#[allow(dead_code)]
	pub type Module<T> = Pallet<T>;
	impl<T: Config> frame::deps::frame_support::traits::GetStorageVersion for Pallet<T> {
		type InCodeStorageVersion = frame::deps::frame_support::traits::NoStorageVersionSet;
		fn in_code_storage_version() -> Self::InCodeStorageVersion {
			core::default::Default::default()
		}
		fn on_chain_storage_version() -> frame::deps::frame_support::traits::StorageVersion {
			frame::deps::frame_support::traits::StorageVersion::get::<Self>()
		}
	}
	impl<T: Config> frame::deps::frame_support::traits::OnGenesis for Pallet<T> {
		fn on_genesis() {
			let storage_version: frame::deps::frame_support::traits::StorageVersion =
				core::default::Default::default();
			storage_version.put::<Self>();
		}
	}
	impl<T: Config> frame::deps::frame_support::traits::PalletInfoAccess for Pallet<T> {
		fn index() -> usize {
			<<T as frame::deps::frame_system::Config>::PalletInfo as frame::deps::frame_support::traits::PalletInfo>::index::<
                Self,
            >()
                .expect(
                    "Pallet is part of the runtime because pallet `Config` trait is \
						implemented by the runtime",
                )
		}
		fn name() -> &'static str {
			<<T as frame::deps::frame_system::Config>::PalletInfo as frame::deps::frame_support::traits::PalletInfo>::name::<
                Self,
            >()
                .expect(
                    "Pallet is part of the runtime because pallet `Config` trait is \
						implemented by the runtime",
                )
		}
		fn name_hash() -> [u8; 16] {
			<<T as frame::deps::frame_system::Config>::PalletInfo as frame::deps::frame_support::traits::PalletInfo>::name_hash::<
                Self,
            >()
                .expect(
                    "Pallet is part of the runtime because pallet `Config` trait is \
						implemented by the runtime",
                )
		}
		fn module_name() -> &'static str {
			<<T as frame::deps::frame_system::Config>::PalletInfo as frame::deps::frame_support::traits::PalletInfo>::module_name::<
                Self,
            >()
                .expect(
                    "Pallet is part of the runtime because pallet `Config` trait is \
						implemented by the runtime",
                )
		}
		fn crate_version() -> frame::deps::frame_support::traits::CrateVersion {
			frame::deps::frame_support::traits::CrateVersion { major: 0u16, minor: 0u8, patch: 0u8 }
		}
	}
	impl<T: Config> frame::deps::frame_support::traits::PalletsInfoAccess for Pallet<T> {
		fn count() -> usize {
			1
		}
		fn infos() -> frame::deps::frame_support::__private::Vec<
			frame::deps::frame_support::traits::PalletInfoData,
		> {
			use frame::deps::frame_support::traits::PalletInfoAccess;
			let item = frame::deps::frame_support::traits::PalletInfoData {
				index: Self::index(),
				name: Self::name(),
				module_name: Self::module_name(),
				crate_version: Self::crate_version(),
			};
			<[_]>::into_vec(::alloc::boxed::box_new([item]))
		}
	}
	impl<T: Config> frame::deps::frame_support::traits::StorageInfoTrait for Pallet<T> {
		fn storage_info() -> frame::deps::frame_support::__private::Vec<
			frame::deps::frame_support::traits::StorageInfo,
		> {
			#[allow(unused_mut)]
			let mut res = ::alloc::vec::Vec::new();
			{
				let mut storage_info = <Something<
                    T,
                > as frame::deps::frame_support::traits::StorageInfoTrait>::storage_info();
				res.append(&mut storage_info);
			}
			res
		}
	}
	use frame::deps::frame_support::traits::{
		StorageInfoTrait, TrackedStorageKey, WhitelistedStorageKeys,
	};
	impl<T: Config> WhitelistedStorageKeys for Pallet<T> {
		fn whitelisted_storage_keys(
		) -> frame::deps::frame_support::__private::Vec<TrackedStorageKey> {
			use frame::deps::frame_support::__private::vec;
			::alloc::vec::Vec::new()
		}
	}
	impl<T> Pallet<T> {
		#[allow(dead_code)]
		#[doc(hidden)]
		pub fn deprecation_info(
		) -> frame::deps::frame_support::__private::metadata_ir::ItemDeprecationInfoIR {
			frame::deps::frame_support::__private::metadata_ir::ItemDeprecationInfoIR::NotDeprecated
		}
	}
	impl<T: Config> Pallet<T> {
		#[doc(hidden)]
		pub fn pallet_associated_types_metadata() -> frame::deps::frame_support::__private::vec::Vec<
			frame::deps::frame_support::__private::metadata_ir::PalletAssociatedTypeMetadataIR,
		> {
			::alloc::vec::Vec::new()
		}
	}
	#[doc(hidden)]
	mod warnings {}
	#[allow(unused_imports)]
	#[doc(hidden)]
	pub mod __substrate_call_check {
		#[doc(hidden)]
		pub use __is_call_part_defined_0 as is_call_part_defined;
	}
	/// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	/// These functions materialize as "extrinsics", which are often compared to transactions.
	/// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	/// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html#dispatchables>
	#[codec(encode_bound())]
	#[codec(decode_bound())]
	#[scale_info(skip_type_params(T), capture_docs = "always")]
	#[allow(non_camel_case_types)]
	pub enum Call<T: Config> {
		#[doc(hidden)]
		#[codec(skip)]
		__Ignore(::core::marker::PhantomData<(T,)>, frame::deps::frame_support::Never),
		/// An example dispatchable that takes a singles value as a parameter, writes the value to
		/// storage and emits an event. This function must be dispatched by a signed extrinsic.
		#[codec(index = 0u8)]
		do_something {
			#[allow(missing_docs)]
			bn: u32,
		},
		/// An example dispatchable that may throw a custom error.
		#[codec(index = 1u8)]
		cause_error {},
	}
	const _: () = {
		#[automatically_derived]
		#[allow(deprecated)]
		impl<T: Config> ::core::fmt::Debug for Call<T> {
			fn fmt(&self, fmt: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				match *self {
					Self::__Ignore(ref _0, ref _1) => {
						fmt.debug_tuple("Call::__Ignore").field(&_0).field(&_1).finish()
					},
					Self::do_something { ref bn } => {
						fmt.debug_struct("Call::do_something").field("bn", &bn).finish()
					},
					Self::cause_error {} => fmt.debug_struct("Call::cause_error").finish(),
				}
			}
		}
	};
	const _: () = {
		#[automatically_derived]
		#[allow(deprecated)]
		impl<T: Config> ::core::clone::Clone for Call<T> {
			fn clone(&self) -> Self {
				match self {
					Self::__Ignore(ref _0, ref _1) => Self::__Ignore(
						::core::clone::Clone::clone(_0),
						::core::clone::Clone::clone(_1),
					),
					Self::do_something { ref bn } => {
						Self::do_something { bn: ::core::clone::Clone::clone(bn) }
					},
					Self::cause_error {} => Self::cause_error {},
				}
			}
		}
	};
	#[allow(deprecated)]
	const _: () = {
		impl<T: Config> ::core::cmp::Eq for Call<T> {}
	};
	const _: () = {
		#[automatically_derived]
		#[allow(deprecated)]
		impl<T: Config> ::core::cmp::PartialEq for Call<T> {
			fn eq(&self, other: &Self) -> bool {
				match (self, other) {
					(Self::__Ignore(_0, _1), Self::__Ignore(_0_other, _1_other)) => {
						true && _0 == _0_other && _1 == _1_other
					},
					(Self::do_something { bn }, Self::do_something { bn: _0 }) => true && bn == _0,
					(Self::cause_error {}, Self::cause_error {}) => true,
					(Self::__Ignore { .. }, Self::do_something { .. }) => false,
					(Self::__Ignore { .. }, Self::cause_error { .. }) => false,
					(Self::do_something { .. }, Self::__Ignore { .. }) => false,
					(Self::do_something { .. }, Self::cause_error { .. }) => false,
					(Self::cause_error { .. }, Self::__Ignore { .. }) => false,
					(Self::cause_error { .. }, Self::do_something { .. }) => false,
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
						Call::do_something { ref bn } => {
							0_usize.saturating_add(::codec::Encode::size_hint(bn))
						},
						Call::cause_error {} => 0_usize,
						_ => 0_usize,
					}
			}
			fn encode_to<__CodecOutputEdqy: ::codec::Output + ?::core::marker::Sized>(
				&self,
				__codec_dest_edqy: &mut __CodecOutputEdqy,
			) {
				#[automatically_derived]
				const _: () = {
					#[allow(clippy::unnecessary_cast)]
					#[allow(clippy::cast_possible_truncation)]
					const indices: [(usize, &'static str); 2usize] = [
						((0usize) as ::core::primitive::usize, "do_something"),
						((1usize) as ::core::primitive::usize, "cause_error"),
					];
					const fn search_for_invalid_index(
						array: &[(usize, &'static str); 2usize],
					) -> (bool, usize) {
						let mut i = 0;
						while i < 2usize {
							if array[i].0 > 255 {
								return (true, i);
							}
							i += 1;
						}
						(false, 0)
					}
					const INVALID_INDEX: (bool, usize) = search_for_invalid_index(&indices);
					if INVALID_INDEX.0 {
						let msg = ::const_format::pmr::__AssertStr {
							x: {
								use ::const_format::__cf_osRcTFl4A;
								({
									#[doc(hidden)]
									#[allow(unused_mut, non_snake_case)]
									const CONCATP_NHPMWYD3NJA: &[__cf_osRcTFl4A::pmr::PArgument] = {
										let fmt = __cf_osRcTFl4A::pmr::FormattingFlags::NEW;
										&[
											__cf_osRcTFl4A::pmr::PConvWrapper("Found variant `")
												.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												indices[INVALID_INDEX.1].1,
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												"` with invalid index: `",
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												indices[INVALID_INDEX.1].0,
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												"`. Max supported index is 255.",
											)
											.to_pargument_display(fmt),
										]
									};
									{
										#[doc(hidden)]
										const ARR_LEN: usize =
											::const_format::pmr::PArgument::calc_len(
												CONCATP_NHPMWYD3NJA,
											);
										#[doc(hidden)]
										const CONCAT_ARR: &::const_format::pmr::LenAndArray<
											[u8; ARR_LEN],
										> = &::const_format::pmr::__priv_concatenate(
											CONCATP_NHPMWYD3NJA,
										);
										#[doc(hidden)]
										#[allow(clippy::transmute_ptr_to_ptr)]
										const CONCAT_STR: &str = unsafe {
											let slice = ::const_format::pmr::transmute::<
												&[u8; ARR_LEN],
												&[u8; CONCAT_ARR.len],
											>(&CONCAT_ARR.array);
											{
												let bytes: &'static [::const_format::pmr::u8] =
													slice;
												let string: &'static ::const_format::pmr::str = {
													::const_format::__hidden_utils::PtrToRef {
														ptr: bytes
															as *const [::const_format::pmr::u8]
															as *const str,
													}
													.reff
												};
												string
											}
										};
										CONCAT_STR
									}
								})
							},
						}
						.x;
						{
							#[cold]
							#[track_caller]
							#[inline(never)]
							#[rustc_const_panic_str]
							#[rustc_do_not_const_check]
							const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
								::core::panicking::panic_display(arg)
							}
							panic_cold_display(&msg);
						};
					}
					const fn duplicate_info(
						array: &[(usize, &'static str); 2usize],
					) -> (bool, usize, usize) {
						let len = 2usize;
						let mut i = 0usize;
						while i < len {
							let mut j = i + 1;
							while j < len {
								if array[i].0 == array[j].0 {
									return (true, i, j);
								}
								j += 1;
							}
							i += 1;
						}
						(false, 0, 0)
					}
					const DUP_INFO: (bool, usize, usize) = duplicate_info(&indices);
					if DUP_INFO.0 {
						let msg = ::const_format::pmr::__AssertStr {
                            x: {
                                use ::const_format::__cf_osRcTFl4A;
                                ({
                                    #[doc(hidden)]
                                    #[allow(unused_mut, non_snake_case)]
                                    const CONCATP_NHPMWYD3NJA: &[__cf_osRcTFl4A::pmr::PArgument] = {
                                        let fmt = __cf_osRcTFl4A::pmr::FormattingFlags::NEW;
                                        &[
                                            __cf_osRcTFl4A::pmr::PConvWrapper(
                                                    "Found variants that have duplicate indexes. Both `",
                                                )
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.1].1)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper("` and `")
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.2].1)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper("` have the index `")
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.1].0)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(
                                                    "`. Use different indexes for each variant.",
                                                )
                                                .to_pargument_display(fmt),
                                        ]
                                    };
                                    {
                                        #[doc(hidden)]
                                        const ARR_LEN: usize = ::const_format::pmr::PArgument::calc_len(
                                            CONCATP_NHPMWYD3NJA,
                                        );
                                        #[doc(hidden)]
                                        const CONCAT_ARR: &::const_format::pmr::LenAndArray<
                                            [u8; ARR_LEN],
                                        > = &::const_format::pmr::__priv_concatenate(
                                            CONCATP_NHPMWYD3NJA,
                                        );
                                        #[doc(hidden)]
                                        #[allow(clippy::transmute_ptr_to_ptr)]
                                        const CONCAT_STR: &str = unsafe {
                                            let slice = ::const_format::pmr::transmute::<
                                                &[u8; ARR_LEN],
                                                &[u8; CONCAT_ARR.len],
                                            >(&CONCAT_ARR.array);
                                            {
                                                let bytes: &'static [::const_format::pmr::u8] = slice;
                                                let string: &'static ::const_format::pmr::str = {
                                                    ::const_format::__hidden_utils::PtrToRef {
                                                        ptr: bytes as *const [::const_format::pmr::u8] as *const str,
                                                    }
                                                        .reff
                                                };
                                                string
                                            }
                                        };
                                        CONCAT_STR
                                    }
                                })
                            },
                        }
                            .x;
						{
							#[cold]
							#[track_caller]
							#[inline(never)]
							#[rustc_const_panic_str]
							#[rustc_do_not_const_check]
							const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
								::core::panicking::panic_display(arg)
							}
							panic_cold_display(&msg);
						};
					}
				};
				match *self {
					Call::do_something { ref bn } => {
						#[allow(clippy::unnecessary_cast)]
						__codec_dest_edqy.push_byte((0usize) as ::core::primitive::u8);
						::codec::Encode::encode_to(bn, __codec_dest_edqy);
					},
					Call::cause_error {} => {
						#[allow(clippy::unnecessary_cast)]
						__codec_dest_edqy.push_byte((1usize) as ::core::primitive::u8);
					},
					_ => {},
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
				#[automatically_derived]
				const _: () = {
					#[allow(clippy::unnecessary_cast)]
					#[allow(clippy::cast_possible_truncation)]
					const indices: [(usize, &'static str); 2usize] = [
						((0usize) as ::core::primitive::usize, "do_something"),
						((1usize) as ::core::primitive::usize, "cause_error"),
					];
					const fn search_for_invalid_index(
						array: &[(usize, &'static str); 2usize],
					) -> (bool, usize) {
						let mut i = 0;
						while i < 2usize {
							if array[i].0 > 255 {
								return (true, i);
							}
							i += 1;
						}
						(false, 0)
					}
					const INVALID_INDEX: (bool, usize) = search_for_invalid_index(&indices);
					if INVALID_INDEX.0 {
						let msg = ::const_format::pmr::__AssertStr {
							x: {
								use ::const_format::__cf_osRcTFl4A;
								({
									#[doc(hidden)]
									#[allow(unused_mut, non_snake_case)]
									const CONCATP_NHPMWYD3NJA: &[__cf_osRcTFl4A::pmr::PArgument] = {
										let fmt = __cf_osRcTFl4A::pmr::FormattingFlags::NEW;
										&[
											__cf_osRcTFl4A::pmr::PConvWrapper("Found variant `")
												.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												indices[INVALID_INDEX.1].1,
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												"` with invalid index: `",
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												indices[INVALID_INDEX.1].0,
											)
											.to_pargument_display(fmt),
											__cf_osRcTFl4A::pmr::PConvWrapper(
												"`. Max supported index is 255.",
											)
											.to_pargument_display(fmt),
										]
									};
									{
										#[doc(hidden)]
										const ARR_LEN: usize =
											::const_format::pmr::PArgument::calc_len(
												CONCATP_NHPMWYD3NJA,
											);
										#[doc(hidden)]
										const CONCAT_ARR: &::const_format::pmr::LenAndArray<
											[u8; ARR_LEN],
										> = &::const_format::pmr::__priv_concatenate(
											CONCATP_NHPMWYD3NJA,
										);
										#[doc(hidden)]
										#[allow(clippy::transmute_ptr_to_ptr)]
										const CONCAT_STR: &str = unsafe {
											let slice = ::const_format::pmr::transmute::<
												&[u8; ARR_LEN],
												&[u8; CONCAT_ARR.len],
											>(&CONCAT_ARR.array);
											{
												let bytes: &'static [::const_format::pmr::u8] =
													slice;
												let string: &'static ::const_format::pmr::str = {
													::const_format::__hidden_utils::PtrToRef {
														ptr: bytes
															as *const [::const_format::pmr::u8]
															as *const str,
													}
													.reff
												};
												string
											}
										};
										CONCAT_STR
									}
								})
							},
						}
						.x;
						{
							#[cold]
							#[track_caller]
							#[inline(never)]
							#[rustc_const_panic_str]
							#[rustc_do_not_const_check]
							const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
								::core::panicking::panic_display(arg)
							}
							panic_cold_display(&msg);
						};
					}
					const fn duplicate_info(
						array: &[(usize, &'static str); 2usize],
					) -> (bool, usize, usize) {
						let len = 2usize;
						let mut i = 0usize;
						while i < len {
							let mut j = i + 1;
							while j < len {
								if array[i].0 == array[j].0 {
									return (true, i, j);
								}
								j += 1;
							}
							i += 1;
						}
						(false, 0, 0)
					}
					const DUP_INFO: (bool, usize, usize) = duplicate_info(&indices);
					if DUP_INFO.0 {
						let msg = ::const_format::pmr::__AssertStr {
                            x: {
                                use ::const_format::__cf_osRcTFl4A;
                                ({
                                    #[doc(hidden)]
                                    #[allow(unused_mut, non_snake_case)]
                                    const CONCATP_NHPMWYD3NJA: &[__cf_osRcTFl4A::pmr::PArgument] = {
                                        let fmt = __cf_osRcTFl4A::pmr::FormattingFlags::NEW;
                                        &[
                                            __cf_osRcTFl4A::pmr::PConvWrapper(
                                                    "Found variants that have duplicate indexes. Both `",
                                                )
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.1].1)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper("` and `")
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.2].1)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper("` have the index `")
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(indices[DUP_INFO.1].0)
                                                .to_pargument_display(fmt),
                                            __cf_osRcTFl4A::pmr::PConvWrapper(
                                                    "`. Use different indexes for each variant.",
                                                )
                                                .to_pargument_display(fmt),
                                        ]
                                    };
                                    {
                                        #[doc(hidden)]
                                        const ARR_LEN: usize = ::const_format::pmr::PArgument::calc_len(
                                            CONCATP_NHPMWYD3NJA,
                                        );
                                        #[doc(hidden)]
                                        const CONCAT_ARR: &::const_format::pmr::LenAndArray<
                                            [u8; ARR_LEN],
                                        > = &::const_format::pmr::__priv_concatenate(
                                            CONCATP_NHPMWYD3NJA,
                                        );
                                        #[doc(hidden)]
                                        #[allow(clippy::transmute_ptr_to_ptr)]
                                        const CONCAT_STR: &str = unsafe {
                                            let slice = ::const_format::pmr::transmute::<
                                                &[u8; ARR_LEN],
                                                &[u8; CONCAT_ARR.len],
                                            >(&CONCAT_ARR.array);
                                            {
                                                let bytes: &'static [::const_format::pmr::u8] = slice;
                                                let string: &'static ::const_format::pmr::str = {
                                                    ::const_format::__hidden_utils::PtrToRef {
                                                        ptr: bytes as *const [::const_format::pmr::u8] as *const str,
                                                    }
                                                        .reff
                                                };
                                                string
                                            }
                                        };
                                        CONCAT_STR
                                    }
                                })
                            },
                        }
                            .x;
						{
							#[cold]
							#[track_caller]
							#[inline(never)]
							#[rustc_const_panic_str]
							#[rustc_do_not_const_check]
							const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
								::core::panicking::panic_display(arg)
							}
							panic_cold_display(&msg);
						};
					}
				};
				match __codec_input_edqy
					.read_byte()
					.map_err(|e| e.chain("Could not decode `Call`, failed to read variant byte"))?
				{
					#[allow(clippy::unnecessary_cast)]
					#[allow(clippy::cast_possible_truncation)]
					__codec_x_edqy if __codec_x_edqy == (0usize) as ::core::primitive::u8 => {
						#[allow(clippy::redundant_closure_call)]
						return (move || {
							::core::result::Result::Ok(Call::do_something::<T> {
								bn: {
									let __codec_res_edqy =
										<u32 as ::codec::Decode>::decode(__codec_input_edqy);
									match __codec_res_edqy {
										::core::result::Result::Err(e) => {
											return ::core::result::Result::Err(e.chain(
												"Could not decode `Call::do_something::bn`",
											));
										},
										::core::result::Result::Ok(__codec_res_edqy) => {
											__codec_res_edqy
										},
									}
								},
							})
						})();
					},
					#[allow(clippy::unnecessary_cast)]
					#[allow(clippy::cast_possible_truncation)]
					__codec_x_edqy if __codec_x_edqy == (1usize) as ::core::primitive::u8 => {
						#[allow(clippy::redundant_closure_call)]
						return (move || ::core::result::Result::Ok(Call::cause_error::<T> {}))();
					},
					_ => {
						#[allow(clippy::redundant_closure_call)]
						return (move || {
							::core::result::Result::Err(<_ as ::core::convert::Into<_>>::into(
								"Could not decode `Call`, variant doesn't exist",
							))
						})();
					},
				}
			}
		}
	};
	#[allow(deprecated)]
	const _: () = {
		#[allow(non_camel_case_types)]
		fn check_struct<T: Config>() {
			fn check_field<T: ::codec::DecodeWithMemTracking>() {}
			check_field::<u32>();
		}
		#[automatically_derived]
		impl<T: Config> ::codec::DecodeWithMemTracking for Call<T> {}
	};
	#[allow(non_upper_case_globals, deprecated, unused_attributes, unused_qualifications)]
	const _: () = {
		impl<T: Config> ::scale_info::TypeInfo for Call<T>
		where
			::core::marker::PhantomData<(T,)>: ::scale_info::TypeInfo + 'static,
			T: Config + 'static,
		{
			type Identity = Self;
			fn type_info() -> ::scale_info::Type {
				::scale_info::Type::builder()
                    .path(
                        ::scale_info::Path::new_with_replace(
                            "Call",
                            "pallet_parachain_template::pallet",
                            &[],
                        ),
                    )
                    .type_params(
                        <[_]>::into_vec(
                            ::alloc::boxed::box_new([
                                ::scale_info::TypeParameter::new(
                                    "T",
                                    ::core::option::Option::None,
                                ),
                            ]),
                        ),
                    )
                    .docs_always(
                        &[
                            "Dispatchable functions allows users to interact with the pallet and invoke state changes.",
                            "These functions materialize as \"extrinsics\", which are often compared to transactions.",
                            "Dispatchable functions must be annotated with a weight and must return a DispatchResult.",
                            "<https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html#dispatchables>",
                        ],
                    )
                    .variant(
                        ::scale_info::build::Variants::new()
                            .variant(
                                "do_something",
                                |v| {
                                    v
                                        .index(0u8 as ::core::primitive::u8)
                                        .fields(
                                            ::scale_info::build::Fields::named()
                                                .field(|f| f.ty::<u32>().name("bn").type_name("u32")),
                                        )
                                        .docs_always(
                                            &[
                                                "An example dispatchable that takes a singles value as a parameter, writes the value to",
                                                "storage and emits an event. This function must be dispatched by a signed extrinsic.",
                                            ],
                                        )
                                },
                            )
                            .variant(
                                "cause_error",
                                |v| {
                                    v
                                        .index(1u8 as ::core::primitive::u8)
                                        .fields(::scale_info::build::Fields::named())
                                        .docs_always(
                                            &["An example dispatchable that may throw a custom error."],
                                        )
                                },
                            ),
                    )
			}
		}
	};
	impl<T: Config> Call<T> {
		///Create a call with the variant `do_something`.
		pub fn new_call_variant_do_something(bn: u32) -> Self {
			Self::do_something { bn }
		}
		///Create a call with the variant `cause_error`.
		pub fn new_call_variant_cause_error() -> Self {
			Self::cause_error {}
		}
	}
	impl<T: Config> frame::deps::frame_support::dispatch::GetDispatchInfo for Call<T> {
		fn get_dispatch_info(&self) -> frame::deps::frame_support::dispatch::DispatchInfo {
			match *self {
				Self::do_something { ref bn } => {
					let __pallet_base_weight =
						Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1);
					let __pallet_weight = <dyn frame::deps::frame_support::dispatch::WeighData<(
						&u32,
					)>>::weigh_data(&__pallet_base_weight, (bn,));
					let __pallet_class = <dyn frame::deps::frame_support::dispatch::ClassifyDispatch<
						(&u32,),
					>>::classify_dispatch(&__pallet_base_weight, (bn,));
					let __pallet_pays_fee = <dyn frame::deps::frame_support::dispatch::PaysFee<(
						&u32,
					)>>::pays_fee(&__pallet_base_weight, (bn,));
					frame::deps::frame_support::dispatch::DispatchInfo {
						call_weight: __pallet_weight,
						extension_weight: Default::default(),
						class: __pallet_class,
						pays_fee: __pallet_pays_fee,
					}
				},
				Self::cause_error {} => {
					let __pallet_base_weight =
						Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1, 1);
					let __pallet_weight =
						<dyn frame::deps::frame_support::dispatch::WeighData<()>>::weigh_data(
							&__pallet_base_weight,
							(),
						);
					let __pallet_class = <dyn frame::deps::frame_support::dispatch::ClassifyDispatch<
						(),
					>>::classify_dispatch(&__pallet_base_weight, ());
					let __pallet_pays_fee =
						<dyn frame::deps::frame_support::dispatch::PaysFee<()>>::pays_fee(
							&__pallet_base_weight,
							(),
						);
					frame::deps::frame_support::dispatch::DispatchInfo {
						call_weight: __pallet_weight,
						extension_weight: Default::default(),
						class: __pallet_class,
						pays_fee: __pallet_pays_fee,
					}
				},
				Self::__Ignore(_, _) => {
					::core::panicking::panic_fmt(format_args!(
						"internal error: entered unreachable code: {0}",
						format_args!("__Ignore cannot be used"),
					));
				},
			}
		}
	}
	impl<T: Config> frame::deps::frame_support::dispatch::CheckIfFeeless for Call<T> {
		type Origin = frame::deps::frame_system::pallet_prelude::OriginFor<T>;
		#[allow(unused_variables)]
		fn is_feeless(&self, origin: &Self::Origin) -> bool {
			match *self {
				Self::do_something { ref bn } => {
					let feeless_check = |_origin, bn| false;
					feeless_check(origin, bn)
				},
				Self::cause_error {} => {
					let feeless_check = |_origin| false;
					feeless_check(origin)
				},
				Self::__Ignore(_, _) => {
					::core::panicking::panic_fmt(format_args!(
						"internal error: entered unreachable code: {0}",
						format_args!("__Ignore cannot be used"),
					));
				},
			}
		}
	}
	impl<T: Config> frame::deps::frame_support::traits::GetCallName for Call<T> {
		fn get_call_name(&self) -> &'static str {
			match *self {
				Self::do_something { .. } => "do_something",
				Self::cause_error { .. } => "cause_error",
				Self::__Ignore(_, _) => {
					::core::panicking::panic_fmt(format_args!(
						"internal error: entered unreachable code: {0}",
						format_args!("__PhantomItem cannot be used."),
					));
				},
			}
		}
		fn get_call_names() -> &'static [&'static str] {
			&["do_something", "cause_error"]
		}
	}
	impl<T: Config> frame::deps::frame_support::traits::GetCallIndex for Call<T> {
		fn get_call_index(&self) -> u8 {
			match *self {
				Self::do_something { .. } => 0u8,
				Self::cause_error { .. } => 1u8,
				Self::__Ignore(_, _) => {
					::core::panicking::panic_fmt(format_args!(
						"internal error: entered unreachable code: {0}",
						format_args!("__PhantomItem cannot be used."),
					));
				},
			}
		}
		fn get_call_indices() -> &'static [u8] {
			&[0u8, 1u8]
		}
	}
	impl<T: Config> frame::deps::frame_support::traits::UnfilteredDispatchable for Call<T> {
		type RuntimeOrigin = frame::deps::frame_system::pallet_prelude::OriginFor<T>;
		fn dispatch_bypass_filter(
			self,
			origin: Self::RuntimeOrigin,
		) -> frame::deps::frame_support::dispatch::DispatchResultWithPostInfo {
			frame::deps::frame_support::dispatch_context::run_in_context(|| match self {
				Self::do_something { bn } => {
					let __within_span__ = {
						use ::tracing::__macro_support::Callsite as _;
						static __CALLSITE: ::tracing::callsite::DefaultCallsite = {
							static META: ::tracing::Metadata<'static> = {
								::tracing_core::metadata::Metadata::new(
									"do_something",
									"pallet_parachain_template::pallet",
									::tracing::Level::TRACE,
									::core::option::Option::Some(
										"templates/parachain/pallets/template/src/lib.rs",
									),
									::core::option::Option::Some(58u32),
									::core::option::Option::Some(
										"pallet_parachain_template::pallet",
									),
									::tracing_core::field::FieldSet::new(
										&[],
										::tracing_core::callsite::Identifier(&__CALLSITE),
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
								interest = __CALLSITE.interest();
								!interest.is_never()
							} && ::tracing::__macro_support::__is_enabled(
							__CALLSITE.metadata(),
							interest,
						) {
							let meta = __CALLSITE.metadata();
							::tracing::Span::new(meta, &{ meta.fields().value_set(&[]) })
						} else {
							let span =
								::tracing::__macro_support::__disabled_span(__CALLSITE.metadata());
							if match ::tracing::Level::TRACE {
								::tracing::Level::ERROR => ::tracing::log::Level::Error,
								::tracing::Level::WARN => ::tracing::log::Level::Warn,
								::tracing::Level::INFO => ::tracing::log::Level::Info,
								::tracing::Level::DEBUG => ::tracing::log::Level::Debug,
								_ => ::tracing::log::Level::Trace,
							} <= ::tracing::log::STATIC_MAX_LEVEL
							{
								if !::tracing::dispatcher::has_been_set() {
									{
										span.record_all(&{
											__CALLSITE.metadata().fields().value_set(&[])
										});
									}
								} else {
									{}
								}
							} else {
								{}
							};
							span
						}
					};
					let __tracing_guard__ = __within_span__.enter();
					<Pallet<T>>::do_something(origin, bn).map(Into::into).map_err(Into::into)
				},
				Self::cause_error {} => {
					let __within_span__ = {
						use ::tracing::__macro_support::Callsite as _;
						static __CALLSITE: ::tracing::callsite::DefaultCallsite = {
							static META: ::tracing::Metadata<'static> = {
								::tracing_core::metadata::Metadata::new(
									"cause_error",
									"pallet_parachain_template::pallet",
									::tracing::Level::TRACE,
									::core::option::Option::Some(
										"templates/parachain/pallets/template/src/lib.rs",
									),
									::core::option::Option::Some(58u32),
									::core::option::Option::Some(
										"pallet_parachain_template::pallet",
									),
									::tracing_core::field::FieldSet::new(
										&[],
										::tracing_core::callsite::Identifier(&__CALLSITE),
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
								interest = __CALLSITE.interest();
								!interest.is_never()
							} && ::tracing::__macro_support::__is_enabled(
							__CALLSITE.metadata(),
							interest,
						) {
							let meta = __CALLSITE.metadata();
							::tracing::Span::new(meta, &{ meta.fields().value_set(&[]) })
						} else {
							let span =
								::tracing::__macro_support::__disabled_span(__CALLSITE.metadata());
							if match ::tracing::Level::TRACE {
								::tracing::Level::ERROR => ::tracing::log::Level::Error,
								::tracing::Level::WARN => ::tracing::log::Level::Warn,
								::tracing::Level::INFO => ::tracing::log::Level::Info,
								::tracing::Level::DEBUG => ::tracing::log::Level::Debug,
								_ => ::tracing::log::Level::Trace,
							} <= ::tracing::log::STATIC_MAX_LEVEL
							{
								if !::tracing::dispatcher::has_been_set() {
									{
										span.record_all(&{
											__CALLSITE.metadata().fields().value_set(&[])
										});
									}
								} else {
									{}
								}
							} else {
								{}
							};
							span
						}
					};
					let __tracing_guard__ = __within_span__.enter();
					<Pallet<T>>::cause_error(origin).map(Into::into).map_err(Into::into)
				},
				Self::__Ignore(_, _) => {
					let _ = origin;
					{
						::core::panicking::panic_fmt(format_args!(
							"internal error: entered unreachable code: {0}",
							format_args!("__PhantomItem cannot be used."),
						));
					};
				},
			})
		}
	}
	impl<T: Config> frame::deps::frame_support::dispatch::Callable<T> for Pallet<T> {
		type RuntimeCall = Call<T>;
	}
	impl<T: Config> Pallet<T> {
		#[allow(dead_code)]
		#[doc(hidden)]
		pub fn call_functions(
		) -> frame::deps::frame_support::__private::metadata_ir::PalletCallMetadataIR {
			frame::deps::frame_support::__private::metadata_ir::PalletCallMetadataIR {
                ty: frame::deps::frame_support::__private::scale_info::meta_type::<
                    Call<T>,
                >(),
                deprecation_info: frame::deps::frame_support::__private::metadata_ir::EnumDeprecationInfoIR::nothing_deprecated(),
            }
		}
	}
	impl<T: Config> frame::deps::frame_support::traits::Authorize for Call<T> {
		fn authorize(
			&self,
			source: frame::deps::frame_support::pallet_prelude::TransactionSource,
		) -> ::core::option::Option<
			::core::result::Result<
				(
					frame::deps::frame_support::pallet_prelude::ValidTransaction,
					frame::deps::frame_support::pallet_prelude::Weight,
				),
				frame::deps::frame_support::pallet_prelude::TransactionValidityError,
			>,
		> {
			match *self {
				Self::do_something { ref bn } => None,
				Self::cause_error {} => None,
				Self::__Ignore(_, _) => {
					let _ = source;
					{
						::core::panicking::panic_fmt(format_args!(
							"internal error: entered unreachable code: {0}",
							format_args!("__Ignore cannot be used"),
						));
					}
				},
			}
		}
		fn weight_of_authorize(&self) -> frame::deps::frame_support::pallet_prelude::Weight {
			match *self {
				Self::do_something { ref bn } => {
					frame::deps::frame_support::pallet_prelude::Weight::zero()
				},
				Self::cause_error {} => frame::deps::frame_support::pallet_prelude::Weight::zero(),
				Self::__Ignore(_, _) => {
					::core::panicking::panic_fmt(format_args!(
						"internal error: entered unreachable code: {0}",
						format_args!("__Ignore cannot be used"),
					));
				},
			}
		}
	}
	impl<T: Config> core::fmt::Debug for Error<T> {
		fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
			f.write_str(self.as_str())
		}
	}
	impl<T: Config> Error<T> {
		#[doc(hidden)]
		pub fn as_str(&self) -> &'static str {
			match &self {
				Self::__Ignore(_, _) => {
					::core::panicking::panic_fmt(format_args!(
						"internal error: entered unreachable code: {0}",
						format_args!("`__Ignore` can never be constructed"),
					));
				},
				Self::NoneValue => "NoneValue",
				Self::StorageOverflow => "StorageOverflow",
			}
		}
	}
	impl<T: Config> From<Error<T>> for &'static str {
		fn from(err: Error<T>) -> &'static str {
			err.as_str()
		}
	}
	impl<T: Config> From<Error<T>> for frame::deps::frame_support::sp_runtime::DispatchError {
		fn from(err: Error<T>) -> Self {
			use frame::deps::frame_support::__private::codec::Encode;
			let index = <<T as frame::deps::frame_system::Config>::PalletInfo as frame::deps::frame_support::traits::PalletInfo>::index::<
                Pallet<T>,
            >()
                .expect("Every active module has an index in the runtime; qed") as u8;
			let mut encoded = err.encode();
			encoded.resize(frame::deps::frame_support::MAX_MODULE_ERROR_ENCODED_SIZE, 0);
			frame::deps::frame_support::sp_runtime::DispatchError::Module(frame::deps::frame_support::sp_runtime::ModuleError {
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
	impl<T: Config> Error<T> {
		#[allow(dead_code)]
		#[doc(hidden)]
		pub fn error_metadata(
		) -> frame::deps::frame_support::__private::metadata_ir::PalletErrorMetadataIR {
			frame::deps::frame_support::__private::metadata_ir::PalletErrorMetadataIR {
                ty: frame::deps::frame_support::__private::scale_info::meta_type::<
                    Error<T>,
                >(),
                deprecation_info: frame::deps::frame_support::__private::metadata_ir::EnumDeprecationInfoIR::nothing_deprecated(),
            }
		}
	}
	#[doc(hidden)]
	pub mod __substrate_event_check {
		#[doc(hidden)]
		pub use __is_event_part_defined_2 as is_event_part_defined;
	}
	impl<T: Config> Pallet<T> {
		pub(super) fn deposit_event(event: Event<T>) {
			let event = <<T as frame::deps::frame_system::Config>::RuntimeEvent as From<
				Event<T>,
			>>::from(event);
			let event = <<T as frame::deps::frame_system::Config>::RuntimeEvent as Into<
				<T as frame::deps::frame_system::Config>::RuntimeEvent,
			>>::into(event);
			<frame::deps::frame_system::Pallet<T>>::deposit_event(event)
		}
	}
	impl<T: Config> From<Event<T>> for () {
		fn from(_: Event<T>) {}
	}
	impl<T: Config> Event<T> {
		#[allow(dead_code)]
		#[doc(hidden)]
		pub fn event_metadata<
			W: frame::deps::frame_support::__private::scale_info::TypeInfo + 'static,
		>() -> frame::deps::frame_support::__private::metadata_ir::PalletEventMetadataIR {
			frame::deps::frame_support::__private::metadata_ir::PalletEventMetadataIR {
                ty: frame::deps::frame_support::__private::scale_info::meta_type::<W>(),
                deprecation_info: frame::deps::frame_support::__private::metadata_ir::EnumDeprecationInfoIR::nothing_deprecated(),
            }
		}
	}
	impl<T: Config> Pallet<T> {
		#[doc(hidden)]
		pub fn storage_metadata(
		) -> frame::deps::frame_support::__private::metadata_ir::PalletStorageMetadataIR {
			frame::deps::frame_support::__private::metadata_ir::PalletStorageMetadataIR {
                prefix: <<T as frame::deps::frame_system::Config>::PalletInfo as frame::deps::frame_support::traits::PalletInfo>::name::<
                    Pallet<T>,
                >()
                    .expect(
                        "No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                    ),
                entries: {
                    #[allow(unused_mut)]
                    let mut entries = ::alloc::vec::Vec::new();
                    (|entries: &mut frame::deps::frame_support::__private::Vec<_>| {
                        {
                            <Something<
                                T,
                            > as frame::deps::frame_support::storage::StorageEntryMetadataBuilder>::build_metadata(
                                frame::deps::frame_support::__private::metadata_ir::ItemDeprecationInfoIR::NotDeprecated,
                                <[_]>::into_vec(
                                    ::alloc::boxed::box_new([
                                        " The pallet\'s storage items.",
                                        " <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html#storage>",
                                        " <https://paritytech.github.io/polkadot-sdk/master/frame_support/pallet_macros/attr.storage.html>",
                                    ]),
                                ),
                                entries,
                            );
                        }
                    })(&mut entries);
                    entries
                },
            }
		}
	}
	#[doc(hidden)]
	pub struct _GeneratedPrefixForStorageSomething<T>(core::marker::PhantomData<(T,)>);
	impl<T: Config> frame::deps::frame_support::traits::StorageInstance
		for _GeneratedPrefixForStorageSomething<T>
	{
		fn pallet_prefix() -> &'static str {
			<<T as frame::deps::frame_system::Config>::PalletInfo as frame::deps::frame_support::traits::PalletInfo>::name::<
                Pallet<T>,
            >()
                .expect(
                    "No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                )
		}
		fn pallet_prefix_hash() -> [u8; 16] {
			<<T as frame::deps::frame_system::Config>::PalletInfo as frame::deps::frame_support::traits::PalletInfo>::name_hash::<
                Pallet<T>,
            >()
                .expect(
                    "No name_hash found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                )
		}
		const STORAGE_PREFIX: &'static str = "Something";
		fn storage_prefix_hash() -> [u8; 16] {
			[
				231u8, 243u8, 48u8, 187u8, 44u8, 72u8, 103u8, 176u8, 105u8, 82u8, 160u8, 51u8,
				20u8, 7u8, 81u8, 142u8,
			]
		}
	}
	impl<T: Config> frame::deps::frame_support::view_functions::ViewFunctionIdPrefix for Pallet<T> {
		fn prefix() -> [::core::primitive::u8; 16usize] {
			<<T as frame::deps::frame_system::Config>::PalletInfo as frame::deps::frame_support::traits::PalletInfo>::name_hash::<
                Pallet<T>,
            >()
                .expect(
                    "No name_hash found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.",
                )
		}
	}
	impl<T: Config> frame::deps::frame_support::view_functions::DispatchViewFunction for Pallet<T> {
		#[deny(unreachable_patterns)]
		fn dispatch_view_function<O: frame::deps::frame_support::__private::codec::Output>(
			id: &frame::deps::frame_support::view_functions::ViewFunctionId,
			input: &mut &[u8],
			output: &mut O,
		) -> Result<(), frame::deps::frame_support::view_functions::ViewFunctionDispatchError> {
			match id.suffix {
				_ => Err(
					frame::deps::frame_support::view_functions::ViewFunctionDispatchError::NotFound(
						id.clone(),
					),
				),
			}
		}
	}
	impl<T: Config> Pallet<T> {
		#[doc(hidden)]
		pub fn pallet_view_functions_metadata() -> frame::deps::frame_support::__private::Vec<
			frame::deps::frame_support::__private::metadata_ir::PalletViewFunctionMetadataIR,
		> {
			::alloc::vec::Vec::new()
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
	impl<T: Config>
		frame::deps::frame_support::traits::OnFinalize<
			frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
		> for Pallet<T>
	{
		fn on_finalize(n: frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>) {
			let __within_span__ = {
				use ::tracing::__macro_support::Callsite as _;
				static __CALLSITE: ::tracing::callsite::DefaultCallsite = {
					static META: ::tracing::Metadata<'static> = {
						::tracing_core::metadata::Metadata::new(
							"on_finalize",
							"pallet_parachain_template::pallet",
							::tracing::Level::TRACE,
							::core::option::Option::Some(
								"templates/parachain/pallets/template/src/lib.rs",
							),
							::core::option::Option::Some(58u32),
							::core::option::Option::Some("pallet_parachain_template::pallet"),
							::tracing_core::field::FieldSet::new(
								&[],
								::tracing_core::callsite::Identifier(&__CALLSITE),
							),
							::tracing::metadata::Kind::SPAN,
						)
					};
					::tracing::callsite::DefaultCallsite::new(&META)
				};
				let mut interest = ::tracing::subscriber::Interest::never();
				if ::tracing::Level::TRACE <= ::tracing::level_filters::STATIC_MAX_LEVEL
					&& ::tracing::Level::TRACE <= ::tracing::level_filters::LevelFilter::current()
					&& {
						interest = __CALLSITE.interest();
						!interest.is_never()
					} && ::tracing::__macro_support::__is_enabled(__CALLSITE.metadata(), interest)
				{
					let meta = __CALLSITE.metadata();
					::tracing::Span::new(meta, &{ meta.fields().value_set(&[]) })
				} else {
					let span = ::tracing::__macro_support::__disabled_span(__CALLSITE.metadata());
					if match ::tracing::Level::TRACE {
						::tracing::Level::ERROR => ::tracing::log::Level::Error,
						::tracing::Level::WARN => ::tracing::log::Level::Warn,
						::tracing::Level::INFO => ::tracing::log::Level::Info,
						::tracing::Level::DEBUG => ::tracing::log::Level::Debug,
						_ => ::tracing::log::Level::Trace,
					} <= ::tracing::log::STATIC_MAX_LEVEL
					{
						if !::tracing::dispatcher::has_been_set() {
							{
								span.record_all(&{ __CALLSITE.metadata().fields().value_set(&[]) });
							}
						} else {
							{}
						}
					} else {
						{}
					};
					span
				}
			};
			let __tracing_guard__ = __within_span__.enter();
			<Self as frame::deps::frame_support::traits::Hooks<
				frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
			>>::on_finalize(n)
		}
	}
	impl<T: Config>
		frame::deps::frame_support::traits::OnIdle<
			frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
		> for Pallet<T>
	{
		fn on_idle(
			n: frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
			remaining_weight: frame::deps::frame_support::weights::Weight,
		) -> frame::deps::frame_support::weights::Weight {
			<Self as frame::deps::frame_support::traits::Hooks<
				frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
			>>::on_idle(n, remaining_weight)
		}
	}
	impl<T: Config>
		frame::deps::frame_support::traits::OnPoll<
			frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
		> for Pallet<T>
	{
		fn on_poll(
			n: frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
			weight: &mut frame::deps::frame_support::weights::WeightMeter,
		) {
			<Self as frame::deps::frame_support::traits::Hooks<
				frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
			>>::on_poll(n, weight);
		}
	}
	impl<T: Config>
		frame::deps::frame_support::traits::OnInitialize<
			frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
		> for Pallet<T>
	{
		fn on_initialize(
			n: frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
		) -> frame::deps::frame_support::weights::Weight {
			let __within_span__ = {
				use ::tracing::__macro_support::Callsite as _;
				static __CALLSITE: ::tracing::callsite::DefaultCallsite = {
					static META: ::tracing::Metadata<'static> = {
						::tracing_core::metadata::Metadata::new(
							"on_initialize",
							"pallet_parachain_template::pallet",
							::tracing::Level::TRACE,
							::core::option::Option::Some(
								"templates/parachain/pallets/template/src/lib.rs",
							),
							::core::option::Option::Some(58u32),
							::core::option::Option::Some("pallet_parachain_template::pallet"),
							::tracing_core::field::FieldSet::new(
								&[],
								::tracing_core::callsite::Identifier(&__CALLSITE),
							),
							::tracing::metadata::Kind::SPAN,
						)
					};
					::tracing::callsite::DefaultCallsite::new(&META)
				};
				let mut interest = ::tracing::subscriber::Interest::never();
				if ::tracing::Level::TRACE <= ::tracing::level_filters::STATIC_MAX_LEVEL
					&& ::tracing::Level::TRACE <= ::tracing::level_filters::LevelFilter::current()
					&& {
						interest = __CALLSITE.interest();
						!interest.is_never()
					} && ::tracing::__macro_support::__is_enabled(__CALLSITE.metadata(), interest)
				{
					let meta = __CALLSITE.metadata();
					::tracing::Span::new(meta, &{ meta.fields().value_set(&[]) })
				} else {
					let span = ::tracing::__macro_support::__disabled_span(__CALLSITE.metadata());
					if match ::tracing::Level::TRACE {
						::tracing::Level::ERROR => ::tracing::log::Level::Error,
						::tracing::Level::WARN => ::tracing::log::Level::Warn,
						::tracing::Level::INFO => ::tracing::log::Level::Info,
						::tracing::Level::DEBUG => ::tracing::log::Level::Debug,
						_ => ::tracing::log::Level::Trace,
					} <= ::tracing::log::STATIC_MAX_LEVEL
					{
						if !::tracing::dispatcher::has_been_set() {
							{
								span.record_all(&{ __CALLSITE.metadata().fields().value_set(&[]) });
							}
						} else {
							{}
						}
					} else {
						{}
					};
					span
				}
			};
			let __tracing_guard__ = __within_span__.enter();
			<Self as frame::deps::frame_support::traits::Hooks<
				frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
			>>::on_initialize(n)
		}
	}
	impl<T: Config> frame::deps::frame_support::traits::BeforeAllRuntimeMigrations for Pallet<T> {
		fn before_all_runtime_migrations() -> frame::deps::frame_support::weights::Weight {
			use frame::deps::frame_support::__private::hashing::twox_128;
			use frame::deps::frame_support::storage::unhashed::contains_prefixed_key;
			use frame::deps::frame_support::traits::{Get, PalletInfoAccess};
			let __within_span__ = {
				use ::tracing::__macro_support::Callsite as _;
				static __CALLSITE: ::tracing::callsite::DefaultCallsite = {
					static META: ::tracing::Metadata<'static> = {
						::tracing_core::metadata::Metadata::new(
							"before_all",
							"pallet_parachain_template::pallet",
							::tracing::Level::TRACE,
							::core::option::Option::Some(
								"templates/parachain/pallets/template/src/lib.rs",
							),
							::core::option::Option::Some(58u32),
							::core::option::Option::Some("pallet_parachain_template::pallet"),
							::tracing_core::field::FieldSet::new(
								&[],
								::tracing_core::callsite::Identifier(&__CALLSITE),
							),
							::tracing::metadata::Kind::SPAN,
						)
					};
					::tracing::callsite::DefaultCallsite::new(&META)
				};
				let mut interest = ::tracing::subscriber::Interest::never();
				if ::tracing::Level::TRACE <= ::tracing::level_filters::STATIC_MAX_LEVEL
					&& ::tracing::Level::TRACE <= ::tracing::level_filters::LevelFilter::current()
					&& {
						interest = __CALLSITE.interest();
						!interest.is_never()
					} && ::tracing::__macro_support::__is_enabled(__CALLSITE.metadata(), interest)
				{
					let meta = __CALLSITE.metadata();
					::tracing::Span::new(meta, &{ meta.fields().value_set(&[]) })
				} else {
					let span = ::tracing::__macro_support::__disabled_span(__CALLSITE.metadata());
					if match ::tracing::Level::TRACE {
						::tracing::Level::ERROR => ::tracing::log::Level::Error,
						::tracing::Level::WARN => ::tracing::log::Level::Warn,
						::tracing::Level::INFO => ::tracing::log::Level::Info,
						::tracing::Level::DEBUG => ::tracing::log::Level::Debug,
						_ => ::tracing::log::Level::Trace,
					} <= ::tracing::log::STATIC_MAX_LEVEL
					{
						if !::tracing::dispatcher::has_been_set() {
							{
								span.record_all(&{ __CALLSITE.metadata().fields().value_set(&[]) });
							}
						} else {
							{}
						}
					} else {
						{}
					};
					span
				}
			};
			let __tracing_guard__ = __within_span__.enter();
			let pallet_hashed_prefix = <Self as PalletInfoAccess>::name_hash();
			let exists = contains_prefixed_key(&pallet_hashed_prefix);
			if !exists {
				let default_version = frame::deps::frame_support::traits::StorageVersion::new(0);
				{
					let lvl = ::log::Level::Info;
					if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
						::log::__private_api::log(
                            format_args!(
                                "🐥 New pallet {0:?} detected in the runtime. The pallet has no defined storage version, so the on-chain version is being initialized to {1:?}.",
                                <<T as frame::deps::frame_system::Config>::PalletInfo as frame::deps::frame_support::traits::PalletInfo>::name::<
                                    Self,
                                >()
                                    .unwrap_or("<unknown pallet name>"),
                                default_version,
                            ),
                            lvl,
                            &(
                                frame::deps::frame_support::LOG_TARGET,
                                "pallet_parachain_template::pallet",
                                ::log::__private_api::loc(),
                            ),
                            (),
                        );
					}
				};
				default_version.put::<Self>();
				<T as frame::deps::frame_system::Config>::DbWeight::get().reads_writes(1, 1)
			} else {
				<T as frame::deps::frame_system::Config>::DbWeight::get().reads(1)
			}
		}
	}
	impl<T: Config> frame::deps::frame_support::traits::OnRuntimeUpgrade for Pallet<T> {
		fn on_runtime_upgrade() -> frame::deps::frame_support::weights::Weight {
			let __within_span__ = {
				use ::tracing::__macro_support::Callsite as _;
				static __CALLSITE: ::tracing::callsite::DefaultCallsite = {
					static META: ::tracing::Metadata<'static> = {
						::tracing_core::metadata::Metadata::new(
							"on_runtime_update",
							"pallet_parachain_template::pallet",
							::tracing::Level::TRACE,
							::core::option::Option::Some(
								"templates/parachain/pallets/template/src/lib.rs",
							),
							::core::option::Option::Some(58u32),
							::core::option::Option::Some("pallet_parachain_template::pallet"),
							::tracing_core::field::FieldSet::new(
								&[],
								::tracing_core::callsite::Identifier(&__CALLSITE),
							),
							::tracing::metadata::Kind::SPAN,
						)
					};
					::tracing::callsite::DefaultCallsite::new(&META)
				};
				let mut interest = ::tracing::subscriber::Interest::never();
				if ::tracing::Level::TRACE <= ::tracing::level_filters::STATIC_MAX_LEVEL
					&& ::tracing::Level::TRACE <= ::tracing::level_filters::LevelFilter::current()
					&& {
						interest = __CALLSITE.interest();
						!interest.is_never()
					} && ::tracing::__macro_support::__is_enabled(__CALLSITE.metadata(), interest)
				{
					let meta = __CALLSITE.metadata();
					::tracing::Span::new(meta, &{ meta.fields().value_set(&[]) })
				} else {
					let span = ::tracing::__macro_support::__disabled_span(__CALLSITE.metadata());
					if match ::tracing::Level::TRACE {
						::tracing::Level::ERROR => ::tracing::log::Level::Error,
						::tracing::Level::WARN => ::tracing::log::Level::Warn,
						::tracing::Level::INFO => ::tracing::log::Level::Info,
						::tracing::Level::DEBUG => ::tracing::log::Level::Debug,
						_ => ::tracing::log::Level::Trace,
					} <= ::tracing::log::STATIC_MAX_LEVEL
					{
						if !::tracing::dispatcher::has_been_set() {
							{
								span.record_all(&{ __CALLSITE.metadata().fields().value_set(&[]) });
							}
						} else {
							{}
						}
					} else {
						{}
					};
					span
				}
			};
			let __tracing_guard__ = __within_span__.enter();
			{
				let lvl = ::log::Level::Debug;
				if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
					::log::__private_api::log(
                        format_args!(
                            "✅ no migration for {0}",
                            <<T as frame::deps::frame_system::Config>::PalletInfo as frame::deps::frame_support::traits::PalletInfo>::name::<
                                Self,
                            >()
                                .unwrap_or("<unknown pallet name>"),
                        ),
                        lvl,
                        &(
                            frame::deps::frame_support::LOG_TARGET,
                            "pallet_parachain_template::pallet",
                            ::log::__private_api::loc(),
                        ),
                        (),
                    );
				}
			};
			<Self as frame::deps::frame_support::traits::Hooks<
				frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
			>>::on_runtime_upgrade()
		}
	}
	impl<T: Config>
		frame::deps::frame_support::traits::OffchainWorker<
			frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
		> for Pallet<T>
	{
		fn offchain_worker(n: frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>) {
			<Self as frame::deps::frame_support::traits::Hooks<
				frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
			>>::offchain_worker(n)
		}
	}
	impl<T: Config> frame::deps::frame_support::traits::IntegrityTest for Pallet<T> {
		fn integrity_test() {
			frame::deps::frame_support::__private::sp_io::TestExternalities::default()
				.execute_with(|| {
					<Self as frame::deps::frame_support::traits::Hooks<
						frame::deps::frame_system::pallet_prelude::BlockNumberFor<T>,
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
	pub use __tt_default_parts_v2_7 as tt_default_parts_v2;
	pub use __tt_extra_parts_7 as tt_extra_parts;
}
