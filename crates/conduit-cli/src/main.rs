use std::process::ExitCode;

fn main() -> ExitCode {
    match conduit_cli::run(std::env::args().skip(1)) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {}", error.message);
            if let Some(hint) = error.hint {
                eprintln!("hint: {hint}");
            }
            ExitCode::from(error.code)
        }
    }
}
