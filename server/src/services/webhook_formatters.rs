pub fn format_github(payload: &serde_json::Value, event: &str) -> String {
    match event {
        "pull_request" => {
            let action = payload["action"].as_str().unwrap_or("unknown");
            let number = payload["pull_request"]["number"].as_u64().unwrap_or(0);
            let title = payload["pull_request"]["title"]
                .as_str()
                .unwrap_or("unknown");
            let url = payload["pull_request"]["html_url"]
                .as_str()
                .unwrap_or("unknown");
            let user = payload["pull_request"]["user"]["login"]
                .as_str()
                .unwrap_or("unknown");
            let base = payload["pull_request"]["base"]["ref"]
                .as_str()
                .unwrap_or("unknown");

            match action {
                "opened" => format!(
                    "GitHub: New PR #{} opened by @{} — \"{}\". Base: {}. {}",
                    number, user, title, base, url
                ),
                "review_requested" => {
                    let reviewer = payload["requested_reviewer"]["login"]
                        .as_str()
                        .unwrap_or("unknown");
                    format!(
                        "GitHub: Review requested on PR #{} — \"{}\" from @{}. {}",
                        number, title, reviewer, url
                    )
                }
                other => format!(
                    "GitHub: PR #{} {} by @{} — \"{}\". {}",
                    number, other, user, title, url
                ),
            }
        }
        "issue_comment" => {
            let user = payload["comment"]["user"]["login"]
                .as_str()
                .unwrap_or("unknown");
            let number = payload["issue"]["number"].as_u64().unwrap_or(0);
            let body = payload["comment"]["body"].as_str().unwrap_or("");
            let url = payload["comment"]["html_url"].as_str().unwrap_or("unknown");
            let truncated_body = if body.len() > 200 {
                format!("{}…", &body[..200])
            } else {
                body.to_string()
            };
            format!(
                "GitHub: @{} commented on issue/PR #{} — \"{}\". {}",
                user, number, truncated_body, url
            )
        }
        "push" => {
            let commits = payload["commits"].as_array().map(|a| a.len()).unwrap_or(0);
            let git_ref = payload["ref"].as_str().unwrap_or("unknown");
            let pusher = payload["pusher"]["name"].as_str().unwrap_or("unknown");
            let latest_message = payload["commits"]
                .as_array()
                .and_then(|a| a.last())
                .and_then(|c| c["message"].as_str())
                .unwrap_or("unknown");
            let url = payload["compare"].as_str().unwrap_or("unknown");
            format!(
                "GitHub: {} commit(s) pushed to {} by @{}. Latest: \"{}\". {}",
                commits, git_ref, pusher, latest_message, url
            )
        }
        "issues" => {
            let action = payload["action"].as_str().unwrap_or("unknown");
            let number = payload["issue"]["number"].as_u64().unwrap_or(0);
            let url = payload["issue"]["html_url"].as_str().unwrap_or("unknown");

            match action {
                "opened" => {
                    let user = payload["issue"]["user"]["login"]
                        .as_str()
                        .unwrap_or("unknown");
                    let title = payload["issue"]["title"].as_str().unwrap_or("unknown");
                    format!(
                        "GitHub: Issue #{} opened by @{} — \"{}\". {}",
                        number, user, title, url
                    )
                }
                other => format!("GitHub: Issue #{} {}. {}", number, other, url),
            }
        }
        other => {
            let action = payload["action"].as_str().unwrap_or("unknown");
            let full_name = payload["repository"]["full_name"]
                .as_str()
                .unwrap_or("unknown");
            format!(
                "GitHub webhook: event={}, action={}. Repo: {}.",
                other, action, full_name
            )
        }
    }
}

