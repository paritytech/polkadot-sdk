use std::cell::RefCell;
use std::rc::Rc;
use frame::testing_prelude::*;

thread_local! {
	pub static RC_STATE: Rc<RefCell<TestState>> = Rc::new(RefCell::new(Default::default()));
	pub static AH_STATE: Rc<RefCell<TestState>> = Rc::new(RefCell::new(Default::default()));
}
