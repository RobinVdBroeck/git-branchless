use lib::testing::{Git, make_git};

#[test]
fn test_advance_basic() -> eyre::Result<()> {
    let git = make_git()?;

    if !git.supports_committer_date_is_author_date()? {
        return Ok(());
    }

    git.init_repo()?;

    // Create branch-1 with a commit, then branch-2 with another commit.
    git.run(&["checkout", "-b", "branch-1"])?;
    git.commit_file("test1", 1)?;
    git.run(&["checkout", "-b", "branch-2"])?;
    git.commit_file("test2", 2)?;

    // Go back to branch-1 and create a new commit on top.
    git.run(&["checkout", "branch-1"])?;
    git.commit_file("test3", 3)?;

    // Verify the graph before advance: branch-2 is still based on the old tip.
    {
        let stdout = git.smartlog()?;
        insta::assert_snapshot!(stdout, @r###"
        O f777ecc (master) create initial.txt
        |
        o 62fc20d create test1.txt
        |\
        | o 96d1c37 (branch-2) create test2.txt
        |
        @ 4838e49 (> branch-1) create test3.txt
        "###);
    }

    // Run advance to move branch-2 onto the new tip of branch-1.
    {
        let (stdout, _stderr) = git.branchless("advance", &[])?;
        insta::assert_snapshot!(stdout, @r###"
        Advancing 1 commit onto 4838e49 create test3.txt.
        Attempting rebase in-memory...
        [1/1] Committed as: d742fb9 create test2.txt
        branchless: processing 1 update: branch branch-2
        branchless: processing 1 rewritten commit
        branchless: running command: <git-executable> checkout branch-1 --
        In-memory rebase succeeded.
        O f777ecc (master) create initial.txt
        |
        o 62fc20d create test1.txt
        |
        @ 4838e49 (> branch-1) create test3.txt
        |
        o d742fb9 (branch-2) create test2.txt
        "###);
    }

    Ok(())
}

#[test]
fn test_advance_no_siblings() -> eyre::Result<()> {
    let git = make_git()?;
    git.init_repo()?;

    // Create a single branch with no siblings.
    git.run(&["checkout", "-b", "branch-1"])?;
    git.commit_file("test1", 1)?;

    {
        let (stdout, _stderr) = git.branchless("advance", &[])?;
        insta::assert_snapshot!(stdout, @"No child commits to advance.
");
    }

    Ok(())
}

#[test]
fn test_advance_multiple_children() -> eyre::Result<()> {
    let git = make_git()?;

    if !git.supports_committer_date_is_author_date()? {
        return Ok(());
    }

    git.init_repo()?;

    // Create branch-1 with a commit.
    git.run(&["checkout", "-b", "branch-1"])?;
    git.commit_file("test1", 1)?;

    // Create branch-2 off branch-1.
    git.run(&["checkout", "-b", "branch-2"])?;
    git.commit_file("test2", 2)?;

    // Create branch-3 also off branch-1.
    git.run(&["checkout", "branch-1"])?;
    git.run(&["checkout", "-b", "branch-3"])?;
    git.commit_file("test3", 3)?;

    // Go back to branch-1 and create a new commit on top.
    git.run(&["checkout", "branch-1"])?;
    git.commit_file("test4", 4)?;

    // Run advance to move both branch-2 and branch-3 onto the new tip.
    {
        let (stdout, _stderr) = git.branchless("advance", &[])?;
        insta::assert_snapshot!(stdout, @r###"
        Advancing 2 commits onto bf0d52a create test4.txt.
        Attempting rebase in-memory...
        [1/2] Committed as: 0a4a701 create test3.txt
        [2/2] Committed as: 44352d0 create test2.txt
        branchless: processing 2 updates: branch branch-2, branch branch-3
        branchless: processing 2 rewritten commits
        branchless: running command: <git-executable> checkout branch-1 --
        In-memory rebase succeeded.
        O f777ecc (master) create initial.txt
        |
        o 62fc20d create test1.txt
        |
        @ bf0d52a (> branch-1) create test4.txt
        |\
        | o 44352d0 (branch-2) create test2.txt
        |
        o 0a4a701 (branch-3) create test3.txt
        "###);
    }

    Ok(())
}

#[test]
fn test_advance_bare_repo_worktree() -> eyre::Result<()> {
    let git = make_git()?;

    if !git.supports_committer_date_is_author_date()? {
        return Ok(());
    }

    // Create a bare repo instead of a normal one.
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

    // Create a worktree with a "main" branch.
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

    // Make an initial commit and initialize branchless.
    wt.commit_file("initial", 0)?;
    wt.branchless("init", &[])?;

    // Create branch-1 with a commit, then branch-2 with another commit.
    wt.run(&["checkout", "-b", "branch-1"])?;
    wt.commit_file("test1", 1)?;
    wt.run(&["checkout", "-b", "branch-2"])?;
    wt.commit_file("test2", 2)?;

    // Go back to branch-1 and create a new commit on top.
    wt.run(&["checkout", "branch-1"])?;
    wt.commit_file("test3", 3)?;

    // Run advance to move branch-2 onto the new tip.
    {
        let (stdout, _stderr) = wt.branchless("advance", &[])?;
        insta::assert_snapshot!(stdout, @r###"
        Advancing 1 commit onto 4838e49 create test3.txt.
        Attempting rebase in-memory...
        [1/1] Committed as: d742fb9 create test2.txt
        branchless: processing 1 update: branch branch-2
        branchless: processing 1 rewritten commit
        branchless: running command: <git-executable> checkout branch-1 --
        In-memory rebase succeeded.
        O f777ecc (main) create initial.txt
        |
        o 62fc20d create test1.txt
        |
        @ 4838e49 (> branch-1) create test3.txt
        |
        o d742fb9 (branch-2) create test2.txt
        "###);
    }

    Ok(())
}

#[test]
fn test_advance_auto() -> eyre::Result<()> {
    let git = make_git()?;

    if !git.supports_committer_date_is_author_date()? {
        return Ok(());
    }

    git.init_repo()?;

    // Create branch-1 with a commit, then branch-2 with another commit.
    git.run(&["checkout", "-b", "branch-1"])?;
    git.commit_file("test1", 1)?;
    git.run(&["checkout", "-b", "branch-2"])?;
    git.commit_file("test2", 2)?;

    // Go back to branch-1 and enable autoadvance.
    git.run(&["checkout", "branch-1"])?;
    git.run(&["config", "branchless.advance.auto", "true"])?;

    // Create a new commit on branch-1. The hook should auto-advance branch-2.
    git.commit_file("test3", 3)?;

    // Verify branch-2 was automatically moved onto the new tip.
    {
        let stdout = git.smartlog()?;
        insta::assert_snapshot!(stdout, @r###"
        O f777ecc (master) create initial.txt
        |
        o 62fc20d create test1.txt
        |
        @ 4838e49 (> branch-1) create test3.txt
        |
        o d742fb9 (branch-2) create test2.txt
        "###);
    }

    Ok(())
}
