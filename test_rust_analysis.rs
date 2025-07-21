use tokio::io::stdin;

fn main() {
    // Variable assignment from function call
    let stdin = stdin();
    
    // Variable reference
    println!("{:?}", stdin);
}