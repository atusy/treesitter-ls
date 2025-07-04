use tokio::io::{stdin, stdout};
use tower_lsp::{LspService, Server};
use treesitter_ls::TreeSitterLs;

#[tokio::main]
async fn main() {
    let stdin = stdin();
    let stdout = stdout();

    let (service, socket) = LspService::new(TreeSitterLs::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
