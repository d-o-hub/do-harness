//! Command-line argument parsing for the build-log filter.

use std::fs;
use std::path::PathBuf;

use crate::parser::Severity;

/// Filtering options shared by every invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    /// Minimum severity retained in the report.
    pub min: Severity,
    /// Optional exact component name.
    pub component: Option<String>,
    /// Optional case-insensitive message substring.
    pub message: Option<String>,
}

/// Parsed invocation options.
#[derive(Debug, Clone)]
pub struct Options {
    pub help: bool,
    pub filter: Filter,
    /// Optional input file; stdin is used when absent.
    pub file: Option<PathBuf>,
}

pub const USAGE: &str = "\
Usage: buildlog-cli [--min-severity INFO|WARN|ERROR] [--component NAME]
                     [--message SUBSTR] [--file PATH]
Filter `[INFO|WARN|ERROR] component: message` lines and print a JSON report.
--help  Print this usage text and exit 0.
";

fn parse_args<I: Iterator<Item = String>>(args: I) -> Result<Options, String> {
    let mut args = args.peekable();
    let mut filter = Filter {
        min: Severity::Info,
        component: None,
        message: None,
    };
    let mut file = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Options { help: true, filter, file }),
            "--min-severity" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--min-severity requires a value".to_owned())?;
                filter.min = Severity::parse(&value)
                    .ok_or_else(|| format!("unknown severity: {value}"))?;
            }
            "--component" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--component requires a value".to_owned())?;
                filter.component = Some(value);
            }
            "--message" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--message requires a value".to_owned())?;
                filter.message = Some(value);
            }
            "--file" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--file requires a value".to_owned())?;
                file = Some(PathBuf::from(value));
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option: {other}"));
            }
            other => return Err(format!("unexpected positional argument: {other}")),
        }
    }
    Ok(Options {
        help: false,
        filter,
        file,
    })
}

impl Options {
    pub fn parse<I: Iterator<Item = String>>(args: I) -> Result<Options, String> {
        parse_args(args)
    }

    pub fn read_input(&self) -> Result<String, String> {
        match &self.file {
            Some(path) => fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display())),
            None => {
                use std::io::Read;
                let mut buffer = String::new();
                std::io::stdin()
                    .read_to_string(&mut buffer)
                    .map_err(|err| format!("stdin: {err}"))?;
                Ok(buffer)
            }
        }
    }
}

