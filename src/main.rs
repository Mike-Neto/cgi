use core::error::Error;
use core::fmt::{self, Display, Formatter};
use dialoguer::Select;
use git2::{BranchType, Repository};
use std::env::current_dir;

/// Errors produced while choosing and switching branches.
#[derive(Debug)]
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

fn main() -> Result<(), Box<dyn Error>> {
    let repo = Repository::open(current_dir()?)?;
    let head = repo.head()?;

    let current_branch_name = head.shorthand().unwrap_or_default();
    let mut branches: Vec<BranchOption> = repo
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
        .collect();
    branches.sort_by_key(|branch| branch.commit_time);

    let items = branches
        .iter()
        .rev()
        .map(|branch| branch.name.as_str())
        .collect::<Vec<&str>>();
    if items.is_empty() {
        return Err(Box::new(AppError::SingleBranch));
    }

    let selected_index = Select::new()
        .with_prompt(format!("Switch Branch? {current_branch_name} ->"))
        .items(&items)
        .default(0)
        .interact_opt()?
        .ok_or(AppError::CanceledSelection)?;

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
