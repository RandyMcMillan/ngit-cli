use anyhow::Result;
use futures::join;
use serial_test::serial;
use test_utils::{git::GitTestRepo, relay::Relay, *};

static FEATURE_BRANCH_NAME_1: &str = "feature-example-t";
static FEATURE_BRANCH_NAME_2: &str = "feature-example-f";
static FEATURE_BRANCH_NAME_3: &str = "feature-example-c";
static FEATURE_BRANCH_NAME_4: &str = "feature-example-d";

static PROPOSAL_TITLE_1: &str = "proposal a";
static PROPOSAL_TITLE_2: &str = "proposal b";
static PROPOSAL_TITLE_3: &str = "proposal c";

fn cli_tester_create_proposals() -> Result<GitTestRepo> {
    let git_repo = GitTestRepo::default();
    git_repo.populate()?;
    cli_tester_create_proposal(
        &git_repo,
        FEATURE_BRANCH_NAME_1,
        "a",
        Some((PROPOSAL_TITLE_1, "proposal a description")),
    )?;
    cli_tester_create_proposal(
        &git_repo,
        FEATURE_BRANCH_NAME_2,
        "b",
        Some((PROPOSAL_TITLE_2, "proposal b description")),
    )?;
    cli_tester_create_proposal(
        &git_repo,
        FEATURE_BRANCH_NAME_3,
        "c",
        Some((PROPOSAL_TITLE_3, "proposal c description")),
    )?;
    Ok(git_repo)
}

fn create_and_populate_branch(
    test_repo: &GitTestRepo,
    branch_name: &str,
    prefix: &str,
    only_one_commit: bool,
) -> Result<()> {
    test_repo.checkout("main")?;
    test_repo.create_branch(branch_name)?;
    test_repo.checkout(branch_name)?;
    std::fs::write(
        test_repo.dir.join(format!("{}3.md", prefix)),
        "some content",
    )?;
    test_repo.stage_and_commit(format!("add {}3.md", prefix).as_str())?;
    if !only_one_commit {
        std::fs::write(
            test_repo.dir.join(format!("{}4.md", prefix)),
            "some content",
        )?;
        test_repo.stage_and_commit(format!("add {}4.md", prefix).as_str())?;
    }
    Ok(())
}

fn cli_tester_create_proposal(
    test_repo: &GitTestRepo,
    branch_name: &str,
    prefix: &str,
    cover_letter_title_and_description: Option<(&str, &str)>,
) -> Result<()> {
    create_and_populate_branch(test_repo, branch_name, prefix, false)?;

    if let Some((title, description)) = cover_letter_title_and_description {
        let mut p = CliTester::new_from_dir(
            &test_repo.dir,
            [
                "--nsec",
                TEST_KEY_1_NSEC,
                "--password",
                TEST_PASSWORD,
                "--disable-cli-spinners",
                "send",
                "--title",
                format!("\"{title}\"").as_str(),
                "--description",
                format!("\"{description}\"").as_str(),
            ],
        );
        p.expect_end_eventually()?;
    } else {
        let mut p = CliTester::new_from_dir(
            &test_repo.dir,
            [
                "--nsec",
                TEST_KEY_1_NSEC,
                "--password",
                TEST_PASSWORD,
                "--disable-cli-spinners",
                "send",
                "--no-cover-letter",
            ],
        );
        p.expect_end_eventually()?;
    }
    Ok(())
}

mod cannot_find_repo_event {
    use super::*;
    mod cli_prompts {
        use nostr::{
            nips::{nip01::Coordinate, nip19::Nip19Event},
            ToBech32,
        };

