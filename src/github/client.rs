use crate::github::types::*;
use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;

const GITHUB_API_URL: &str = "https://api.github.com";

pub struct GitHubClient {
    token: String,
    agent: ureq::Agent,
}

impl GitHubClient {
    pub fn new() -> Result<Self> {
        let token = discover_github_token()
            .context("Could not find GitHub token. Set GITHUB_TOKEN env var or install gh CLI.")?;
        let agent = ureq::Agent::new();
        Ok(GitHubClient { token, agent })
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{GITHUB_API_URL}{path}");
        let resp = self
            .agent
            .get(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("User-Agent", "linear-claude")
            .call()
            .with_context(|| format!("Failed to GET {url}"))?;
        resp.into_json().with_context(|| format!("Failed to parse response from {url}"))
    }

    fn get_text(&self, path: &str) -> Result<String> {
        let url = format!("{GITHUB_API_URL}{path}");
        let resp = self
            .agent
            .get(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("User-Agent", "linear-claude")
            .call()
            .with_context(|| format!("Failed to GET {url}"))?;
        resp.into_string().with_context(|| format!("Failed to read response from {url}"))
    }

    fn post_json<T: DeserializeOwned>(&self, path: &str, body: &serde_json::Value) -> Result<T> {
        let url = format!("{GITHUB_API_URL}{path}");
        let resp = self
            .agent
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("User-Agent", "linear-claude")
            .send_json(body)
            .with_context(|| format!("Failed to POST {url}"))?;
        resp.into_json().with_context(|| format!("Failed to parse response from {url}"))
    }

    fn post_json_no_response(&self, path: &str, body: &serde_json::Value) -> Result<()> {
        let url = format!("{GITHUB_API_URL}{path}");
        self.agent
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("User-Agent", "linear-claude")
            .send_json(body)
            .with_context(|| format!("Failed to POST {url}"))?;
        Ok(())
    }

    /// Check if a PR already exists for a branch. Returns the PR number if found.
    pub fn find_pr_for_branch(&self, owner: &str, repo: &str, branch: &str) -> Result<Option<u64>> {
        let prs: Vec<PullRequest> =
            self.get(&format!("/repos/{owner}/{repo}/pulls?head={owner}:{branch}&state=open"))?;
        Ok(prs.first().map(|pr| pr.number))
    }

    /// Create a new pull request. Returns the PR number.
    pub fn create_pr(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
        head: &str,
        base: &str,
        draft: bool,
    ) -> Result<u64> {
        let payload = serde_json::json!({
            "title": title,
            "body": body,
            "head": head,
            "base": base,
            "draft": draft,
        });
        let pr: PullRequest = self.post_json(&format!("/repos/{owner}/{repo}/pulls"), &payload)?;
        Ok(pr.number)
    }

    /// Post a comment on an issue/PR.
    pub fn post_comment(&self, owner: &str, repo: &str, issue_number: u64, body: &str) -> Result<()> {
        let payload = serde_json::json!({ "body": body });
        self.post_json_no_response(
            &format!("/repos/{owner}/{repo}/issues/{issue_number}/comments"),
            &payload,
        )
    }

    /// Get the head SHA of a PR.
    pub fn get_pr_head_sha(&self, owner: &str, repo: &str, pr_number: u64) -> Result<String> {
        let pr: PullRequest = self.get(&format!("/repos/{owner}/{repo}/pulls/{pr_number}"))?;
        Ok(pr.head.sha)
    }

    /// Get the head branch ref of a PR.
    pub fn get_pr_head_ref(&self, owner: &str, repo: &str, pr_number: u64) -> Result<String> {
        let pr: PullRequest = self.get(&format!("/repos/{owner}/{repo}/pulls/{pr_number}"))?;
        Ok(pr.head.ref_name)
    }

    /// Get failed check runs for a commit.
    pub fn get_failed_checks(&self, owner: &str, repo: &str, sha: &str) -> Result<Vec<CheckRun>> {
        let runs: CheckRunsResponse =
            self.get(&format!("/repos/{owner}/{repo}/commits/{sha}/check-runs"))?;
        Ok(runs
            .check_runs
            .into_iter()
            .filter(|r| {
                r.conclusion.as_deref() == Some("failure")
                    || r.conclusion.as_deref() == Some("timed_out")
            })
            .collect())
    }

    /// Get annotations for a check run.
    pub fn get_check_annotations(&self, owner: &str, repo: &str, check_run_id: u64) -> Result<Vec<Annotation>> {
        self.get(&format!("/repos/{owner}/{repo}/check-runs/{check_run_id}/annotations"))
    }

    /// Get failed commit statuses (legacy status API).
    pub fn get_failed_statuses(&self, owner: &str, repo: &str, sha: &str) -> Result<Vec<CommitStatus>> {
        let status: CommitStatusResponse =
            self.get(&format!("/repos/{owner}/{repo}/commits/{sha}/status"))?;
        Ok(status
            .statuses
            .into_iter()
            .filter(|s| s.state == "failure" || s.state == "error")
            .collect())
    }

    /// Get failed workflow run IDs for a commit.
    pub fn get_failed_workflow_runs(&self, owner: &str, repo: &str, sha: &str) -> Result<Vec<u64>> {
        let runs: WorkflowRunsResponse =
            self.get(&format!("/repos/{owner}/{repo}/actions/runs?head_sha={sha}"))?;
        Ok(runs
            .workflow_runs
            .into_iter()
            .filter(|r| r.conclusion.as_deref() == Some("failure"))
            .map(|r| r.id)
            .collect())
    }

    /// Get failed jobs for a workflow run.
    pub fn get_failed_jobs(&self, owner: &str, repo: &str, run_id: u64) -> Result<Vec<Job>> {
        let jobs: JobsResponse =
            self.get(&format!("/repos/{owner}/{repo}/actions/runs/{run_id}/jobs"))?;
        Ok(jobs
            .jobs
            .into_iter()
            .filter(|j| j.conclusion.as_deref() == Some("failure"))
            .collect())
    }

    /// Get job logs (last 100 lines).
    pub fn get_job_logs(&self, owner: &str, repo: &str, job_id: u64) -> Result<String> {
        let text = self.get_text(&format!("/repos/{owner}/{repo}/actions/jobs/{job_id}/logs"))?;
        let lines: Vec<&str> = text.lines().collect();
        let start = lines.len().saturating_sub(100);
        Ok(lines[start..].join("\n"))
    }

    /// Get inline review comments on a PR.
    pub fn get_pr_review_comments(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
    ) -> Result<Vec<PrComment>> {
        self.get(&format!("/repos/{owner}/{repo}/pulls/{pr_number}/comments"))
    }

    /// Get review bodies on a PR.
    pub fn get_pr_reviews(&self, owner: &str, repo: &str, pr_number: u64) -> Result<Vec<PrReview>> {
        let reviews: Vec<PrReview> =
            self.get(&format!("/repos/{owner}/{repo}/pulls/{pr_number}/reviews"))?;
        Ok(reviews
            .into_iter()
            .filter(|r| r.body.as_ref().map(|b| !b.is_empty()).unwrap_or(false))
            .collect())
    }

    /// Get conversation (issue) comments on a PR.
    pub fn get_issue_comments(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
    ) -> Result<Vec<PrComment>> {
        self.get(&format!("/repos/{owner}/{repo}/issues/{issue_number}/comments"))
    }
}

fn discover_github_token() -> Result<String> {
    // 1. Environment variable
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // 2. Fall back to `gh auth token`
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .context("Failed to run 'gh auth token'. Is gh CLI installed?")?;

    if output.status.success() {
        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    bail!("No GitHub token found")
}
