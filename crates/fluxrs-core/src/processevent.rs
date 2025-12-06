use crate::datatype::DataType;

#[derive(Debug)]
pub enum ProcessEvent {
    Query(QueryEvent),
    Read(ReadEvent),
    Insert(InsertEvent),
    Progress(ProgressEvent),
    Done(Result<(), String>),
}

#[derive(Debug)]
pub enum QueryEvent {
    InitStarted,
    InitEnded,
    HeightFail(String),
    CyclesFail(String),
    DbFail(String),
    QueryComplete,
    NoGasData(String),
    NoGasDataDay(String),
}

#[derive(Debug)]
pub enum ReadEvent {
    File(String),
    FileDetail(String, String),
    FileFail(String, String),
    DataFail { kind: DataType, file: String, reason: String },
    RowFail(String),
    FileRows(String, usize),
}

#[derive(Debug)]
pub enum InsertEvent {
    Ok(String, u64),
    DataOkSkip { kind: DataType, inserts: usize, skips: usize },
    Fail(String),
}

#[derive(Debug)]
pub enum ProgressEvent {
    DisableUI,
    EnableUI,
    CalculationStarted,
    Recalced(usize, usize),
    Generic(String),
    Day(String),
    Rows(usize, usize),
    NoGas(String),
}

pub trait ProcessEventSink {
    fn on_query_event(&mut self, ev: &QueryEvent);
    fn on_progress_event(&mut self, ev: &ProgressEvent);
    fn on_read_event(&mut self, ev: &ReadEvent);
    fn on_insert_event(&mut self, ev: &InsertEvent);
    fn on_done(&mut self, res: &Result<(), String>);
}

impl ReadEvent {
    pub fn gas_fail(file: impl AsRef<std::path::Path>, reason: impl Into<String>) -> Self {
        let file_str = file.as_ref().to_string_lossy().into_owned();
        Self::DataFail { kind: DataType::Gas, file: file_str, reason: reason.into() }
    }
    pub fn meteo_fail(file: impl AsRef<std::path::Path>, reason: impl Into<String>) -> Self {
        let file_str = file.as_ref().to_string_lossy().into_owned();
        Self::DataFail { kind: DataType::Meteo, file: file_str, reason: reason.into() }
    }

    pub fn cycle_fail(file: impl AsRef<std::path::Path>, reason: impl Into<String>) -> Self {
        let file_str = file.as_ref().to_string_lossy().into_owned();
        Self::DataFail { kind: DataType::Cycle, file: file_str, reason: reason.into() }
    }

    pub fn chamber_fail(file: impl AsRef<std::path::Path>, reason: impl Into<String>) -> Self {
        let file_str = file.as_ref().to_string_lossy().into_owned();
        Self::DataFail { kind: DataType::Chamber, file: file_str, reason: reason.into() }
    }

    pub fn height_fail(file: impl AsRef<std::path::Path>, reason: impl Into<String>) -> Self {
        let file_str = file.as_ref().to_string_lossy().into_owned();
        Self::DataFail { kind: DataType::Height, file: file_str, reason: reason.into() }
    }
}

impl InsertEvent {
    pub fn gas_okskip(inserts: usize, skips: usize) -> Self {
        Self::DataOkSkip { kind: DataType::Gas, inserts, skips }
    }
    pub fn meteo_okskip(inserts: usize, skips: usize) -> Self {
        Self::DataOkSkip { kind: DataType::Meteo, inserts, skips }
    }

    pub fn cycle_okskip(inserts: usize, skips: usize) -> Self {
        Self::DataOkSkip { kind: DataType::Cycle, inserts, skips }
    }

    pub fn chamber_okskip(inserts: usize, skips: usize) -> Self {
        Self::DataOkSkip { kind: DataType::Chamber, inserts, skips }
    }

    pub fn height_okskip(inserts: usize, skips: usize) -> Self {
        Self::DataOkSkip { kind: DataType::Height, inserts, skips }
    }
}
