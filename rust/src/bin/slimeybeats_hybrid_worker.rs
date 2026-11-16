//! One private server command per process. See hybrid_worker/README.md.
#[path = "../hybrid_worker/mod.rs"]
mod hybrid_worker;

fn main() {
    if let Err(error) = hybrid_worker::run() {
        eprintln!("Hybrid worker failed: {error:#}");
        std::process::exit(2);
    }
}
