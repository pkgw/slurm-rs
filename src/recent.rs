// Copyright 2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Make a list of recent jobs belonging to this user.
 */

use chrono::{Duration, Utc};
use colorio::ColorIo;
use failure::Error;
use itertools::Itertools;
use slurm::{self, JobState, JobStepRecordSharedFields};
use std::collections::HashMap;
use users;
use util;


#[derive(Debug, StructOpt)]
pub struct RecentCommand {
}

impl RecentCommand {
    pub fn cli(self, cio: &mut ColorIo) -> Result<i32, Error> {
        let now = Utc::now();
        let min_start = now - Duration::days(7);

        // Note that the userid we have to filter on must be a string
        // representation of the numeric UID.
        let uid = users::get_current_uid();
        let mut filter = slurm::JobFiltersOwned::default();
        filter.userid_list_mut().append(format!("{}", uid));
        filter.usage_start(min_start);

        let mut grouped = HashMap::new();
        let db = slurm::DatabaseConnectionOwned::new()?;
        let jobs = db.get_jobs(&filter)?;

        for job in jobs.iter() {
            let group_id = job.array_job_id().unwrap_or_else(|| job.job_id());
            let group_info = grouped.entry(group_id).or_insert_with(|| JobGroupInfo::new(group_id));
            group_info.accumulate(&job);
        }

        for group_info in grouped.values() {
            group_info.emit(cio);
        }

        Ok(0)
    }
}

struct JobGroupInfo {
    id: slurm::JobId,
    n_jobs: usize,
    states: HashMap<JobState, usize>,
}

impl JobGroupInfo {
    pub fn new(id: slurm::JobId) -> Self {
        JobGroupInfo {
            id,
            n_jobs: 0,
            states: HashMap::new(),
        }
    }

    pub fn accumulate(&mut self, job: &slurm::JobRecord) {
        self.n_jobs += 1;
        let slot = self.states.entry(job.state()).or_insert(0);
        *slot += 1;
    }

    pub fn emit(&self, cio: &mut ColorIo) {
        cprint!(cio, hl, "{}:", self.id);

        if self.n_jobs == 1 {
            let state = self.states.keys().next().unwrap();
            cprint!(cio, pl, " ");
            util::colorize_state(cio, *state);
            cprintln!(cio, pl, "");
        } else {
            let seen_states = self.states.keys().sorted();
            let text = seen_states
                .iter()
                .map(|s| format!("{} {}", self.states.get(s).unwrap(), s.shortcode()))
                .join(", ");
            cprintln!(cio, pl, " {} ({} total)", text, self.n_jobs);
        }
    }
}
