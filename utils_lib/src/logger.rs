// RUNNING EXAMPLE

#[macro_export]
macro_rules! set_running_local {
    () => {
        std::env::set_var("RUNNING_LOCAL", "true");
    };
}

// LOG FILE

#[macro_export]
macro_rules! set_log_file {
    ($value:expr) => {
        std::env::set_var("LOG_FILE", $value.to_string());
    };
}

pub fn is_running_local() -> bool {
    match std::env::var("RUNNING_LOCAL") {
        Ok(val) => match val.parse() {
            Ok(val) => val,
            Err(_) => false,
        },
        Err(_) => false,
    }
}

#[macro_export]
macro_rules! log {
    ($fmt:expr $(, $arg:expr)*) => {{
        use std::env;
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::time::SystemTime;
        use chrono::{DateTime, Utc};
        use inline_colorization::*;

        if $crate::logger::is_running_local() {
            println!($fmt $(, $arg)*);
        } else {
            let current_time = SystemTime::now();
            let datetime: DateTime<Utc> = current_time.into();
            let timestamp = datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string();

            if let Ok(log_file) = env::var("LOG_FILE") {
                let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_file)
                .expect("Unable to open log file");
                writeln!(file, "\n[{}] {}", timestamp, format!($fmt $(, $arg)*)).expect("Unable to write to log file");
            } else {
                println!($fmt $(, $arg)*);
            }
        }
}}}

#[macro_export]
macro_rules! log_magenta {
    ($fmt:expr $(, $arg:expr)*) => {{
        use inline_colorization::*;

        if $crate::logger::is_running_local() {
            // add color
            println!("{color_magenta}{}{style_reset}", format!($fmt $(, $arg)*));
        } else {
            log!("{}", format!($fmt $(, $arg)*));
        }
    }};
}

#[macro_export]
macro_rules! log_green {
    ($fmt:expr $(, $arg:expr)*) => {{
        use inline_colorization::*;
        if $crate::logger::is_running_local() {
            // add color
            println!("{color_green}{}{style_reset}", format!($fmt $(, $arg)*));
        } else {
            log!("{}", format!($fmt $(, $arg)*));
        }
    }};
}

#[macro_export]
macro_rules! log_yellow {
    ($fmt:expr $(, $arg:expr)*) => {{
        use inline_colorization::*;
        if $crate::logger::is_running_local() {
            // add color
            println!("{color_yellow}{}{style_reset}", format!($fmt $(, $arg)*));
        } else {
            println!("{color_yellow}{}{style_reset}", format!($fmt $(, $arg)*));
            log!("{}", format!($fmt $(, $arg)*));
        }
    }};
}

#[macro_export]
macro_rules! log_red {
    ($fmt:expr $(, $arg:expr)*) => {{
        use inline_colorization::*;
        if $crate::logger::is_running_local() {
            // add color
            println!("{color_red}{}{style_reset}", format!($fmt $(, $arg)*));
        } else {
            log!("{}", format!($fmt $(, $arg)*));
        }
    }};
}

#[macro_export]
macro_rules! log_blue {
    ($fmt:expr $(, $arg:expr)*) => {{
        use inline_colorization::*;
        if $crate::logger::is_running_local() {
            // add color
            println!("{color_blue}{}{style_reset}", format!($fmt $(, $arg)*));
        } else {
            println!("{color_blue}{}{style_reset}", format!($fmt $(, $arg)*));
            log!("{}", format!($fmt $(, $arg)*));
        }
    }};
}

#[macro_export]
macro_rules! log_bright_yellow {
    ($fmt:expr $(, $arg:expr)*) => {{
        use inline_colorization::*;
        if $crate::logger::is_running_local() {
            // add color
            println!("{color_bright_yellow}{}{style_reset}", format!($fmt $(, $arg)*));
        } else {
            log!("{}", format!($fmt $(, $arg)*));
        }
    }};
}

pub use log;
pub use log_magenta;
pub use log_green;
pub use log_yellow;
pub use log_red;
pub use log_blue;
pub use log_bright_yellow;