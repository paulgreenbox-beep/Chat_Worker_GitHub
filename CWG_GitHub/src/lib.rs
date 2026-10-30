//! Created issue-comment ingress. No shell, model invocation, or chat dispatch.
#![forbid(unsafe_code)]
use cwg_protocol::{GithubSource, Message};
use cwg_security::{Result, decode, verify_signature};
use cwg_state::{Binding, Store};
use serde::Deserialize;
use std::{collections::BTreeSet, net::SocketAddr, path::PathBuf};

// Private typed-boundary seam; generic security decoding remains format-neutral.
fn decode_model<T: serde::de::DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T> {
    decode(bytes, limit)
}

pub mod http;
pub const MAX_EVENT_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub binding: Binding,
    pub author_ids: Vec<u64>,
    /// Explicit replay cutoff at owner registration, in GitHub UTC format.
    pub not_before: String,
    pub installation_id: Option<u64>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub enabled: bool,
    pub listen: SocketAddr,
    pub state_directory: PathBuf,
    pub webhook_secret_env: String,
    pub github_token_env: String,
    pub routes: Vec<Route>,
}
impl Config {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let config: Self = decode_model(bytes, 65_536)?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema != "cwg-github-receiver-v1" || !self.listen.ip().is_loopback()
            || self.listen.port() == 0 || !self.state_directory.is_absolute()
            || self.routes.is_empty() || self.routes.len() > 128 {
            return Err("CONFIG_INVALID");
        }
        for name in [&self.webhook_secret_env, &self.github_token_env] {
            if name.is_empty() || name.len() > 64 || !name.as_bytes()[0].is_ascii_uppercase()
                || !name.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_') {
                return Err("SECRET_ENV_INVALID");
            }
        }
        if self.webhook_secret_env == self.github_token_env { return Err("SECRETS_MUST_DIFFER"); }
        let mut ids = BTreeSet::new(); let mut sources = BTreeSet::new(); let mut goals = BTreeSet::new();
        for r in &self.routes {
            r.binding.validate()?;
            if !ids.insert(&r.binding.id) || !sources.insert((r.binding.repository_id, r.binding.issue_number))
                || !goals.insert((r.binding.repository.to_ascii_lowercase(), &r.binding.goal_id))
                || r.author_ids.is_empty() || r.author_ids.len() > 128 || r.author_ids.contains(&0)
                || r.author_ids.iter().collect::<BTreeSet<_>>().len() != r.author_ids.len()
                || !github_time(&r.not_before) || r.installation_id == Some(0) {
                return Err("ROUTE_INVALID_OR_AMBIGUOUS");
            }
        }
        Ok(())
    }
}

/// Implemented in the Windows binary by a fixed-host HTTPS client. Paths are
/// built ONLY from validated config/IDs, never from URLs supplied in an event.
pub trait GithubApi { fn get(&self, path: &str) -> Result<Vec<u8>>; }

#[derive(Deserialize)]
struct User { id: u64, login: String, #[serde(rename = "type")] kind: String }
#[derive(Deserialize)]
struct Repository { id: u64, full_name: String, owner: User }
#[derive(Deserialize)]
struct Issue { id: u64, number: u64, state: String, locked: bool, pull_request: Option<serde_json::Value> }
#[derive(Deserialize)]
struct Comment { id: u64, body: String, user: User, issue_url: String, created_at: String, updated_at: String }
#[derive(Deserialize)]
struct Permission { permission: String, user: User }
#[derive(Deserialize)]
struct Installation { id: u64 }
#[derive(Deserialize)]
struct Event { action: String, repository: Repository, issue: Issue, comment: Comment, sender: User, installation: Option<Installation> }

pub struct Ingress<'a, A: GithubApi> {
    pub config: &'a Config,
    pub api: &'a A,
    pub store: &'a Store,
    pub secret: &'a [u8],
}
impl<A: GithubApi> Ingress<'_, A> {
    /// true: newly committed; false: identical duplicate. No success before
    /// durable commit, and neither value asserts delivery to a Codex thread.
    pub fn receive(&self, event_name: &str, delivery: &str, signature: &str, raw: &[u8]) -> Result<bool> {
        self.config.validate()?;
        if !self.config.enabled { return Err("RECEIVER_DISABLED"); }
        if raw.len() > MAX_EVENT_BYTES || event_name != "issue_comment"
            || delivery.is_empty() || delivery.len() > 128
            || !delivery.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err("EVENT_ENVELOPE_INVALID");
        }
        verify_signature(self.secret, raw, signature)?;
        let event: Event = decode_model(raw, MAX_EVENT_BYTES)?;
        let (binding, message) = reconcile(self.config, self.api, &event)?;
        self.store.put_verified(binding, delivery, message)
    }
}

