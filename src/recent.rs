// Copyright 2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Make a list of recent jobs belonging to this user.
 */

use chrono::{DateTime, Duration, Utc};
use colorio::ColorIo;
use failure::Error;
use itertools::Itertools;
use slurm::{self, JobState, JobStepRecordSharedFields};
use std::cmp;
use std::collections::HashMap;
use users;
use util;

#[derive(Debug, StructOpt)]
pub struct RecentCommand {
    #[structopt(short = "s", long = "span", default_value = "7")]
    /// How many days back to query the accounting database.
    span_days: usize,

    #[structopt(short = "l", long = "limit", default_value = "30")]
    /// Limit the output to at most this number of recent jobs.
    limit: usize,
}

impl RecentCommand {
    pub fn cli(self, cio: &mut ColorIo) -> Result<i32, Error> {
        let now = Utc::now();
        let min_start = now - Duration::days(self.span_days as i64);

        // Note that the userid we have to filter on must be a string
        // representation of the numeric UID.
        let uid = users::get_current_uid();
        let mut filter = slurm::JobFiltersOwned::default();
        filter.userid_list_mut().append(format!("{}", uid));
        filter.usage_start(min_start);

        let mut grouped = HashMap::new();
        let db = slurm::DatabaseConnectionOwned::new()?;
        let jobs = db.get_jobs(&filter)?;
        let mut max_name_len = 0;
        let mut max_time_len = 0;

        for job in jobs.iter() {
            let group_id = job.group_id();
            let group_info = grouped.entry(group_id).or_insert_with(|| {
                let info = JobGroupInfo::new(&job, &now);
                max_name_len = cmp::max(max_name_len, info.name.len());
                max_time_len = cmp::max(max_time_len, info.submit_text.len());
                info
            });
            group_info.accumulate(&job);
        }

        let skip = if grouped.len() < self.limit {
            0
        } else {
            grouped.len() - self.limit
        };

        for group_info in grouped
            .values()
            .sorted_by_key(|gi| gi.submit_time)
            .skip(skip)
        {
            group_info.emit(cio, max_name_len, max_time_len);
        }

        Ok(0)
    }
}

trait JobRecordExt {
    fn group_id(&self) -> slurm::JobId;
}

impl JobRecordExt for slurm::JobRecord {
    fn group_id(&self) -> slurm::JobId {
        self.array_job_id().unwrap_or_else(|| self.job_id())
    }
}

struct JobGroupInfo {
    id: slurm::JobId,
    name: String,
    submit_time: DateTime<Utc>,
    submit_text: String,
    n_jobs: usize,
    states: HashMap<JobState, usize>,
}

impl JobGroupInfo {
    pub fn new(job: &slurm::JobRecord, now: &DateTime<Utc>) -> Self {
        let submit_time = job.submit_time();
        let submit_text = util::dur_to_text(&now.signed_duration_since(submit_time));

        JobGroupInfo {
            id: job.group_id(),
            name: job.job_name().into_owned(),
            submit_time,
            submit_text,
            n_jobs: 0,
            states: HashMap::new(),
        }
    }

    pub fn accumulate(&mut self, job: &slurm::JobRecord) {
        self.n_jobs += 1;
        let slot = self.states.entry(job.state()).or_insert(0);
        *slot += 1;
    }

    pub fn emit(&self, cio: &mut ColorIo, max_name_len: usize, max_time_len: usize) {
        cprint!(cio, hl, "{}", self.id);
        cprint!(cio, pl, " {1:0$}", max_name_len, self.name);

        let stext = format!("{} ago", self.submit_text);
        cprint!(cio, pl, "  {1:0$} ", max_time_len + 4, stext);

        if self.n_jobs == 1 {
            let state = self.states.keys().next().unwrap();
            cprint!(cio, pl, " ");
            util::colorize_state(cio, *state);
            cprintln!(cio, pl, "");
        } else {
            let seen_states = self.states.keys().sorted();
            let mut first = true;

            for state in seen_states {
                if first {
                    first = false;
                } else {
                    cprint!(cio, pl, ",");
                }

                cprint!(cio, pl, " {} ", self.states.get(state).unwrap());
                util::colorize_state(cio, *state);
            }

            cprintln!(cio, pl, " ({} total)", self.n_jobs);
        }
    }
}
