use environmental::environmental;

environmental!(ethereum_flag: bool);

pub fn using<R>(val: bool, f: impl FnOnce() -> R) -> R {
	ethereum_flag::using(&mut val.clone(), f)
}

pub fn is_ethereum_call() -> bool {
	ethereum_flag::with(|v| *v).unwrap_or(false)
}
