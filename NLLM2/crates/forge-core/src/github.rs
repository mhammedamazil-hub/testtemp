//! GitHub repositories as sources: the README and the issue list.
//!
//! Forge has no GitHub account, no token, and no OAuth app — this module talks to the public
//! REST API (`api.github.com`) with the same SSRF-guarded fetch path as any other URL, and
//! nothing else. That has three consequences the code states out loud:
//!
//! * **Rate limits are the anonymous ones.** A 403/429 from GitHub is reported as such, with
//!   GitHub's own message, instead of being retried into a wall.
//! * **Only public repositories.** No credentials exist, so a private repo is simply a 404 as
//!   far as Forge is concerned, and the error says which repository was asked for.
//! * **Issues, not code.** Forge indexes issue titles and bodies as dated, numbered items. It
//!   does not clone, browse files, or claim to have read the source tree.
//!
//! The bytes that reach the ingestion pipeline are a small JSON envelope of what was actually
//! collected (`{"repo":…,"readme":…,"issues":[…]}`), which is also what is retained on disk so
//! a retry re-indexes the same collection rather than silently fetching a newer one.

use crate::domain::Locator;
use crate::ingest::{self, Block, Parsed};
use crate::json::{self, Value};
use crate::obj;

/// The issue number as a locator position. u32 because a locator position is an index, and
/// GitHub issue numbers above 4 billion do not exist.
fn issue_pos(number: u64) -> u32 {
    number.min(u32::MAX as u64) as u32
}

/// Hard cap on issues per source: a repository with 40k issues is not a document, and
/// indexing all of them would drown every other source in the notebook.
pub const ISSUE_MAX: usize = 100;
pub const ISSUE_DEFAULT: usize = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct Repo {
    pub owner: String,
    pub name: String,
}

impl Repo {
    pub fn full(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// Accept the shapes a person actually pastes: `owner/name`, a github.com URL (with or
    /// without `.git`), or an SSH clone URL.
    pub fn parse(input: &str) -> Result<Repo, String> {
        let raw = input.trim();
        if raw.is_empty() {
            return Err("repo is required, e.g. \"rust-lang/rust\"".to_string());
        }
        let mut rest = raw.to_string();
        if let Some((scheme, tail)) = raw.split_once("://") {
            let host = tail.split('/').next().unwrap_or("").to_ascii_lowercase();
            let host = host.split('@').next_back().unwrap_or("").to_string();
            if !matches!(host.as_str(), "github.com" | "www.github.com") {
                return Err(format!(
                    "{} is not GitHub: this source reads github.com repositories only",
                    scheme
                ));
            }
        }
        for prefix in ["https://", "http://", "git@", "ssh://git@"] {
            if let Some(stripped) = rest.strip_prefix(prefix) {
                rest = stripped.to_string();
                break;
            }
        }
        // git@host:owner/name
        let rest = match rest.split_once(':') {
            Some((host, tail)) if host.contains("github.com") && !tail.starts_with("//") => tail.to_string(),
            _ => rest,
        };
        let rest = rest.strip_prefix("github.com/").unwrap_or(&rest).to_string();
        let rest = rest.strip_suffix(".git").unwrap_or(&rest).to_string();
        let rest = rest.split(['?', '#']).next().unwrap_or("").trim_end_matches('/').to_string();
        let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() < 2 {
            return Err(format!("\"{}\" is not a repository: expected owner/name", raw));
        }
        let (owner, name) = (parts[0], parts[1]);
        for (label, value) in [("owner", owner), ("repository", name)] {
            if !value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
            {
                return Err(format!("the {} \"{}\" has characters GitHub does not allow", label, value));
            }
        }
        Ok(Repo { owner: owner.to_string(), name: name.to_string() })
    }
}

/// The API root. Overridable so tests can point at a local origin without touching the code
/// path that production uses; the default is the real public API.
pub fn api_base() -> String {
    std::env::var("FORGE_GITHUB_API")
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "https://api.github.com".to_string())
}

pub fn readme_url(repo: &Repo) -> String {
    format!("{}/repos/{}/{}/readme", api_base(), repo.owner, repo.name)
}

