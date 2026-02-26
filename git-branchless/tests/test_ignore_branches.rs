use lib::testing::make_git;

#[test]
fn test_ignore_branches_exact() -> eyre::Result<()> {
    let git = make_git()?;
    git.init_repo()?;

    // Create a commit on master, then a branch "test" with a commit.
    git.commit_file("file1", 1)?;
    git.run(&["checkout", "-b", "test"])?;
    git.commit_file("file2", 2)?;
    git.run(&["checkout", "master"])?;

    // Before ignoring, "test" branch should appear in the smartlog.
    {
        let stdout = git.smartlog()?;
        assert!(
            stdout.contains("(test)"),
            "test branch should appear before ignoring"
        );
    }

    // Ignore the "test" branch.
    git.run(&["config", "--add", "branchless.core.ignoreBranches", "test"])?;

    // After ignoring, "test" branch should not appear.
    {
        let stdout = git.smartlog()?;
        assert!(
            !stdout.contains("(test)"),
            "test branch should not appear after ignoring"
        );
        assert!(stdout.contains("(> master)"), "master should still appear");
    }

    Ok(())
}

#[test]
fn test_ignore_branches_glob_pattern() -> eyre::Result<()> {
    let git = make_git()?;
    git.init_repo()?;

    // Create branches matching and not matching a glob pattern.
    git.commit_file("file1", 1)?;

    git.run(&["checkout", "-b", "release/v1"])?;
    git.commit_file("file2", 2)?;
    git.run(&["checkout", "master"])?;

    git.run(&["checkout", "-b", "release/v2"])?;
    git.commit_file("file3", 3)?;
    git.run(&["checkout", "master"])?;

    git.run(&["checkout", "-b", "feature-x"])?;
    git.commit_file("file4", 4)?;
    git.run(&["checkout", "master"])?;

    // Before ignoring, all branches should appear.
    {
        let stdout = git.smartlog()?;
        assert!(stdout.contains("release/v1"));
        assert!(stdout.contains("release/v2"));
        assert!(stdout.contains("feature-x"));
    }

    // Ignore branches matching "release/*".
    git.run(&[
        "config",
        "--add",
        "branchless.core.ignoreBranches",
        "release/*",
    ])?;

    // After ignoring, release branches should not appear, but feature-x should.
    {
        let stdout = git.smartlog()?;
        assert!(!stdout.contains("release/v1"));
        assert!(!stdout.contains("release/v2"));
        assert!(stdout.contains("feature-x"));
    }

    Ok(())
}

#[test]
fn test_ignore_branches_multiple_patterns() -> eyre::Result<()> {
    let git = make_git()?;
    git.init_repo()?;

    git.commit_file("file1", 1)?;

    git.run(&["checkout", "-b", "test"])?;
    git.commit_file("file2", 2)?;
    git.run(&["checkout", "master"])?;

    git.run(&["checkout", "-b", "staging"])?;
    git.commit_file("file3", 3)?;
    git.run(&["checkout", "master"])?;

    git.run(&["checkout", "-b", "feature-y"])?;
    git.commit_file("file4", 4)?;
    git.run(&["checkout", "master"])?;

    // Ignore both "test" and "staging" via two config entries.
    git.run(&["config", "--add", "branchless.core.ignoreBranches", "test"])?;
    git.run(&[
        "config",
        "--add",
        "branchless.core.ignoreBranches",
        "staging",
    ])?;

    {
        let stdout = git.smartlog()?;
        assert!(!stdout.contains("(test)"));
        assert!(!stdout.contains("(staging)"));
        assert!(stdout.contains("feature-y"));
    }

    Ok(())
}
