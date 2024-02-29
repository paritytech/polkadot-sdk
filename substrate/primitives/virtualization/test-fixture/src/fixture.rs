#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
	unsafe {
		core::arch::asm!("unimp", options(noreturn));
	}
}

mod sys {
	#[polkavm_derive::polkavm_import]
	extern "C" {
		#[polkavm_import(symbol = 1u32)]
		pub fn read_counter(buf_ptr: *mut u8) -> u32;
		#[polkavm_import(symbol = 2u32)]
		pub fn increment_counter(buf_ptr: *const u8) -> u64;
		#[polkavm_import(symbol = 3u32)]
		pub fn exit();
		#[polkavm_import(symbol = 4u32)]
		pub fn subcall();
	}
}

static mut OFFSET: u64 = 3;

fn read_counter() -> u64 {
	let mut buffer = [42u8; 8];
	let ret = unsafe { sys::read_counter(buffer.as_mut_ptr()) };
	assert_eq!(ret, 1);
	u64::from_le_bytes(buffer)
}

fn increment_counter(inc: u64) {
	let ret = unsafe { sys::increment_counter(inc.to_le_bytes().as_ptr()) };
	assert_eq!(ret, 2 << 56);
}

fn exit() {
	unsafe {
		sys::exit();
	}
	// if exit() isn't working properly we can observe that using the counter
	increment_counter(1_000);
	panic!("Exit should not return");
}

fn subcall() {
	unsafe {
		sys::subcall();
	}
}

#[polkavm_derive::polkavm_export]
fn counter() {
	let initial_counter = read_counter();
	increment_counter(7);
	assert_eq!(read_counter(), initial_counter + 7);
	increment_counter(1);
	assert_eq!(read_counter(), initial_counter + 8);
}

#[polkavm_derive::polkavm_export]
fn add_99() {
	increment_counter(99);
}

#[polkavm_derive::polkavm_export]
extern "C" fn do_panic() {
	panic!("panic_me was called")
}

#[polkavm_derive::polkavm_export]
extern "C" fn do_exit() {
	exit();
}

#[polkavm_derive::polkavm_export]
extern "C" fn increment_forever() {
	loop {
		increment_counter(1);
	}
}

#[polkavm_derive::polkavm_export]
extern "C" fn offset() {
	let offset = unsafe { OFFSET };
	increment_counter(offset);
	unsafe {
		OFFSET += 1;
	}
}

#[polkavm_derive::polkavm_export]
extern "C" fn do_subcall() {
	subcall();
}
