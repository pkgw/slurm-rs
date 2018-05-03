// Copyright 2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Make a list of recent jobs belonging to this user.
 */

use chrono::{Duration, Utc};
use failure::Error;
use itertools::Itertools;
use slurm::{self, JobStepRecordSharedFields};
use std::collections::HashMap;
use termcolor::StandardStream;
use users;


#[derive(Debug, StructOpt)]
pub struct RecentCommand {
}

impl RecentCommand {
    pub fn cli(self, _stdout: StandardStream) -> Result<i32, Error> {
        let now = Utc::now();
        let min_start = now - Duration::days(7);

        // Note that the userid we have to filter on must be a string
        // representation of the numeric UID.
        let uid = users::get_current_uid();
        let mut filter = slurm::JobFiltersOwned::default();
        filter.userid_list_mut().append(format!("{}", uid));
        filter.usage_start(min_start);

        let db = slurm::DatabaseConnectionOwned::new()?;
        let jobs = db.get_jobs(&filter)?;

        for (arrayid, group) in &jobs.iter().group_by(|job| job.array_job_id().unwrap_or_else(|| job.job_id())) {
            let mut n_jobs = 0;
            let mut last_state = slurm::JobState::Failed;
            let mut states = HashMap::new();

            for job in group {
                if n_jobs == 0 {
                    print!("{} {}: ", arrayid, job.job_name());
                }

                n_jobs += 1;
                last_state = job.state();
                let slot = states.entry(last_state).or_insert(0);
                *slot += 1;
            }

            if n_jobs == 1 {
                println!("{:2}", last_state.shortcode());
            } else {
                let seen_states = states.keys().sorted();
                let text = seen_states
                    .iter()
                    .map(|s| format!("{} {}", states.get(s).unwrap(), s.shortcode()))
                    .join(", ");
                println!("{} ({} total)", text, n_jobs);
            }
        }

        Ok(0)
    }
}