fn valid_login(s: &str) -> bool {
    !s.is_empty() && s.len() <= 39 && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
fn user_matches(a: &User, b: &User) -> bool {
    a.id != 0 && a.id == b.id && a.kind == "User" && b.kind == "User"
        && valid_login(&a.login) && a.login.eq_ignore_ascii_case(&b.login)
}
fn repo_matches(r: &Repository, b: &Binding) -> bool {
    r.id == b.repository_id && r.owner.id == b.owner_id && r.full_name.eq_ignore_ascii_case(&b.repository)
}
fn open_issue(i: &Issue, b: &Binding) -> bool {
    i.id != 0 && i.number == b.issue_number && i.state == "open" && !i.locked && i.pull_request.is_none()
}
fn reconcile<'a>(cfg: &'a Config, api: &impl GithubApi, event: &Event) -> Result<(&'a Binding, Message)> {
    if event.action != "created" { return Err("ONLY_CREATED_COMMENTS"); }
    let route = cfg.routes.iter().find(|r| r.binding.repository_id == event.repository.id && r.binding.issue_number == event.issue.number).ok_or("SOURCE_NOT_ALLOWED")?;
    let b = &route.binding;
    if !repo_matches(&event.repository, b) || !open_issue(&event.issue, b)
        || !route.author_ids.contains(&event.sender.id) || !user_matches(&event.sender, &event.comment.user)
        || route.installation_id.is_some_and(|id| event.installation.as_ref().map(|i| i.id) != Some(id)) {
        return Err("ACTOR_OR_SOURCE_NOT_ALLOWED");
    }
    let base = format!("/repos/{}", b.repository);
    let repo: Repository = decode_model(&api.get(&base)?, MAX_EVENT_BYTES)?;
    let issue: Issue = decode_model(&api.get(&format!("{base}/issues/{}", b.issue_number))?, MAX_EVENT_BYTES)?;
    let comment: Comment = decode_model(&api.get(&format!("{base}/issues/comments/{}", event.comment.id))?, MAX_EVENT_BYTES)?;
    if !repo_matches(&repo, b) || !open_issue(&issue, b) || issue.id != event.issue.id
        || comment.id == 0 || comment.id != event.comment.id || !user_matches(&comment.user, &event.sender)
        || !comment.issue_url.eq_ignore_ascii_case(&format!("https://api.github.com{base}/issues/{}", b.issue_number))
        || !event.comment.issue_url.eq_ignore_ascii_case(&comment.issue_url)
        || !github_time(&comment.created_at) || comment.created_at != comment.updated_at
        || comment.created_at < route.not_before || comment.created_at != event.comment.created_at
        || comment.updated_at != event.comment.updated_at || comment.body != event.comment.body {
        return Err("API_RECONCILIATION_FAILED");
    }
    let permission: Permission = decode_model(&api.get(&format!("{base}/collaborators/{}/permission", comment.user.login))?, MAX_EVENT_BYTES)?;
    if !user_matches(&permission.user, &comment.user) || !matches!(permission.permission.as_str(), "write" | "maintain" | "admin") {
        return Err("CURRENT_PERMISSION_DENIED");
    }
    let message = Message {
        schema: "cwg-message-v1".into(), message_id: format!("gh:{}:{}", b.repository_id, comment.id),
        source: GithubSource { repository: b.repository.clone(), issue_number: b.issue_number, comment_id: comment.id, author_id: comment.user.id },
        target: b.target.clone(), goal_id: b.goal_id.clone(), body: comment.body,
    };
    cwg_protocol::validate(&message).map_err(|_| "MESSAGE_INVALID")?;
    Ok((b, message))
}

fn github_time(s: &str) -> bool {
    if s.len() != 20 || !s.is_ascii() { return false; }
    let b = s.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' { return false; }
    if b.iter().enumerate().any(|(i, c)| ![4, 7, 10, 13, 16, 19].contains(&i) && !c.is_ascii_digit()) { return false; }
    let part = |range: std::ops::Range<usize>| s[range].parse::<u32>().ok();
    let (Some(y), Some(m), Some(d), Some(h), Some(min), Some(sec)) = (part(0..4), part(5..7), part(8..10), part(11..13), part(14..16), part(17..19)) else { return false; };
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = match m { 2 if leap => 29, 2 => 28, 4 | 6 | 9 | 11 => 30, 1 | 3 | 5 | 7 | 8 | 10 | 12 => 31, _ => 0 };
    y >= 1970 && d > 0 && d <= days && h < 24 && min < 60 && sec < 60
}

#[cfg(test)]
mod tests;