pub fn issues_url(repo: &Repo, limit: usize) -> String {
    format!(
        "{}/repos/{}/{}/issues?state=all&per_page={}",
        api_base(),
        repo.owner,
        repo.name,
        limit.min(ISSUE_MAX)
    )
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub author: String,
    pub created: String,
    pub body: String,
    pub pull_request: bool,
}

/// Parse the JSON array returned by `/issues`. A non-array is an error, not an empty list:
/// "GitHub answered with something else" must never look like "this repo has no issues".
pub fn parse_issues(text: &str) -> Result<Vec<Issue>, String> {
    let value = json::parse(text).map_err(|e| format!("GitHub's issue list is not JSON: {}", e))?;
    let arr = value
        .as_arr()
        .ok_or_else(|| "GitHub's issue list is not a JSON array".to_string())?;
    let mut out = Vec::new();
    for item in arr {
        let number = item
            .get("number")
            .and_then(|n| n.as_f64())
            .ok_or_else(|| "an issue has no number".to_string())? as u64;
        out.push(Issue {
            number,
            title: item.str_of("title").unwrap_or("").trim().to_string(),
            state: item.str_of("state").unwrap_or("").trim().to_string(),
            author: item
                .get("user")
                .and_then(|u| u.str_of("login"))
                .unwrap_or("")
                .to_string(),
            created: item.str_of("created_at").unwrap_or("").trim().to_string(),
            body: item.str_of("body").unwrap_or("").to_string(),
            pull_request: item.get("pull_request").map(|p| !p.is_null()).unwrap_or(false),
        });
    }
    Ok(out)
}

/// What is retained on disk and re-parsed by the ingestion pipeline.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Collection {
    pub repo: String,
    pub readme: String,
    pub issues: Vec<Issue>,
    pub skipped_pull_requests: usize,
    pub api_base: String,
}

