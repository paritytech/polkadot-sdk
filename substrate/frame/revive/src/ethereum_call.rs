use environmental::environmental;

environmental!(ethereum_flag: ());

pub fn do_ethereum_call<R>(f: impl FnOnce() -> R) -> R {
	ethereum_flag::using(&mut (), f)
}

pub fn is_ethereum_call() -> bool {
	ethereum_flag::with(|v| *v).is_some()
}