        use super::*;
        async fn run_async_repo_event_ref_needed(
            invalid_input: bool,
            nevent: bool,
            naddr: bool,
        ) -> Result<()> {
            let (mut r51, mut r52, mut r53, mut r55, mut r56) = (
                Relay::new(8051, None, None),
                Relay::new(8052, None, None),
                Relay::new(8053, None, None),
                Relay::new(8055, None, None),
                Relay::new(8056, None, None),
            );

            r51.events.push(generate_test_key_1_relay_list_event());
            r51.events.push(generate_test_key_1_metadata_event("fred"));

            r55.events.push(generate_test_key_1_relay_list_event());
            r55.events.push(generate_test_key_1_metadata_event("fred"));

            let repo_event = generate_repo_ref_event();
            r56.events.push(repo_event.clone());

            let cli_tester_handle = std::thread::spawn(move || -> Result<()> {
                let test_repo = GitTestRepo::default();
                test_repo.populate()?;
                let mut p = CliTester::new_from_dir(&test_repo.dir, ["list"]);

                p.expect("cannot find repo event\r\n")?;

                if invalid_input {
                    let mut input = p.expect_input("repository naddr or nevent")?;
                    input.succeeds_with("dfgvfvfzadvd")?;
                    p.expect("not a valid nevent or naddr\r\n")?;
                    let _ = p.expect_input("repository naddr or nevent")?;
                    p.exit()?;
                }
                if nevent {
                    let mut input = p.expect_input("repository naddr or nevent")?;
                    input.succeeds_with(
                        &Nip19Event {
                            event_id: repo_event.id,
                            author: Some(TEST_KEY_1_KEYS.public_key()),
                            relays: vec!["ws://localhost:8056".to_string()],
                        }
                        .to_bech32()?,
                    )?;
                    p.expect("finding proposals...\r\n")?;
                    p.expect_end_with("no proposals found... create one? try `ngit send`\r\n")?;
                }
                if naddr {
                    let mut input = p.expect_input("repository naddr or nevent")?;
                    input.succeeds_with(
                        &Coordinate {
                            kind: nostr::Kind::Custom(REPOSITORY_KIND),
                            pubkey: TEST_KEY_1_KEYS.public_key(),
                            identifier: repo_event.identifier().unwrap().to_string(),
                            relays: vec!["ws://localhost:8056".to_string()],
                        }
                        .to_bech32()?,
                    )?;
                    p.expect("finding proposals...\r\n")?;
                    p.expect_end_with("no proposals found... create one? try `ngit send`\r\n")?;
                    p.expect_end_eventually()?;
                }

                for p in [51, 52, 53, 55, 56] {
                    relay::shutdown_relay(8000 + p)?;
                }
                Ok(())
            });

            // launch relay
            let _ = join!(
                r51.listen_until_close(),
                r52.listen_until_close(),
                r53.listen_until_close(),
                r55.listen_until_close(),
                r56.listen_until_close(),
            );
            cli_tester_handle.join().unwrap()?;
            Ok(())
        }

        #[tokio::test]
        #[serial]
        async fn warns_not_valid_input_and_asks_again() -> Result<()> {
            let _ = run_async_repo_event_ref_needed(true, false, false).await;
            Ok(())
        }

        #[tokio::test]
        #[serial]
        async fn finds_based_on_nevent_on_embeded_relay() -> Result<()> {
            let _ = run_async_repo_event_ref_needed(false, true, false).await;
            Ok(())
        }

        #[tokio::test]
        #[serial]
        async fn finds_based_on_naddr_on_embeded_relay() -> Result<()> {
            let _ = run_async_repo_event_ref_needed(false, false, true).await;
            Ok(())
        }
    }
}
mod when_main_branch_is_uptodate {
    use super::*;

    mod when_proposal_branch_doesnt_exist {
        use super::*;

        mod when_main_is_checked_out {
            use super::*;

            mod when_first_proposal_selected {
                use super::*;

                // TODO: test when other proposals with the same name but from other
                // repositories are       present on relays
                async fn prep_and_run() -> Result<(GitTestRepo, GitTestRepo)> {
                    // fallback (51,52) user write (53, 55) repo (55, 56)
                    let (mut r51, mut r52, mut r53, mut r55, mut r56) = (
                        Relay::new(8051, None, None),
                        Relay::new(8052, None, None),
                        Relay::new(8053, None, None),
                        Relay::new(8055, None, None),
                        Relay::new(8056, None, None),
                    );

                    r51.events.push(generate_test_key_1_relay_list_event());
                    r51.events.push(generate_test_key_1_metadata_event("fred"));
                    r51.events.push(generate_repo_ref_event());

                    r55.events.push(generate_repo_ref_event());
                    r55.events.push(generate_test_key_1_metadata_event("fred"));
                    r55.events.push(generate_test_key_1_relay_list_event());

                    let cli_tester_handle =
                        std::thread::spawn(move || -> Result<(GitTestRepo, GitTestRepo)> {
                            let originating_repo = cli_tester_create_proposals()?;

                            let test_repo = GitTestRepo::default();
                            test_repo.populate()?;
                            let mut p = CliTester::new_from_dir(&test_repo.dir, ["list"]);

                            p.expect("finding proposals...\r\n")?;
                            let mut c = p.expect_choice(
                                "all proposals",
                                vec![
                                    format!("\"{PROPOSAL_TITLE_1}\""),
                                    format!("\"{PROPOSAL_TITLE_2}\""),
                                    format!("\"{PROPOSAL_TITLE_3}\""),
                                ],
                            )?;
                            c.succeeds_with(0, true)?;
                            let mut confirm =
                                p.expect_confirm_eventually("check out branch?", Some(true))?;
                            confirm.succeeds_with(None)?;
                            p.expect_end_eventually_and_print()?;

                            for p in [51, 52, 53, 55, 56] {
                                relay::shutdown_relay(8000 + p)?;
                            }
                            Ok((originating_repo, test_repo))
                        });

                    // launch relay
                    let _ = join!(
                        r51.listen_until_close(),
                        r52.listen_until_close(),
                        r53.listen_until_close(),
                        r55.listen_until_close(),
                        r56.listen_until_close(),
                    );
                    let res = cli_tester_handle.join().unwrap()?;

                    Ok(res)
                }