impl Collection {
    /// The retained envelope. Bodies are kept verbatim so a citation can quote them.
    pub fn to_bytes(&self) -> Vec<u8> {
        let issues: Vec<Value> = self
            .issues
            .iter()
            .map(|i| {
                obj! {
                    "number" => i.number as u64,
                    "title" => i.title.as_str(),
                    "state" => i.state.as_str(),
                    "author" => i.author.as_str(),
                    "created" => i.created.as_str(),
                    "body" => i.body.as_str(),
                }
            })
            .collect();
        let doc = obj! {
            "repo" => self.repo.as_str(),
            "readme" => self.readme.as_str(),
            "issues" => Value::Arr(issues),
            "skipped_pull_requests" => self.skipped_pull_requests as u64,
            "api_base" => self.api_base.as_str(),
        };
        json::to_string(&doc).into_bytes()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Collection, String> {
        let text = ingest::decode_text(bytes);
        let value = json::parse(&text).map_err(|e| format!("the stored GitHub collection is not JSON: {}", e))?;
        let repo = value.str_of("repo").unwrap_or("").to_string();
        if repo.is_empty() {
            return Err("the stored GitHub collection names no repository".to_string());
        }
        let body = value.str_of("readme").unwrap_or("").to_string();
        let mut issues = Vec::new();
        if let Some(arr) = value.get("issues").and_then(|v| v.as_arr()) {
            for item in arr {
                issues.push(Issue {
                    number: item.get("number").and_then(|n| n.as_f64()).unwrap_or(0.0) as u64,
                    title: item.str_of("title").unwrap_or("").to_string(),
                    state: item.str_of("state").unwrap_or("").to_string(),
                    author: item.str_of("author").unwrap_or("").to_string(),
                    created: item.str_of("created").unwrap_or("").to_string(),
                    body: item.str_of("body").unwrap_or("").to_string(),
                    pull_request: false,
                });
            }
        }
        Ok(Collection {
            repo,
            readme: body,
            issues,
            skipped_pull_requests: value
                .get("skipped_pull_requests")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as usize,
            api_base: value.str_of("api_base").unwrap_or("").to_string(),
        })
    }

    /// Blocks: the README first (as sections), then issues in ascending number order, so
    /// "issue 12" means the same thing on every re-index.
    pub fn blocks(&self) -> Vec<Block> {
        let mut blocks: Vec<Block> = Vec::new();
        if !self.readme.trim().is_empty() {
            let mut readme_blocks = ingest::text::markdown_blocks(&self.readme);
            for b in readme_blocks.iter_mut() {
                b.locator.section = Some("README".to_string());
                b.locator.kind = "section".to_string();
            }
            // The first paragraph is the heading; keep it, it is the README's own words.
            blocks.append(&mut readme_blocks);
        }
        let mut issues: Vec<&Issue> = self.issues.iter().collect();
        issues.sort_by_key(|i| i.number);
        for issue in issues {
            let mut text = format!("#{} {}", issue.number, issue.title);
            let mut meta: Vec<String> = Vec::new();
            if !issue.state.is_empty() {
                meta.push(issue.state.clone());
            }
            if !issue.author.is_empty() {
                meta.push(format!("opened by {}", issue.author));
            }
            if !issue.created.is_empty() {
                meta.push(issue.created.clone());
            }
            if !meta.is_empty() {
                text.push('\n');
                text.push_str(&meta.join(" · "));
            }
            let body = issue.body.trim();
            if !body.is_empty() {
                text.push('\n');
                text.push_str(body);
            }
            let section = format!("{} · {}", issue.number, issue.title);
            blocks.push(Block {
                text,
                locator: Locator::issue(issue_pos(issue.number), &section),
            });
        }
        blocks
    }

    pub fn warnings(&self) -> Vec<String> {
        let mut w = Vec::new();
        if self.skipped_pull_requests > 0 {
            w.push(format!(
                "{} pull request(s) were skipped: Forge reads issues, not code review threads",
                self.skipped_pull_requests
            ));
        }
        if self.readme.trim().is_empty() {
            w.push("the repository has no README at the default branch".to_string());
        }
        w.push(format!(
            "GitHub collection for {}: {} issue(s){} (public API, anonymous rate limits)",
            self.repo,
            self.issues.len(),
            if self.readme.trim().is_empty() { String::new() } else { ", README included".to_string() }
        ));
        w
    }
}

/// Build the parsed form used by the ingestion pipeline.
pub fn extract(bytes: &[u8]) -> Result<Parsed, String> {
    let collection = Collection::from_bytes(bytes)?;
    let blocks = ingest::chunk_blocks(collection.blocks());
    if blocks.is_empty() {
        return Err("the repository produced no readable text (no README and no issues)".to_string());
    }
    Ok(Parsed {
        page_count: collection.issues.len() as u32,
        warnings: collection.warnings(),
        blocks,
        tables: Vec::new(),
    })
}

/// A human title for the source row.
pub fn title_for(collection: &Collection, include_readme: bool, include_issues: bool) -> String {
    let parts: Vec<String> = [
        include_readme.then(|| "README".to_string()),
        include_issues.then(|| format!("{} issue(s)", collection.issues.len())),
    ]
    .into_iter()
    .flatten()
    .collect();
    format!("{} — {}", collection.repo, parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_specs_are_parsed_from_the_shapes_people_paste() {
        let expect = Repo { owner: "rust-lang".into(), name: "rust".into() };
        for input in [
            "rust-lang/rust",
            "  rust-lang/rust  ",
            "https://github.com/rust-lang/rust",
            "https://github.com/rust-lang/rust.git",
            "https://github.com/rust-lang/rust/issues?q=1",
            "git@github.com:rust-lang/rust.git",
        ] {
            assert_eq!(Repo::parse(input).unwrap(), expect, "input={}", input);
        }
    }

    #[test]
    fn a_repo_spec_that_is_not_a_repo_is_refused() {
        for bad in ["", "rust", "https://example.com/a/b"] {
            let err = Repo::parse(bad).unwrap_err();
            assert!(!err.is_empty());
        }
        assert!(Repo::parse("owner/na me").unwrap_err().contains("characters"));
    }

    const ISSUES: &str = r#"[
      {"number":12,"title":"Retry budget is not respected","state":"open",
       "user":{"login":"okafor"},"created_at":"2026-03-01T10:00:00Z",
       "body":"With three tiers the client retries 5 times instead of 2."},
      {"number":13,"title":"Add EPUB support","state":"closed",
       "user":{"login":"menon"},"created_at":"2026-03-02T10:00:00Z",
       "body":"Closed by the 3.0 release."},
      {"number":14,"title":"Docs: wrong install line","state":"open",
       "user":{"login":"lee"},"created_at":"2026-03-03T10:00:00Z",
       "body":"","pull_request":{"url":"https://api.github.com/repos/o/n/pulls/14"}}
    ]"#;

    #[test]
    fn issues_keep_numbers_dates_authors_and_bodies_and_pull_requests_are_flagged() {
        let issues = parse_issues(ISSUES).unwrap();
        assert_eq!(issues.len(), 3);
        assert_eq!(issues[0].number, 12);
        assert_eq!(issues[0].author, "okafor");
        assert_eq!(issues[0].created, "2026-03-01T10:00:00Z");
        assert!(issues[0].body.contains("5 times"));
        assert!(!issues[0].pull_request);
        assert!(issues[2].pull_request, "a PR is not an issue");
    }

    #[test]
    fn a_non_array_answer_is_an_error_not_an_empty_list() {
        let err = parse_issues(r#"{"message":"API rate limit exceeded"}"#).unwrap_err();
        assert!(err.contains("not a JSON array"), "{}", err);
        assert!(parse_issues("not json").is_err());
    }

    #[test]
    fn blocks_are_a_readme_section_then_issues_in_number_order() {
        let mut issues = parse_issues(ISSUES).unwrap();
        issues.retain(|i| !i.pull_request);
        issues.reverse(); // deliberately out of order; the locator must not follow the API
        let collection = Collection {
            repo: "acme/retry".into(),
            readme: "# acme/retry\n\nThe retry budget is 2.75 attempts per request.".into(),
            issues,
            skipped_pull_requests: 1,
            api_base: "http://127.0.0.1:1".into(),
        };
        let blocks = collection.blocks();
        assert_eq!(blocks[0].locator.section.as_deref(), Some("README"));
        assert!(
            blocks.iter().any(|b| b.locator.section.as_deref() == Some("README") && b.text.contains("2.75 attempts")),
            "the README body is kept: {:?}",
            blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
        let issues: Vec<&Block> = blocks.iter().filter(|b| b.locator.kind == "issue").collect();
        assert_eq!(issues[0].locator.human(), "issue 12 · Retry budget is not respected");
        assert_eq!(issues[1].locator.human(), "issue 13 · Add EPUB support");
        assert!(issues[0].text.starts_with("#12 Retry budget is not respected\nopen · opened by okafor"));
    }

    #[test]
    fn the_retained_envelope_round_trips_through_bytes() {
        let collection = Collection {
            repo: "acme/retry".into(),
            readme: "# Title\n\nBody with a \"quote\" and a newline\nin it.".into(),
            issues: parse_issues(ISSUES).unwrap().into_iter().filter(|i| !i.pull_request).collect(),
            skipped_pull_requests: 1,
            api_base: "https://api.github.com".into(),
        };
        let bytes = collection.to_bytes();
        let back = Collection::from_bytes(&bytes).unwrap();
        assert_eq!(back, collection);
        let parsed = extract(&bytes).unwrap();
        assert_eq!(parsed.page_count, 2);
        assert!(parsed.warnings.iter().any(|w| w.contains("pull request(s) were skipped")));
        assert!(parsed.warnings.iter().any(|w| w.contains("GitHub collection for acme/retry")));
    }

    #[test]
    fn a_collection_without_text_is_refused_rather_than_indexed_empty() {
        let collection = Collection { repo: "acme/empty".into(), ..Default::default() };
        let err = extract(&collection.to_bytes()).unwrap_err();
        assert!(err.contains("no readable text"), "{}", err);
        assert!(Collection::from_bytes(b"{}").is_err(), "a payload with no repo is refused");
    }

    #[test]
    fn urls_point_at_the_public_api_and_respect_the_issue_cap() {
        let repo = Repo::parse("acme/retry").unwrap();
        std::env::set_var("FORGE_GITHUB_API", "http://127.0.0.1:1/");
        assert_eq!(readme_url(&repo), "http://127.0.0.1:1/repos/acme/retry/readme");
        assert!(issues_url(&repo, 5000).ends_with("per_page=100"), "the cap holds: {}", issues_url(&repo, 5000));
        assert!(issues_url(&repo, 0).ends_with("per_page=0"));
        std::env::remove_var("FORGE_GITHUB_API");
        assert_eq!(readme_url(&repo), "https://api.github.com/repos/acme/retry/readme");
    }
}
