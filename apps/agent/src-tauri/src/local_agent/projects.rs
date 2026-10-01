use reqwest::blocking::Client;
use serde_json::{json, Map, Value};

use super::connectors;

fn limited_client() -> Result<Client, String> {
    connectors::client()
}

fn jira_base() -> Result<String, String> {
    let conn =
        rusqlite::Connection::open(crate::paths::db_path()?).map_err(|error| error.to_string())?;
    let cloud_id: String = conn
        .query_row(
            "SELECT value FROM config WHERE key='jira_cloud_id'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| "Connect Jira to FlowSight first.".to_string())?;
    Ok(format!(
        "https://api.atlassian.com/ex/jira/{cloud_id}/rest/api/3"
    ))
}

fn update_jira(item_id: &str, status: &str, note: Option<&str>) -> Result<Value, String> {
    let client = limited_client()?;
    let token = crate::jira::get_valid_token()?;
    let base = jira_base()?;
    let issue = urlencoding::encode(item_id);
    let response = client
        .get(format!("{base}/issue/{issue}?fields=status"))
        .bearer_auth(&token)
        .send()
        .map_err(|error| error.to_string())?;
    let current = connectors::checked_json(response, "Jira")?;
    let already_in_status = current["fields"]["status"]["name"]
        .as_str()
        .is_some_and(|name| name.eq_ignore_ascii_case(status));
    if !already_in_status {
        let response = client
            .get(format!("{base}/issue/{issue}/transitions"))
            .bearer_auth(&token)
            .send()
            .map_err(|error| error.to_string())?;
        let options = connectors::checked_json(response, "Jira")?;
        let transition = options["transitions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|item| {
                item["name"]
                    .as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(status))
            })
            .and_then(|item| item["id"].as_str())
            .ok_or_else(|| {
                format!("Jira has no available transition named '{status}' for {item_id}.")
            })?;
        let response = client
            .post(format!("{base}/issue/{issue}/transitions"))
            .bearer_auth(&token)
            .json(&json!({"transition":{"id":transition}}))
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Jira returned HTTP {} while changing status.",
                response.status()
            ));
        }
    }
    let mut result = json!({"provider":"jira","itemId":item_id,"status":status,"statusUpdated":!already_in_status,"alreadyInStatus":already_in_status});
    if let Some(note) = note {
        let response = client.post(format!("{base}/issue/{issue}/comment"))
            .bearer_auth(&token).json(&json!({"body":{"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":note}]}]}}))
            .send();
        match response {
            Ok(response) if response.status().is_success() => result["noteAdded"] = json!(true),
            Ok(response) => {
                result["noteError"] = json!(format!(
                    "Jira returned HTTP {} while adding the note.",
                    response.status()
                ))
            }
            Err(error) => {
                result["noteError"] = json!(format!("Could not add the Jira note: {error}"))
            }
        }
    }
    Ok(result)
}

fn linear_call(
    client: &Client,
    token: &str,
    query: &str,
    variables: Value,
) -> Result<Value, String> {
    let response = client
        .post("https://api.linear.app/graphql")
        .bearer_auth(token)
        .json(&json!({"query":query,"variables":variables}))
        .send()
        .map_err(|error| error.to_string())?;
    let body = connectors::checked_json(response, "Linear")?;
    if let Some(error) = body["errors"].as_array().and_then(|items| items.first()) {
        return Err(format!(
            "Linear: {}",
            error["message"].as_str().unwrap_or("GraphQL error")
        ));
    }
    Ok(body["data"].clone())
}

