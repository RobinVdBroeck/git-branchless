//! Move sibling commits onto the current commit.
//!
//! When you add a new commit to a branch that has child branches stacked on it,
//! the children remain based on the old tip. The `advance` command moves those
//! sibling commits/branches onto the new HEAD so the stack stays connected.
//!
//! # Example
//!
//! Suppose you have this stack:
//!
//! ```text
//! O main
//! |
//! o commit-a (branch-1)
//! |
//! o commit-b (branch-2)
//! ```
//!
//! You check out `branch-1` and add a new commit `commit-c`:
//!
//! ```text
//! O main
//! |
//! o commit-a
//! |\
//! | o commit-b (branch-2)    <-- still based on commit-a
//! |
//! @ commit-c (branch-1)
//! ```
//!
//! Running `git advance` moves `branch-2` onto `commit-c`:
//!
//! ```text
//! O main
//! |
//! o commit-a
//! |
//! @ commit-c (branch-1)
//! |
//! o commit-b (branch-2)
//! ```

use std::collections::HashSet;
use std::fmt::Write;
use std::time::SystemTime;

use git_branchless_opts::MoveOptions;
use git_branchless_smartlog::smartlog;
use itertools::Itertools;
use lib::core::check_out::CheckOutCommitOptions;
use lib::core::config::get_restack_preserve_timestamps;
use lib::core::dag::{CommitSet, Dag};
use lib::core::effects::Effects;
use lib::core::eventlog::{EventLogDb, EventReplayer};
use lib::core::formatting::Pluralize;
use lib::core::repo_ext::RepoExt;
use lib::core::rewrite::{
    BuildRebasePlanError, BuildRebasePlanOptions, ExecuteRebasePlanOptions,
    ExecuteRebasePlanResult, MergeConflictRemediation, RebasePlanBuilder, RebasePlanPermissions,
    RepoResource, execute_rebase_plan,
};
use lib::git::{GitRunInfo, NonZeroOid, Repo};
use lib::util::{ExitCode, EyreExitOr};
use rayon::ThreadPoolBuilder;
use tracing::instrument;

