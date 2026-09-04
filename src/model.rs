use std::path::PathBuf;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum DeltaSelection {
    Enabled,
    Disabled,
    Both,
}

impl DeltaSelection {
    pub fn modes(self) -> &'static [DeltaMode] {
        match self {
            Self::Enabled => &[DeltaMode::Enabled],
            Self::Disabled => &[DeltaMode::Disabled],
            Self::Both => &[DeltaMode::Disabled, DeltaMode::Enabled],
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeltaMode {
    Enabled,
    Disabled,
}

impl DeltaMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureManifest {
    pub version: u32,
    pub name: String,
    pub object_format: String,
    pub heads: Vec<String>,
    pub expected_objects: PathBuf,
    #[serde(default)]
    pub expectations: FixtureExpectations,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureExpectations {
    #[serde(default)]
    pub delta_required: bool,
    pub max_delta_ratio: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct Implementation {
    pub name: String,
    pub command: Vec<String>,
    pub working_directory: Option<PathBuf>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub capabilities: Capabilities,
}

fn default_timeout() -> u64 {
    120
}

#[derive(Debug, Default, Deserialize)]
pub struct Capabilities {
    #[serde(default)]
    pub sha1: bool,
    #[serde(default)]
    pub delta_disable: bool,
    #[allow(dead_code)]
    #[serde(default)]
    pub offset_deltas: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackRequest {
    pub version: u32,
    pub object_format: String,
    pub delta_compression: DeltaMode,
}
