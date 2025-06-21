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
    QueryComplete,
    NoGasData(String),
    NoGasDataDay(String),
}

#[derive(Debug)]
pub enum ReadEvent {
    File(String),
    FileFail(String, String),
    RowFail(String, String),
    FileRows(String, usize),
}

#[derive(Debug)]
pub enum InsertEvent {
    Ok(usize),
    OkSkip(usize, usize),
    Fail(String),
}

#[derive(Debug)]
pub enum ProgressEvent {
    Day(String),
    Rows(usize, usize),
    NoGas(String),
}