/// Move child commits of HEAD's parent onto HEAD.
#[instrument]
pub fn advance(
    effects: &Effects,
    git_run_info: &GitRunInfo,
    move_options: &MoveOptions,
) -> EyreExitOr<()> {
    let now = SystemTime::now();
    let repo = Repo::from_current_dir()?;
    let conn = repo.get_db_conn()?;
    let event_log_db = EventLogDb::new(&conn)?;
    let event_tx_id = event_log_db.make_transaction_id(now, "advance")?;

    let references_snapshot = repo.get_references_snapshot()?;
    let event_replayer = EventReplayer::from_event_log_db(effects, &repo, &event_log_db)?;
    let event_cursor = event_replayer.make_default_cursor();
    let dag = Dag::open_and_sync(
        effects,
        &repo,
        &event_replayer,
        event_cursor,
        &references_snapshot,
    )?;

    let head_info = repo.get_head_info()?;
    let head_oid = match head_info.oid {
        Some(oid) => oid,
        None => {
            writeln!(
                effects.get_output_stream(),
                "No commit is currently checked out.",
            )?;
            return Ok(Err(ExitCode(1)));
        }
    };

    let head_commit = repo.find_commit_or_fail(head_oid)?;
    let head_commit_set = CommitSet::from(head_oid);
    let parents = dag.query_parents(head_commit_set.clone())?;
    let children = dag.query_children(parents)?;
    let siblings = children.difference(&head_commit_set);
    let siblings = dag.filter_visible_commits(siblings)?;

    if dag.set_is_empty(&siblings)? {
        writeln!(effects.get_output_stream(), "No child commits to advance.",)?;
        return Ok(Ok(()));
    }

    let sibling_count = dag.set_count(&siblings)?;
    writeln!(
        effects.get_output_stream(),
        "Advancing {} onto {}.",
        Pluralize {
            determiner: None,
            amount: sibling_count,
            unit: ("commit", "commits"),
        },
        effects
            .get_glyphs()
            .render(head_commit.friendly_describe(effects.get_glyphs())?)?,
    )?;

    let build_options = BuildRebasePlanOptions {
        force_rewrite_public_commits: move_options.force_rewrite_public_commits,
        dump_rebase_constraints: move_options.dump_rebase_constraints,
        dump_rebase_plan: move_options.dump_rebase_plan,
        detect_duplicate_commits_via_patch_id: move_options.detect_duplicate_commits_via_patch_id,
    };

    let rebase_plan_result =
        match RebasePlanPermissions::verify_rewrite_set(&dag, build_options, &siblings)? {
            Err(err) => Err(err),
            Ok(permissions) => {
                let head_commit_parents: HashSet<_> =
                    head_commit.get_parent_oids().into_iter().collect();
                let mut builder = RebasePlanBuilder::new(&dag, permissions);
                for sibling_oid in dag.commit_set_to_vec(&siblings)? {
                    let sibling_commit = repo.find_commit_or_fail(sibling_oid)?;
                    let parent_oids = sibling_commit.get_parent_oids();
                    let new_parent_oids = parent_oids
                        .into_iter()
                        .map(|parent_oid| {
                            if head_commit_parents.contains(&parent_oid) {
                                head_oid
                            } else {
                                parent_oid
                            }
                        })
                        .collect_vec();
                    builder.move_subtree(sibling_oid, new_parent_oids)?;
                }
                let thread_pool = ThreadPoolBuilder::new().build()?;
                let repo_pool = RepoResource::new_pool(&repo)?;
                builder.build(effects, &thread_pool, &repo_pool)?
            }
        };

    let rebase_plan = match rebase_plan_result {
        Ok(Some(rebase_plan)) => rebase_plan,

        Ok(None) => {
            writeln!(effects.get_output_stream(), "No child commits to advance.",)?;
            return Ok(Ok(()));
        }

        Err(BuildRebasePlanError::ConstraintCycle { .. }) => {
            writeln!(
                effects.get_output_stream(),
                "BUG: constraint cycle detected when moving siblings, which shouldn't be possible."
            )?;
            return Ok(Err(ExitCode(1)));
        }

        Err(err @ BuildRebasePlanError::MoveIllegalCommits { .. }) => {
            err.describe(effects, &repo, &dag)?;
            return Ok(Err(ExitCode(1)));
        }

        Err(BuildRebasePlanError::MovePublicCommits {
            public_commits_to_move,
        }) => {
            let example_bad_commit_oid = dag
                .set_first(&public_commits_to_move)?
                .ok_or_else(|| eyre::eyre!("BUG: could not get OID of a public commit to move"))?;
            let example_bad_commit_oid = NonZeroOid::try_from(example_bad_commit_oid)?;
            let example_bad_commit = repo.find_commit_or_fail(example_bad_commit_oid)?;
            writeln!(
                effects.get_output_stream(),
                "\
You are trying to rewrite {}, such as: {}
It is generally not advised to rewrite public commits, because your
collaborators will have difficulty merging your changes.
To proceed anyways, run: git advance -f",
                Pluralize {
                    determiner: None,
                    amount: dag.set_count(&public_commits_to_move)?,
                    unit: ("public commit", "public commits")
                },
                effects
                    .get_glyphs()
                    .render(example_bad_commit.friendly_describe(effects.get_glyphs())?)?,
            )?;
            return Ok(Ok(()));
        }
    };

    let execute_options = ExecuteRebasePlanOptions {
        now,
        event_tx_id,
        preserve_timestamps: get_restack_preserve_timestamps(&repo)?,
        force_in_memory: move_options.force_in_memory,
        force_on_disk: move_options.force_on_disk,
        dry_run: false,
        resolve_merge_conflicts: move_options.resolve_merge_conflicts,
        check_out_commit_options: CheckOutCommitOptions {
            additional_args: Default::default(),
            force_detach: false,
            reset: false,
            render_smartlog: false,
        },
    };
    let result = execute_rebase_plan(
        effects,
        git_run_info,
        &repo,
        &event_log_db,
        &rebase_plan,
        &execute_options,
    )?;
    match result {
        ExecuteRebasePlanResult::Succeeded { rewritten_oids: _ }
        | ExecuteRebasePlanResult::WouldSucceed => {}
        ExecuteRebasePlanResult::DeclinedToMerge { failed_merge_info } => {
            failed_merge_info.describe(effects, &repo, MergeConflictRemediation::Retry)?;
            return Ok(Err(ExitCode(1)));
        }
        ExecuteRebasePlanResult::Failed { exit_code } => return Ok(Err(exit_code)),
    }

    smartlog(effects, git_run_info, Default::default())
}
