fn main() {
    let code = match torii::app::run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Error: {error}");
            error.exit_code()
        }
    };
    std::process::exit(code);
}
