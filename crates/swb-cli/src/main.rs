fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let exit_code = swb_cli::run_cli(None, &args, &mut std::io::stdout(), &mut std::io::stderr());
    std::process::exit(exit_code);
}
