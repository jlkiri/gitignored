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

    pub fn get_parents(&self) -> Vec<String> {
        let mut segments: Vec<&str> = self.string.split("/").collect();
        let mut parents: Vec<String> = Vec::new();
        while segments.len() > 1 {
            let mut joined = segments[..segments.len() - 1].join("/");
            joined.push_str("/");
            parents.push(joined);
            segments.pop();
        }

        parents.into_iter().filter(|p| !p.is_empty()).collect()
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
        let full_path = self.make_full_path(&glob);

        let matcher = PatternMatcher::new(&full_path).unwrap();

        if glob.negated {
            !matcher.matches_path_with(target.as_ref(), self.options)
        } else {
            matcher.matches_path_with(target.as_ref(), self.options)
        }
    }

    fn make_full_path(&mut self, glob: &Pattern) -> String {
        match (&glob.path_kind, &glob.match_type) {
            (PathKind::Both, Match::Anywhere) => self.make_absolute_anywhere(&glob.string) + "*",
            (PathKind::File, Match::Anywhere) => self.make_absolute_anywhere(&glob.string),
            (PathKind::Dir, Match::Anywhere) => self.make_absolute_anywhere(&glob.string) + "**/*",
            (PathKind::Both, Match::Relative) => self.make_absolute(&glob.string) + "*",
            (PathKind::File, Match::Relative) => self.make_absolute(&glob.string),
            (PathKind::Dir, Match::Relative) => self.make_absolute(&glob.string) + "**/*",
        }
    }

    pub fn ignores(&mut self, lines: &[&str], target: impl AsRef<Path>) -> bool {
        let mut ignored_dirs: Vec<String> = Vec::new();

        for line in lines.iter() {
            let glob = Pattern::new(line);
            let Pattern {
                path_kind,
                match_type,
                ..
            } = &glob;

            let parents: Vec<String> = glob.get_parents().into_iter().map(|p| negate(&p)).collect();

            // TODO: make this not so hacky?
            let relative_parents: Vec<String> = glob
                .get_parents()
                .into_iter()
                .map(|p| negate(&format!("/{}", p)[..]))
                .collect();

            let contains_negated_parents = parents.iter().any(|p| lines.contains(&&p[..]));
            let contains_negated_relative_parents =
                relative_parents.iter().any(|p| lines.contains(&&p[..]));

            // Disallow re-include by negation if parent dir is ignored unless the same parent is negated, with or without /
            match (path_kind, match_type) {
                (PathKind::Both, Match::Anywhere) | (PathKind::Dir, Match::Anywhere) => {
                    if !glob.negated
                        && !contains_negated_parents
                        && !contains_negated_relative_parents
                    {
                        ignored_dirs.push(glob.string.clone());
                    }
                }
                (PathKind::Both, Match::Relative) | (PathKind::Dir, Match::Relative) => {
                    if !glob.negated && !contains_negated_parents {
                        ignored_dirs.push(glob.string.clone());
                    }
                }
                _ => (),
            }
        }

        let mut is_ignored = false;

        for line in lines.iter() {
            let glob = Pattern::new(line);

            let has_ignored_parent = ignored_dirs.iter().any(|dir| {
                let s = match (&glob.path_kind, &glob.match_type) {
                    (PathKind::Both, Match::Anywhere) => self.make_absolute_anywhere(dir) + "*",
                    (PathKind::File, Match::Anywhere) => self.make_absolute_anywhere(dir),
                    (PathKind::Dir, Match::Anywhere) => self.make_absolute_anywhere(dir) + "**/*",
                    (PathKind::Both, Match::Relative) => self.make_absolute(dir) + "*",
                    (PathKind::File, Match::Relative) => self.make_absolute(dir),
                    (PathKind::Dir, Match::Relative) => self.make_absolute(dir) + "**/*",
                };

                let path = target.as_ref().display().to_string();

                let matcher = PatternMatcher::new(&s).unwrap();
                matcher.matches(&path)
            });

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
        let d = vec!["lib/", "!lib/deep/include.js"];
        let e = vec!["/lib/", "!/lib/deep/"];
        let f = vec!["lib/", "!/lib/"];
        let g = vec!["!/lib/", "lib/"];
        let h = vec!["**/remove-items.js"];
        let i = vec!["remove-items*"];
        let j = vec!["remove*, !remove-items.js"];

        let k = vec!["lib/*.js", "!lib/include.js"];
        let l = vec!["lib/*.js", "!lib/"];
        let m = vec!["lib/", "!lib/"];
        let n = vec!["lib/", "!/lib/"];

        assert!(ig.ignores(&a, ig.root.join("lib/include.js")));
        assert!(ig.ignores(&b, ig.root.join("lib/include.js")));
        assert!(ig.ignores(&c, ig.root.join("lib/include.js")));
        assert!(ig.ignores(&d, ig.root.join("lib/deep/include.js")));
        assert!(ig.ignores(&e, ig.root.join("lib/deep/include.js")));
        assert!(ig.ignores(&f, ig.root.join("deep/lib/include.js")));
        assert!(ig.ignores(&g, ig.root.join("deep/lib/include.js")));
        assert!(ig.ignores(&h, ig.root.join("deep/lib/remove-items.js")));
        assert!(ig.ignores(&i, ig.root.join("deep/lib/remove-items.js")));

        assert!(!ig.ignores(&j, ig.root.join("deep/lib/remove-items.js")));
        assert!(!ig.ignores(&k, ig.root.join("lib/include.js")));
        assert!(!ig.ignores(&l, ig.root.join("lib/include.js")));
        assert!(!ig.ignores(&m, ig.root.join("lib/include.js")));
        assert!(!ig.ignores(&n, ig.root.join("lib/include.js")));
    }
}
