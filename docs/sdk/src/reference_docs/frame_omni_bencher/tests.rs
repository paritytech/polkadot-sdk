use serde_json::{json, Value};
use std::{process::Command, str};

const WASM_FILE_PATH: &str =
	"../../../../../target/release/wbuild/westend-runtime/westend-runtime-compact-compressed.wasm";