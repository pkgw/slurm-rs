// Copyright 2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Miscellaneous utility functions.

 */

use colorio::ColorIo;
use slurm::{JobState};


/// Print out a shortcode for a job state with affective color.
pub fn colorize_state(cio: &mut ColorIo, state: JobState) {
    match state {
        JobState::Pending => {
            cprint!(cio, pl, "{:2}", state.shortcode());
        },

        JobState::Running | JobState::Complete => {
            cprint!(cio, green, "{:2}", state.shortcode());
        },

        JobState::Suspended | JobState::Cancelled | JobState::Failed |
        JobState::Timeout | JobState::NodeFail | JobState::Preempted |
        JobState::BootFail | JobState::Deadline | JobState::OutOfMemory => {
            cprint!(cio, red, "{:2}", state.shortcode());
        },
    }
}
