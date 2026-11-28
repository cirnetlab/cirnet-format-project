#![cfg(feature = "native")]

#[path = "../examples/qualified_consumer/workflow.rs"]
mod workflow;

#[test]
fn qualifies_a_standalone_public_api_consumer() {
    let workspace = tempfile::tempdir().unwrap();
    workflow::run(&workspace.path().join("qualification")).unwrap();
}
