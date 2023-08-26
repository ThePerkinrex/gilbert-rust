use std::path::PathBuf;

use crate::log::LogMessage;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum AlfredRunnerRequest {
    RunTask {
        job: PathBuf,
        params: Vec<serde_json::Value>,
    },
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum RunnerResponse {
    StartingJob { stages: Vec<String> },
    StartingStage { stage: String },
    FinishedStage { stage: String },
    JobStdout { msg: String },
    JobStderr { msg: String },
    Log(LogMessage),
    FinishedJob,
}
