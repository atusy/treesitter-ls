use clap::{Parser, Subcommand};
use std::path::PathBuf;
use treesitter_ls::install::{default_data_dir, metadata, parser, queries};

/// A Language Server Protocol (LSP) server using Tree-sitter for parsing
#[derive(Parser)]
#[command(name = "treesitter-ls")]
#[command(version)]
#[command(about = "A Language Server Protocol (LSP) server using Tree-sitter for parsing")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage language parsers and queries
    Language {
        #[command(subcommand)]
        action: LanguageAction,
    },
    /// Manage configuration files
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum LanguageAction {
    /// Install a Tree-sitter parser and its queries for a language
    Install {
        /// The language to install (e.g., lua, rust, python)
        language: String,

        /// Custom data directory (default: ~/.local/share/treesitter-ls on Linux)
        #[arg(long)]
        data_dir: Option<PathBuf>,

        /// Overwrite existing files if they exist
        #[arg(long)]
        force: bool,

        /// Print verbose output
        #[arg(long, short)]
        verbose: bool,

        /// Bypass the metadata cache and fetch fresh data from network
        #[arg(long)]
        no_cache: bool,
    },
    /// List supported languages for installation
    List {
        /// Bypass the metadata cache and fetch fresh data from network
        #[arg(long)]
        no_cache: bool,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Generate a default configuration file
    Init {
        /// Overwrite existing configuration file if it exists
        #[arg(long)]
        force: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Language { action }) => match action {
            LanguageAction::Install {
                language,
                data_dir,
                force,
                verbose,
                no_cache,
            } => {
                run_install(&language, data_dir, force, verbose, no_cache);
            }
            LanguageAction::List { no_cache } => {
                run_list_languages(no_cache);
            }
        },
        Some(Commands::Config { action }) => match action {
            ConfigAction::Init { force } => {
                run_config_init(force);
            }
        },
        None => {
            // Start LSP server (backward compatible default behavior)
            // Only create tokio runtime for LSP mode to avoid conflicts with reqwest::blocking
            run_lsp_server();
        }
    }
}

/// Run the list-languages command
fn run_list_languages(no_cache: bool) {
    let data_dir = default_data_dir();
    let options = metadata::FetchOptions {
        data_dir: data_dir.as_deref(),
        use_cache: !no_cache,
    };

    if no_cache {
        eprintln!("Fetching supported languages from nvim-treesitter (cache bypassed)...");
    } else {
        eprintln!("Fetching supported languages from nvim-treesitter...");
    }

    match metadata::list_supported_languages(Some(&options)) {
        Ok(languages) => {
            eprintln!("Supported languages ({} total):", languages.len());
            for lang in languages {
                println!("  {}", lang);
            }
        }
        Err(e) => {
            eprintln!("Failed to fetch language list: {}", e);
            std::process::exit(1);
        }
    }
}

/// Default configuration template
const CONFIG_TEMPLATE: &str = r#"# treesitter-ls configuration
# Documentation: https://github.com/atusy/treesitter-ls/blob/main/docs/README.md

# Auto-install missing parsers and queries (default: true)
# autoInstall = true

# Custom search paths for parsers and queries
# Default: ~/.local/share/treesitter-ls (Linux), ~/Library/Application Support/treesitter-ls (macOS)
# searchPaths = ["/custom/path"]

# Language-specific configuration (usually not needed)
# [languages.custom_lang]
# filetypes = ["ext"]
# library = "/path/to/parser.so"
# highlight = [{ path = "./queries/highlights.scm" }]

# Capture name remapping (Tree-sitter -> LSP semantic tokens)
# [captureMappings._.highlights]
# "variable.builtin" = "variable.defaultLibrary"
"#;

/// Run the config init command
fn run_config_init(force: bool) {
    let config_path = PathBuf::from("treesitter-ls.toml");

    if config_path.exists() && !force {
        eprintln!(
            "Error: Configuration file '{}' already exists.",
            config_path.display()
        );
        eprintln!("Use --force to overwrite.");
        std::process::exit(1);
    }

    match std::fs::write(&config_path, CONFIG_TEMPLATE) {
        Ok(()) => {
            eprintln!("Created configuration file: {}", config_path.display());
        }
        Err(e) => {
            eprintln!("Failed to write configuration file: {}", e);
            std::process::exit(1);
        }
    }
}

/// Run the install command (synchronous - no tokio runtime)
fn run_install(
    language: &str,
    data_dir: Option<PathBuf>,
    force: bool,
    verbose: bool,
    no_cache: bool,
) {
    let data_dir = data_dir.or_else(default_data_dir).unwrap_or_else(|| {
        eprintln!("Error: Could not determine data directory. Please specify --data-dir.");
        std::process::exit(1);
    });

    // Track success/failure for exit code
    let mut parser_success = true;
    let mut queries_success = true;

    // Install parser
    eprintln!("Installing parser for '{}' to {:?}...", language, data_dir);

    let options = parser::InstallOptions {
        data_dir: data_dir.clone(),
        force,
        verbose,
        no_cache,
    };

    match parser::install_parser(language, &options) {
        Ok(result) => {
            eprintln!("✓ Parser installed: {}", result.install_path.display());
            if verbose {
                eprintln!("  Revision: {}", result.revision);
            }
        }
        Err(e) => {
            eprintln!("✗ Parser installation failed: {}", e);
            parser_success = false;
        }
    }

    // Install queries (with inherited dependencies)
    eprintln!("Installing queries for '{}' to {:?}...", language, data_dir);

    match queries::install_queries_with_dependencies(language, &data_dir, force) {
        Ok(result) => {
            eprintln!("✓ Queries installed: {}", result.install_path.display());
            if verbose {
                eprintln!("  Files: {}", result.files_downloaded.join(", "));
            }
        }
        Err(e) => {
            eprintln!("✗ Query installation failed: {}", e);
            queries_success = false;
        }
    }

    // Summary
    if parser_success && queries_success {
        eprintln!("\nSuccessfully installed '{}' language support.", language);
    } else if !parser_success && !queries_success {
        eprintln!("\nFailed to install '{}' language support.", language);
        std::process::exit(1);
    } else {
        eprintln!("\nPartially installed '{}' language support.", language);
        std::process::exit(1);
    }
}

/// Run the LSP server (requires tokio runtime)
#[tokio::main]
async fn run_lsp_server() {
    use tokio::io::{stdin, stdout};
    use tower_lsp::{LspService, Server};
    use treesitter_ls::lsp::TreeSitterLs;

    let stdin = stdin();
    let stdout = stdout();

    let (service, socket) = LspService::new(TreeSitterLs::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
