use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use ngit::login::SignerInfo;

use crate::sub_commands;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    /// remote signer address
    #[arg(long, global = true)]
    pub bunker_uri: Option<String>,
    /// remote signer app secret key
    #[arg(long, global = true)]
    pub bunker_app_key: Option<String>,
    /// nsec or hex private key
    #[arg(short, long, global = true)]
    pub nsec: Option<String>,
    /// password to decrypt nsec
    #[arg(short, long, global = true)]
    pub password: Option<String>,
    /// disable spinner animations
    #[arg(long, action)]
    pub disable_cli_spinners: bool,
}

pub fn extract_signer_cli_arguments(args: &Cli) -> Result<Option<SignerInfo>> {
    if let Some(nsec) = &args.nsec {
        Ok(Some(SignerInfo::Nsec {
            nsec: nsec.to_string(),
            password: None,
            npub: None,
        }))
    } else if let Some(bunker_uri) = args.bunker_uri.clone() {
        if let Some(bunker_app_key) = args.bunker_app_key.clone() {
            Ok(Some(SignerInfo::Bunker {
                bunker_uri,
                bunker_app_key,
                npub: None,
            }))
        } else {
            bail!("cli argument bunker-app-key must be supplied when bunker-uri is")
        }
    } else if args.bunker_app_key.is_some() {
        bail!("cli argument bunker-uri must be supplied when bunker-app-key is")
    } else {
        Ok(None)
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// update cache with latest updates from nostr
    Fetch(sub_commands::fetch::SubCommandArgs),
    /// signal you are this repo's maintainer accepting proposals via nostr
    Init(sub_commands::init::SubCommandArgs),
    /// issue commits as a proposal
    Send(sub_commands::send::SubCommandArgs),
    /// list proposals; checkout, apply or download selected
    List,
    /// send proposal revision
    Push(sub_commands::push::SubCommandArgs),
    /// fetch and apply new proposal commits / revisions linked to branch
    Pull,
    /// run with --nsec flag to change npub
    Login(sub_commands::login::SubCommandArgs),
}
