use crate::linear::types::{GraphQLResponse, LinearIssue};
use anyhow::{bail, Context, Result};
use regex::Regex;

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

pub struct LinearClient {
    api_key: String,
    agent: ureq::Agent,
}

impl LinearClient {
    pub fn new() -> Result<Self> {
        let api_key = discover_api_key()
            .context("Could not find Linear API key. Set LINEAR_API_KEY env var or configure ~/.config/linear/credentials.toml")?;
        let agent = ureq::Agent::new();
        Ok(LinearClient { api_key, agent })
    }

    pub fn fetch_view_issues(&self, view_url_or_id: &str) -> Result<Vec<LinearIssue>> {
        let view_id = extract_view_id(view_url_or_id)?;

        // Validate view_id format to prevent GraphQL injection
        let id_re = Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
        if !id_re.is_match(&view_id) {
            bail!("Invalid view ID format (must be alphanumeric/hyphens/underscores): {view_id}");
        }

        let mut all_issues = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let after_clause = cursor
                .as_ref()
                .map(|c| format!(r#", after: "{c}""#))
                .unwrap_or_default();

            let query = format!(
                r#"{{ customView(id: "{view_id}") {{ issues(first: 50{after_clause}) {{ nodes {{ id identifier title description branchName state {{ name }} attachments {{ nodes {{ url title sourceType }} }} }} pageInfo {{ hasNextPage endCursor }} }} }} }}"#
            );

            let body = serde_json::json!({ "query": query });

            let resp = self
                .agent
                .post(LINEAR_API_URL)
                .set("Authorization", &self.api_key)
                .set("Content-Type", "application/json")
                .send_json(&body)
                .context("Failed to send request to Linear API")?;

            let gql_resp: GraphQLResponse =
                resp.into_json().context("Failed to parse Linear API response")?;

            if let Some(errors) = gql_resp.errors {
                if let Some(first) = errors.first() {
                    bail!("Linear API error: {}", first.message);
                }
            }

            let custom_view = gql_resp
                .data
                .and_then(|d| d.custom_view)
                .context("Could not find view or no issues in view")?;

            let page_info = custom_view.issues.page_info.as_ref();
            let has_next = page_info.map(|p| p.has_next_page).unwrap_or(false);
            let end_cursor = page_info.and_then(|p| p.end_cursor.clone());

            for raw_issue in custom_view.issues.nodes {
                all_issues.push(raw_issue.into_linear_issue());
            }

            if has_next {
                cursor = end_cursor;
            } else {
                break;
            }
        }

        Ok(all_issues)
    }

    pub fn fetch_issue(&self, issue_id_or_url: &str) -> Result<LinearIssue> {
        let identifier = extract_issue_identifier(issue_id_or_url)?;

        let query = r#"
            query($identifier: String!) {
                issue(id: $identifier) {
                    id
                    identifier
                    title
                    description
                    branchName
                    state { name }
                    attachments { nodes { url title sourceType } }
                }
            }
        "#;

        let body = serde_json::json!({
            "query": query,
            "variables": { "identifier": identifier }
        });

        let resp = self
            .agent
            .post(LINEAR_API_URL)
            .set("Authorization", &self.api_key)
            .set("Content-Type", "application/json")
            .send_json(&body)
            .context("Failed to send request to Linear API")?;

        let gql_resp: GraphQLResponse =
            resp.into_json().context("Failed to parse Linear API response")?;

        if let Some(errors) = gql_resp.errors {
            if let Some(first) = errors.first() {
                bail!("Linear API error: {}", first.message);
            }
        }

        let raw_issue = gql_resp
            .data
            .and_then(|d| d.issue)
            .context(format!("Could not find issue: {identifier}"))?;

        Ok(raw_issue.into_linear_issue())
    }
}

fn extract_issue_identifier(url_or_id: &str) -> Result<String> {
    if url_or_id.starts_with("http") {
        // URL format: https://linear.app/team/issue/TEAM-123/optional-slug
        let re = Regex::new(r"/issue/([A-Za-z]+-\d+)").unwrap();
        let caps = re
            .captures(url_or_id)
            .context("Could not extract issue identifier from URL")?;
        Ok(caps[1].to_string())
    } else {
        Ok(url_or_id.to_string())
    }
}

fn extract_view_id(url_or_id: &str) -> Result<String> {
    if url_or_id.starts_with("http") {
        let re = Regex::new(r"/view/([^/]+)").unwrap();
        let caps = re
            .captures(url_or_id)
            .context("Could not extract view ID from URL")?;
        Ok(caps[1].to_string())
    } else {
        Ok(url_or_id.to_string())
    }
}

fn discover_api_key() -> Result<String> {
    // 1. Environment variable
    if let Ok(key) = std::env::var("LINEAR_API_KEY") {
        if !key.is_empty() {
            return Ok(key);
        }
    }

    // 2. ~/.config/linear/credentials.toml
    if let Some(config_dir) = dirs::config_dir() {
        let creds_path = config_dir.join("linear").join("credentials.toml");
        if creds_path.exists() {
            let content = std::fs::read_to_string(&creds_path)
                .context("Failed to read Linear credentials file")?;
            let parsed: toml::Value =
                content.parse().context("Failed to parse Linear credentials TOML")?;

            // Try to find the default workspace key
            if let Some(default_workspace) = parsed.get("default").and_then(|v| v.as_str()) {
                if let Some(key) = parsed.get(default_workspace).and_then(|v| v.as_str()) {
                    return Ok(key.to_string());
                }
            }

            // Fall back to any string value that looks like an API key
            if let Some(table) = parsed.as_table() {
                for (key, value) in table {
                    if key != "default" {
                        if let Some(api_key) = value.as_str() {
                            if api_key.starts_with("lin_api_") {
                                return Ok(api_key.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    bail!("No Linear API key found")
}
