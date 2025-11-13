use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct AppState {
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
}
