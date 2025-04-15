#![cfg_attr(not(test), warn(clippy::pedantic))]
#![allow(clippy::large_futures)]
#![allow(clippy::single_match_else)]
#![cfg_attr(not(test), warn(clippy::expect_used))]

use anyhow::Result;
use clap::Parser;
use gnostr_ngit::{
    cli::{AccountCommands, Cli, Commands},
    cli_interactor, client, git, git_events, login, repo_ref, sub_commands,
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Account(args) => match &args.account_command {
            AccountCommands::Login(sub_args) => sub_commands::login::launch(sub_args).await,
            AccountCommands::Logout => sub_commands::logout::launch().await,
            AccountCommands::ExportKeys => sub_commands::export_keys::launch().await,
        },
        Commands::Init(args) => sub_commands::init::launch(args).await,
        Commands::List => sub_commands::list::launch().await,
        Commands::Send(args) => sub_commands::send::launch(args, false).await,
    }
}
