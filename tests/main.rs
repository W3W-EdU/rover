use assert_cmd::Command;
use predicates::prelude::*;

mod composition;
mod dev;
mod graphql;
mod output;
mod schema;

#[test]
fn its_executable() {
    let mut cmd = Command::cargo_bin("rover").unwrap();

    // running the CLI with no command returns the help message to stderr
    let result = cmd.assert();

    // let's make sure the help message includes the word "Rover"
    result.stderr(predicate::str::contains("Rover"));
}
