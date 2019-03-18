// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Macros that define an Event types. Events can be used to easily report changes or conditions
//! in your runtime to external entities like users, chain explorers, or dApps.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

pub use srml_metadata::{EventMetadata, DecodeDifferent, OuterEventMetadata, FnEncode};

/// Implement the `Event` for a module.
///
/// # Simple Event Example:
///
/// ```rust
/// #[macro_use]
/// extern crate srml_support;
/// #[macro_use]
/// extern crate parity_codec as codec;
/// #[macro_use]
/// extern crate serde_derive;
///
/// decl_event!(
///	   pub enum Event {
///       Success,
///       Failure(String),
///    }
/// );
///# fn main() {}
/// ```
///
/// # Generic Event Example:
///
/// ```rust
/// #[macro_use]
/// extern crate srml_support;
/// extern crate parity_codec as codec;
/// #[macro_use]
/// extern crate parity_codec;
/// #[macro_use]
/// extern crate serde_derive;
///
/// trait Trait {
///     type Balance;
///     type Token;
/// }
///
/// mod event1 {
///     // Event that specifies the generic parameter explicitly (`Balance`).
///     decl_event!(
///	       pub enum Event<T> where Balance = <T as super::Trait>::Balance {
///           Message(Balance),
///        }
///     );
/// }
///
/// mod event2 {
///     // Event that uses the generic parameter `Balance`.
///     // If no name for the generic parameter is specified explicitly,
///     // the name will be taken from the type name of the trait.
///     decl_event!(
///	       pub enum Event<T> where <T as super::Trait>::Balance {
///           Message(Balance),
///        }
///     );
/// }
///
/// mod event3 {
///     // And we even support declaring multiple generic parameters!
///     decl_event!(
///	       pub enum Event<T> where <T as super::Trait>::Balance, <T as super::Trait>::Token {
///           Message(Balance, Token),
///        }
///     );
/// }
///# fn main() {}
/// ```
///
/// The syntax for generic events requires the `where`.
///
/// # Generic Event with Instance Example:
///
/// ```rust
/// #[macro_use]
/// extern crate srml_support;
/// extern crate parity_codec as codec;
/// #[macro_use]
/// extern crate parity_codec;
/// #[macro_use]
/// extern crate serde_derive;
///
///# struct DefaultInstance;
///# trait Instance {}
///# impl Instance for DefaultInstance {}
/// trait Trait<I: Instance=DefaultInstance> {
///     type Balance;
///     type Token;
/// }
///
/// // For module with instances, DefaultInstance is optionnal
/// decl_event!(
///    pub enum Event<T, I: Instance = DefaultInstance> where
///       <T as Trait>::Balance,
///       <T as Trait>::Token
///    {
///       Message(Balance, Token),
///    }
/// );
///# fn main() {}
/// ```
#[macro_export]
macro_rules! decl_event {
	(
		$(#[$attr:meta])*
		pub enum Event<$evt_generic_param:ident $(, $instance:ident $(: $instantiable:ident)? $( = $event_default_instance:path)? )?> where
			$( $tt:tt )*
	) => {
		$crate::__decl_generic_event!(
			$( #[ $attr ] )*;
			$evt_generic_param;
			$($instance $( = $event_default_instance)? )?;
			{ $( $tt )* };
		);
	};
	(
		$(#[$attr:meta])*
		pub enum Event {
			$(
				$events:tt
			)*
		}
	) => {
		// Workaround for https://github.com/rust-lang/rust/issues/26925 . Remove when sorted.
		#[derive(Clone, PartialEq, Eq, $crate::codec::Encode, $crate::codec::Decode)]
		#[cfg_attr(feature = "std", derive(Debug))]
		$(#[$attr])*
		pub enum Event {
			$(
				$events
			)*
		}
		impl From<Event> for () {
			fn from(_: Event) -> () { () }
		}
		impl Event {
			#[allow(dead_code)]
			pub fn metadata() -> &'static [ $crate::event::EventMetadata ] {
				$crate::__events_to_metadata!(; $( $events )* )
			}
		}
	}
}

#[macro_export]
#[doc(hidden)]
// This parsing to retrieve last ident on unnamed generic could be improved.
// but user can still name it if the parsing fails. And improving parsing seems difficult.
macro_rules! __decl_generic_event {
	(
		$(#[$attr:meta])*;
		$event_generic_param:ident;
		$($instance:ident $( = $event_default_instance:path)? )?;
		{ $( $tt:tt )* };
	) => {
		$crate::__decl_generic_event!(@format_generic
			$( #[ $attr ] )*;
			$event_generic_param;
			$($instance $( = $event_default_instance)? )?;
			{ $( $tt )* };
			{};
		);
	};
	// Parse named
	(@format_generic
		$(#[$attr:meta])*;
		$event_generic_param:ident;
		$($instance:ident $( = $event_default_instance:path)? )?;
		{ $generic_rename:ident = $generic_type:ty, $($rest:tt)* };
		{$( $parsed:tt)*};
	) => {
		$crate::__decl_generic_event!(@format_generic
			$( #[ $attr ] )*;
			$event_generic_param;
			$($instance $( = $event_default_instance)? )?;
			{ $($rest)* };
			{ $($parsed)*, $generic_rename = $generic_type };
		);
	};
	// Parse unnamed
	(@format_generic
		$(#[$attr:meta])*;
		$event_generic_param:ident;
		$($instance:ident $( = $event_default_instance:path)? )?;
		{ <$generic:ident as $trait:path>::$trait_type:ident, $($rest:tt)* };
		{$($parsed:tt)*};
	) => {
		$crate::__decl_generic_event!(@format_generic
			$( #[ $attr ] )*;
			$event_generic_param;
			$($instance $( = $event_default_instance)? )?;
			{ $($rest)* };
			{ $($parsed)*, $trait_type = <$generic as $trait>::$trait_type };
		);
	};
	// Unnamed type can't be parsed
	(@format_generic
		$(#[$attr:meta])*;
		$event_generic_param:ident;
		$($instance:ident $( = $event_default_instance:path)? )?;
		{ $generic_type:ty, $($rest:tt)* };
		{$($parsed:tt)*};
	) => {
		$crate::__decl_generic_event!(@cannot_parse $generic_type);
	};
	// Finish formatting on an unnamed one
	(@format_generic
		$(#[$attr:meta])*;
		$event_generic_param:ident;
		$($instance:ident $( = $event_default_instance:path)? )?;
		{ <$generic:ident as $trait:path>::$trait_type:ident { $( $events:tt )* } };
		{$( $parsed:tt)*};
	) => {
		$crate::__decl_generic_event!(@generate
			$( #[ $attr ] )*;
			$event_generic_param;
			$($instance $( = $event_default_instance)? )?;
			{ $($events)* };
			{ $($parsed)*, $trait_type = <$generic as $trait>::$trait_type};
		);
	};
	// Finish formatting on a named one
	(@format_generic
		$(#[$attr:meta])*;
		$event_generic_param:ident;
		$($instance:ident $( = $event_default_instance:path)? )?;
		{ $generic_rename:ident = $generic_type:ty { $( $events:tt )* } };
		{$( $parsed:tt)*};
	) => {
		$crate::__decl_generic_event!(@generate
			$(#[$attr])*;
			$event_generic_param;
			$($instance $( = $event_default_instance)? )?;
			{ $($events)* };
			{ $($parsed)*, $generic_rename = $generic_type};
		);
	};
	// Final unnamed type can't be parsed
	(@format_generic
		$(#[$attr:meta])*;
		$event_generic_param:ident;
		$($instance:ident $( = $event_default_instance:path)? )?;
		{ $generic_type:ty { $( $events:tt )* } };
		{$( $parsed:tt)*};
	) => {
		$crate::__decl_generic_event!(@cannot_parse $generic_type);
	};
	(@generate
		$(#[$attr:meta])*;
		$event_generic_param:ident;
		$($instance:ident $( = $event_default_instance:path)? )?;
		{ $( $events:tt )* };
		{ ,$( $generic_param:ident = $generic_type:ty ),* };
	) => {
		/// [`RawEvent`] specialized for the configuration [`Trait`]
		///
		/// [`RawEvent`]: enum.RawEvent.html
		/// [`Trait`]: trait.Trait.html
		pub type Event<$event_generic_param $(, $instance $( = $event_default_instance)? )?> = RawEvent<$( $generic_type ),* $(, $instance)? >;
		// Workaround for https://github.com/rust-lang/rust/issues/26925 . Remove when sorted.
		#[derive(Clone, PartialEq, Eq, $crate::codec::Encode, $crate::codec::Decode)]
		#[cfg_attr(feature = "std", derive(Debug))]
		$(#[$attr])*
		pub enum RawEvent<$( $generic_param ),* $(, $instance)? > {
			$(
				$events
			)*
			$(
				#[doc(hidden)]
				PhantomData($crate::rstd::marker::PhantomData<$instance>),
			)?
		}
		impl<$( $generic_param ),* $(, $instance)? > From<RawEvent<$( $generic_param ),* $(, $instance)?>> for () {
			fn from(_: RawEvent<$( $generic_param ),* $(, $instance)?>) -> () { () }
		}
		impl<$( $generic_param ),* $(, $instance)?> RawEvent<$( $generic_param ),* $(, $instance)?> {
			#[allow(dead_code)]
			pub fn metadata() -> &'static [$crate::event::EventMetadata] {
				$crate::__events_to_metadata!(; $( $events )* )
			}
		}
	};
	(@cannot_parse $ty:ty) => {
		compile_error!(concat!("The type `", stringify!($ty), "` can't be parsed as an unnamed one, please name it `Name = ", stringify!($ty), "`"));
	}
}

#[macro_export]
#[doc(hidden)]
macro_rules! __events_to_metadata {
	(
		$( $metadata:expr ),*;
		$( #[doc = $doc_attr:tt] )*
		$event:ident $( ( $( $param:path ),* ) )*,
		$( $rest:tt )*
	) => {
		$crate::__events_to_metadata!(
			$( $metadata, )*
			$crate::event::EventMetadata {
				name: $crate::event::DecodeDifferent::Encode(stringify!($event)),
				arguments: $crate::event::DecodeDifferent::Encode(&[
					$( $( stringify!($param) ),* )*
				]),
				documentation: $crate::event::DecodeDifferent::Encode(&[
					$( $doc_attr ),*
				]),
			};
			$( $rest )*
		)
	};
	(
		$( $metadata:expr ),*;
	) => {
		&[ $( $metadata ),* ]
	}
}

/// Constructs an Event type for a runtime. This is usually called automatically by the
/// construct_runtime macro. See also __create_decl_macro.
#[macro_export]
macro_rules! impl_outer_event {

	// Macro transformations (to convert invocations with incomplete parameters to the canonical
	// form)

	(
		$(#[$attr:meta])*
		pub enum $name:ident for $runtime:ident {
			$( $rest:tt $( <$t:ident $(, $rest_instance:path)? > )*, )*
		}
	) => {
		$crate::impl_outer_event!(
			$( #[$attr] )*;
			$name;
			$runtime;
			system;
			Modules { $( $rest $(<$t $(, $rest_instance)? >)*, )* };
			;
		);
	};
	(
		$(#[$attr:meta])*
		pub enum $name:ident for $runtime:ident where system = $system:ident {
			$( $rest:tt $( <$t:ident $(, $rest_instance:path)? > )*, )*
		}
	) => {
		$crate::impl_outer_event!(
			$( #[$attr] )*;
			$name;
			$runtime;
			$system;
			Modules { $( $rest $(<$t $(, $rest_instance)? >)*, )* };
			;
		);
	};
	(
		$(#[$attr:meta])*;
		$name:ident;
		$runtime:ident;
		$system:ident;
		Modules {
			$module:ident<T $(, $instance:path)? >,
			$( $rest:tt $( <$t:ident $(, $rest_instance:path)? > )*, )*
		};
		$( $module_name:ident::Event $( <$generic_param:ident $(, $generic_instance:path)? > )*, )*;
	) => {
		$crate::impl_outer_event!(
			$( #[$attr] )*;
			$name;
			$runtime;
			$system;
			Modules { $( $rest $(<$t $(, $rest_instance)? >)*, )* };
			$( $module_name::Event $( <$generic_param $(, $generic_instance)? > )*, )* $module::Event<$runtime $(, $instance)? >,;
		);
	};
	(
		$(#[$attr:meta])*;
		$name:ident;
		$runtime:ident;
		$system:ident;
		Modules {
			$module:ident,
			$( $rest:tt )*
		};
		$( $module_name:ident::Event $( <$generic_param:ident $(, $generic_instance:path)? > )*, )*;
	) => {
		$crate::impl_outer_event!(
			$( #[$attr] )*;
			$name;
			$runtime;
			$system;
			Modules { $( $rest )* };
			$( $module_name::Event $( <$generic_param $(, $generic_instance)? > )*, )* $module::Event,;
		);
	};

	// The main macro expansion that actually renders the Event enum code.

	(
		$(#[$attr:meta])*;
		$name:ident;
		$runtime:ident;
		$system:ident;
		Modules {};
		$( $module_name:ident::Event $( <$generic_param:ident $(, $generic_instance:path)? > )*, )*;
	) => {
		// Workaround for https://github.com/rust-lang/rust/issues/26925 . Remove when sorted.
		#[derive(Clone, PartialEq, Eq, $crate::codec::Encode, $crate::codec::Decode)]
		#[cfg_attr(feature = "std", derive(Debug))]
		$(#[$attr])*
		#[allow(non_camel_case_types)]
		pub enum $name {
			system($system::Event),
			$(
				$module_name( $module_name::Event $( <$generic_param $(, $generic_instance)? > )* ),
			)*
		}
		impl From<$system::Event> for $name {
			fn from(x: $system::Event) -> Self {
				$name::system(x)
			}
		}
		$(
			impl From<$module_name::Event $( <$generic_param $(, $generic_instance)? > )*> for $name {
				fn from(x: $module_name::Event $( <$generic_param $(, $generic_instance)? > )*) -> Self {
					$name::$module_name(x)
				}
			}
		)*
		$crate::__impl_outer_event_json_metadata!(
			$runtime;
			$name;
			$system;
			$( $module_name::Event $( <$generic_param $(, $generic_instance)? > )*, )*;
		);
	}
}

#[macro_export]
#[doc(hidden)]
macro_rules! __impl_outer_event_json_metadata {
	(
		$runtime:ident;
		$event_name:ident;
		$system:ident;
		$( $module_name:ident::Event $( <$generic_param:ident $(, $generic_instance:path)? > )*, )*;
	) => {
		impl $runtime {
			#[allow(dead_code)]
			pub fn outer_event_metadata() -> $crate::event::OuterEventMetadata {
				$crate::event::OuterEventMetadata {
					name: $crate::event::DecodeDifferent::Encode(stringify!($event_name)),
					events: $crate::event::DecodeDifferent::Encode(&[
						("system", $crate::event::FnEncode(system::Event::metadata))
						$(
							, (
								stringify!($module_name),
								$crate::event::FnEncode(
									$module_name::Event $( ::<$generic_param $(, $generic_instance)? > )* ::metadata
								)
							)
						)*
					])
				}
			}
			#[allow(dead_code)]
			pub fn __module_events_system() -> &'static [$crate::event::EventMetadata] {
				system::Event::metadata()
			}
			$(
				#[allow(dead_code)]
				$crate::paste::item!{
					pub fn [< __module_events_ $module_name >] () -> &'static [$crate::event::EventMetadata] {
						$module_name::Event $( ::<$generic_param $(, $generic_instance)? > )* ::metadata()
					}
				}
			)*
		}
	}
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
	use super::*;
	use serde_derive::Serialize;
	use parity_codec::{Encode, Decode};

	mod system {
		pub trait Trait {
			type Origin;
			type BlockNumber;
		}

		decl_module! {
			pub struct Module<T: Trait> for enum Call where origin: T::Origin {}
		}

		decl_event!(
			pub enum Event {
				SystemEvent,
			}
		);
	}

	mod system_renamed {
		pub trait Trait {
			type Origin;
			type BlockNumber;
		}

		decl_module! {
			pub struct Module<T: Trait> for enum Call where origin: T::Origin {}
		}

		decl_event!(
			pub enum Event {
				SystemEvent,
			}
		);
	}

	mod event_module {
		pub trait Trait {
			type Origin;
			type Balance;
			type BlockNumber;
		}

		decl_module! {
			pub struct Module<T: Trait> for enum Call where origin: T::Origin {}
		}

		decl_event!(
			/// Event without renaming the generic parameter `Balance` and `Origin`.
			pub enum Event<T> where <T as Trait>::Balance, <T as Trait>::Origin
			{
				/// Hi, I am a comment.
				TestEvent(Balance, Origin),
				/// Dog
				EventWithoutParams,
			}
		);
	}

	mod event_module2 {
		pub trait Trait {
			type Origin;
			type Balance;
			type BlockNumber;
		}

		decl_module! {
			pub struct Module<T: Trait> for enum Call where origin: T::Origin {}
		}

		decl_event!(
			/// Event with renamed generic parameter
			pub enum Event<T> where
				BalanceRenamed = <T as Trait>::Balance,
				OriginRenamed = <T as Trait>::Origin
			{
				TestEvent(BalanceRenamed),
				TestOrigin(OriginRenamed),
			}
		);
	}

	mod event_module3 {
		decl_event!(
			pub enum Event {
				HiEvent,
			}
		);
	}

	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize)]
	pub struct TestRuntime;

	impl_outer_event! {
		pub enum TestEvent for TestRuntime {
			event_module<T>,
			event_module2<T>,
			event_module3,
		}
	}

	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize)]
	pub struct TestRuntime2;

	impl_outer_event! {
		pub enum TestEventSystemRenamed for TestRuntime2 where system = system_renamed {
			event_module<T>,
			event_module2<T>,
			event_module3,
		}
	}

	impl event_module::Trait for TestRuntime {
		type Origin = u32;
		type Balance = u32;
		type BlockNumber = u32;
	}

	impl event_module2::Trait for TestRuntime {
		type Origin = u32;
		type Balance = u32;
		type BlockNumber = u32;
	}

	impl system::Trait for TestRuntime {
		type Origin = u32;
		type BlockNumber = u32;
	}

	impl event_module::Trait for TestRuntime2 {
		type Origin = u32;
		type Balance = u32;
		type BlockNumber = u32;
	}

	impl event_module2::Trait for TestRuntime2 {
		type Origin = u32;
		type Balance = u32;
		type BlockNumber = u32;
	}

	impl system_renamed::Trait for TestRuntime2 {
		type Origin = u32;
		type BlockNumber = u32;
	}

	const EXPECTED_METADATA: OuterEventMetadata = OuterEventMetadata {
		name: DecodeDifferent::Encode("TestEvent"),
		events: DecodeDifferent::Encode(&[
			(
				"system",
				FnEncode(|| &[
					EventMetadata {
						name: DecodeDifferent::Encode("SystemEvent"),
						arguments: DecodeDifferent::Encode(&[]),
						documentation: DecodeDifferent::Encode(&[]),
					}
				])
			),
			(
				"event_module",
				FnEncode(|| &[
					EventMetadata {
						name: DecodeDifferent::Encode("TestEvent"),
						arguments: DecodeDifferent::Encode(&[ "Balance", "Origin" ]),
						documentation: DecodeDifferent::Encode(&[ " Hi, I am a comment." ])
					},
					EventMetadata {
						name: DecodeDifferent::Encode("EventWithoutParams"),
						arguments: DecodeDifferent::Encode(&[]),
						documentation: DecodeDifferent::Encode(&[ " Dog" ]),
					},
				])
			),
			(
				"event_module2",
				FnEncode(|| &[
					EventMetadata {
						name: DecodeDifferent::Encode("TestEvent"),
						arguments: DecodeDifferent::Encode(&[ "BalanceRenamed" ]),
						documentation: DecodeDifferent::Encode(&[])
					},
					EventMetadata {
						name: DecodeDifferent::Encode("TestOrigin"),
						arguments: DecodeDifferent::Encode(&[ "OriginRenamed" ]),
						documentation: DecodeDifferent::Encode(&[]),
					},
				])
			),
			(
				"event_module3",
				FnEncode(|| &[
					EventMetadata {
						name: DecodeDifferent::Encode("HiEvent"),
						arguments: DecodeDifferent::Encode(&[]),
						documentation: DecodeDifferent::Encode(&[])
					}
				])
			)
		])
	};

	#[test]
	fn outer_event_metadata() {
		assert_eq!(EXPECTED_METADATA, TestRuntime::outer_event_metadata());
	}
}
