// Copyright 2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Miscellaneous utility functions.

 */

use chrono::Duration;
use colorio::ColorIo;
use slurm::{JobState};


/// Print out a shortcode for a job state with affective color.
pub fn colorize_state(cio: &mut ColorIo, state: JobState) {
    match state {
        JobState::Pending => {
            cprint!(cio, pl, "{}", state.shortcode());
        },

        JobState::Running => {
            cprint!(cio, hl, "{}", state.shortcode());
        },

        JobState::Complete => {
            cprint!(cio, green, "{}", state.shortcode());
        },

        JobState::Cancelled | JobState::Failed | JobState::NodeFail |
        JobState::BootFail | JobState::Deadline | JobState::OutOfMemory => {
            cprint!(cio, red, "{}", state.shortcode());
        },

        JobState::Suspended | JobState::Timeout | JobState::Preempted => {
            cprint!(cio, yellow, "{}", state.shortcode());
        },
    }
}


/// Express a duration in text, approximately.
pub fn dur_to_text(dur: &Duration) -> String {
    if dur.num_days() > 2 {
        format!("{} days", dur.num_days())
    } else if dur.num_hours() > 2 {
        format!("{} hours", dur.num_hours())
    } else if dur.num_minutes() > 2 {
        format!("{} minutes", dur.num_minutes())
    } else {
        format!("{} seconds", dur.num_seconds())
    }
}

