use std::error::Error;
use std::fs::File;
use std::path::Path;

struct LinReg {
    intercept: f64,
    slope: f64,
}

impl LinReg {
    fn calculate(&self, x: f64) -> f64 {
        self.intercept + self.slope * x
    }

    fn train(input: &[(f64, f64)]) -> Self {
        let x: Vec<f64> = input.iter().map(|pairs| pairs.0).collect();
        let y: Vec<f64> = input.iter().map(|pairs| pairs.1).collect();

        let avg_x: f64 = x.iter().sum::<f64>() / x.len() as f64;
        let x_differences_to_average: Vec<f64> = x.iter().map(|value| avg_x - value).collect();
        let x_differences_to_average_squared: Vec<f64> = x_differences_to_average
            .iter()
            .map(|value| value.powi(2))
            .collect();
        let ss_xx: f64 = x_differences_to_average_squared.iter().sum();

        let avg_y = y.iter().sum::<f64>() / y.len() as f64;
        let y_differences_to_average: Vec<f64> = y.iter().map(|value| avg_y - value).collect();
        let x_and_y_differences_multiplied: Vec<f64> = x_differences_to_average
            .iter()
            .zip(y_differences_to_average.iter())
            .map(|(a, b)| a * b)
            .collect();
        let ss_xy: f64 = x_and_y_differences_multiplied.iter().sum();
        let slope = ss_xy / ss_xx;
        let intercept = avg_y - slope * avg_x;

        Self { intercept, slope }
    }
}

#[derive(Debug)]
struct DataFrame {
    header: StringRecord,
    datetime: Vec<DateTime<Utc>>,
    secs: Vec<u64>,
    fsecs: Vec<f64>,
    nsecs: Vec<u64>,
    gas: Vec<f64>,
    diag: Vec<u32>,
}

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
    let mut gas: Vec<f64> = Vec::new();
    let mut diag: Vec<u32> = Vec::new();
    let mut date: Vec<String> = Vec::new();
    let mut time: Vec<String> = Vec::new();
    //let mut ntime: Vec<f64> = Vec::new();
    let mut fsecs: Vec<f64> = Vec::new();
    let mut secs: Vec<u64> = Vec::new();
    let mut nsecs: Vec<u64> = Vec::new();
    let mut header = csv::StringRecord::new();

    for (i, r) in rdr.records().enumerate() {
        let record: &csv::StringRecord = &r?;

        if i == 0 {
            header = record.clone();
            continue;
        }
        if i == 1 {
            continue;
        }
        date.push(record[6].to_string());
        time.push(record[7].to_string());

        if let Ok(val) = record[9].parse::<f64>() {
            gas.push(val)
        }
        if let Ok(val) = record[4].parse::<u32>() {
            diag.push(val)
        }

        if let Ok(val) = record[1].parse::<u64>() {
            secs.push(val)
        }
        if let Ok(val) = record[2].parse::<u64>() {
            nsecs.push(val)
        }
        if let Ok(val) = record[2].parse::<f64>() {
            fsecs.push(val)
        }
    }

    let datetime: Vec<DateTime<Utc>> = secs
        .iter()
        .zip(nsecs.iter())
        .map(|(&sec, &nsec)| {
            let d = UNIX_EPOCH + Duration::from_secs(sec) + Duration::from_nanos(nsec);
            DateTime::<Utc>::from(d) // Convert to DateTime<Utc>
        })
        .collect();
    let df = DataFrame {
        header,
        datetime,
        secs,
        fsecs,
        nsecs,
        gas,
        diag,
    };
    Ok(df)
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
