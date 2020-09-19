use fs::File;
use gitignored::Gitignore;
use std::env;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use walkdir::{DirEntry, WalkDir};

fn is_excluded(entry: &DirEntry) -> bool {
    let mut ig = Gitignore::default();
    let paths = vec![".git/", "target/", "src/*.rs", "!src/a.rs"];

    if entry.path() == ig.root {
        return false;
    }

    entry
        .path()
        .to_str()
        .map(|s| ig.ignores(&paths, s))
        .unwrap_or(false)
}

#[test]
fn it_includes_unignored_paths() -> std::io::Result<()> {
    let cwd = env::current_dir()?;

    let dir = tempdir()?;
    let dir_path = dir.path();

    fs::create_dir_all(dir_path.join("src")).unwrap();
    fs::create_dir_all(dir_path.join(".git")).unwrap();
    fs::create_dir_all(dir_path.join("target")).unwrap();

    let _a = File::create(dir_path.join("src/a.rs"))?;
    let _b = File::create(dir_path.join("src/b.rs"))?;
    let _c = File::create(dir_path.join("src/c.rs"))?;
    let _d = File::create(dir_path.join(".git/gitfile"))?;
    let _e = File::create(dir_path.join("Cargo.toml"))?;
    let _f = File::create(dir_path.join("target/targetfile"))?;

    env::set_current_dir(dir.path())?;

    let expected_paths = vec![dir.path().join("Cargo.toml"), dir.path().join("src/a.rs")];

    let mut result: Vec<PathBuf> = Vec::new();

    for entry in WalkDir::new(std::env::current_dir().unwrap())
        .into_iter()
        .filter_entry(|a| !is_excluded(a))
    {
        let entry = entry.unwrap();
        if !entry.path().is_dir() {
            result.push(entry.path().to_owned());
        }
    }

    assert_eq!(expected_paths, result);

    env::set_current_dir(cwd)?;

    drop(_a);
    drop(_b);
    drop(_c);
    drop(_d);
    drop(_e);
    drop(_f);

    dir.close()?;

    Ok(())
}
