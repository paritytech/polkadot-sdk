use std::io::Write;
use std::fs::File;
use std::env;
use std::path;
use std::path::Path;
use std::process::Command;

fn replace_dashes(k: &str) -> String {
    k.replace("-", "_")
}

fn make_env_key(k: &str) -> String {
    replace_dashes(&k.to_ascii_uppercase())
}

fn find_wasm(chain: &str, mut f: &File) -> Option<String> {
    const PROFILES: [&str; 2] = ["release", "testnet"];
    let manifest_path = env::var("CARGO_WORKSPACE_ROOT_DIR").unwrap();
    let manifest_path = manifest_path.strip_suffix("/").unwrap();
    write!(f, "manifest_path is  : {}\n", manifest_path);
    let package = format!("{chain}-runtime");
    let profile = PROFILES.into_iter().find(|p| {
        let full_path = format!("{}/target/{}/wbuild/{}/{}.wasm", manifest_path, p, &package, replace_dashes(&package));
        write!(f, "checking wasm at : {}\n", full_path);
        match path::PathBuf::from(&full_path).try_exists() {
            Ok(true) => true,
            _ => false
        }
    });

    write!(f, "profile is : {:?}\n", profile);
    if let Some(profile) = profile {
        Some(format!("{}/target/{}/wbuild/{}/{}.wasm", manifest_path, profile, &package, replace_dashes(&package)))
    } else {
        None
    }
}

// based on https://gist.github.com/s0me0ne-unkn0wn/bbd83fe32ce10327086adbf13e750eec
fn build_wasm(chain: &str) -> String {
    let package = format!("{chain}-runtime");

	let cargo = env::var("CARGO").unwrap();
	let target = env::var("TARGET").unwrap();
	let out_dir = env::var("OUT_DIR").unwrap();
	let target_dir = format!("{}/runtimes", out_dir);
 	let args = vec!["build", "-p", &package, "--profile", "release", "--target", &target, "--target-dir", &target_dir];
	Command::new(cargo).args(&args).status().unwrap();

    format!("{target_dir}/{}.wasm", replace_dashes(&package))
}

fn generate_metadata_file(wasm_path: &str) {

}

fn fetch_metadata_file(chain: &str, output_path: &Path, mut f: &File) {
    // First check if we have an explicit path to use
    let env_key = format!("{}_METADATA_FILE", make_env_key(chain));

    if let Ok(path_to_use) = env::var(env_key) {
        write!(f, "metadata file to use (from env): {}\n", path_to_use);
        // fs copy
    } else if let Some(exisiting_wasm) = find_wasm(chain, f) {
        write!(f, "exisiting wasm: {}\n", exisiting_wasm);
        // generate metadata
        generate_metadata_file(&exisiting_wasm);
    } else {
        // build it
        let wasm_path = build_wasm(chain);
        write!(f, "created wasm: {}\n", wasm_path);
        // genetate metadata
        generate_metadata_file(&wasm_path);
    }
}


fn main() {
    // Ensure we have the needed metadata files in place to run zombienet tests
    let manifest_path = env::var("CARGO_MANIFEST_DIR").unwrap();
    const metadata_dir: &str = "metadata-files";
    const chains: [&str; 2] = ["rococo", "coretime-rococo"];


    let mut f = std::fs::File::create("/tmp/v.txt").unwrap();

    for chain in chains {
        let full_path = format!("{manifest_path}/{metadata_dir}/{chain}-local.scale");
        let output_path = path::PathBuf::from(&full_path);

        match output_path.try_exists() {
            Ok(true) => { write!(f,"got: {}\n", full_path);    },
            _ => {
                write!(f,"needs: {}\n", full_path);
                fetch_metadata_file(chain, &output_path, &f);
            }
        };
    }

    //CARGO_MANIFEST_DIR
    // if let Ok(_) = sdt::env::var("RUN_ZOMBIENET_METADATA_TEST") else {
    //     return
    // }




    // // CARGO_TARGET_TMPDIR
    // if let Ok(tmp_dir) = std::env::var("PROFILE") {
    //     write!(f, "In integration test, tmp_dir {}", tmp_dir).unwrap();
    // } else {
    //     write!(f, "No in test, noop").unwrap();
    // }

    for (key, value) in std::env::vars() {
        write!(f, "{}: {}\n", key, value).unwrap();
    }
}
