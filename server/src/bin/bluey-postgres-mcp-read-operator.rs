use std::ffi::OsString;

use bluey_server::db::jobs_mcp_operator::{check_configured_target, source_id, CheckedState};

#[derive(Debug, Eq, PartialEq)]
enum Command {
    SourceId,
    Check,
    MutationDisabled,
}

fn command(args: &[OsString]) -> Result<Command, &'static str> {
    let [_, operation] = args else {
        return Err("MCP operator accepts exactly one command");
    };
    match operation.to_str() {
        Some("source-id") => Ok(Command::SourceId),
        Some("check") => Ok(Command::Check),
        Some("install-v1" | "disable-v1") => Ok(Command::MutationDisabled),
        _ => Err("unsupported MCP operator command"),
    }
}

fn main() -> anyhow::Result<()> {
    match command(&std::env::args_os().collect::<Vec<_>>()).map_err(anyhow::Error::msg)? {
        Command::SourceId => println!("{}", source_id()?),
        Command::MutationDisabled => anyhow::bail!("MCP operator mutation is not enabled"),
        Command::Check => {
            if !std::env::var("BLUEY_JOBS_MCP_POSTGRES_OPERATOR_ENABLED")
                .is_ok_and(|gate| gate == "true")
            {
                anyhow::bail!("MCP operator check is disabled");
            }
            let target = std::env::var("BLUEY_JOBS_MCP_POSTGRES_OPERATOR_TARGET")
                .map_err(|_| anyhow::anyhow!("MCP operator target is required"))?;
            let url = std::env::var("BLUEY_JOBS_MCP_POSTGRES_OPERATOR_URL")
                .map_err(|_| anyhow::anyhow!("MCP operator database configuration is required"))?;
            let state = match check_configured_target(&target, &url)? {
                CheckedState::Absent => "absent",
                CheckedState::ExactInstalled => "exact-installed",
            };
            println!("MCP read-only check passed: {state}; mutation is disabled");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fixed_commands_and_closed_mutation_refusals() {
        for (name, value) in [
            ("source-id", Command::SourceId),
            ("check", Command::Check),
            ("install-v1", Command::MutationDisabled),
            ("disable-v1", Command::MutationDisabled),
        ] {
            assert_eq!(command(&["mcp".into(), name.into()]), Ok(value));
        }
        for other in ["CHECK", "install-sql", "check 052", "", "check.sql"] {
            assert!(command(&["mcp".into(), other.into()]).is_err());
        }
        assert!(command(&["mcp".into(), "check".into(), "extra".into()]).is_err());
    }
}