pub fn format_gitlab(payload: &serde_json::Value, event: &str) -> String {
    match event {
        "Push Hook" => {
            let user = payload["user_name"].as_str().unwrap_or("unknown");
            let git_ref = payload["ref"].as_str().unwrap_or("unknown");
            let commits = payload["commits"].as_array().map(|a| a.len()).unwrap_or(0);
            let latest_message = payload["commits"]
                .as_array()
                .and_then(|a| a.last())
                .and_then(|c| c["message"].as_str())
                .unwrap_or("unknown");
            let url = payload["repository"]["homepage"]
                .as_str()
                .unwrap_or("unknown");
            format!(
                "GitLab: {} commit(s) pushed to {} by {}. Latest: \"{}\". {}",
                commits, git_ref, user, latest_message, url
            )
        }
        "Merge Request Hook" => {
            let action = payload["object_attributes"]["action"]
                .as_str()
                .unwrap_or("unknown");
            let iid = payload["object_attributes"]["iid"].as_u64().unwrap_or(0);
            let title = payload["object_attributes"]["title"]
                .as_str()
                .unwrap_or("unknown");
            let url = payload["object_attributes"]["url"]
                .as_str()
                .unwrap_or("unknown");
            let user = payload["user"]["name"].as_str().unwrap_or("unknown");
            let branch = payload["object_attributes"]["target_branch"]
                .as_str()
                .unwrap_or("unknown");

            match action {
                "open" => format!(
                    "GitLab: New MR !{} opened by {} — \"{}\". Target: {}. {}",
                    iid, user, title, branch, url
                ),
                "merge" => format!("GitLab: MR !{} merged — \"{}\". {}", iid, title, url),
                other => format!("GitLab: MR !{} {} — \"{}\". {}", iid, other, title, url),
            }
        }
        "Issue Hook" => {
            let action = payload["object_attributes"]["action"]
                .as_str()
                .unwrap_or("unknown");
            let iid = payload["object_attributes"]["iid"].as_u64().unwrap_or(0);
            let title = payload["object_attributes"]["title"]
                .as_str()
                .unwrap_or("unknown");
            let url = payload["object_attributes"]["url"]
                .as_str()
                .unwrap_or("unknown");
            let user = payload["user"]["name"].as_str().unwrap_or("unknown");

            match action {
                "open" => format!(
                    "GitLab: Issue #{} opened by {} — \"{}\". {}",
                    iid, user, title, url
                ),
                other => format!("GitLab: Issue #{} {}. {}", iid, other, url),
            }
        }
        "Note Hook" => {
            let user = payload["user"]["name"].as_str().unwrap_or("unknown");
            let note = payload["object_attributes"]["note"].as_str().unwrap_or("");
            let url = payload["object_attributes"]["url"]
                .as_str()
                .unwrap_or("unknown");
            let truncated_note = if note.len() > 200 {
                format!("{}…", &note[..200])
            } else {
                note.to_string()
            };
            format!(
                "GitLab: {} commented — \"{}\". {}",
                user, truncated_note, url
            )
        }
        other => {
            let namespace = payload["project"]["path_with_namespace"]
                .as_str()
                .unwrap_or("unknown");
            format!("GitLab webhook: event={}. Project: {}.", other, namespace)
        }
    }
}

pub fn format_generic(payload: &serde_json::Value, source: &str) -> String {
    let pretty = serde_json::to_string_pretty(payload).unwrap_or_default();
    let truncated = if pretty.len() > 500 {
        format!("{}…", &pretty[..500])
    } else {
        pretty
    };
    format!("Webhook received from {}: {}", source, truncated)
}

