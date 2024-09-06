use std::{
    collections::HashMap,
    io::{self, Stdin},
};

use anyhow::{bail, Context, Result};
use git2::Repository;
use ngit::{
    client::{
        get_all_proposal_patch_events_from_cache, get_events_from_cache,
        get_proposals_and_revisions_from_cache,
    },
    git::{
        nostr_url::{CloneUrl, NostrUrlDecoded, ServerProtocol},
        Repo, RepoActions,
    },
    git_events::{
        event_is_revision_root, event_to_cover_letter, get_most_recent_patch_with_ancestors,
        status_kinds,
    },
    repo_ref::RepoRef,
};
use nostr_sdk::{Event, EventId, Kind, PublicKey, Url};

pub fn get_short_git_server_name(git_repo: &Repo, url: &str) -> std::string::String {
    if let Ok(name) = get_remote_name_by_url(&git_repo.git_repo, url) {
        return name;
    }
    if let Ok(url) = Url::parse(url) {
        if let Some(domain) = url.domain() {
            return domain.to_string();
        }
    }
    url.to_string()
}

pub fn get_remote_name_by_url(git_repo: &Repository, url: &str) -> Result<String> {
    let remotes = git_repo.remotes()?;
    Ok(remotes
        .iter()
        .find(|r| {
            if let Some(name) = r {
                if let Some(remote_url) = git_repo.find_remote(name).unwrap().url() {
                    url == remote_url
                } else {
                    false
                }
            } else {
                false
            }
        })
        .context("could not find remote with matching url")?
        .context("remote with matching url must be named")?
        .to_string())
}

pub fn get_oids_from_fetch_batch(
    stdin: &Stdin,
    initial_oid: &str,
    initial_refstr: &str,
) -> Result<HashMap<String, String>> {
    let mut line = String::new();
    let mut batch = HashMap::new();
    batch.insert(initial_refstr.to_string(), initial_oid.to_string());
    loop {
        let tokens = read_line(stdin, &mut line)?;
        match tokens.as_slice() {
            ["fetch", oid, refstr] => {
                batch.insert((*refstr).to_string(), (*oid).to_string());
            }
            [] => break,
            _ => bail!(
                "after a `fetch` command we are only expecting another fetch or an empty line"
            ),
        }
    }
    Ok(batch)
}

/// Read one line from stdin, and split it into tokens.
pub fn read_line<'a>(stdin: &io::Stdin, line: &'a mut String) -> io::Result<Vec<&'a str>> {
    line.clear();

    let read = stdin.read_line(line)?;
    if read == 0 {
        return Ok(vec![]);
    }
    let line = line.trim();
    let tokens = line.split(' ').filter(|t| !t.is_empty()).collect();

    Ok(tokens)
}

pub fn switch_clone_url_between_ssh_and_https(url: &str) -> Result<String> {
    if url.starts_with("https://") {
        // Convert HTTPS to git@ syntax
        let parts: Vec<&str> = url.trim_start_matches("https://").split('/').collect();
        if parts.len() >= 2 {
            // Construct the git@ URL
            Ok(format!("git@{}:{}", parts[0], parts[1..].join("/")))
        } else {
            // If the format is unexpected, return an error
            bail!("Invalid HTTPS URL format: {}", url);
        }
    } else if url.starts_with("ssh://") {
        // Convert SSH to git@ syntax
        let parts: Vec<&str> = url.trim_start_matches("ssh://").split('/').collect();
        if parts.len() >= 2 {
            // Construct the git@ URL
            Ok(format!("git@{}:{}", parts[0], parts[1..].join("/")))
        } else {
            // If the format is unexpected, return an error
            bail!("Invalid SSH URL format: {}", url);
        }
    } else if url.starts_with("git@") {
        // Convert git@ syntax to HTTPS
        let parts: Vec<&str> = url.split(':').collect();
        if parts.len() == 2 {
            // Construct the HTTPS URL
            Ok(format!(
                "https://{}/{}",
                parts[0].trim_end_matches('@'),
                parts[1]
            ))
        } else {
            // If the format is unexpected, return an error
            bail!("Invalid git@ URL format: {}", url);
        }
    } else {
        // If the URL is neither HTTPS, SSH, nor git@, return an error
        bail!("Unsupported URL protocol: {}", url);
    }
}

