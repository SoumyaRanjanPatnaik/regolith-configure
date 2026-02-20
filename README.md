# Regolith Config

A command-line tool for managing and searching Regolith Linux configurations.

## Overview

Regolith Config is a Rust-based CLI tool that helps users edit, manage, and search through Regolith Linux configurations. It provides functionality to search for keybindings, resources, and configuration files across different session types (X11 and Wayland).

## Features

- **Search Configurations**: Find keybindings, resources, and configuration files
- **Session Support**: Works with both X11 and Wayland sessions
- **Config Management**: Handle configuration partials and imports
- **Resource Integration**: Integrates with Trawl D-Bus service for resource management
- **Keybinding Search**: Search and resolve keybinding definitions with variable substitution

## Installation

1. Clone the repository:

   ```bash
   git clone https://github.com/your-repo/regolith-config.git
   cd regolith-config
   ```

2. Build the project:

   ```bash
   cargo build --release
   ```

3. Install the binary (optional):
   ```bash
   sudo cp target/release/regolith-config /usr/local/bin/
   ```

## Usage

### Basic Usage

```bash
# Search for keybindings
regolith-config search --filter bindings "Super+Enter"

# Search with specific session
regolith-config --session X11 search --filter bindings "Super+D"

# Search with Wayland session
regolith-config --session Wayland search --filter bindings "Super+Enter"
```

### Commands

#### Search

Search for configurations using different filters:

```bash
regolith-config search [OPTIONS] <PATTERN>
```

Options:

- `-f, --filter <FILTER>` - Filter type: `bindings`, `keyword`, or `resource`
- `-s, --session <SESSION>` - Session type: `X11` or `Wayland` (optional if $XDG_SESSION_TYPE is set)

#### Eject

Create a copy of a config partial and disable its system instance:

```bash
regolith-config eject [OPTIONS] <PATTERN>
```

Options:

- `-f, --filter <FILTER>` - Filter type
- `-o, --output <OUTPUT>` - File to write to

#### Reconcile

Help diff and reconcile upstream configs with local versions:

```bash
regolith-config reconcile <NAME>
```

## Configuration

The tool automatically detects the session type from `$XDG_SESSION_TYPE` environment variable. If not available, you must specify the session using the `--session` flag.

Session Mappings:

- **X11**: `/etc/regolith/i3/config`
- **Wayland**: `/etc/regolith/sway/config`

## Development

### Prerequisites

- Rust 2024 edition
- Cargo package manager

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
```

### Dependencies

- `anyhow` - Error handling
- `clap` - CLI argument parsing
- `glob` - File pattern matching
- `zbus` - D-Bus communication with Trawl service

## Architecture

### Core Components

1. **FullConfig**: Represents the complete Regolith configuration including all imported partials
2. **ConfigPartial**: Represents individual configuration files with their imports
3. **CLIArguments**: Command-line argument parser
4. **Search System**: Handles different types of searches (bindings, keywords, resources)

### Key Features

- **Config Discovery**: Recursively discovers all imported configuration files
- **Variable Resolution**: Substitutes configuration variables in keybindings
- **D-Bus Integration**: Communicates with Trawl service for resource management
- **Session Awareness**: Adapts to X11 or Wayland session types

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

This project is licensed under the MIT License.

## Related Projects

- [Regolith Linux](https://regolith-linux.org/) - The desktop environment this tool supports
- [Trawl](https://github.com/regolith-linux/trawl) - The resource management service

