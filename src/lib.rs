pub mod cli_args;
use cli_args::{EjectArgs, SearchArgs, Session};
use std::env;

/// Filters packages and files related to user's search query
pub fn search_config(_args: &SearchArgs, _session: Session) {}

/// Eject the system config and copy it to the user's local config directory
pub fn eject_config(_args: &EjectArgs, _session: Session) {}

pub fn get_session_type() -> Option<Session> {
    return env::vars().find_map(|(name, value)| {
        return match name.as_str() {
            "XDG_SESSION_TYPE" => match value.as_str() {
                "wayland" => Some(Session::Wayland),
                "x11" => Some(Session::X11),
                _ => None,
            },
            _ => None,
        };
    });
}
