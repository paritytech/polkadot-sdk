//! Test error when attaching the derive builder macro to something
//! other than the XCM `Instruction` enum.

use xcm_procedural::Builder;

#[derive(Builder)]
struct SomeStruct;

fn main() {}
