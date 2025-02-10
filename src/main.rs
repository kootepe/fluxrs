use std::error::Error;
use std::fs::File;
use std::path::Path;

fn read_csv<P: AsRef<Path>>(filename: P) -> Result<(), Box<dyn Error>> {
fn read_csv<P: AsRef<Path>>(filename: P) -> Result<DataFrame, Box<dyn Error>> {
    let file = File::open(filename)?;
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .skip_rows(5)
        .from_reader(file);

    for result in rdr.records() {
        let record = result?;
        println!("{:?}", record);
    }

    Ok(())
}

fn main() {
    let fpath =
        "/home/eerokos/code/python/autochambers/flux_calc/fluxObject/data/gas_data/240126.DAT";

    let df = match read_csv(fpath) {
        Ok(res) => Some(res),
        Err(err) => {
            println!("Crashed with {}", err);
            None
        }
    };
    if let Some(df) = &df {
        let s = df.fsecs.clone();
        let gas = df.gas.clone();
    }
}
