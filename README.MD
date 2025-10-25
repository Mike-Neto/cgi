# gci - git checkout interactive

inspired by https://github.com/cookpete/git-checkout-interactive/tree/master, a quick interactive cli select for locally checked out branches

## Usage Example

    gci

## Install

    cargo install git-checkout-interactive

## Release/Publish to crates.io

    cargo bump --git-tag && git push --tags

## All checks

    cargo build && cargo test && cargo bench && cargo fmt --check && cargo doc && cargo clippy && cargo clippy --all -- -W clippy::all -W clippy::correctness -W clippy::suspicious -W clippy::style -W clippy::complexity -W clippy::perf -W clippy::pedantic -W clippy::nursery -W clippy::cargo