                mod cli_prompts {
                    use super::*;
                    async fn run_async_prompts_to_choose_from_proposal_titles() -> Result<()> {
                        let (mut r51, mut r52, mut r53, mut r55, mut r56) = (
                            Relay::new(8051, None, None),
                            Relay::new(8052, None, None),
                            Relay::new(8053, None, None),
                            Relay::new(8055, None, None),
                            Relay::new(8056, None, None),
                        );

                        r51.events.push(generate_test_key_1_relay_list_event());
                        r51.events.push(generate_test_key_1_metadata_event("fred"));
                        r51.events.push(generate_repo_ref_event());

                        r55.events.push(generate_repo_ref_event());
                        r55.events.push(generate_test_key_1_metadata_event("fred"));
                        r55.events.push(generate_test_key_1_relay_list_event());

                        let cli_tester_handle = std::thread::spawn(move || -> Result<()> {
                            cli_tester_create_proposals()?;

                            let test_repo = GitTestRepo::default();
                            test_repo.populate()?;
                            let mut p = CliTester::new_from_dir(&test_repo.dir, ["list"]);

                            p.expect("finding proposals...\r\n")?;
                            let mut c = p.expect_choice(
                                "all proposals",
                                vec![
                                    format!("\"{PROPOSAL_TITLE_1}\""),
                                    format!("\"{PROPOSAL_TITLE_2}\""),
                                    format!("\"{PROPOSAL_TITLE_3}\""),
                                ],
                            )?;
                            c.succeeds_with(0, true)?;
                            p.expect("finding commits...\r\n")?;
                            let mut confirm = p.expect_confirm("check out branch?", Some(true))?;
                            confirm.succeeds_with(None)?;
                            p.expect("checked out proposal branch. pulled 2 new commits\r\n")?;
                            p.expect_end()?;

                            for p in [51, 52, 53, 55, 56] {
                                relay::shutdown_relay(8000 + p)?;
                            }
                            Ok(())
                        });

                        // launch relay
                        let _ = join!(
                            r51.listen_until_close(),
                            r52.listen_until_close(),
                            r53.listen_until_close(),
                            r55.listen_until_close(),
                            r56.listen_until_close(),
                        );
                        cli_tester_handle.join().unwrap()?;
                        println!("{:?}", r55.events);
                        Ok(())
                    }

                    #[tokio::test]
                    #[serial]
                    async fn prompts_to_choose_from_proposal_titles() -> Result<()> {
                        let _ = run_async_prompts_to_choose_from_proposal_titles().await;
                        Ok(())
                    }
                }

                #[tokio::test]
                #[serial]
                async fn proposal_branch_created_with_correct_name() -> Result<()> {
                    let (_, test_repo) = prep_and_run().await?;
                    assert_eq!(
                        vec![FEATURE_BRANCH_NAME_1, "main"],
                        test_repo.get_local_branch_names()?
                    );
                    Ok(())
                }

                #[tokio::test]
                #[serial]
                async fn proposal_branch_checked_out() -> Result<()> {
                    let (_, test_repo) = prep_and_run().await?;
                    assert_eq!(
                        FEATURE_BRANCH_NAME_1,
                        test_repo.get_checked_out_branch_name()?,
                    );
                    Ok(())
                }

                #[tokio::test]
                #[serial]
                async fn proposal_branch_tip_is_most_recent_patch() -> Result<()> {
                    let (originating_repo, test_repo) = prep_and_run().await?;
                    assert_eq!(
                        originating_repo.get_tip_of_local_branch(FEATURE_BRANCH_NAME_1)?,
                        test_repo.get_tip_of_local_branch(FEATURE_BRANCH_NAME_1)?,
                    );
                    Ok(())
                }
            }
            mod when_third_proposal_selected {
                use super::*;

