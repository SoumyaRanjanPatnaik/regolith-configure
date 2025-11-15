use clap::{CommandFactory, Parser, error::ErrorKind};
use regolith_config::{
    cli_args::{self, CLIArguments},
    eject_config, get_session_type, search_config,
};

fn main() {
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

    match args.sub_command() {
        cli_args::OperationType::Search(search_args) => search_config(search_args, session),
        cli_args::OperationType::Eject(eject_args) => eject_config(eject_args, session),
        cli_args::OperationType::Reconcile { name } => todo!(),
    }
}
