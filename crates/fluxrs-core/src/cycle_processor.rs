use crate::cycle::cycle::{
    insert_flux_results, insert_fluxes_ignore_duplicates, load_cycles, process_cycles,
    update_fluxes, Cycle,
};
use crate::data_formats::chamberdata::{insert_chamber_metadata, read_chamber_metadata, Chamber};
use crate::data_formats::gasdata::{insert_measurements, GasData};
use crate::data_formats::heightdata::{
    insert_height_data, query_height, read_height_csv, HeightData,
};
use crate::data_formats::meteodata::{insert_meteo_data, read_meteo_csv, MeteoData};
use crate::data_formats::timedata::{insert_cycles, try_all_formats, TimeData};
use crate::processevent::{
    self, InsertEvent, ProcessEvent, ProcessEventSink, ProgressEvent, QueryEvent, ReadEvent,
};
use crate::project::Project;

use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver, UnboundedSender};

const MAX_CONCURRENT_TASKS: usize = 10;
type GasDataSet = HashMap<String, Arc<GasData>>;
type HeightDataSet = HeightData;
type ChamberDataSet = HashMap<String, Chamber>;
type MeteoDataSet = MeteoData;
type TimeDataSet = TimeData;
type CycleDataSet = Vec<Cycle>;

pub struct Datasets {
    pub gas: Arc<GasDataSet>,
    pub meteo: MeteoDataSet,
    pub height: HeightDataSet,
    pub chambers: ChamberDataSet,
}

pub struct Infra {
    pub conn: Arc<Mutex<rusqlite::Connection>>,
    pub progress: UnboundedSender<ProcessEvent>,
}

pub struct Processor {
    project: Project,
    data: Arc<Datasets>, // Arc so tasks can share cheaply
    infra: Infra,
}
impl Processor {
    pub fn new(project: Project, data: Datasets, infra: Infra) -> Self {
        Self { project, data: Arc::new(data), infra }
    }
    pub async fn run_processing_dynamic(&self, times: TimeDataSet) {
        let all_empty = self.data.gas.values().all(|g| g.datetime.is_empty());
        if all_empty {
            let _ =
                self.infra.progress.send(ProcessEvent::Done(Err("No data available".to_owned())));
            return;
        }

        let total_cycles = times.start_time.len();
        let gas_data_arc = Arc::clone(&self.data.gas); // cheap

        let mut time_chunks = VecDeque::from(times.chunk());
        let mut active_tasks = Vec::new();

        // track progress correctly
        use std::sync::atomic::{AtomicUsize, Ordering};
        let processed = Arc::new(AtomicUsize::new(0));

        let mut total_inserts = 0;
        let mut total_skips = 0;

        let mut fatal_error: Option<String> = None;

        while !time_chunks.is_empty() || !active_tasks.is_empty() {
            while active_tasks.len() < MAX_CONCURRENT_TASKS && !time_chunks.is_empty() {
                let chunk = time_chunks.pop_front().unwrap();

                // Build a lightweight map of ARC references (no deep clone)
                let mut chunk_gas_data = HashMap::new();
                for dt in &chunk.start_time {
                    let dt_utc = DateTime::<Utc>::from_timestamp(*dt, 0).unwrap();
                    let date_str = dt_utc.format("%Y-%m-%d").to_string();
                    if let Some(data) = gas_data_arc.get(&date_str) {
                        chunk_gas_data.insert(date_str, Arc::clone(data)); // bump refcount only
                    }
                }

                let meteo = self.data.meteo.clone();
                let height = self.data.height.clone();
                let chambers = self.data.chambers.clone();
                let project_clone = self.project.clone();
                let progress_sender = self.infra.progress.clone();
                let processed_ctr = Arc::clone(&processed);

                let task = tokio::task::spawn_blocking(move || {
                    match process_cycles(
                        &chunk,
                        &chunk_gas_data,
                        &meteo,
                        &height,
                        &chambers,
                        &project_clone,
                        progress_sender.clone(),
                    ) {
                        Ok(result) => {
                            let count = result.iter().flatten().count();
                            processed_ctr.fetch_add(count, Ordering::Relaxed);
                            let _ = progress_sender.send(ProcessEvent::Progress(
                                ProgressEvent::Rows(count, total_cycles),
                            ));
                            Ok(result)
                        },
                        Err(e) => {
                            // No error sending, just propagate
                            Err(e)
                        },
                    }
                });

                active_tasks.push(task);
            }

            let (result, _i, remaining_tasks) = futures::future::select_all(active_tasks).await;
            active_tasks = remaining_tasks;

            match result {
                // Inner task ok, process_cycles ok
                Ok(Ok(cycles)) => {
                    if !cycles.is_empty() {
                        let mut conn = self.infra.conn.lock().unwrap();
                        match insert_fluxes_ignore_duplicates(
                            &mut conn,
                            &cycles,
                            &self.project.id.unwrap(),
                        ) {
                            Ok((inserts, skips)) => {
                                total_inserts += inserts;
                                total_skips += skips;
                            },
                            Err(e) => {
                                fatal_error = Some(format!("Insert error: {e}"));
                                break;
                            },
                        }
                    }
                },

                // process_cycles returned Err
                Ok(Err(e)) => {
                    fatal_error = Some(format!("Cycle processing error: {e}"));
                    break;
                },

                // spawn_blocking join error (panic / cancellation)
                Err(e) => {
                    fatal_error = Some(format!("Worker join error: {e}"));
                    break;
                },
            }
        }

        let progress_sender = self.infra.progress.clone();

        // Only report inserts/skips if no fatal error
        if fatal_error.is_none() {
            let _ = progress_sender
                .send(ProcessEvent::Insert(InsertEvent::cycle_okskip(total_inserts, total_skips)));
        }

        let done_event = match fatal_error {
            Some(msg) => ProcessEvent::Done(Err(msg)),
            None => {
                let _ = progress_sender.send(ProcessEvent::Progress(ProgressEvent::EnableUI));
                ProcessEvent::Done(Ok(()))
            },
        };

        let _ = progress_sender.send(done_event);
    }
}
