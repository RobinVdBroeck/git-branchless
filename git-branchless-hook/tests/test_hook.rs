use lib::testing::{Git, GitRunOptions, GitWorktreeWrapper, make_git, make_git_worktree};

#[test]
fn test_is_rebase_underway() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    let repo = git.get_repo()?;
    assert!(!repo.is_rebase_underway()?);

    let oid1 = git.commit_file_with_contents("test", 1, "foo")?;
    git.run(&["checkout", "HEAD^"])?;
    git.commit_file_with_contents("test", 1, "bar")?;
    git.run_with_options(
        &["rebase", &oid1.to_string()],
        &GitRunOptions {
            expected_exit_code: 1,
            ..Default::default()
        },
    )?;
    assert!(repo.is_rebase_underway()?);

    Ok(())
}

#[test]
fn test_rebase_no_process_new_commits_until_conclusion() -> eyre::Result<()> {
    let git = make_git()?;

    if !git.supports_reference_transactions()? {
        return Ok(());
    }
    git.init_repo()?;

    git.detach_head()?;
    git.commit_file("test1", 1)?;
    let test2_oid = git.commit_file("test2", 2)?;

    // Ensure commits aren't preserved if the rebase is aborted.
    {
        git.run_with_options(
            &["rebase", "master", "--force", "--exec", "exit 1"],
            &GitRunOptions {
                expected_exit_code: 1,
                ..Default::default()
            },
        )?;
        git.run(&[
            "commit",
            "--amend",
            "--message",
            "this commit shouldn't show up in the smartlog",
        ])?;
        git.commit_file("test3", 3)?;
        git.run(&["rebase", "--abort"])?;

        {
            let stdout = git.smartlog()?;
            insta::assert_snapshot!(stdout, @r###"
        O f777ecc (master) create initial.txt
        |
        o 62fc20d create test1.txt
        |
        @ 96d1c37 create test2.txt
        "###);
        }
    }

    // Ensure commits are preserved if the rebase succeeds.
    {
        git.run(&["checkout", "HEAD^"])?;

        {
            let (stdout, stderr) = git.run_with_options(
                &["rebase", "master", "--force", "--exec", "exit 1"],
                &GitRunOptions {
                    expected_exit_code: 1,
                    ..Default::default()
                },
            )?;

            // As of `38c541ce94048cf72aa4f465be9314423a57f445` (Git >=v2.36.0),
            // `git checkout` is called in fewer cases, which affects the stderr
            // output for the test.
            let stderr: String = stderr
                .lines()
                .filter_map(|line| {
                    if line.starts_with("branchless:") {
                        None
                    } else {
                        Some(format!("{line}\n"))
                    }
                })
                .collect();

            insta::assert_snapshot!(stderr, @r###"
            Executing: exit 1
            warning: execution failed: exit 1
            You can fix the problem, and then run

              git rebase --continue


            "###);
            insta::assert_snapshot!(stdout, @"");
        }

        {
            let (stdout, stderr) = git.run(&["rebase", "--continue"])?;
            insta::assert_snapshot!(stderr, @r###"
            branchless: processing 1 rewritten commit
            branchless: This operation abandoned 1 commit!
            branchless: Consider running one of the following:
            branchless:   - git restack: re-apply the abandoned commits/branches
            branchless:     (this is most likely what you want to do)
            branchless:   - git smartlog: assess the situation
            branchless:   - git hide [<commit>...]: hide the commits from the smartlog
            branchless:   - git undo: undo the operation
            hint: disable this hint by running: git config --global branchless.hint.restackWarnAbandoned false
            Successfully rebased and updated detached HEAD.
            "###);
            insta::assert_snapshot!(stdout, @"");
        }

        // Switch away to make sure that the new commit isn't visible just
        // because it's reachable from `HEAD`.
        git.run(&["checkout", &test2_oid.to_string()])?;

        {
            let stdout = git.smartlog()?;
            insta::assert_snapshot!(stdout, @r###"
            O f777ecc (master) create initial.txt
            |\
            | o 047b7ad create test1.txt
            |
            x 62fc20d (rewritten as 047b7ad7) create test1.txt
            |
            @ 96d1c37 create test2.txt
            hint: there is 1 abandoned commit in your commit graph
            hint: to fix this, run: git restack
            hint: disable this hint by running: git config --global branchless.hint.smartlogFixAbandoned false
            "###);
        }
    }

    Ok(())
}

#[test]
fn test_hooks_in_worktree() -> eyre::Result<()> {
    let git = make_git()?;

    if !git.supports_reference_transactions()? {
        return Ok(());
    }
    git.init_repo()?;

    git.commit_file("test1", 1)?;
    git.detach_head()?;

    let GitWorktreeWrapper {
        temp_dir: _temp_dir,
        worktree,
    } = make_git_worktree(&git, "new-worktree")?;

    {
        let (stdout, stderr) =
            worktree.run(&["commit", "--allow-empty", "-m", "new empty commit"])?;
        insta::assert_snapshot!(stderr, @r###"
        branchless: processing 1 update: ref HEAD
        branchless: processed commit: 1bed0d8 new empty commit
        "###);
        insta::assert_snapshot!(stdout, @r###"
        [detached HEAD 1bed0d8] new empty commit
        "###);
    }

    {
        let stdout = git.smartlog()?;
        insta::assert_snapshot!(stdout, @r###"
        :
        @ 62fc20d (master) create test1.txt
        |
        o 1bed0d8 new empty commit
        "###);
    }
    {
        let stdout = worktree.smartlog()?;
        insta::assert_snapshot!(stdout, @r###"
        :
        O 62fc20d (master) create test1.txt
        |
        @ 1bed0d8 new empty commit
        "###);
    }

    {
        let (stdout, stderr) =
            worktree.run(&["commit", "--amend", "--allow-empty", "--message", "amended"])?;
        insta::assert_snapshot!(stderr, @r###"
        branchless: processing 1 update: ref HEAD
        branchless: processed commit: cc4313e amended
        hint: to move child commits onto this commit, run: git advance
        hint: disable this hint by running: git config --global branchless.hint.advanceChildCommits false
        branchless: processing 1 rewritten commit
        "###);
        insta::assert_snapshot!(stdout, @r###"
        [detached HEAD cc4313e] amended
         Date: Thu Oct 29 12:34:56 2020 +0000
        "###);
    }
    {
        let stdout = git.smartlog()?;
        insta::assert_snapshot!(stdout, @r###"
        :
        @ 62fc20d (master) create test1.txt
        |
        o cc4313e amended
        "###);
    }
    {
        let stdout = worktree.smartlog()?;
        insta::assert_snapshot!(stdout, @r###"
        :
        O 62fc20d (master) create test1.txt
        |
        @ cc4313e amended
        "###);
    }

    Ok(())
}

/// Verify that `git pack-refs` does not cause branches to disappear from the
/// smartlog in a linked worktree.
///
/// When git packs refs, it fires the reference-transaction hook with fake
/// "creation" (0→abc123) and "deletion" (abc123→0) events per packed ref.
/// `fix_packed_reference_oid` is supposed to detect these as no-ops by
/// consulting the packed-refs file. Previously, the packed-refs file was read
/// from the worktree-specific git dir (where it doesn't exist) instead of the
/// parent repo's git dir, so the empty HashMap caused the fake deletion events
/// to be recorded, removing branches from the event log.
#[test]
fn test_hook_reference_transaction_pack_refs_in_worktree() -> eyre::Result<()> {
    let git = make_git()?;

    if !git.supports_reference_transactions()? {
        return Ok(());
    }

    git.init_repo()?;
    git.commit_file("test1", 1)?;

    // Create a second branch so there are multiple refs to pack.
    git.run(&["checkout", "-b", "feature"])?;
    git.commit_file("test2", 2)?;
    git.run(&["checkout", "master"])?;

    let GitWorktreeWrapper {
        temp_dir: _temp_dir,
        worktree,
    } = make_git_worktree(&git, "new-worktree")?;

    // Verify the baseline smartlog (both branches visible).
    let stdout_before = worktree.smartlog()?;
    insta::assert_snapshot!(stdout_before, @r###"
    :
    @ 62fc20d (master) create test1.txt
    |
    o 96d1c37 (feature) create test2.txt
    "###);

    // Pack all refs. This fires the reference-transaction hook twice per ref:
    // once with 0→abc123 (fake creation) and once with abc123→0 (fake deletion).
    // The fix ensures we read packed-refs from the parent repo's git dir so
    // fix_packed_reference_oid can detect and ignore these no-op events.
    worktree.run(&["pack-refs", "--all"])?;

    // Smartlog should be identical after packing — no spurious deletions.
    let stdout_after = worktree.smartlog()?;
    assert_eq!(
        stdout_before,
        stdout_after,
        "git pack-refs should not cause branches to disappear from the smartlog",
    );

    Ok(())
}

/// Same as above but using a bare repo as the primary repo, which was the
/// original context where the packed-refs path bug was discovered.
#[test]
fn test_hook_reference_transaction_pack_refs_in_bare_worktree() -> eyre::Result<()> {
    let git = make_git()?;

    if !git.supports_reference_transactions()? {
        return Ok(());
    }

    git.run(&["init", "--bare"])?;
    git.run(&["config", "user.name", "Testy McTestface"])?;
    git.run(&["config", "user.email", "test@example.com"])?;
    git.run(&["config", "core.abbrev", "7"])?;
    git.run(&[
        "config",
        "branchless.commitDescriptors.relativeTime",
        "false",
    ])?;
    git.run(&["config", "branchless.restack.preserveTimestamps", "true"])?;
    git.run(&["config", "core.autocrlf", "false"])?;

    let worktree_path = git.repo_path.join("wt-main");
    git.run(&[
        "worktree",
        "add",
        worktree_path.to_str().unwrap(),
        "-b",
        "main",
    ])?;
    let wt = Git {
        repo_path: worktree_path,
        ..(*git).clone()
    };

    wt.commit_file("initial", 0)?;
    wt.branchless("init", &[])?;

    // Create a second branch so there are multiple refs to pack.
    wt.run(&["checkout", "-b", "feature"])?;
    wt.commit_file("test1", 1)?;
    wt.run(&["checkout", "main"])?;

    // Verify baseline — both branches visible.
    let stdout_before = wt.smartlog()?;
    insta::assert_snapshot!(stdout_before, @r###"
    @ f777ecc (> main) create initial.txt
    |
    o 62fc20d (feature) create test1.txt
    "###);

    // Pack all refs from the worktree.
    wt.run(&["pack-refs", "--all"])?;

    // Smartlog must be unchanged — no spurious deletion events.
    let stdout_after = wt.smartlog()?;
    assert_eq!(
        stdout_before,
        stdout_after,
        "git pack-refs should not cause branches to disappear from the smartlog",
    );

    Ok(())
}
