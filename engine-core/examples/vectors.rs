//! Prints canonical cross-implementation vectors. The TypeScript SDK tests assert
//! these exact values.
//!
//! Run: `cargo run --example vectors`

use loom_engine_core::addressing::{component_address, to_hex};
use loom_engine_core::hash::fnv1a;

fn main() {
    println!("fnv1a(\"loom\")        = {:#018x}", fnv1a(b"loom"));
    println!(
        "componentAddress(42,3,1) = {}",
        to_hex(&component_address(42, 3, 1))
    );
    println!(
        "componentAddress(1,1,0)  = {}",
        to_hex(&component_address(1, 1, 0))
    );
}
