use serde::Deserialize;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LinearIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub branch_name: Option<String>,
    pub state: IssueStatus,
    pub pr_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IssueStatus {
    Done,
    InProgress,
    InReview,
    Other(String),
}

impl IssueStatus {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "done" => IssueStatus::Done,
            "in progress" => IssueStatus::InProgress,
            "in review" => IssueStatus::InReview,
            other => IssueStatus::Other(other.to_string()),
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            IssueStatus::Done => "Done",
            IssueStatus::InProgress => "In Progress",
            IssueStatus::InReview => "In Review",
            IssueStatus::Other(s) => s.as_str(),
        }
    }
}

// Raw GraphQL response types for deserialization
#[derive(Deserialize)]
pub struct GraphQLResponse {
    pub data: Option<GraphQLData>,
    pub errors: Option<Vec<GraphQLError>>,
}

#[derive(Deserialize)]
pub struct GraphQLError {
    pub message: String,
}

#[derive(Deserialize)]
pub struct GraphQLData {
    #[serde(rename = "customView")]
    pub custom_view: Option<CustomView>,
    pub issue: Option<RawIssue>,
}

#[derive(Deserialize)]
pub struct CustomView {
    pub issues: IssueConnection,
}

#[derive(Deserialize)]
pub struct IssueConnection {
    pub nodes: Vec<RawIssue>,
    #[serde(rename = "pageInfo")]
    pub page_info: Option<PageInfo>,
}

#[derive(Deserialize)]
pub struct PageInfo {
    #[serde(rename = "hasNextPage")]
    pub has_next_page: bool,
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct RawIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(rename = "branchName")]
    pub branch_name: Option<String>,
    pub state: Option<RawState>,
    pub attachments: Option<AttachmentConnection>,
}

#[derive(Deserialize)]
pub struct RawState {
    pub name: String,
}

#[derive(Deserialize)]
pub struct AttachmentConnection {
    pub nodes: Vec<RawAttachment>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct RawAttachment {
    pub url: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "sourceType")]
    pub source_type: Option<String>,
}

impl RawIssue {
    pub fn into_linear_issue(self) -> LinearIssue {
        let state = self
            .state
            .map(|s| IssueStatus::from_str(&s.name))
            .unwrap_or(IssueStatus::Other("unknown".to_string()));

        let pr_url = self
            .attachments
            .and_then(|a| {
                a.nodes
                    .into_iter()
                    .find(|att| {
                        att.url
                            .as_ref()
                            .map(|u| u.contains("github.com") && u.contains("/pull/"))
                            .unwrap_or(false)
                    })
                    .and_then(|att| att.url)
            });

        let description = self
            .description
            .map(|d| if d.len() > 500 { d[..500].to_string() } else { d });

        LinearIssue {
            id: self.id,
            identifier: self.identifier,
            title: self.title,
            description,
            branch_name: self.branch_name.filter(|s| !s.is_empty()),
            state,
            pr_url,
        }
    }
}
