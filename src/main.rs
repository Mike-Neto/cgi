use core::error::Error;
use dialoguer::Select;
use git2::Repository;
use std::env::current_dir;

fn main() -> Result<(), Box<dyn Error>> {
    let repo = Repository::open(current_dir()?)?;
    git_checkout_interactive::run_with_selector(&repo, |prompt, items| {
        Ok(Select::new()
            .with_prompt(prompt)
            .items(items)
            .default(0)
            .interact_opt()?)
    })
}
