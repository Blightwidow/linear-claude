use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PullRequest {
    pub number: u64,
    pub head: PrHead,
    pub title: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PrHead {
    pub sha: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
}

#[derive(Debug, Deserialize)]
pub struct CheckRunsResponse {
    pub check_runs: Vec<CheckRun>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRun {
    pub id: u64,
    pub name: String,
    pub conclusion: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Annotation {
    pub annotation_level: String,
    pub path: String,
    pub start_line: u64,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct CommitStatusResponse {
    pub statuses: Vec<CommitStatus>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CommitStatus {
    pub context: String,
    pub state: String,
    pub description: Option<String>,
    pub target_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowRunsResponse {
    pub workflow_runs: Vec<WorkflowRun>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct WorkflowRun {
    pub id: u64,
    pub head_sha: String,
    pub conclusion: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JobsResponse {
    pub jobs: Vec<Job>,
}

#[derive(Debug, Deserialize)]
pub struct Job {
    pub id: u64,
    pub name: String,
    pub conclusion: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PrComment {
    pub user: PrUser,
    pub body: Option<String>,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PrUser {
    pub login: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PrReview {
    pub user: PrUser,
    pub body: Option<String>,
    pub state: Option<String>,
    pub submitted_at: Option<String>,
}
