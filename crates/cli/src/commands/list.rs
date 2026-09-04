//! `skl list` — local skills from state.db, plus remote presence when logged in.

use std::collections::BTreeSet;

use crate::api::ApiClient;
use crate::auth;
use crate::config::Paths;
use crate::error::Result;
use crate::local::db::LocalDb;

pub async fn run(api_base: String) -> Result<()> {
    let paths = Paths::resolve()?;
    if !paths.db_file.exists() {
        eprintln!("no local skill index; run `skl init` first");
        return Ok(());
    }

    let db = LocalDb::open(&paths.db_file)?;
    let local = db.list_skills()?;

    let remote_names = match auth::load_device_token() {
        Ok(token) => match fetch_remote_names(&api_base, &token).await {
            Ok(names) => Some(names),
            Err(err) => {
                eprintln!("remote list skipped: {err}");
                None
            }
        },
        Err(_) => None,
    };

    if local.is_empty() {
        println!("(no local skills)");
    } else {
        println!(
            "{:<24} {:<16} {:<8} {:<40} {}",
            "name", "tree_hash", "source", "path", "remote"
        );
        for skill in &local {
            let short = skill.tree.tree_hash.get(..12).unwrap_or(&skill.tree.tree_hash);
            let remote = match &remote_names {
                Some(names) if names.contains(&skill.name) => "yes",
                Some(_) => "no",
                None => "-",
            };
            println!(
                "{:<24} {:<16} {:<8} {:<40} {}",
                skill.name,
                short,
                skill.source,
                skill.path.display(),
                remote
            );
        }
    }

    if let Some(names) = remote_names {
        let local_names: BTreeSet<&str> = local.iter().map(|s| s.name.as_str()).collect();
        let remote_only: Vec<_> = names
            .iter()
            .filter(|name| !local_names.contains(name.as_str()))
            .cloned()
            .collect();
        if !remote_only.is_empty() {
            println!();
            println!("remote only:");
            for name in remote_only {
                println!("  {name}");
            }
        }
    }

    Ok(())
}

async fn fetch_remote_names(api_base: &str, token: &str) -> Result<BTreeSet<String>> {
    let client = ApiClient::new(api_base)?.with_token(token);
    let list = client.list_skills().await?;
    Ok(list.skills.into_iter().map(|s| s.name).collect())
}