                async fn prep_and_run() -> Result<(GitTestRepo, GitTestRepo)> {
                    // fallback (51,52) user write (53, 55) repo (55, 56)
                    let (mut r51, mut r52, mut r53, mut r55, mut r56) = (
                        Relay::new(8051, None, None),
                        Relay::new(8052, None, None),
                        Relay::new(8053, None, None),
                        Relay::new(8055, None, None),
                        Relay::new(8056, None, None),
                    );

                    r51.events.push(generate_test_key_1_relay_list_event());
                    r51.events.push(generate_test_key_1_metadata_event("fred"));
                    r51.events.push(generate_repo_ref_event());

                    r55.events.push(generate_repo_ref_event());
                    r55.events.push(generate_test_key_1_metadata_event("fred"));
                    r55.events.push(generate_test_key_1_relay_list_event());

                    let cli_tester_handle =
                        std::thread::spawn(move || -> Result<(GitTestRepo, GitTestRepo)> {
                            let originating_repo = cli_tester_create_proposals()?;

                            let test_repo = GitTestRepo::default();
                            test_repo.populate()?;
                            let mut p = CliTester::new_from_dir(&test_repo.dir, ["list"]);

                            p.expect("finding proposals...\r\n")?;
                            let mut c = p.expect_choice(
                                "all proposals",
                                vec![
                                    format!("\"{PROPOSAL_TITLE_1}\""),
                                    format!("\"{PROPOSAL_TITLE_2}\""),
                                    format!("\"{PROPOSAL_TITLE_3}\""),
                                ],
                            )?;
                            c.succeeds_with(2, true)?;
                            let mut confirm =
                                p.expect_confirm_eventually("check out branch?", Some(true))?;
                            confirm.succeeds_with(None)?;
                            p.expect_end_eventually_and_print()?;

                            for p in [51, 52, 53, 55, 56] {
                                relay::shutdown_relay(8000 + p)?;
                            }
                            Ok((originating_repo, test_repo))
                        });

                    // launch relay
                    let _ = join!(
                        r51.listen_until_close(),
                        r52.listen_until_close(),
                        r53.listen_until_close(),
                        r55.listen_until_close(),
                        r56.listen_until_close(),
                    );
                    let res = cli_tester_handle.join().unwrap()?;

                    Ok(res)
                }

                mod cli_prompts {
                    use super::*;
                    async fn run_async_prompts_to_choose_from_proposal_titles() -> Result<()> {
                        let (mut r51, mut r52, mut r53, mut r55, mut r56) = (
                            Relay::new(8051, None, None),
                            Relay::new(8052, None, None),
                            Relay::new(8053, None, None),
                            Relay::new(8055, None, None),
                            Relay::new(8056, None, None),
                        );

                        r51.events.push(generate_test_key_1_relay_list_event());
                        r51.events.push(generate_test_key_1_metadata_event("fred"));
                        r51.events.push(generate_repo_ref_event());

                        r55.events.push(generate_repo_ref_event());
                        r55.events.push(generate_test_key_1_metadata_event("fred"));
                        r55.events.push(generate_test_key_1_relay_list_event());

                        let cli_tester_handle = std::thread::spawn(move || -> Result<()> {
                            cli_tester_create_proposals()?;

                            let test_repo = GitTestRepo::default();
                            test_repo.populate()?;
                            let mut p = CliTester::new_from_dir(&test_repo.dir, ["list"]);

                            p.expect("finding proposals...\r\n")?;
                            let mut c = p.expect_choice(
                                "all proposals",
                                vec![
                                    format!("\"{PROPOSAL_TITLE_1}\""),
                                    format!("\"{PROPOSAL_TITLE_2}\""),
                                    format!("\"{PROPOSAL_TITLE_3}\""),
                                ],
                            )?;
                            c.succeeds_with(2, true)?;
                            p.expect("finding commits...\r\n")?;
                            let mut confirm = p.expect_confirm("check out branch?", Some(true))?;
                            confirm.succeeds_with(None)?;
                            p.expect("checked out proposal branch. pulled 2 new commits\r\n")?;
                            p.expect_end()?;

                            for p in [51, 52, 53, 55, 56] {
                                relay::shutdown_relay(8000 + p)?;
                            }
                            Ok(())
                        });

                        // launch relay
                        let _ = join!(
                            r51.listen_until_close(),
                            r52.listen_until_close(),
                            r53.listen_until_close(),
                            r55.listen_until_close(),
                            r56.listen_until_close(),
                        );
                        cli_tester_handle.join().unwrap()?;
                        println!("{:?}", r55.events);
                        Ok(())
                    }

                    #[tokio::test]
                    #[serial]
                    async fn prompts_to_choose_from_proposal_titles() -> Result<()> {
                        let _ = run_async_prompts_to_choose_from_proposal_titles().await;
                        Ok(())
                    }
                }

                #[tokio::test]
                #[serial]
                async fn proposal_branch_created_with_correct_name() -> Result<()> {
                    let (_, test_repo) = prep_and_run().await?;
                    assert_eq!(
                        vec![FEATURE_BRANCH_NAME_3, "main"],
                        test_repo.get_local_branch_names()?
                    );
                    Ok(())
                }

                #[tokio::test]
                #[serial]
                async fn proposal_branch_checked_out() -> Result<()> {
                    let (_, test_repo) = prep_and_run().await?;
                    assert_eq!(
                        FEATURE_BRANCH_NAME_3,
                        test_repo.get_checked_out_branch_name()?,
                    );
                    Ok(())
                }

