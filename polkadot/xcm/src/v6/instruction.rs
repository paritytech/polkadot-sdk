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
