use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    println!("VisiData (Rust) — Phase 1 scaffold");
    println!("Run `cargo test --workspace` to verify the core data model.");
}
