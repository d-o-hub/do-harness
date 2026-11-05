//! BUGGY VARIANT (scenario: multi-file filter fix): passes the full parsed set
//! into `summary::report` so its planted counts-from-all bug engages. Used only
//! by the adopter corpus together with `adopter-summary-all-counts.rs`.

mod cli;
mod filter;
mod parser;
mod summary;

use std::process::ExitCode;

fn main() -> ExitCode {
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
    println!("{}", summary::report(&retained, &records));
    ExitCode::SUCCESS
}
