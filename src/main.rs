use std::path::Path;

use anyhow::{Result, anyhow};
use clap::{CommandFactory, Parser, error::ErrorKind};
use regolith_configure::{
    FullConfig,
    cli_args::{self, CLIArguments, Session},
    execute_search, get_session_type,
    resources::{ResourceProvider, TrawlResourceProvider, XrdbResourceProvider},
    set_user_xresource,
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
                    "{} [--session=<SESSION>] [OPTIONS] <COMMAND>",
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
    let wm_config = FullConfig::load_for_session(session, &session_mappings)?;

    let provider: &dyn ResourceProvider = match session {
        Session::Wayland => &TrawlResourceProvider,
        Session::X11 => &XrdbResourceProvider,
    };

    let result = match args.sub_command() {
        cli_args::OperationType::Search(search_args) => execute_search(
            search_args.filter(),
            search_args.pattern(),
            &wm_config,
            provider,
        )
        .map(|r| r.format(args.output_mode())),
        cli_args::OperationType::Eject(_eject_args) => todo!(),
        cli_args::OperationType::Reconcile { .. } => todo!(),
        cli_args::OperationType::SetResource(set_args) => {
            let path = set_user_xresource(set_args.resource(), set_args.value())?;
            Some(format!(
                "Successfully set '{}' to '{}' in {}",
                set_args.resource(),
                set_args.value(),
                path.display()
            ))
        }
    }
    .ok_or(anyhow!("Operation did not return any result"))?;

    println!("{}", result);
    Ok(())
}
