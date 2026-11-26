use std::ffi::OsString;

use bluey_server::db::operator::{
    compiled_operator_source_identity, run_selected_audience_command,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    SourceId,
    Check,
    Migration(&'static str),
}

fn parse_command(args: &[OsString]) -> Result<Command, &'static str> {
    if args.len() != 2 {
        return Err("operator accepts exactly one command");
    }
    match args[1].to_str() {
        Some("source-id") => Ok(Command::SourceId),
        Some("check") => Ok(Command::Check),
        Some("036") => Ok(Command::Migration("036")),
        Some("040") => Ok(Command::Migration("040")),
        Some("045") => Ok(Command::Migration("045")),
        Some("049") => Ok(Command::Migration("049")),
        _ => Err("unsupported operator command"),
    }
}

fn main() -> anyhow::Result<()> {
    let args = std::env::args_os().collect::<Vec<_>>();
    let command = parse_command(&args).map_err(anyhow::Error::msg)?;
    if command == Command::SourceId {
        // This branch intentionally precedes every environment and DB read.
        println!("{}", compiled_operator_source_identity());
        return Ok(());
    }

    let token = match command {
        Command::Check => None,
        Command::Migration(value) => Some(value),
        Command::SourceId => unreachable!("source-id returned before database setup"),
    };
    let ledger = std::env::var("BLUEY_POSTGRES_MIGRATION_TABLE")
        .map_err(|_| anyhow::anyhow!("selected migration ledger is required"))?;
    let url = std::env::var("BLUEY_DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("database configuration is required"))?;
    run_selected_audience_command(&url, &ledger, token).map_err(|_| {
        anyhow::anyhow!("operator command failed; outcome may be unknown; no retry performed")
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_command, Command};
    use std::ffi::OsString;

    #[test]
    fn exact_closed_commands_parse_without_environment() {
        for (value, expected) in [
            ("source-id", Command::SourceId),
            ("check", Command::Check),
            ("036", Command::Migration("036")),
            ("040", Command::Migration("040")),
            ("045", Command::Migration("045")),
            ("049", Command::Migration("049")),
        ] {
            assert_eq!(
                parse_command(&["operator".into(), value.into()]),
                Ok(expected)
            );
        }
    }

    #[test]
    fn wrong_count_and_unrecognized_commands_refuse() {
        for args in [
            vec![],
            vec!["operator".into()],
            vec!["operator".into(), "source-id".into(), "extra".into()],
            vec!["operator".into(), "049.sql".into()],
            vec!["operator".into(), "SOURCE-ID".into()],
            vec!["operator".into(), "".into()],
        ] {
            assert!(parse_command(&args).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_command_refuses() {
        use std::os::unix::ffi::OsStringExt;

        assert!(parse_command(&["operator".into(), OsString::from_vec(vec![0xff]),]).is_err());
    }
}