                #[tokio::test]
                #[serial]
                async fn proposal_branch_tip_is_most_recent_patch() -> Result<()> {
                    let (originating_repo, test_repo) = prep_and_run().await?;
                    assert_eq!(
                        originating_repo.get_tip_of_local_branch(FEATURE_BRANCH_NAME_3)?,
                        test_repo.get_tip_of_local_branch(FEATURE_BRANCH_NAME_3)?,
                    );
                    Ok(())
                }
            }
            mod when_forth_proposal_has_no_cover_letter {
                use super::*;

                async fn prep_and_run() -> Result<(GitTestRepo, GitTestRepo)> {
                    // fallback (51,52) user write (53, 55) repo (55, 56)
                    let (mut r51, mut r52, mut r53, mut r55, mut r56) = (
                        Relay::new(8051, None, None),
                        Relay::new(8052, None, None),
                        Relay::new(8053, None, None),
                        Relay::new(8055, None, None),
                        Relay::new(8056, None, None),
                    );

                    r51.events.push(generate_test_key_1_relay_list_event());
                    r51.events.push(generate_test_key_1_metadata_event("fred"));
                    r51.events.push(generate_repo_ref_event());

                    r55.events.push(generate_repo_ref_event());
                    r55.events.push(generate_test_key_1_metadata_event("fred"));
                    r55.events.push(generate_test_key_1_relay_list_event());

                    let cli_tester_handle =
                        std::thread::spawn(move || -> Result<(GitTestRepo, GitTestRepo)> {
                            let originating_repo = cli_tester_create_proposals()?;
                            cli_tester_create_proposal(
                                &originating_repo,
                                FEATURE_BRANCH_NAME_4,
                                "d",
                                None,
                            )?;
                            let test_repo = GitTestRepo::default();
                            test_repo.populate()?;
                            let mut p = CliTester::new_from_dir(&test_repo.dir, ["list"]);

                            p.expect("finding proposals...\r\n")?;
                            let mut c = p.expect_choice(
                                "all proposals",
                                vec![
                                    format!("\"{PROPOSAL_TITLE_1}\""),
                                    format!("\"{PROPOSAL_TITLE_2}\""),
                                    format!("\"{PROPOSAL_TITLE_3}\""),
                                    format!("add d3.md"), // commit msg title
                                ],
                            )?;
                            c.succeeds_with(3, true)?;
                            let mut confirm =
                                p.expect_confirm_eventually("check out branch?", Some(true))?;
                            confirm.succeeds_with(None)?;
                            p.expect_end_eventually_and_print()?;

                            for p in [51, 52, 53, 55, 56] {
                                relay::shutdown_relay(8000 + p)?;
                            }
                            Ok((originating_repo, test_repo))
                        });

                    // launch relay
                    let _ = join!(
                        r51.listen_until_close(),
                        r52.listen_until_close(),
                        r53.listen_until_close(),
                        r55.listen_until_close(),
                        r56.listen_until_close(),
                    );
                    let res = cli_tester_handle.join().unwrap()?;

                    Ok(res)
                }

                mod cli_prompts {
                    use super::*;
                    async fn run_async_prompts_to_choose_from_proposal_titles() -> Result<()> {
                        let (mut r51, mut r52, mut r53, mut r55, mut r56) = (
                            Relay::new(8051, None, None),
                            Relay::new(8052, None, None),
                            Relay::new(8053, None, None),
                            Relay::new(8055, None, None),
                            Relay::new(8056, None, None),
                        );

                        r51.events.push(generate_test_key_1_relay_list_event());
                        r51.events.push(generate_test_key_1_metadata_event("fred"));
                        r51.events.push(generate_repo_ref_event());

                        r55.events.push(generate_repo_ref_event());
                        r55.events.push(generate_test_key_1_metadata_event("fred"));
                        r55.events.push(generate_test_key_1_relay_list_event());

                        let cli_tester_handle = std::thread::spawn(move || -> Result<()> {
                            let originating_repo = cli_tester_create_proposals()?;
                            cli_tester_create_proposal(
                                &originating_repo,
                                FEATURE_BRANCH_NAME_4,
                                "d",
                                None,
                            )?;
                            let test_repo = GitTestRepo::default();
                            test_repo.populate()?;
                            let mut p = CliTester::new_from_dir(&test_repo.dir, ["list"]);

                            p.expect("finding proposals...\r\n")?;
                            let mut c = p.expect_choice(
                                "all proposals",
                                vec![
                                    format!("\"{PROPOSAL_TITLE_1}\""),
                                    format!("\"{PROPOSAL_TITLE_2}\""),
                                    format!("\"{PROPOSAL_TITLE_3}\""),
                                    format!("add d3.md"), // commit msg title
                                ],
                            )?;
                            c.succeeds_with(3, true)?;
                            p.expect("finding commits...\r\n")?;
                            let mut confirm = p.expect_confirm("check out branch?", Some(true))?;
                            confirm.succeeds_with(None)?;
                            p.expect("checked out proposal branch. pulled 2 new commits\r\n")?;
                            p.expect_end()?;

                            for p in [51, 52, 53, 55, 56] {
                                relay::shutdown_relay(8000 + p)?;
                            }
                            Ok(())
                        });

                        // launch relay
                        let _ = join!(
                            r51.listen_until_close(),
                            r52.listen_until_close(),
                            r53.listen_until_close(),
                            r55.listen_until_close(),
                            r56.listen_until_close(),
                        );
                        cli_tester_handle.join().unwrap()?;
                        println!("{:?}", r55.events);
                        Ok(())
                    }

