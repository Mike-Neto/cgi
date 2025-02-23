use dialoguer::Select;
use git2::{BranchType, Repository};

fn main() -> anyhow::Result<()> {
    let repo = Repository::open(std::env::current_dir()?)?;
    let head = repo.head()?;

    if let Some(Some(current_branch)) = head.is_branch().then(|| head.shorthand()) {
        let mut branches: Vec<(String, i64)> = repo
            .branches(Some(BranchType::Local))?
            .filter_map(|b| b.ok())
            .filter_map(|(b, _)| match (b.name(), b.get().peel_to_commit()) {
                (Ok(Some(name)), Ok(commit)) => {
                    if name != current_branch {
                        Some((name.to_string(), commit.committer().when().seconds()))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();

        branches.sort_by_key(|(_, d)| *d);

        let items = branches
            .iter()
            .rev()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<&str>>();

        let selection = Select::new()
            .with_prompt(format!("Switch Branch? {current_branch} ->"))
            .items(&items)
            .default(0)
            .interact_opt()?;

        if let Some(selection) = selection {
            let branch = repo.find_branch(items[selection], BranchType::Local)?;
            if let Some(name) = branch.get().name() {
                let target = branch.get().peel_to_commit()?;
                repo.checkout_tree(target.as_object(), None)?;
                repo.set_head(name)?;
                // TODO this works but makes it so when I push it counts all elements
            }
        } else {
            println!("canceled selection")
        }
    }
    Ok(())
}
