use std::error::Error;
use std::fs::File;
use std::path::Path;

fn read_csv<P: AsRef<Path>>(filename: P) -> Result<(), Box<dyn Error>> {
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

fn main() -> Result<(), Box<dyn Error>> {
    let fpath = "/home/eekoskin22/code/python/autochambers/flux_calc/fluxObject/data/gas_data/211026.DAT";
    read_csv(fpath)
}
