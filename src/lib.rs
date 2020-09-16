use glob::{MatchOptions, Pattern as PatternMatcher};
use regex::Regex;
use std::env;
use std::path::{Path, PathBuf};

fn first_char(string: &str) -> char {
    string.chars().nth(0).unwrap()
}

fn has_no_middle_separators(string: &str) -> bool {
    let segments: Vec<&str> = string.split("/").collect();
    let non_empty: Vec<&str> = segments.iter().filter(|s| !s.is_empty()).copied().collect();
    non_empty.len() <= 1
}

#[derive(Debug)]
enum Match {
    Anywhere,
    Relative,
}

#[derive(Debug)]
enum Extension {
    Defined,
    Undefined,
}

#[derive(Debug)]
enum PathKind {
    Directory,
    File,
    Both,
}

pub struct Pattern {
    pub string: String,
    match_type: Match,
    extension_type: Extension,
    path_kind: PathKind,
    negated: bool,
}

impl Pattern {
    pub fn new<P: AsRef<Path>>(glob: P) -> Self {
        let has_extension = Regex::new(r"\*\..+?$").unwrap();
        let glob = glob.as_ref().to_str().unwrap_or("");
        let negated = glob.starts_with("!");
        let normalized_glob = if negated { &glob[1..] } else { glob };

        let match_type = if !normalized_glob.starts_with("**")
            && first_char(normalized_glob) != '/'
            && has_no_middle_separators(normalized_glob)
        {
            Match::Anywhere
        } else {
            Match::Relative
        };
        let extension_type = if has_extension.is_match(normalized_glob) {
            Extension::Defined
        } else {
            Extension::Undefined
        };
        let path_kind = match extension_type {
            Extension::Defined => PathKind::File,
            Extension::Undefined => {
                if normalized_glob.ends_with("/") {
                    PathKind::Directory
                } else {
                    PathKind::Both
                }
            }
        };

        Self {
            string: String::from(normalized_glob),
            negated,
            match_type,
            extension_type,
            path_kind,
        }
    }
}

pub struct Gitignore<P: AsRef<Path>> {
    root: P,
    options: MatchOptions,
}

impl Default for Gitignore<PathBuf> {
    fn default() -> Self {
        let mut options = MatchOptions::new();
        options.require_literal_separator = false;

        Self {
            root: env::current_dir().unwrap(),
            options,
        }
    }
}

impl<P: AsRef<Path>> Gitignore<P> {
    pub fn new(root: P, options: MatchOptions) -> Gitignore<P> {
        Gitignore { root, options }
    }

    fn make_absolute(&mut self, p: &str) -> String {
        self.options.require_literal_separator = true;

        let root_str = self.root.as_ref().display();

        if p.starts_with("**/") {
            return String::from(p);
        }

        if p.starts_with("/") {
            return format!("{}{}", root_str, p);
        }

        format!("{}/{}", root_str, p)
    }

    fn make_absolute_anywhere(&mut self, p: &str) -> String {
        self.options.require_literal_separator = false;

        let mut unformatted = p;

        if unformatted.ends_with("*") {
            unformatted = &p[..p.len() - 1];
        }

        format!("{}{}", "**/", unformatted)
    }

    pub fn ignores(&mut self, glob: impl AsRef<Path>, target: impl AsRef<Path>) -> bool {
        let glob = Pattern::new(glob);

        let full_path = match (&glob.path_kind, &glob.match_type) {
            (PathKind::Both, Match::Anywhere) => self.make_absolute_anywhere(&glob.string) + "*",
            (PathKind::File, Match::Anywhere) => self.make_absolute_anywhere(&glob.string),
            (PathKind::Directory, Match::Anywhere) => {
                self.make_absolute_anywhere(&glob.string) + "**/*"
            }
            (PathKind::Both, Match::Relative) => self.make_absolute(&glob.string) + "*",
            (PathKind::File, Match::Relative) => self.make_absolute(&glob.string),
            (PathKind::Directory, Match::Relative) => self.make_absolute(&glob.string) + "**/*",
        };

        let matcher = PatternMatcher::new(&full_path).unwrap();

        if glob.negated {
            !matcher.matches_path_with(target.as_ref(), self.options)
        } else {
            matcher.matches_path_with(target.as_ref(), self.options)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let cwd = std::env::current_dir().unwrap();
        let mut gitignore = Gitignore::default();

        assert!(gitignore.ignores("**/dist/*.js", gitignore.root.join("build/dist/lib.js")));
        assert!(gitignore.ignores("/**/dist/*.js", gitignore.root.join("build/dist/lib.js")));
        assert!(gitignore.ignores("/dist/**/*.js", gitignore.root.join("dist/types/types.js")));

        assert!(gitignore.ignores("/lib.js", gitignore.root.join("lib.js")));
        assert!(gitignore.ignores("lib/*.js", gitignore.root.join("lib/module.js")));
        assert!(gitignore.ignores("/lib/", gitignore.root.join("lib/module.js")));
        assert!(gitignore.ignores("lib/", gitignore.root.join("lib/module.js")));
        assert!(gitignore.ignores("lib/", gitignore.root.join("dist/lib/module.js")));
        assert!(gitignore.ignores("lib/", gitignore.root.join("lib/nested/module.js")));
        assert!(gitignore.ignores("lib", gitignore.root.join("lib/nested/module.js")));
        assert!(gitignore.ignores("lib", gitignore.root.join("lib")));
        assert!(gitignore.ignores("lib", gitignore.root.join("lib/module.js")));

        assert!(gitignore.ignores("remove-*", gitignore.root.join("remove-items.js")));
        assert!(gitignore.ignores("/*.js", gitignore.root.join("module.js")));
        assert!(gitignore.ignores("!/lib.js", gitignore.root.join("lib/lib.js")));
        assert!(gitignore.ignores("!lib/*.js", gitignore.root.join("dist/lib/module.js")));
        assert!(gitignore.ignores("*.js", gitignore.root.join("dist/module.js")));
        assert!(gitignore.ignores("*.js", gitignore.root.join("module.js")));

        assert!(!gitignore.ignores("!/*.js", gitignore.root.join("module.js")));
        assert!(!gitignore.ignores("/lib.js", gitignore.root.join("lib/lib.js")));
        assert!(!gitignore.ignores("/dist/*.js", gitignore.root.join("dist/types/types.js")));
        assert!(!gitignore.ignores("lib/*.js", gitignore.root.join("dist/lib/module.js")));
        assert!(!gitignore.ignores(
            "dist/lib/",
            gitignore.root.join("parent/dist/lib/module.js")
        ));
        assert!(!gitignore.ignores("!lib/", gitignore.root.join("dist/lib/module.js")));
        assert!(!gitignore.ignores("!lib", gitignore.root.join("dist/lib/module.js")));
    }
}
