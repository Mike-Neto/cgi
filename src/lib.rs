use core::error::Error;
use core::fmt::{self, Display, Formatter};
use git2::{BranchType, Repository};

/// Errors produced while choosing and switching branches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppError {
    /// The interactive selection prompt was canceled.
    CanceledSelection,
    /// The selection prompt returned an index outside the item list.
    InvalidSelectionIndex {
        /// Invalid selected index.
        index: usize,
    },
    /// There are no other local branches to switch to.
    SingleBranch,
}

impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match *self {
            Self::CanceledSelection => f.write_str("canceled selection"),
            Self::InvalidSelectionIndex { index } => {
                write!(f, "invalid selection index: {index}")
            }
            Self::SingleBranch => f.write_str("cant switch from a single branch"),
        }
    }
}

#[expect(
    clippy::missing_trait_methods,
    reason = "std::error::Error default methods are sufficient for this leaf error type"
)]
impl Error for AppError {}

/// Local branch choice shown in the interactive prompt.
struct BranchOption {
    /// Commit timestamp used for recent-branch ordering.
    commit_time: i64,
    /// Branch shorthand name.
    name: String,
}

/// Run the branch-switching workflow with a caller-provided selector.
///
/// # Errors
///
/// Returns an error when the repository cannot be inspected, no alternate local
/// branches exist, selection is canceled or invalid, or checkout fails.
#[inline]
pub fn run_with_selector<S>(repo: &Repository, selector: S) -> Result<(), Box<dyn Error>>
where
    S: FnOnce(&str, &[&str]) -> Result<Option<usize>, Box<dyn Error>>,
{
    let current_branch_name = repo.head()?.shorthand().unwrap_or_default().to_owned();
    let mut branches = branch_options(repo, &current_branch_name)?;
    branches.sort_by_key(|branch| branch.commit_time);

    let items = branches
        .iter()
        .rev()
        .map(|branch| branch.name.as_str())
        .collect::<Vec<&str>>();
    if items.is_empty() {
        return Err(Box::new(AppError::SingleBranch));
    }

    let prompt = format!("Switch Branch? {current_branch_name} ->");
    let selected_index = selector(&prompt, &items)?.ok_or(AppError::CanceledSelection)?;
    let branch_name =
        items
            .get(selected_index)
            .copied()
            .ok_or(AppError::InvalidSelectionIndex {
                index: selected_index,
            })?;

    let branch = repo.find_branch(branch_name, BranchType::Local)?;
    if let Some(name) = branch.get().name() {
        let target = branch.get().peel_to_commit()?;
        repo.checkout_tree(target.as_object(), None)?;
        repo.set_head(name)?;
        // TODO this works but makes it so when I push it counts all elements
        // I'm unsure why and how to fix it
    }
    Ok(())
}

