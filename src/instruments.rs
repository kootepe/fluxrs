pub struct Instrument {
    skiprows: i64,
    skip_after_header: i64,
    time_col: String,
    gas_cols: Vec<String>,
    diag_col: String,
    has_header: bool,
}

pub struct Li7810 {
    pub base: Instrument, // ✅ Composition: LI_7810 contains an Instrument
}

impl Instrument {
    pub fn mk_rdr<P: AsRef<Path>>(&self, filename: P) -> Result<csv::Reader<File>, Box<dyn Error>> {
        let file = File::open(filename)?;
        let rdr = csv::ReaderBuilder::new()
            .delimiter(b'\t')
            .has_headers(true)
            .flexible(true)
            .from_reader(file);
        Ok(rdr)
    }
}
impl Default for Li7810 {
    fn default() -> Self {
        Self {
            base: Instrument {
                skiprows: 4,
                skip_after_header: 1,
                time_col: "SECONDS".to_string(),
                gas_cols: vec!["CO2".to_string(), "CH4".to_string(), "H2O".to_string()],
                diag_col: "DIAG".to_string(),
                has_header: true,
            },
        }
    }
}

impl Li7810 {
    pub fn mk_rdr<P: AsRef<Path>>(&self, filename: P) -> Result<csv::Reader<File>, Box<dyn Error>> {
        self.base.mk_rdr(filename)
    }
}
}
