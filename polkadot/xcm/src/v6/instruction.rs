// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

#[macro_export]
macro_rules! impl_xcm_instruction {
    (
		$( #[$doc:meta] )*
		$vis:vis enum $name:ident<$generic:ident> {
			$(
				$( #[$instr_doc:meta] )*
				$instr:ident $(< $instr_generic:ident >)?,
			)+
		}
    ) => {
        $( #[$doc] )*
        $vis enum $name<$generic> {
            $(
                // $( #[$instr_doc] )*
                $instr($instr $(< $instr_generic >)?),
            )+
        }

		impl <$generic> $name<$generic> {
			pub fn into<C>(self) -> $name<C> {
				$name::from(self)
			}

			pub fn from<C>(value: $name<C>) -> Self {
				match value {
					$(
						$name::$instr(x) => $name::$instr(x.into()),
					)+
				}
			}
		}

		$(
			impl<$generic> From<$instr $(< $instr_generic >)?> for $name<$generic> {
				fn from(x: $instr $(< $instr_generic >)?) -> Self {
					Self::$instr(x)
				}
			}

			impl<$generic> TryFrom<$name<$generic>> for $instr $(< $instr_generic >)? {
				type Error = ();
				fn try_from(x: $name<$generic>) -> Result<Self, ()> {
					match x {
						$name::$instr(x) => Ok(x),
						_ => Err(()),
					}
				}
			}

			// TODO: make it $crate::IntoInstruction
			impl<$generic> IntoInstruction<$generic> for $instr $(< $instr_generic >)? {
				fn into_instruction(self) -> $name<$generic> {
					$name::$instr(self)
				}
			}
		)+
    };
}
