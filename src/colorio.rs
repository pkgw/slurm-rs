// Copyright 2018 Peter Williams <peter@newton.cx>
// Licensed under the MIT License.

/*! Colorized CLI output.

There are a few common colorized output styles that we use.

*/

use failure::Error;
use std::fmt;
use std::io::Write;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};


/// How to style some text to print.
///
/// Instead of using this type directly, use the `cprint!` family of macros.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Style {
    /// Style some text with a green color.
    Green,

    /// Style some text with a bold, bright color.
    Highlight,

    /// Style some text in the standard plain way.
    Plain,

    /// Style some text with a red color.
    Red,

    /// Style some text with a yellow color.
    Yellow,
}


/// How to style some text to print.
///
/// Instead of using this type directly, use the `cprint!` family of macros.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stream {
    /// Print to standard error.
    Stderr,

    /// Print to standard output.
    Stdout,
}


macro_rules! cprint {
    ($cio:expr, green, $($fmt_args:expr),*) => {{
        use $crate::colorio::{Stream, Style};
        $cio.print_core(Stream::Stdout, Style::Green, format_args!($($fmt_args),*))
    }};

    ($cio:expr, hl, $($fmt_args:expr),*) => {{
        use $crate::colorio::{Stream, Style};
        $cio.print_core(Stream::Stdout, Style::Highlight, format_args!($($fmt_args),*))
    }};

    ($cio:expr, pl, $($fmt_args:expr),*) => {{
        use $crate::colorio::{Stream, Style};
        $cio.print_core(Stream::Stdout, Style::Plain, format_args!($($fmt_args),*))
    }};

    ($cio:expr, red, $($fmt_args:expr),*) => {{
        use $crate::colorio::{Stream, Style};
        $cio.print_core(Stream::Stdout, Style::Red, format_args!($($fmt_args),*))
    }};

    ($cio:expr, yellow, $($fmt_args:expr),*) => {{
        use $crate::colorio::{Stream, Style};
        $cio.print_core(Stream::Stdout, Style::Yellow, format_args!($($fmt_args),*))
    }};
}

macro_rules! cprintln {
    ($cio:expr, $style:ident, $($fmt_args:expr),*) => {
        cprint!($cio, $style, $($fmt_args),*);
        cprint!($cio, pl, "\n");
    };
}

macro_rules! ecprint {
    ($cio:expr, green, $($fmt_args:expr),*) => {{
        use $crate::colorio::{Stream, Style};
        $cio.print_core(Stream::Stderr, Style::Green, format_args!($($fmt_args),*))
    }};

    ($cio:expr, hl, $($fmt_args:expr),*) => {{
        use $crate::colorio::{Stream, Style};
        $cio.print_core(Stream::Stderr, Style::Highlight, format_args!($($fmt_args),*))
    }};

    ($cio:expr, pl, $($fmt_args:expr),*) => {{
        use $crate::colorio::{Stream, Style};
        $cio.print_core(Stream::Stderr, Style::Plain, format_args!($($fmt_args),*))
    }};

    ($cio:expr, red, $($fmt_args:expr),*) => {{
        use $crate::colorio::{Stream, Style};
        $cio.print_core(Stream::Stderr, Style::Red, format_args!($($fmt_args),*))
    }};

    ($cio:expr, yellow, $($fmt_args:expr),*) => {{
        use $crate::colorio::{Stream, Style};
        $cio.print_core(Stream::Stderr, Style::Yellow, format_args!($($fmt_args),*))
    }};
}

macro_rules! ecprintln {
    ($cio:expr, $style:ident, $($fmt_args:expr),*) => {
        ecprint!($cio, $style, $($fmt_args),*);
        ecprint!($cio, pl, "\n");
    };
}

/// State needed for our colorized I/O.
pub struct ColorIo {
    stdout: StandardStream,
    stderr: StandardStream,
    red: ColorSpec,
    green: ColorSpec,
    highlight: ColorSpec,
    yellow: ColorSpec,
}


impl ColorIo {
    pub fn new() -> Self {
        let stdout = StandardStream::stdout(ColorChoice::Auto);
        let stderr = StandardStream::stderr(ColorChoice::Auto);

        let mut green = ColorSpec::new();
        green.set_fg(Some(Color::Green)).set_bold(true);

        let mut highlight = ColorSpec::new();
        highlight.set_bold(true);

        let mut red = ColorSpec::new();
        red.set_fg(Some(Color::Red)).set_bold(true);

        let mut yellow = ColorSpec::new();
        yellow.set_fg(Some(Color::Yellow)).set_bold(true);

        ColorIo { stdout, stderr, green, highlight, red, yellow }
    }

    pub fn print_error(&mut self, err: Error) {
        let mut first = true;

        for cause in err.causes() {
            if first {
                ecprint!(self, red, "error:");
                ecprintln!(self, pl, " {}", cause);
                first = false;
            } else {
                ecprint!(self, pl, "  ");
                ecprint!(self, red, "caused by:");
                ecprintln!(self, pl, " {}", cause);
            }
        }
    }

    /// Print formatted arguments to the standard output stream.
    ///
    /// Use the `println_*!` macros instead of this function.
    #[inline(always)]
    pub fn print_core(&mut self, stream: Stream, style: Style, args: fmt::Arguments) {
        let stream = match stream {
            Stream::Stderr => &mut self.stderr,
            Stream::Stdout => &mut self.stdout,
        };

        match style {
            Style::Green => {
                let _r = stream.set_color(&self.green);
            },

            Style::Highlight => {
                let _r = stream.set_color(&self.highlight);
            },

            Style::Plain => {
            },

            Style::Red => {
                let _r = stream.set_color(&self.red);
            },

            Style::Yellow => {
                let _r = stream.set_color(&self.yellow);
            },
        }

        let _r = write!(stream, "{}", args);

        match style {
            Style::Green | Style::Highlight | Style::Red | Style::Yellow => {
                let _r = stream.reset();
            },

            Style::Plain => {
            },
        }
    }
}