pub async fn get_open_proposals(
    git_repo: &Repo,
    repo_ref: &RepoRef,
) -> Result<HashMap<EventId, (Event, Vec<Event>)>> {
    let git_repo_path = git_repo.get_path()?;
    let proposals: Vec<nostr::Event> =
        get_proposals_and_revisions_from_cache(git_repo_path, repo_ref.coordinates())
            .await?
            .iter()
            .filter(|e| !event_is_revision_root(e))
            .cloned()
            .collect();

    let statuses: Vec<nostr::Event> = {
        let mut statuses = get_events_from_cache(
            git_repo_path,
            vec![
                nostr::Filter::default()
                    .kinds(status_kinds().clone())
                    .events(proposals.iter().map(nostr::Event::id)),
            ],
        )
        .await?;
        statuses.sort_by_key(|e| e.created_at);
        statuses.reverse();
        statuses
    };
    let mut open_proposals = HashMap::new();

    for proposal in proposals {
        let status = if let Some(e) = statuses
            .iter()
            .filter(|e| {
                status_kinds().contains(&e.kind())
                    && e.tags()
                        .iter()
                        .any(|t| t.as_vec()[1].eq(&proposal.id.to_string()))
            })
            .collect::<Vec<&nostr::Event>>()
            .first()
        {
            e.kind()
        } else {
            Kind::GitStatusOpen
        };
        if status.eq(&Kind::GitStatusOpen) {
            if let Ok(commits_events) =
                get_all_proposal_patch_events_from_cache(git_repo_path, repo_ref, &proposal.id)
                    .await
            {
                if let Ok(most_recent_proposal_patch_chain) =
                    get_most_recent_patch_with_ancestors(commits_events.clone())
                {
                    open_proposals
                        .insert(proposal.id(), (proposal, most_recent_proposal_patch_chain));
                }
            }
        }
    }
    Ok(open_proposals)
}

pub async fn get_all_proposals(
    git_repo: &Repo,
    repo_ref: &RepoRef,
) -> Result<HashMap<EventId, (Event, Vec<Event>)>> {
    let git_repo_path = git_repo.get_path()?;
    let proposals: Vec<nostr::Event> =
        get_proposals_and_revisions_from_cache(git_repo_path, repo_ref.coordinates())
            .await?
            .iter()
            .filter(|e| !event_is_revision_root(e))
            .cloned()
            .collect();

    let mut all_proposals = HashMap::new();

    for proposal in proposals {
        if let Ok(commits_events) =
            get_all_proposal_patch_events_from_cache(git_repo_path, repo_ref, &proposal.id).await
        {
            if let Ok(most_recent_proposal_patch_chain) =
                get_most_recent_patch_with_ancestors(commits_events.clone())
            {
                all_proposals.insert(proposal.id(), (proposal, most_recent_proposal_patch_chain));
            }
        }
    }
    Ok(all_proposals)
}

pub fn find_proposal_and_patches_by_branch_name<'a>(
    refstr: &'a str,
    open_proposals: &'a HashMap<EventId, (Event, Vec<Event>)>,
    current_user: &Option<PublicKey>,
) -> Option<(&'a EventId, &'a (Event, Vec<Event>))> {
    open_proposals.iter().find(|(_, (proposal, _))| {
        if let Ok(cl) = event_to_cover_letter(proposal) {
            if let Ok(mut branch_name) = cl.get_branch_name() {
                branch_name = if let Some(public_key) = current_user {
                    if proposal.author().eq(public_key) {
                        cl.branch_name.to_string()
                    } else {
                        branch_name
                    }
                } else {
                    branch_name
                };
                branch_name.eq(&refstr.replace("refs/heads/", ""))
            } else {
                false
            }
        } else {
            false
        }
    })
}

pub fn join_with_and<T: ToString>(items: &[T]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].to_string(),
        _ => {
            let last_item = items.last().unwrap().to_string();
            let rest = &items[..items.len() - 1];
            format!(
                "{} and {}",
                rest.iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                last_item
            )
        }
    }
}

/// get an ordered vector of server protocols to attempt
pub fn get_read_protocols_to_try(
    server_url: &CloneUrl,
    decoded_nostr_url: &NostrUrlDecoded,
) -> Vec<ServerProtocol> {
    if server_url.protocol() == ServerProtocol::Filesystem {
        vec![(ServerProtocol::Filesystem)]
    } else if let Some(protocol) = &decoded_nostr_url.protocol {
        vec![protocol.clone()]
    } else if server_url.protocol() == ServerProtocol::Http {
        vec![
            ServerProtocol::UnauthHttp,
            ServerProtocol::Ssh,
            ServerProtocol::Http,
        ]
    } else if server_url.protocol() == ServerProtocol::Ftp {
        vec![ServerProtocol::Ftp, ServerProtocol::Ssh]
    } else {
        vec![
            ServerProtocol::UnauthHttps,
            ServerProtocol::Ssh,
            ServerProtocol::Https,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod join_with_and {
        use super::*;
        #[test]
        fn test_empty() {
            let items: Vec<&str> = vec![];
            assert_eq!(join_with_and(&items), "");
        }

        #[test]
        fn test_single_item() {
            let items = vec!["apple"];
            assert_eq!(join_with_and(&items), "apple");
        }

        #[test]
        fn test_two_items() {
            let items = vec!["apple", "banana"];
            assert_eq!(join_with_and(&items), "apple and banana");
        }

        #[test]
        fn test_three_items() {
            let items = vec!["apple", "banana", "cherry"];
            assert_eq!(join_with_and(&items), "apple, banana and cherry");
        }

        #[test]
        fn test_four_items() {
            let items = vec!["apple", "banana", "cherry", "date"];
            assert_eq!(join_with_and(&items), "apple, banana, cherry and date");
        }

        #[test]
        fn test_multiple_items() {
            let items = vec!["one", "two", "three", "four", "five"];
            assert_eq!(join_with_and(&items), "one, two, three, four and five");
        }
    }
}