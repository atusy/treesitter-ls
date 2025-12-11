use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tokio::io::{stdin, stdout};
use tower_lsp::{LspService, Server};
use treesitter_ls::install::{self, queries};
use treesitter_ls::lsp::TreeSitterLs;

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
    },
    /// Download Tree-sitter query files for a language from nvim-treesitter
    InstallQueries {
        /// The language to download queries for (e.g., lua, rust, python)
        language: String,

        /// Custom data directory (default: ~/.local/share/treesitter-ls on Linux)
        #[arg(long)]
        data_dir: Option<PathBuf>,

        /// Overwrite existing queries if they exist
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Install { language }) => {
            eprintln!(
                "Error: Install command not yet implemented for language: {}",
                language
            );
            eprintln!("This feature will be available in a future release.");
            std::process::exit(1);
        }
        Some(Commands::InstallQueries {
            language,
            data_dir,
            force,
        }) => {
            let data_dir = data_dir
                .or_else(install::default_data_dir)
                .unwrap_or_else(|| {
                    eprintln!(
                        "Error: Could not determine data directory. Please specify --data-dir."
                    );
                    std::process::exit(1);
                });

            eprintln!("Installing queries for '{}' to {:?}...", language, data_dir);

            match queries::install_queries(&language, &data_dir, force) {
                Ok(result) => {
                    eprintln!("Successfully installed queries for '{}'", result.language);
                    eprintln!("Location: {}", result.install_path.display());
                    eprintln!("Files: {}", result.files_downloaded.join(", "));
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        None => {
            // Start LSP server (backward compatible default behavior)
            let stdin = stdin();
            let stdout = stdout();

            let (service, socket) = LspService::new(TreeSitterLs::new);
            Server::new(stdin, stdout, socket).serve(service).await;
        }
    }
}
