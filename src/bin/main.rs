use clap::{Parser, Subcommand};
use tokio::io::{stdin, stdout};
use tower_lsp::{LspService, Server};
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
        None => {
            // Start LSP server (backward compatible default behavior)
            let stdin = stdin();
            let stdout = stdout();

            let (service, socket) = LspService::new(TreeSitterLs::new);
            Server::new(stdin, stdout, socket).serve(service).await;
        }
    }
}