pub fn format(payload: &serde_json::Value, source: &str, event: &str) -> String {
    match source {
        "github" => format_github(payload, event),
        "gitlab" => format_gitlab(payload, event),
        _ => format_generic(payload, source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn format_github_pull_request_opened() {
        let payload = json!({
            "action": "opened",
            "pull_request": {
                "number": 42,
                "title": "Add new feature",
                "html_url": "https://github.com/org/repo/pull/42",
                "user": { "login": "alice" },
                "base": { "ref": "main" }
            }
        });
        let result = format_github(&payload, "pull_request");
        assert!(result.contains("PR #42"), "expected PR number: {}", result);
        assert!(result.contains("@alice"), "expected username: {}", result);
        assert!(
            result.contains("Add new feature"),
            "expected title: {}",
            result
        );
        assert!(result.contains("main"), "expected base branch: {}", result);
        assert!(result.contains("opened"), "expected action: {}", result);
    }

    #[test]
    fn format_github_pull_request_review_requested() {
        let payload = json!({
            "action": "review_requested",
            "pull_request": {
                "number": 7,
                "title": "Fix bug",
                "html_url": "https://github.com/org/repo/pull/7",
                "user": { "login": "bob" },
                "base": { "ref": "main" }
            },
            "requested_reviewer": { "login": "carol" }
        });
        let result = format_github(&payload, "pull_request");
        assert!(result.contains("PR #7"), "expected PR number: {}", result);
        assert!(result.contains("@carol"), "expected reviewer: {}", result);
        assert!(result.contains("Fix bug"), "expected title: {}", result);
    }

    #[test]
    fn format_github_pull_request_other_action() {
        let payload = json!({
            "action": "closed",
            "pull_request": {
                "number": 10,
                "title": "Some PR",
                "html_url": "https://github.com/org/repo/pull/10",
                "user": { "login": "dave" },
                "base": { "ref": "main" }
            }
        });
        let result = format_github(&payload, "pull_request");
        assert!(result.contains("PR #10"), "expected PR number: {}", result);
        assert!(result.contains("closed"), "expected action: {}", result);
        assert!(result.contains("@dave"), "expected username: {}", result);
    }

    #[test]
    fn format_github_issue_comment() {
        let payload = json!({
            "comment": {
                "user": { "login": "eve" },
                "body": "Looks good to me!",
                "html_url": "https://github.com/org/repo/issues/5#issuecomment-1"
            },
            "issue": { "number": 5 }
        });
        let result = format_github(&payload, "issue_comment");
        assert!(result.contains("@eve"), "expected username: {}", result);
        assert!(
            result.contains("issue/PR #5"),
            "expected issue number: {}",
            result
        );
        assert!(
            result.contains("Looks good to me!"),
            "expected body: {}",
            result
        );
    }

    #[test]
    fn format_github_push() {
        let payload = json!({
            "ref": "refs/heads/main",
            "pusher": { "name": "frank" },
            "commits": [
                { "message": "Initial commit" },
                { "message": "Fix typo" }
            ],
            "compare": "https://github.com/org/repo/compare/abc...def"
        });
        let result = format_github(&payload, "push");
        assert!(result.contains("2 commit(s)"), "expected count: {}", result);
        assert!(
            result.contains("refs/heads/main"),
            "expected ref: {}",
            result
        );
        assert!(result.contains("@frank"), "expected pusher: {}", result);
        assert!(
            result.contains("Fix typo"),
            "expected latest message: {}",
            result
        );
    }

    #[test]
    fn format_github_issues_opened() {
        let payload = json!({
            "action": "opened",
            "issue": {
                "number": 99,
                "title": "Something is broken",
                "html_url": "https://github.com/org/repo/issues/99",
                "user": { "login": "grace" }
            }
        });
        let result = format_github(&payload, "issues");
        assert!(
            result.contains("Issue #99"),
            "expected issue number: {}",
            result
        );
        assert!(result.contains("@grace"), "expected username: {}", result);
        assert!(
            result.contains("Something is broken"),
            "expected title: {}",
            result
        );
    }

    #[test]
    fn format_github_issues_other_action() {
        let payload = json!({
            "action": "labeled",
            "issue": {
                "number": 12,
                "html_url": "https://github.com/org/repo/issues/12"
            }
        });
        let result = format_github(&payload, "issues");
        assert!(
            result.contains("Issue #12"),
            "expected issue number: {}",
            result
        );
        assert!(result.contains("labeled"), "expected action: {}", result);
    }

    #[test]
    fn format_github_unknown_event_fallback() {
        let payload = json!({
            "action": "created",
            "repository": { "full_name": "org/repo" }
        });
        let result = format_github(&payload, "star");
        assert!(
            result.contains("event=star"),
            "expected event name: {}",
            result
        );
        assert!(
            result.contains("action=created"),
            "expected action: {}",
            result
        );
        assert!(result.contains("org/repo"), "expected repo: {}", result);
    }

    #[test]
    fn format_generic_basic_output() {
        let payload = json!({ "key": "value" });
        let result = format_generic(&payload, "stripe");
        assert!(
            result.starts_with("Webhook received from stripe:"),
            "expected source prefix: {}",
            result
        );
        assert!(
            result.contains("value"),
            "expected payload content: {}",
            result
        );
    }

    #[test]
    fn format_generic_truncates_long_payload() {
        let long_string: String = "x".repeat(1000);
        let payload = json!({ "data": long_string });
        let result = format_generic(&payload, "custom");
        assert!(
            result.len() < 700,
            "expected truncation, got length {}",
            result.len()
        );
        assert!(result.contains('…'), "expected ellipsis after truncation");
    }

    #[test]
    fn format_dispatcher_routes_github() {
        let payload = json!({
            "action": "opened",
            "pull_request": {
                "number": 1,
                "title": "Test",
                "html_url": "https://github.com/org/repo/pull/1",
                "user": { "login": "user" },
                "base": { "ref": "main" }
            }
        });
        let result = format(&payload, "github", "pull_request");
        assert!(
            result.contains("GitHub:"),
            "should route to github formatter"
        );
    }

    #[test]
    fn format_dispatcher_routes_unknown_source_to_generic() {
        let payload = json!({ "event": "payment.succeeded" });
        let result = format(&payload, "stripe", "charge");
        assert!(
            result.starts_with("Webhook received from stripe:"),
            "should route to generic formatter"
        );
    }

    // --- GitLab formatter tests ---

    #[test]
    fn format_gitlab_push() {
        let payload = json!({
            "user_name": "alice",
            "ref": "refs/heads/main",
            "commits": [
                { "message": "Initial commit" },
                { "message": "Add readme" }
            ],
            "repository": {
                "homepage": "https://gitlab.com/org/repo"
            }
        });
        let result = format_gitlab(&payload, "Push Hook");
        assert!(
            result.contains("2 commit(s)"),
            "expected commit count: {}",
            result
        );
        assert!(
            result.contains("refs/heads/main"),
            "expected ref: {}",
            result
        );
        assert!(result.contains("alice"), "expected user: {}", result);
        assert!(
            result.contains("Add readme"),
            "expected latest message: {}",
            result
        );
        assert!(
            result.contains("https://gitlab.com/org/repo"),
            "expected url: {}",
            result
        );
        assert!(
            result.starts_with("GitLab:"),
            "expected GitLab prefix: {}",
            result
        );
    }

    #[test]
    fn format_gitlab_merge_request_opened() {
        let payload = json!({
            "user": { "name": "bob" },
            "object_attributes": {
                "action": "open",
                "iid": 5,
                "title": "Feature branch",
                "url": "https://gitlab.com/org/repo/-/merge_requests/5",
                "target_branch": "main"
            }
        });
        let result = format_gitlab(&payload, "Merge Request Hook");
        assert!(result.contains("!5"), "expected MR iid: {}", result);
        assert!(result.contains("bob"), "expected user: {}", result);
        assert!(
            result.contains("Feature branch"),
            "expected title: {}",
            result
        );
        assert!(
            result.contains("main"),
            "expected target branch: {}",
            result
        );
        assert!(
            result.contains("opened") || result.contains("New MR"),
            "expected open wording: {}",
            result
        );
    }

    #[test]
    fn format_gitlab_merge_request_merged() {
        let payload = json!({
            "user": { "name": "carol" },
            "object_attributes": {
                "action": "merge",
                "iid": 12,
                "title": "Squash bugs",
                "url": "https://gitlab.com/org/repo/-/merge_requests/12",
                "target_branch": "main"
            }
        });
        let result = format_gitlab(&payload, "Merge Request Hook");
        assert!(result.contains("!12"), "expected MR iid: {}", result);
        assert!(result.contains("merged"), "expected 'merged': {}", result);
        assert!(result.contains("Squash bugs"), "expected title: {}", result);
    }

    #[test]
    fn format_gitlab_issue_opened() {
        let payload = json!({
            "user": { "name": "dave" },
            "object_attributes": {
                "action": "open",
                "iid": 7,
                "title": "Bug report",
                "url": "https://gitlab.com/org/repo/-/issues/7"
            }
        });
        let result = format_gitlab(&payload, "Issue Hook");
        assert!(result.contains("#7"), "expected issue iid: {}", result);
        assert!(result.contains("dave"), "expected user: {}", result);
        assert!(result.contains("Bug report"), "expected title: {}", result);
        assert!(
            result.contains("opened") || result.contains("open"),
            "expected open wording: {}",
            result
        );
    }

    #[test]
    fn format_gitlab_note() {
        let payload = json!({
            "user": { "name": "eve" },
            "object_attributes": {
                "note": "LGTM!",
                "url": "https://gitlab.com/org/repo/-/merge_requests/3#note_1"
            }
        });
        let result = format_gitlab(&payload, "Note Hook");
        assert!(result.contains("eve"), "expected user: {}", result);
        assert!(result.contains("LGTM!"), "expected note: {}", result);
        assert!(
            result.contains("https://gitlab.com/org/repo/-/merge_requests/3#note_1"),
            "expected url: {}",
            result
        );
        assert!(
            result.starts_with("GitLab:"),
            "expected GitLab prefix: {}",
            result
        );
    }

    #[test]
    fn format_gitlab_unknown_event_fallback() {
        let payload = json!({
            "project": {
                "path_with_namespace": "org/myrepo"
            }
        });
        let result = format_gitlab(&payload, "Pipeline Hook");
        assert!(
            result.contains("event=Pipeline Hook"),
            "expected event name: {}",
            result
        );
        assert!(
            result.contains("org/myrepo"),
            "expected project namespace: {}",
            result
        );
        assert!(
            result.starts_with("GitLab webhook:"),
            "expected fallback prefix: {}",
            result
        );
    }

    #[test]
    fn format_dispatcher_routes_gitlab() {
        let payload = json!({
            "user_name": "frank",
            "ref": "refs/heads/dev",
            "commits": [{ "message": "wip" }],
            "repository": { "homepage": "https://gitlab.com/org/repo" }
        });
        let result = format(&payload, "gitlab", "Push Hook");
        assert!(
            result.contains("GitLab:"),
            "should route to gitlab formatter: {}",
            result
        );
        assert!(
            result.contains("frank"),
            "expected user in output: {}",
            result
        );
    }
}
