//! Phantom browser bootstrap executable.

#![forbid(unsafe_code)]

use phantom_core::BuildInfo;
use phantom_engine::Engine;

fn main() {
    let build = BuildInfo::current();
    let engine = Engine::new();

    println!(
        "{} {} — foundation engine state: {:?}",
        build.product,
        build.version,
        engine.state()
    );
}