fn update_linear(item_id: &str, status: &str, note: Option<&str>) -> Result<Value, String> {
    let client = limited_client()?;
    let token = crate::linear::get_linear_token()?;
    let issue = linear_call(
        &client,
        &token,
        "query($id:String!){issue(id:$id){id team{states{nodes{id name}}}}}",
        json!({"id":item_id}),
    )?;
    let issue_id = issue["issue"]["id"]
        .as_str()
        .ok_or("Linear issue not found.")?;
    let state_id = issue["issue"]["team"]["states"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| {
            item["name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(status))
        })
        .and_then(|item| item["id"].as_str())
        .ok_or_else(|| format!("Linear has no state named '{status}' for that issue."))?;
    let changed = linear_call(&client, &token,
        "mutation($id:String!,$stateId:String!){issueUpdate(id:$id,input:{stateId:$stateId}){success}}",
        json!({"id":issue_id,"stateId":state_id}))?;
    if changed["issueUpdate"]["success"] != true {
        return Err("Linear did not update the issue.".into());
    }
    let mut result =
        json!({"provider":"linear","itemId":item_id,"status":status,"statusUpdated":true});
    if let Some(note) = note {
        match linear_call(&client, &token,
            "mutation($issueId:String!,$body:String!){commentCreate(input:{issueId:$issueId,body:$body}){success}}",
            json!({"issueId":issue_id,"body":note})) {
            Ok(body) if body["commentCreate"]["success"] == true => result["noteAdded"] = json!(true),
            Ok(_) => result["noteError"] = json!("Linear did not add the note."),
            Err(error) => result["noteError"] = json!(error),
        }
    }
    Ok(result)
}

fn github_issue_path(item_id: &str) -> Result<String, String> {
    let (repo, number) = item_id
        .split_once('#')
        .ok_or("Use owner/repo#issue-number for GitHub.")?;
    let (owner, name) = repo
        .split_once('/')
        .ok_or("Use owner/repo#issue-number for GitHub.")?;
    if owner.is_empty()
        || name.is_empty()
        || !owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        || number.parse::<u64>().is_err()
    {
        return Err("Use a valid GitHub owner/repo#issue-number.".into());
    }
    Ok(format!(
        "https://api.github.com/repos/{owner}/{name}/issues/{number}"
    ))
}

fn update_github(item_id: &str, status: &str, note: Option<&str>) -> Result<Value, String> {
    let state = match status.to_ascii_lowercase().as_str() {
        "open" | "reopened" => "open",
        "closed" => "closed",
        _ => return Err("GitHub issue status must be open or closed.".into()),
    };
    let token = connectors::credential("github")?;
    let client = limited_client()?;
    let path = github_issue_path(item_id)?;
    let request = |method: reqwest::Method, url: &str| {
        client
            .request(method, url)
            .bearer_auth(&token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "FlowSight-Local-Agent")
    };
    let existing = connectors::checked_json(
        request(reqwest::Method::GET, &path)
            .send()
            .map_err(|error| error.to_string())?,
        "GitHub",
    )?;
    if !existing["pull_request"].is_null() {
        return Err("Use a GitHub issue, not a pull request, for status changes.".into());
    }
    let response = request(reqwest::Method::PATCH, &path)
        .json(&json!({"state":state}))
        .send()
        .map_err(|error| error.to_string())?;
    connectors::checked_json(response, "GitHub")?;
    let mut result =
        json!({"provider":"github","itemId":item_id,"status":state,"statusUpdated":true});
    if let Some(note) = note {
        match request(reqwest::Method::POST, &format!("{path}/comments"))
            .json(&json!({"body":note}))
            .send()
        {
            Ok(response) if response.status().is_success() => result["noteAdded"] = json!(true),
            Ok(response) => {
                result["noteError"] = json!(format!(
                    "GitHub returned HTTP {} while adding the note.",
                    response.status()
                ))
            }
            Err(error) => result["noteError"] = json!(format!("Could not add the note: {error}")),
        }
    }
    Ok(result)
}

fn update_notion(item_id: &str, status: &str, note: Option<&str>) -> Result<Value, String> {
    let token = connectors::credential("notion")?;
    let client = limited_client()?;
    let id = urlencoding::encode(item_id);
    let page = format!("https://api.notion.com/v1/pages/{id}");
    let request = |method: reqwest::Method, url: &str| {
        client
            .request(method, url)
            .bearer_auth(&token)
            .header("Notion-Version", "2022-06-28")
    };
    let existing = connectors::checked_json(
        request(reqwest::Method::GET, &page)
            .send()
            .map_err(|error| error.to_string())?,
        "Notion",
    )?;
    let property = existing["properties"]
        .as_object()
        .and_then(|properties| {
            properties
                .iter()
                .find(|(_, value)| value["type"] == "status")
        })
        .map(|(name, _)| name.clone())
        .ok_or("This Notion page has no Status property.")?;
    let mut properties = Map::new();
    properties.insert(property, json!({"status":{"name":status}}));
    let response = request(reqwest::Method::PATCH, &page)
        .json(&json!({"properties":properties}))
        .send()
        .map_err(|error| error.to_string())?;
    connectors::checked_json(response, "Notion")?;
    let mut result =
        json!({"provider":"notion","itemId":item_id,"status":status,"statusUpdated":true});
    if let Some(note) = note {
        let url = format!("https://api.notion.com/v1/blocks/{id}/children");
        match request(reqwest::Method::PATCH, &url)
            .json(&json!({"children":[{"object":"block","type":"paragraph","paragraph":{"rich_text":[{"type":"text","text":{"content":note}}]}}]}))
            .send() {
            Ok(response) if response.status().is_success() => result["noteAdded"] = json!(true),
            Ok(response) => result["noteError"] = json!(format!("Notion returned HTTP {} while adding the note.", response.status())),
            Err(error) => result["noteError"] = json!(format!("Could not add the note: {error}")),
        }
    }
    Ok(result)
}

pub fn create_subtask(
    provider: &str,
    parent_id: &str,
    title: &str,
    description: Option<&str>,
) -> Result<Value, String> {
    if provider == "jira" || provider == "linear" {
        crate::entitlements::require_feature(&crate::paths::db_path()?, "integrations")?;
    }
    let title = title.trim();
    let description = description.unwrap_or("").trim();
    match provider {
        "github" => {
            let token = connectors::credential("github")?;
            let client = limited_client()?;
            let parent_path = github_issue_path(parent_id)?;
            let request = |method: reqwest::Method, url: &str| {
                client
                    .request(method, url)
                    .bearer_auth(&token)
                    .header("Accept", "application/vnd.github+json")
                    .header("X-GitHub-Api-Version", "2022-11-28")
                    .header("User-Agent", "FlowSight-Local-Agent")
            };
            let parent = connectors::checked_json(
                request(reqwest::Method::GET, &parent_path)
                    .send()
                    .map_err(|error| error.to_string())?,
                "GitHub",
            )?;
            if !parent["pull_request"].is_null() {
                return Err("Choose a GitHub issue as the parent, not a pull request.".into());
            }
            let issues_path = parent_path
                .rsplit_once('/')
                .map(|(base, _)| base)
                .ok_or("Invalid GitHub issue path.")?;
            let parent_url = parent["html_url"].as_str().unwrap_or("");
            let body = if description.is_empty() {
                format!("Parent issue: {parent_url}")
            } else {
                format!("{description}\n\nParent issue: {parent_url}")
            };
            let created = connectors::checked_json(
                request(reqwest::Method::POST, issues_path)
                    .json(&json!({"title":title,"body":body}))
                    .send()
                    .map_err(|error| error.to_string())?,
                "GitHub",
            )?;
            let child_id = created["id"].as_u64()
                .ok_or("GitHub created an issue but did not return its ID. Check the repository before retrying.")?;
            let child_url = created["html_url"].as_str().unwrap_or("");
            let link = request(reqwest::Method::POST, &format!("{parent_path}/sub_issues"))
                .json(&json!({"sub_issue_id":child_id}))
                .send();
            let (linked, link_error) = match link {
                Ok(response) if response.status().is_success() => (true, None),
                Ok(response) => (
                    false,
                    Some(format!(
                        "GitHub returned HTTP {} while linking the child issue.",
                        response.status()
                    )),
                ),
                Err(error) => (
                    false,
                    Some(format!("Could not link the child issue: {error}")),
                ),
            };
            Ok(
                json!({"provider":"github","parentId":parent_id,"childId":child_id,
                "url":child_url,"linked":linked,"linkError":link_error}),
            )
        }
        "linear" => {
            let client = limited_client()?;
            let token = crate::linear::get_linear_token()?;
            let parent = linear_call(
                &client,
                &token,
                "query($id:String!){issue(id:$id){id team{id}}}",
                json!({"id":parent_id}),
            )?;
            let issue_id = parent["issue"]["id"]
                .as_str()
                .ok_or("Linear parent issue not found.")?;
            let team_id = parent["issue"]["team"]["id"]
                .as_str()
                .ok_or("Linear parent team not found.")?;
            let created = linear_call(&client, &token,
                "mutation($input:IssueCreateInput!){issueCreate(input:$input){success issue{id identifier url}}}",
                json!({"input":{"teamId":team_id,"parentId":issue_id,"title":title,"description":description}}))?;
            if created["issueCreate"]["success"] != true {
                return Err("Linear did not create the subtask.".into());
            }
            Ok(json!({"provider":"linear","parentId":parent_id,
                "childId":created["issueCreate"]["issue"]["identifier"],
                "url":created["issueCreate"]["issue"]["url"],"linked":true}))
        }
        "jira" => {
            let client = limited_client()?;
            let token = crate::jira::get_valid_token()?;
            let base = jira_base()?;
            let issue = urlencoding::encode(parent_id);
            let parent = connectors::checked_json(
                client
                    .get(format!("{base}/issue/{issue}?fields=project"))
                    .bearer_auth(&token)
                    .send()
                    .map_err(|error| error.to_string())?,
                "Jira",
            )?;
            let project_id = parent["fields"]["project"]["id"]
                .as_str()
                .ok_or("Jira parent project not found.")?;
            let project_key = parent["fields"]["project"]["key"]
                .as_str()
                .ok_or("Jira parent project key not found.")?;
            let issue_types = connectors::checked_json(
                client
                    .get(format!("{base}/issuetype/project?projectId={project_id}"))
                    .bearer_auth(&token)
                    .send()
                    .map_err(|error| error.to_string())?,
                "Jira",
            )?;
            let subtask_type = issue_types
                .as_array()
                .into_iter()
                .flatten()
                .find(|kind| kind["subtask"] == true)
                .and_then(|kind| kind["id"].as_str())
                .ok_or("This Jira project has no subtask issue type available.")?;
            let mut fields = json!({"project":{"key":project_key},"parent":{"key":parent_id},
                "summary":title,"issuetype":{"id":subtask_type}});
            if !description.is_empty() {
                fields["description"] = json!({"type":"doc","version":1,"content":[
                    {"type":"paragraph","content":[{"type":"text","text":description}]}
                ]});
            }
            let created = connectors::checked_json(
                client
                    .post(format!("{base}/issue"))
                    .bearer_auth(&token)
                    .json(&json!({"fields":fields}))
                    .send()
                    .map_err(|error| error.to_string())?,
                "Jira",
            )?;
            let child_key = created["key"].as_str()
                .ok_or("Jira created a subtask but did not return its key. Check the project before retrying.")?;
            Ok(json!({"provider":"jira","parentId":parent_id,"childId":child_key,"linked":true}))
        }
        "notion" => {
            let token = connectors::credential("notion")?;
            let client = limited_client()?;
            let parent = urlencoding::encode(parent_id);
            connectors::checked_json(
                client
                    .get(format!("https://api.notion.com/v1/pages/{parent}"))
                    .bearer_auth(&token)
                    .header("Notion-Version", "2022-06-28")
                    .send()
                    .map_err(|error| error.to_string())?,
                "Notion",
            )?;
            let mut payload = json!({"parent":{"type":"page_id","page_id":parent_id},
                "properties":{"title":{"type":"title","title":[{"type":"text","text":{"content":title}}]}}});
            if !description.is_empty() {
                payload["children"] = json!([{"object":"block","type":"paragraph",
                    "paragraph":{"rich_text":[{"type":"text","text":{"content":description}}]}}]);
            }
            let created = connectors::checked_json(
                client
                    .post("https://api.notion.com/v1/pages")
                    .bearer_auth(&token)
                    .header("Notion-Version", "2022-06-28")
                    .json(&payload)
                    .send()
                    .map_err(|error| error.to_string())?,
                "Notion",
            )?;
            let child_id = created["id"].as_str().ok_or(
                "Notion created a page but did not return its ID. Check Notion before retrying.",
            )?;
            Ok(
                json!({"provider":"notion","parentId":parent_id,"childId":child_id,
                "url":created["url"],"linked":true}),
            )
        }
        _ => Err("Unsupported project provider.".into()),
    }
}

pub fn update_status(
    provider: &str,
    item_id: &str,
    status: &str,
    note: Option<&str>,
) -> Result<Value, String> {
    if provider == "jira" || provider == "linear" {
        crate::entitlements::require_feature(&crate::paths::db_path()?, "integrations")?;
    }
    match provider {
        "jira" => update_jira(item_id, status, note),
        "linear" => update_linear(item_id, status, note),
        "github" => update_github(item_id, status, note),
        "notion" => update_notion(item_id, status, note),
        _ => Err("Unsupported project provider.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_issue_target_cannot_escape_the_api_path() {
        assert_eq!(
            github_issue_path("mancasvel/flowsight#12").unwrap(),
            "https://api.github.com/repos/mancasvel/flowsight/issues/12"
        );
        assert!(github_issue_path("mancasvel/../evil#12").is_err());
        assert!(github_issue_path("mancasvel/repo#12/comments").is_err());
    }
}
