use std::path::Path;

use anyhow::{Context, Result, anyhow};
use clap::{CommandFactory, Parser, error::ErrorKind};
use regolith_config::{
    FullConfig,
    cli_args::{self, CLIArguments, Session},
    get_session_type, get_trawl_resources, search_config,
};

fn main() -> Result<()> {
    let args = CLIArguments::parse();

    let session = match args.session() {
        Some(session) => session,

        // If session not explictly defined, infer it from $XDG_SESSION_TYPE
        None => match get_session_type() {
            Some(curr_session) => curr_session,
            None => {
                // Indicate the mandatory usage of --session when a valid
                // $XDG_SESSION_TYPE is not available
                let cmd = CLIArguments::command();
                let expected_usage = format!(
                    "{} --session=<SESSION> [OPTIONS] <COMMAND>",
                    env!("CARGO_PKG_NAME")
                );
                cmd.override_usage(expected_usage)
                    .error(
                        ErrorKind::MissingRequiredArgument,
                        "$XDG_SESSION_TYPE is not defined. Please use the --session \
                        flag to specify the session for which you want to perform this operation.",
                    )
                    .exit()
            }
        },
    };

    let session_mappings = [
        (Session::X11, Path::new("/etc/regolith/i3/config")),
        (Session::Wayland, Path::new("/etc/regolith/sway/config")),
    ];
    let wm_config = FullConfig::new_from_session(session, &session_mappings)?;

    let trawl_resources = get_trawl_resources().context("Failed to get Trawl resources")?;
    let result = match args.sub_command() {
        cli_args::OperationType::Search(search_args) => {
            search_config(search_args, &wm_config, &trawl_resources)
        }

        cli_args::OperationType::Eject(_eject_args) => todo!(),
        cli_args::OperationType::Reconcile { .. } => todo!(),
    }
    .ok_or(anyhow!("Operation did not return any result"))?;

    println!("{}", result);
    Ok(())
}
