#![cfg_attr(not(feature = "std"), no_std)]

pub trait CurrentSessionIndex {
    fn current_session_index() -> sp_staking::SessionIndex;
}