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
macro_rules! apply_instructions {
	($mac:ident, $($args:tt)*) => {
		$mac!($($args)*,
			WithdrawAsset,
			ReserveAssetDeposited,
			ReceiveTeleportedAsset,
			QueryResponse,
			TransferAsset,
			TransferReserveAsset,
			Transact<Call>,
			HrmpNewChannelOpenRequest,
			HrmpChannelAccepted,
			HrmpChannelClosing,
			ClearOrigin,
			DescendOrigin,
			ReportError,
			DepositAsset,
			DepositReserveAsset,
			ExchangeAsset,
			InitiateReserveWithdraw,
			InitiateTeleport,
			ReportHolding,
			BuyExecution,
			RefundSurplus,
			SetErrorHandler<Call>,
			SetAppendix<Call>,
			ClearError,
			ClaimAsset,
			Trap,
			SubscribeVersion,
			UnsubscribeVersion,
			BurnAsset,
			ExpectAsset,
			ExpectOrigin,
			ExpectError,
			ExpectTransactStatus,
			QueryPallet,
			ExpectPallet,
			ReportTransactStatus,
			ClearTransactStatus,
			UniversalOrigin,
			ExportMessage,
			LockAsset,
			UnlockAsset,
			NoteUnlockable,
			RequestUnlock,
			SetFeesMode,
			SetTopic,
			ClearTopic,
			AliasOrigin,
			UnpaidExecution,
			PayFees,
			InitiateTransfer,
			ExecuteWithOrigin<Call>,
			SetHints
		);
	};
}

#[macro_export]
macro_rules! impl_xcm_instruction {
    (
		$vis:vis enum $name:ident<$generic:ident>, $( $instr:ident $( < $instr_generic:ty > )? ),*
    ) => {
		/// Cross-Consensus Message: A message from one consensus system to another.
		///
		/// Consensus systems that may send and receive messages include blockchains and smart contracts.
		///
		/// All messages are delivered from a known *origin*, expressed as a `Location`.
		///
		/// This is the inner XCM format and is version-sensitive. Messages are typically passed using the
		/// outer XCM format, known as `VersionedXcm`.
		#[derive(
			Educe,
			Encode,
			Decode,
			TypeInfo,
		)]
		#[educe(Clone(bound = false), Eq, PartialEq(bound = false), Debug(bound = false))]
		#[codec(encode_bound())]
		#[codec(decode_bound())]
		#[scale_info(bounds(), skip_type_params(Call))]
        $vis enum $name<$generic> where $generic: 'static {
            $(
                // $( #[$instr_doc] )*
                $instr($instr $(< $instr_generic >)?),
            )+
        }

		impl <$generic: 'static> $name<$generic> {
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
			impl<$generic: 'static> From<$instr $(< $instr_generic >)?> for $name<$generic> {
				fn from(x: $instr $(< $instr_generic >)?) -> Self {
					Self::$instr(x)
				}
			}

			impl<$generic> TryFrom<$name<$generic>> for $instr $(< $instr_generic >)? {
				type Error = ();
				fn try_from(x: $name<$generic>) -> result::Result<Self, ()> {
					match x {
						$name::$instr(x) => Ok(x),
						_ => Err(()),
					}
				}
			}

			impl<$generic> $crate::traits::IntoInstruction<$name<$generic>> for $instr $(< $instr_generic >)? {
				fn into_instruction(self) -> $name<$generic> {
					$name::$instr(self)
				}
			}
		)+
    };
}
