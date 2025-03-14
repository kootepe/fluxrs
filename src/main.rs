use std::env;
use std::process;

use fluxrs::myapp;
use fluxrs::Config;

fn main() -> eframe::Result {
    // fn main() -> Result<()> {
    let inputs = env::args();
    let config = Config::build(inputs).unwrap_or_else(|err| {
        println!("Parsing problem {err}");
        process::exit(1)
    });

    // NOTE: I dont think this error will ever happen since they are being handled in run?
    // if let Err(e) = fluxrs::run(config) {
    //     println!("App error: {e}.")
    // }

    match fluxrs::query::initiate_tables() {
        Ok(_) => println!("Successfully initiated db tables"),
        Err(e) => println!("Err:\n {}", e),
    }
    // let mut data = fluxrs::run(config).unwrap();

    let app = myapp::MyApp::new();
    eframe::run_native("fluxrs", Default::default(), Box::new(|_cc| Ok(Box::new(app))))
}
