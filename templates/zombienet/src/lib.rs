use zombienet_sdk::{NetworkConfigBuilder, NetworkConfig};
use anyhow::anyhow;
use std::{env, fs};

pub fn get_config(cmd: &str, para_cmd: Option<&str>) -> Result<NetworkConfig, anyhow::Error> {
    let chain = if cmd == "polkadot" { "rococo-local" } else { "dev" };
    let config = NetworkConfigBuilder::new()
    .with_relaychain(|r| {
        r.with_chain(chain)
            .with_default_command(cmd)
            .with_node(|node| node.with_name("alice"))
            .with_node(|node| node.with_name("bob"))
    });

    let config = if let Some(para_cmd) = para_cmd {
        config.with_parachain(|p| {
        p.with_id(1000)
            .with_default_command(para_cmd)
            .with_collator(|n| n.with_name("collator"))
        })
    } else {
        config
    };

    Ok(config.build().map_err(|e| {
        let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
        anyhow!("config errs: {errs}")
    })?)
}

pub fn requirements_are_meet(cmds: &[&str]) -> Result<(), anyhow::Error> {
    let missing = cmds_are_presents(cmds);
    if !missing.is_empty() {
        let mut msg = vec![String::from("\nBinaries requirement is not meet, please review:")];
        for cmd in missing {
            msg.push(help_msg(cmd))
        }

        msg.push(String::from("Then you need yo export your path including the compiled binaries."));
        msg.push(String::from("E.g: export PATH=<path to polkadot-sdk repo>/target/release:$PATH"));

        return Err(anyhow::anyhow!(format!("{}\n", msg.join("\n"))))
    }

    Ok(())
}

pub fn cmds_are_presents<'a>(cmds: &'a[&'a str]) -> Vec<&str> {
    let mut missing_msgs = vec![];
    if let Ok(path) = env::var("PATH") {
        let parts: Vec<_> = path.split(":").collect();
        for cmd in cmds {
            if !parts.iter().any(|part| { fs::metadata(format!("{}/{}", part, cmd)).is_ok()}) {
                missing_msgs.push(*cmd);
            }
        }
    } else {
        log::warn!("PATH not set");
        return cmds.to_vec();
    }

    missing_msgs
}

fn help_msg(cmd: &str) -> String {
    match cmd {
        "parachain-template-node" | "solochain-template-node" | "minimal-template-node" => {
            format!("compile {cmd} by running: \n\tcargo build --package {cmd} --release")
        },
        "polkadot" => {
            format!("compile {cmd} by running: \n\t cargo build --locked --release --features fast-runtime --bin {cmd} --bin polkadot-prepare-worker --bin polkadot-execute-worker")
        },
        _ => {
            format!("unknown command {cmd}, please verify config.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn err_msg() {
        let cmds = vec!["ls", "other"];
        let result = requirements_are_meet(&cmds);
        assert!(result.is_err())
    }

    #[test]
    fn cmds_are_presents_works() {
        let cmds = vec!["ls"];
        let missing = cmds_are_presents(&cmds);
        assert!(missing.is_empty())
    }

    #[test]
    fn cmds_are_presents_detect_missing() {
        let cmds = vec!["ls", "other"];
        let missing = cmds_are_presents(&cmds);
        assert_eq!(missing, vec!["other"]);
    }
}