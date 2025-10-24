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
    Ok(String, usize),
    OkSkip(usize, usize),
    Fail(String),
}

#[derive(Debug)]
pub enum ProgressEvent {
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
