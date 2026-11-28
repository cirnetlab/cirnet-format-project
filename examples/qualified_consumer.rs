//! Run a standalone Project Format consumer with test-only domain codecs.

#[cfg(feature = "native")]
#[path = "qualified_consumer/workflow.rs"]
mod workflow;

#[cfg(feature = "native")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .ok_or("supply a new directory to hold the qualification outputs")?;
    workflow::run(std::path::Path::new(&output))?;
    println!("PASS standalone transaction, migration, recovery and package workflow");
    Ok(())
}

#[cfg(not(feature = "native"))]
fn main() {
    eprintln!("Enable the native feature to run the qualification example.");
}
