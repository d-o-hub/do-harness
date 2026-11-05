//! Offline build-log filtering CLI used as the Rust adopter fixture.
//!
//! Parses `[INFO|WARN|ERROR] component: message` lines, applies a minimum
//! severity plus optional exact-component and message-substring filters, and
//! emits a stable JSON report that preserves record order.

mod cli;
mod filter;
mod parser;
mod summary;

use std::process::ExitCode;

fn main() -> ExitCode {
    // This fixture CLI reads only local log paths from the pinned corpus
    // commands; argv is never spliced into a shell or a security operation.
    // nosemgrep: rust.lang.security.args.args
    let options = match cli::Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    if options.help {
        print!("{}", cli::USAGE);
        return ExitCode::SUCCESS;
    }
    let source = match options.read_input() {
        Ok(text) => text,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let records = parser::parse(&source);
    let retained = filter::apply(&records, &options.filter);
    println!("{}", summary::report(&retained));
    ExitCode::SUCCESS
}
