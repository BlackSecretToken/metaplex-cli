use crate::transaction::Base64;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};

const STRFTIME: &str = "%Y-%m-%d %H:%M:%S";

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct RawStatus {
    pub block_height: u64,
    pub block_indep_hash: Base64,
    pub number_of_confirmations: u64,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone)]
pub enum StatusCode {
    #[default]
    Submitted,
    Pending,
    Confirmed,
    NotFound,
}
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Status {
    pub id: Base64,
    pub status: StatusCode,
    pub file_path: Option<PathBuf>,
    pub created_at: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
    pub reward: u64,
    #[serde(flatten)]
    pub raw_status: Option<RawStatus>,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            id: Base64(vec![]),
            status: StatusCode::default(),
            file_path: None,
            created_at: Utc::now(),
            last_modified: Utc::now(),
            reward: 0,
            raw_status: None,
        }
    }
}

impl QuietDisplay for Status {
    fn write_str(&self, _w: &mut dyn fmt::Write) -> fmt::Result {
        Ok(())
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            " {:<20}  {}  {:<9?}  {:>8}",
            self.file_path
                .as_ref()
                .map(|f| f.display().to_string())
                .unwrap_or("".to_string()),
            self.id,
            self.status,
            self.raw_status
                .as_ref()
                .map(|f| f.number_of_confirmations)
                .unwrap_or(0),
        )
    }
}

impl VerboseDisplay for Status {
    fn write_str(&self, w: &mut dyn fmt::Write) -> fmt::Result {
        writeln!(w, "{:<15} {}", "id:", self.id)?;
        writeln!(w, "{:<15} {:?}", "status:", self.status)?;
        if let Some(file_path) = &self.file_path {
            writeln!(
                w,
                "{:<15} {}",
                "file_path:",
                file_path.display().to_string()
            )?;
        };
        writeln!(
            w,
            "{:<15} {}",
            "created_at:",
            self.created_at.format(STRFTIME).to_string()
        )?;
        writeln!(
            w,
            "{:<15} {}",
            "last_modified:",
            self.last_modified.format(STRFTIME).to_string()
        )?;
        if let Some(raw_status) = &self.raw_status {
            writeln!(w, "{:<15}: {}", "height:", raw_status.block_height)?;
            writeln!(w, "{:<15}: {}", "indep_hash:", raw_status.block_indep_hash)?;
            writeln!(
                w,
                "{:<15}: {}",
                "confirms", raw_status.number_of_confirmations
            )?;
        };
        writeln!(w, "")
    }
}

impl OutputHeader<Status> for Status {
    fn header_string(output_format: &OutputFormat) -> String {
        match output_format {
            OutputFormat::Display => {
                format!(
                    " {:<20}  {:<43}  {:<9}  {}\n{:-<90}",
                    "path", "id", "status", "confirms", ""
                )
            }
            _ => format!("{}", ""),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Display,
    Json,
    JsonCompact,
    DisplayQuiet,
    DisplayVerbose,
}

impl OutputFormat {
    pub fn formatted_string<T>(&self, item: &T) -> String
    where
        T: Serialize + fmt::Display + QuietDisplay + VerboseDisplay,
    {
        match self {
            OutputFormat::Display => format!("{}", item),
            OutputFormat::DisplayQuiet => {
                let mut s = String::new();
                QuietDisplay::write_str(item, &mut s).unwrap();
                s
            }
            OutputFormat::DisplayVerbose => {
                let mut s = String::new();
                VerboseDisplay::write_str(item, &mut s).unwrap();
                s
            }
            OutputFormat::Json => {
                let mut string = serde_json::to_string_pretty(item).unwrap();
                ",\n".chars().for_each(|c| string.push(c));
                string
            }
            OutputFormat::JsonCompact => {
                let mut string = serde_json::to_value(item).unwrap().to_string();
                ",\n".chars().for_each(|c| string.push(c));
                string
            }
        }
    }
}

pub trait OutputHeader<T> {
    fn header_string(output_format: &OutputFormat) -> String
    where
        T: Serialize + fmt::Display + QuietDisplay + VerboseDisplay;
}

pub trait QuietDisplay: fmt::Display {
    fn write_str(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        write!(w, "{}", self)
    }
}

pub trait VerboseDisplay: fmt::Display {
    fn write_str(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        write!(w, "{}", self)
    }
}