                    #[tokio::test]
                    #[serial]
                    async fn prompts_to_choose_from_proposal_titles() -> Result<()> {
                        let _ = run_async_prompts_to_choose_from_proposal_titles().await;
                        Ok(())
                    }
                }

                #[tokio::test]
                #[serial]
                async fn proposal_branch_created_with_correct_name() -> Result<()> {
                    let (_, test_repo) = prep_and_run().await?;
                    assert_eq!(
                        vec![FEATURE_BRANCH_NAME_4, "main"],
                        test_repo.get_local_branch_names()?
                    );
                    Ok(())
                }

                #[tokio::test]
                #[serial]
                async fn proposal_branch_checked_out() -> Result<()> {
                    let (_, test_repo) = prep_and_run().await?;
                    assert_eq!(
                        FEATURE_BRANCH_NAME_4,
                        test_repo.get_checked_out_branch_name()?,
                    );
                    Ok(())
                }

                #[tokio::test]
                #[serial]
                async fn proposal_branch_tip_is_most_recent_patch() -> Result<()> {
                    let (originating_repo, test_repo) = prep_and_run().await?;
                    assert_eq!(
                        originating_repo.get_tip_of_local_branch(FEATURE_BRANCH_NAME_4)?,
                        test_repo.get_tip_of_local_branch(FEATURE_BRANCH_NAME_4)?,
                    );
                    Ok(())
                }
            }
        }
    }

    mod when_proposal_branch_exists {
        use super::*;

        mod when_main_is_checked_out {
            use super::*;

            mod when_branch_is_up_to_date {
                use super::*;
                async fn prep_and_run() -> Result<(GitTestRepo, GitTestRepo)> {
                    // fallback (51,52) user write (53, 55) repo (55, 56)
                    let (mut r51, mut r52, mut r53, mut r55, mut r56) = (
                        Relay::new(8051, None, None),
                        Relay::new(8052, None, None),
                        Relay::new(8053, None, None),
                        Relay::new(8055, None, None),
                        Relay::new(8056, None, None),
                    );

                    r51.events.push(generate_test_key_1_relay_list_event());
                    r51.events.push(generate_test_key_1_metadata_event("fred"));
                    r51.events.push(generate_repo_ref_event());

                    r55.events.push(generate_repo_ref_event());
                    r55.events.push(generate_test_key_1_metadata_event("fred"));
                    r55.events.push(generate_test_key_1_relay_list_event());

                    let cli_tester_handle =
                        std::thread::spawn(move || -> Result<(GitTestRepo, GitTestRepo)> {
                            let originating_repo = cli_tester_create_proposals()?;

                            let test_repo = GitTestRepo::default();
                            test_repo.populate()?;
                            let mut p = CliTester::new_from_dir(&test_repo.dir, ["list"]);

                            create_and_populate_branch(
                                &test_repo,
                                FEATURE_BRANCH_NAME_1,
                                "a",
                                false,
                            )?;
                            test_repo.checkout("main")?;
                            p.expect("finding proposals...\r\n")?;
                            let mut c = p.expect_choice(
                                "all proposals",
                                vec![
                                    format!("\"{PROPOSAL_TITLE_1}\""),
                                    format!("\"{PROPOSAL_TITLE_2}\""),
                                    format!("\"{PROPOSAL_TITLE_3}\""),
                                ],
                            )?;
                            c.succeeds_with(0, true)?;
                            let mut confirm =
                                p.expect_confirm_eventually("check out branch?", Some(true))?;
                            confirm.succeeds_with(None)?;
                            p.expect_end_eventually_and_print()?;

                            for p in [51, 52, 53, 55, 56] {
                                relay::shutdown_relay(8000 + p)?;
                            }
                            Ok((originating_repo, test_repo))
                        });

                    // launch relay
                    let _ = join!(
                        r51.listen_until_close(),
                        r52.listen_until_close(),
                        r53.listen_until_close(),
                        r55.listen_until_close(),
                        r56.listen_until_close(),
                    );
                    let res = cli_tester_handle.join().unwrap()?;

                    Ok(res)
                }

