//! Operant CLI (C14): run | compile | dry-run | list | install | bench | doctor | explain.
//! L13A implements the verbs. Scaffold prints usage and exits 0 so the workspace links a binary.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") | Some("-V") => println!("operant 1.0.0"),
        Some(verb) => {
            eprintln!("operant: verb '{verb}' not yet implemented in this build");
        }
        None => {
            println!("operant 1.0.0");
            println!("usage: operant <run|compile|dry-run|list|install|bench|doctor|explain> [args]");
        }
    }
}
