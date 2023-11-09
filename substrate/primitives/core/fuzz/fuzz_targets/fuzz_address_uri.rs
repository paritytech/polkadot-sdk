#![no_main]

extern crate libfuzzer_sys;
extern crate regex;
extern crate sp_core;

use libfuzzer_sys::fuzz_target;
use regex::Regex;
use sp_core::crypto::AddressUri;

lazy_static::lazy_static! {
    static ref SS58_REGEX: Regex = Regex::new(r"^(?P<ss58>[a-zA-Z0-9 ]+)?(?P<path>(//?[^/]+)*)$")
        .expect("constructed from known-good static value; qed");
    static ref SECRET_PHRASE_REGEX: Regex = Regex::new(r"^(?P<phrase>[a-zA-Z0-9 ]+)?(?P<path>(//?[^/]+)*)(///(?P<password>.*))?$")
        .expect("constructed from known-good static value; qed");
    static ref JUNCTION_REGEX: Regex = Regex::new(r"/(/?[^/]+)")
        .expect("constructed from known-good static value; qed");
}


fuzz_target!(|input: &str| {
    let regex_result = SECRET_PHRASE_REGEX.captures(input);
    let manual_result = AddressUri::parse(input);
    assert_eq!(regex_result.is_some(), manual_result.is_some());
    if let (Some(regex_result), Some(manual_result)) = (regex_result, manual_result) {
        assert_eq!(
            regex_result.name("phrase").map(|ss58| ss58.as_str()),
            manual_result.ss58
        );

        let mut manual_paths = manual_result
            .paths
            .iter()
            .map(|s| {
                let mut s = String::from(*s);
                s = "/".to_string() + &s;
                s
            })
            .collect::<Vec<_>>()
            .join("");

        assert_eq!(
            regex_result.name("path").unwrap().as_str().to_string(),
            manual_paths
        );
        assert_eq!(
            regex_result.name("password").map(|ss58| ss58.as_str()),
            manual_result.pass
        );
    }
});
