use glob::{MatchOptions, Pattern as PatternMatcher};
use regex::Regex;
use std::env;
use std::path::{Path, PathBuf};

fn first_char(string: &str) -> char {
    string.chars().nth(0).unwrap()
}

fn negate(string: &str) -> String {
    format!("!{}", string)
}

fn has_no_middle_separators(string: &str) -> bool {
    let segments: Vec<&str> = string.split("/").filter(|s| !s.is_empty()).collect();
    segments.len() <= 1
}

fn first_segment(string: &str) -> String {
    let normalized = if string.starts_with("/") {
        &string[1..]
    } else {
        string
    };
    let segments: Vec<&str> = normalized.split("/").collect();

    String::from(*segments.first().unwrap())
}

fn remove_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
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
    Dir,
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
        let has_extension = Regex::new(r"\*\.[^\*]+$").unwrap();
        let glob = glob.as_ref().to_str().unwrap_or("");
        let negated = glob.starts_with("!");
        let without_neg = if negated { &glob[1..] } else { glob };
        let normalized_glob = remove_whitespace(without_neg);

        let match_type = if !normalized_glob.starts_with("**")
            && first_char(&normalized_glob) != '/'
            && has_no_middle_separators(&normalized_glob)
        {
            Match::Anywhere
        } else {
            Match::Relative
        };

        let extension_type = if has_extension.is_match(&normalized_glob) {
            Extension::Defined
        } else {
            Extension::Undefined
        };

        let path_kind = match extension_type {
            Extension::Defined => PathKind::File,
            Extension::Undefined => {
                if normalized_glob.ends_with("/") {
                    PathKind::Dir
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

    pub fn ignores_path(&mut self, glob: Pattern, target: impl AsRef<Path>) -> bool {
        let full_path = match (glob.path_kind, glob.match_type) {
            (PathKind::Both, Match::Anywhere) => self.make_absolute_anywhere(&glob.string) + "*",
            (PathKind::File, Match::Anywhere) => self.make_absolute_anywhere(&glob.string),
            (PathKind::Dir, Match::Anywhere) => self.make_absolute_anywhere(&glob.string) + "**/*",
            (PathKind::Both, Match::Relative) => self.make_absolute(&glob.string) + "*",
            (PathKind::File, Match::Relative) => self.make_absolute(&glob.string),
            (PathKind::Dir, Match::Relative) => self.make_absolute(&glob.string) + "**/*",
        };

        let matcher = PatternMatcher::new(&full_path).unwrap();

        if glob.negated {
            !matcher.matches_path_with(target.as_ref(), self.options)
        } else {
            matcher.matches_path_with(target.as_ref(), self.options)
        }
    }

    pub fn ignores(&mut self, lines: Vec<&str>, target: impl AsRef<Path>) -> bool {
        let mut ignored_dirs: Vec<String> = Vec::new();

        for line in lines.iter() {
            let glob = Pattern::new(line);
            let Pattern {
                path_kind,
                match_type,
                string,
                ..
            } = &glob;

            // Disallow re-include by negation if parent dir is ignored unless the same parent is negated, with or without /
            match (path_kind, match_type) {
                (PathKind::Both, Match::Anywhere) | (PathKind::Dir, Match::Anywhere) => {
                    let neg = &negate(string)[..];
                    let neg_with_root = &negate(&format!("/{}", string))[..];

                    if !glob.negated && !lines.contains(&neg) && !lines.contains(&neg_with_root) {
                        ignored_dirs.push(first_segment(string));
                    }
                }
                _ => (),
            }
        }

        let mut is_ignored = false;

        for line in lines.iter() {
            let glob = Pattern::new(line);

            let unprefixed = target
                .as_ref()
                .strip_prefix(&self.root)
                .expect("Target must be an absolute path!");

            let has_ignored_parent =
                ignored_dirs.contains(&first_segment(unprefixed.to_str().unwrap()));

            if has_ignored_parent {
                return true;
            }

            is_ignored = self.ignores_path(glob, target.as_ref());
        }

        is_ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line() {
        let mut ig = Gitignore::default();

        assert!(ig.ignores_path(
            Pattern::new("**/dist/*.js"),
            ig.root.join("build/dist/lib.js")
        ));
        assert!(ig.ignores_path(
            Pattern::new("/**/dist/*.js"),
            ig.root.join("build/dist/lib.js")
        ));
        assert!(ig.ignores_path(
            Pattern::new("/dist/**/*.js"),
            ig.root.join("dist/types/types.js")
        ));

        assert!(ig.ignores_path(Pattern::new("/lib.js"), ig.root.join("lib.js")));
        assert!(ig.ignores_path(Pattern::new("lib/*.js"), ig.root.join("lib/module.js")));
        assert!(ig.ignores_path(Pattern::new("/lib/"), ig.root.join("lib/module.js")));
        assert!(ig.ignores_path(Pattern::new("lib/"), ig.root.join("lib/module.js")));
        assert!(ig.ignores_path(Pattern::new("lib/"), ig.root.join("dist/lib/module.js")));
        assert!(ig.ignores_path(Pattern::new("lib/"), ig.root.join("lib/nested/module.js")));
        assert!(ig.ignores_path(Pattern::new("lib"), ig.root.join("lib/nested/module.js")));
        assert!(ig.ignores_path(Pattern::new("lib"), ig.root.join("lib")));
        assert!(ig.ignores_path(Pattern::new("lib"), ig.root.join("lib/module.js")));

        assert!(ig.ignores_path(Pattern::new("remove-*"), ig.root.join("remove-items.js")));
        assert!(ig.ignores_path(Pattern::new("/*.js"), ig.root.join("module.js")));
        assert!(ig.ignores_path(Pattern::new("!/lib.js"), ig.root.join("lib/lib.js")));
        assert!(ig.ignores_path(
            Pattern::new("!lib/*.js"),
            ig.root.join("dist/lib/module.js")
        ));
        assert!(ig.ignores_path(Pattern::new("*.js"), ig.root.join("dist/module.js")));
        assert!(ig.ignores_path(Pattern::new("*.js"), ig.root.join("module.js")));

        assert!(!ig.ignores_path(Pattern::new("!/*.js"), ig.root.join("module.js")));
        assert!(!ig.ignores_path(Pattern::new("/lib.js"), ig.root.join("lib/lib.js")));
        assert!(!ig.ignores_path(
            Pattern::new("/dist/*.js"),
            ig.root.join("dist/types/types.js")
        ));
        assert!(!ig.ignores_path(Pattern::new("lib/*.js"), ig.root.join("dist/lib/module.js")));
        assert!(!ig.ignores_path(
            Pattern::new("dist/lib/"),
            ig.root.join("parent/dist/lib/module.js")
        ));
        assert!(!ig.ignores_path(Pattern::new("!lib/"), ig.root.join("dist/lib/module.js")));
        assert!(!ig.ignores_path(Pattern::new("!lib"), ig.root.join("dist/lib/module.js")));
    }

    #[test]
    fn multiple_lines() {
        let mut ig = Gitignore::default();

        let a = vec!["lib/", "!lib/*.js"];
        let b = vec!["lib", "!lib/*.js"];
        let c = vec!["!lib/*.js", "lib"];

        let i = vec!["lib/*.js", "!lib/include.js"];
        let j = vec!["lib/*.js", "!lib/"];
        let k = vec!["lib/", "!lib/"];
        let k = vec!["lib/", "!/lib/"];

        assert!(ig.ignores(a, ig.root.join("lib/include.js")));
        assert!(ig.ignores(b, ig.root.join("lib/include.js")));
        assert!(ig.ignores(c, ig.root.join("lib/include.js")));

        assert!(!ig.ignores(i, ig.root.join("lib/include.js")));
        assert!(!ig.ignores(j, ig.root.join("lib/include.js")));
        assert!(!ig.ignores(k, ig.root.join("lib/include.js")));
    }
}