                mod cli_prompts {
                    use super::*;
                    async fn run_async_prompts_to_choose_from_proposal_titles() -> Result<()> {
                        let (mut r51, mut r52, mut r53, mut r55, mut r56) = (
                            Relay::new(8051, None, None),
                            Relay::new(8052, None, None),
                            Relay::new(8053, None, None),
                            Relay::new(8055, None, None),
                            Relay::new(8056, None, None),
                        );

                        r51.events.push(generate_test_key_1_relay_list_event());
                        r51.events.push(generate_test_key_1_metadata_event("fred"));
                        r51.events.push(generate_repo_ref_event());

                        r55.events.push(generate_repo_ref_event());
                        r55.events.push(generate_test_key_1_metadata_event("fred"));
                        r55.events.push(generate_test_key_1_relay_list_event());

                        let cli_tester_handle = std::thread::spawn(move || -> Result<()> {
                            cli_tester_create_proposals()?;

                            let test_repo = GitTestRepo::default();
                            test_repo.populate()?;
                            let mut p = CliTester::new_from_dir(&test_repo.dir, ["list"]);

                            create_and_populate_branch(
                                &test_repo,
                                FEATURE_BRANCH_NAME_1,
                                "a",
                                false,
                            )?;
                            test_repo.checkout("main")?;

                            p.expect("finding proposals...\r\n")?;
                            let mut c = p.expect_choice(
                                "all proposals",
                                vec![
                                    format!("\"{PROPOSAL_TITLE_1}\""),
                                    format!("\"{PROPOSAL_TITLE_2}\""),
                                    format!("\"{PROPOSAL_TITLE_3}\""),
                                ],
                            )?;
                            c.succeeds_with(0, true)?;
                            p.expect("finding commits...\r\n")?;
                            let mut confirm = p.expect_confirm("check out branch?", Some(true))?;
                            confirm.succeeds_with(None)?;
                            p.expect("checked out proposal branch. no new commits to pull\r\n")?;
                            p.expect_end()?;

                            for p in [51, 52, 53, 55, 56] {
                                relay::shutdown_relay(8000 + p)?;
                            }
                            Ok(())
                        });

                        // launch relay
                        let _ = join!(
                            r51.listen_until_close(),
                            r52.listen_until_close(),
                            r53.listen_until_close(),
                            r55.listen_until_close(),
                            r56.listen_until_close(),
                        );
                        cli_tester_handle.join().unwrap()?;
                        println!("{:?}", r55.events);
                        Ok(())
                    }

                    #[tokio::test]
                    #[serial]
                    async fn prompts_to_choose_from_proposal_titles() -> Result<()> {
                        let _ = run_async_prompts_to_choose_from_proposal_titles().await;
                        Ok(())
                    }
                }

                #[tokio::test]
                #[serial]
                async fn proposal_branch_checked_out() -> Result<()> {
                    let (_, test_repo) = prep_and_run().await?;
                    assert_eq!(
                        FEATURE_BRANCH_NAME_1,
                        test_repo.get_checked_out_branch_name()?,
                    );
                    Ok(())
                }
            }

            mod when_branch_is_behind {
                use super::*;

                async fn prep_and_run() -> Result<(GitTestRepo, GitTestRepo)> {
                    // fallback (51,52) user write (53, 55) repo (55, 56)
                    let (mut r51, mut r52, mut r53, mut r55, mut r56) = (
                        Relay::new(8051, None, None),
                        Relay::new(8052, None, None),
                        Relay::new(8053, None, None),
                        Relay::new(8055, None, None),
                        Relay::new(8056, None, None),
                    );

                    r51.events.push(generate_test_key_1_relay_list_event());
                    r51.events.push(generate_test_key_1_metadata_event("fred"));
                    r51.events.push(generate_repo_ref_event());

                    r55.events.push(generate_repo_ref_event());
                    r55.events.push(generate_test_key_1_metadata_event("fred"));
                    r55.events.push(generate_test_key_1_relay_list_event());

                    let cli_tester_handle =
                        std::thread::spawn(move || -> Result<(GitTestRepo, GitTestRepo)> {
                            let originating_repo = cli_tester_create_proposals()?;

                            let test_repo = GitTestRepo::default();
                            test_repo.populate()?;
                            let mut p = CliTester::new_from_dir(&test_repo.dir, ["list"]);

                            create_and_populate_branch(
                                &test_repo,
                                FEATURE_BRANCH_NAME_1,
                                "a",
                                true,
                            )?;
                            test_repo.checkout("main")?;

                            p.expect("finding proposals...\r\n")?;
                            let mut c = p.expect_choice(
                                "all proposals",
                                vec![
                                    format!("\"{PROPOSAL_TITLE_1}\""),
                                    format!("\"{PROPOSAL_TITLE_2}\""),
                                    format!("\"{PROPOSAL_TITLE_3}\""),
                                ],
                            )?;
                            c.succeeds_with(0, true)?;
                            let mut confirm =
                                p.expect_confirm_eventually("check out branch?", Some(true))?;
                            confirm.succeeds_with(None)?;
                            p.expect_end_eventually_and_print()?;

                            for p in [51, 52, 53, 55, 56] {
                                relay::shutdown_relay(8000 + p)?;
                            }
                            Ok((originating_repo, test_repo))
                        });

                    // launch relay
                    let _ = join!(
                        r51.listen_until_close(),
                        r52.listen_until_close(),
                        r53.listen_until_close(),
                        r55.listen_until_close(),
                        r56.listen_until_close(),
                    );
                    let res = cli_tester_handle.join().unwrap()?;

                    Ok(res)
                }

