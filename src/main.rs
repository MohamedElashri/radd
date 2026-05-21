mod bench;
mod cache;
mod cli;
mod config;
mod executor;
mod hadd;
mod input;
mod inspect;
mod planner;
mod staging;
mod telemetry;
mod update;
mod validate;

use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn package_version_is_available() {
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }
}