/// Return local branches other than the currently checked-out branch.
#[expect(
    clippy::single_call_fn,
    reason = "keeps branch discovery testable and readable"
)]
fn branch_options(
    repo: &Repository,
    current_branch_name: &str,
) -> Result<Vec<BranchOption>, git2::Error> {
    Ok(repo
        .branches(Some(BranchType::Local))?
        .filter_map(Result::ok)
        .filter_map(
            |(branch, _)| match (branch.name(), branch.get().peel_to_commit()) {
                (Ok(Some(name)), Ok(commit)) => {
                    (name != current_branch_name).then(|| BranchOption {
                        name: name.to_owned(),
                        commit_time: commit.committer().when().seconds(),
                    })
                }
                _ => None,
            },
        )
        .collect())
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "tests favor direct assertions and fixture setup"
)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU64, Ordering};
    use git2::{IndexAddOption, Oid, Signature, Time};
    use std::env::temp_dir;
    use std::fs;
    use std::path::PathBuf;
    use std::process::id;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_REPO_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestRepo {
        path: PathBuf,
        repo: Repository,
    }

    impl TestRepo {
        fn checkout(&self, branch_name: &str) {
            let object = self
                .repo
                .revparse_single(&format!("refs/heads/{branch_name}"))
                .expect("find branch object");
            self.repo
                .checkout_tree(&object, None)
                .expect("checkout tree");
            self.repo
                .set_head(&format!("refs/heads/{branch_name}"))
                .expect("set head");
        }

        fn commit_file(&self, branch_name: &str, timestamp: i64) -> Oid {
            fs::write(
                self.path.join("branch.txt"),
                format!("{branch_name}:{timestamp}\n"),
            )
            .expect("write branch file");

            let mut index = self.repo.index().expect("open index");
            index
                .add_all(["*"], IndexAddOption::DEFAULT, None)
                .expect("add files");
            index.write().expect("write index");
            let tree_oid = index.write_tree().expect("write tree");
            let tree = self.repo.find_tree(tree_oid).expect("find tree");
            let signature = Signature::new(
                "gci tests",
                "gci-tests@example.com",
                &Time::new(timestamp, 0),
            )
            .expect("create signature");
            let parent = self
                .repo
                .head()
                .ok()
                .and_then(|head| head.target())
                .and_then(|oid| self.repo.find_commit(oid).ok());
            let message = format!("commit {branch_name}");

            parent.map_or_else(
                || {
                    self.repo
                        .commit(Some("HEAD"), &signature, &signature, &message, &tree, &[])
                        .expect("initial commit")
                },
                |parent_commit| {
                    self.repo
                        .commit(
                            Some("HEAD"),
                            &signature,
                            &signature,
                            &message,
                            &tree,
                            &[&parent_commit],
                        )
                        .expect("commit with parent")
                },
            )
        }

        fn create_branch_from_head(&self, branch_name: &str) {
            let head_commit = self
                .repo
                .head()
                .expect("read head")
                .peel_to_commit()
                .expect("peel head");
            self.repo
                .branch(branch_name, &head_commit, false)
                .expect("create branch");
        }

        fn new() -> Self {
            let path = unique_temp_path();
            fs::create_dir_all(&path).expect("create temp repo dir");
            let repo = Repository::init(&path).expect("init repository");

            Self { path, repo }
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ignored = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn returns_single_branch_error_when_no_other_branch_exists() {
        let test_repo = TestRepo::new();
        test_repo.commit_file("master", 1_700_000_000);

        let error = run_with_selector(&test_repo.repo, |_, _| Ok(Some(0))).unwrap_err();

        assert_app_error(error.as_ref(), &AppError::SingleBranch);
    }

    #[test]
    fn returns_canceled_selection_error() {
        let test_repo = repo_with_three_branches();

        let error = run_with_selector(&test_repo.repo, |_, _| Ok(None)).unwrap_err();

        assert_app_error(error.as_ref(), &AppError::CanceledSelection);
    }

    #[test]
    fn returns_invalid_selection_index_error() {
        let test_repo = repo_with_three_branches();

        let error = run_with_selector(&test_repo.repo, |_, _| Ok(Some(99))).unwrap_err();

        assert_app_error(
            error.as_ref(),
            &AppError::InvalidSelectionIndex { index: 99 },
        );
    }

    #[test]
    fn offers_other_local_branches_newest_first() {
        let test_repo = repo_with_three_branches();
        let mut offered_items = Vec::new();

        run_with_selector(&test_repo.repo, |_, items| {
            offered_items = items.iter().map(ToString::to_string).collect();
            Ok(Some(0))
        })
        .expect("checkout selected branch");

        assert_eq!(offered_items, ["newer", "older"]);
    }

    #[test]
    fn selecting_a_branch_updates_head() {
        let test_repo = repo_with_three_branches();

        run_with_selector(&test_repo.repo, |_, items| {
            assert_eq!(items, ["newer", "older"]);
            Ok(Some(1))
        })
        .expect("checkout selected branch");

        let head = test_repo.repo.head().expect("read head");
        assert_eq!(head.shorthand(), Some("older"));
    }

    fn repo_with_three_branches() -> TestRepo {
        let test_repo = TestRepo::new();
        test_repo.commit_file("master", 1_700_000_000);
        test_repo.create_branch_from_head("older");
        test_repo.create_branch_from_head("newer");

        test_repo.checkout("older");
        test_repo.commit_file("older", 1_700_000_100);

        test_repo.checkout("newer");
        test_repo.commit_file("newer", 1_700_000_200);

        test_repo.checkout("master");
        test_repo
    }

    fn assert_app_error(error: &(dyn Error + 'static), expected: &AppError) {
        match error.downcast_ref::<AppError>() {
            Some(actual) if actual == expected => {}
            other => panic!("expected {expected:?}, got {other:?}"),
        }
    }

    #[expect(
        clippy::single_call_fn,
        reason = "keeps test fixture creation readable"
    )]
    fn unique_temp_path() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        let counter = TEST_REPO_COUNTER.fetch_add(1, Ordering::Relaxed);
        temp_dir().join(format!("gci-test-{}-{timestamp}-{counter}", id()))
    }
}
