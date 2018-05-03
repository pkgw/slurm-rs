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
            cprint!(cio, pl, "{}", state.shortcode());
        },

        JobState::Running | JobState::Complete => {
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
