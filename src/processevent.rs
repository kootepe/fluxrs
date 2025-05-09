#[derive(Debug)]
pub enum ProcessEvent {
    // first value is some part of the second, eg number of cycles out of total cycles
    Progress((usize, usize)),     // number of rows progressed and total
    ProgressDay(String),          // When progressing to new day in w/e
    Error(String),                // Error message
    QueryComplete,                // Send when a long query completes
    NoGasData(String),            // start_time of the cycle with no gas data
    ReadFile(String),             // name of the file read
    ReadFileFail(String, String), // filename, and string error
    ReadFileRows(String, usize),  // filename, amount of rows read
    InsertOk(usize),              // number of rows inserted
    InsertOkSkip(usize, usize),   // number of rows inserted and skipped
    InsertFail(String),           // String error when insert fails
    Done,                         // When some process is done
}