                mod cli_prompts {
                    use super::*;
                    async fn run_async_prompts_to_choose_from_proposal_titles() -> Result<()> {
                        let (mut r51, mut r52, mut r53, mut r55, mut r56) = (
                            Relay::new(8051, None, None),
                            Relay::new(8052, None, None),
                            Relay::new(8053, None, None),
                            Relay::new(8055, None, None),
                            Relay::new(8056, None, None),
                        );

                        r51.events.push(generate_test_key_1_relay_list_event());
                        r51.events.push(generate_test_key_1_metadata_event("fred"));
                        r51.events.push(generate_repo_ref_event());

                        r55.events.push(generate_repo_ref_event());
                        r55.events.push(generate_test_key_1_metadata_event("fred"));
                        r55.events.push(generate_test_key_1_relay_list_event());

                        let cli_tester_handle = std::thread::spawn(move || -> Result<()> {
                            cli_tester_create_proposals()?;

                            let test_repo = GitTestRepo::default();
                            test_repo.populate()?;
                            let mut p = CliTester::new_from_dir(&test_repo.dir, ["list"]);

                            create_and_populate_branch(
                                &test_repo,
                                FEATURE_BRANCH_NAME_1,
                                "a",
                                true,
                            )?;
                            test_repo.checkout("main")?;

                            p.expect("finding proposals...\r\n")?;
                            let mut c = p.expect_choice(
                                "all proposals",
                                vec![
                                    format!("\"{PROPOSAL_TITLE_1}\""),
                                    format!("\"{PROPOSAL_TITLE_2}\""),
                                    format!("\"{PROPOSAL_TITLE_3}\""),
                                ],
                            )?;
                            c.succeeds_with(0, true)?;
                            p.expect("finding commits...\r\n")?;
                            let mut confirm = p.expect_confirm("check out branch?", Some(true))?;
                            confirm.succeeds_with(None)?;
                            p.expect("checked out proposal branch. pulled 1 new commits\r\n")?;
                            p.expect_end()?;

                            for p in [51, 52, 53, 55, 56] {
                                relay::shutdown_relay(8000 + p)?;
                            }
                            Ok(())
                        });

                        // launch relay
                        let _ = join!(
                            r51.listen_until_close(),
                            r52.listen_until_close(),
                            r53.listen_until_close(),
                            r55.listen_until_close(),
                            r56.listen_until_close(),
                        );
                        cli_tester_handle.join().unwrap()?;
                        println!("{:?}", r55.events);
                        Ok(())
                    }

                    #[tokio::test]
                    #[serial]
                    async fn prompts_to_choose_from_proposal_titles() -> Result<()> {
                        let _ = run_async_prompts_to_choose_from_proposal_titles().await;
                        Ok(())
                    }
                }

                #[tokio::test]
                #[serial]
                async fn proposal_branch_checked_out() -> Result<()> {
                    let (_, test_repo) = prep_and_run().await?;
                    assert_eq!(
                        FEATURE_BRANCH_NAME_1,
                        test_repo.get_checked_out_branch_name()?,
                    );
                    Ok(())
                }

                #[tokio::test]
                #[serial]
                async fn proposal_branch_tip_is_most_recent_patch() -> Result<()> {
                    let (originating_repo, test_repo) = prep_and_run().await?;
                    assert_eq!(
                        originating_repo.get_tip_of_local_branch(FEATURE_BRANCH_NAME_1)?,
                        test_repo.get_tip_of_local_branch(FEATURE_BRANCH_NAME_1)?,
                    );
                    Ok(())
                }
            }

            mod when_branch_is_ahead {
                // use super::*;
                // TODO latest commit in proposal builds off an older commit in
                // proposal instead of previous.
                // TODO current git user created commit on branch
            }

            mod when_latest_event_rebases_branch {
                // use super::*;
                // TODO
            }
        }
    }
}
