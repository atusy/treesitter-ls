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

        /// Only install queries, skip parser compilation
        #[arg(long)]
        queries_only: bool,

        /// Only install parser, skip query download
        #[arg(long)]
        parser_only: bool,
    },
    /// List supported languages for installation
    ListLanguages,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Install {
            language,
            data_dir,
            force,
            verbose,
            queries_only,
            parser_only,
        }) => {
            run_install(
                &language,
                data_dir,
                force,
                verbose,
                queries_only,
                parser_only,
            );
        }
        Some(Commands::ListLanguages) => {
            eprintln!("Supported languages:");
            for lang in metadata::list_supported_languages() {
                println!("  {}", lang);
            }
        }
        None => {
            // Start LSP server (backward compatible default behavior)
            // Only create tokio runtime for LSP mode to avoid conflicts with reqwest::blocking
            run_lsp_server();
        }
    }
}

/// Run the install command (synchronous - no tokio runtime)
fn run_install(
    language: &str,
    data_dir: Option<PathBuf>,
    force: bool,
    verbose: bool,
    queries_only: bool,
    parser_only: bool,
) {
    let data_dir = data_dir.or_else(default_data_dir).unwrap_or_else(|| {
        eprintln!("Error: Could not determine data directory. Please specify --data-dir.");
        std::process::exit(1);
    });

    // Track success/failure for exit code
    let mut parser_success = true;
    let mut queries_success = true;

    // Install parser (unless --queries-only)
    if !queries_only {
        eprintln!("Installing parser for '{}' to {:?}...", language, data_dir);

        let options = parser::InstallOptions {
            data_dir: data_dir.clone(),
            force,
            verbose,
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
    }

    // Install queries (unless --parser-only)
    if !parser_only {
        eprintln!("Installing queries for '{}' to {:?}...", language, data_dir);

        match queries::install_queries(language, &data_dir, force) {
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
