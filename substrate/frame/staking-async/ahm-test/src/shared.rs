use frame::testing_prelude::*;
use std::cell::UnsafeCell;

thread_local! {
	pub static RC_STATE: UnsafeCell<TestState> = UnsafeCell::new(Default::default());
	pub static AH_STATE: UnsafeCell<TestState> = UnsafeCell::new(Default::default());
}

parameter_types! {
	// counts how many times a new offence message is sent from RC -> AH.
	pub static CounterRCAHNewOffence: u32 = 0;
	// counts how many times a new session report is sent from RC -> AH.
	pub static CounterRCAHSessionReport: u32 = 0;
}
pub fn put_ah_state(ah: TestState) {
	AH_STATE.with(|state| unsafe {
		let ptr = state.get();
		*ptr = ah;
	})
}

pub fn in_ah(f: impl Fn() -> ()) {
	AH_STATE.with(|state| unsafe {
		let ptr = state.get();
		(*ptr).execute_with(f)
	})
}

pub fn put_rc_state(rc: TestState) {
	RC_STATE.with(|state| unsafe {
		let ptr = state.get();
		*ptr = rc;
	})
}

pub fn in_rc(f: impl Fn() -> ()) {
	RC_STATE.with(|state| unsafe {
		let ptr = state.get();
		(*ptr).execute_with(f)
	})
}
