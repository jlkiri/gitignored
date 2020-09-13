use glob::{MatchOptions, Pattern as PatternMatcher};
use regex::Regex;
use std::env;
use std::path::{Path, PathBuf};

fn first_char(string: &str) -> char {
    string.chars().nth(0).unwrap()
}

fn first_segment<'a, P: AsRef<Path>>(path: &'a P) -> &'a str {
    let str_path = path.as_ref().to_str().unwrap();
    let _path = match first_char(str_path) {
        '/' => &str_path[1..],
        _ => str_path,
    };

    let segments: Vec<&str> = _path.split("/").collect();

    segments.first().unwrap()
}

#[derive(Debug)]
enum Filepath {
    Absolute,
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
}

pub struct Pattern {
    pub string: String,
    path_type: Filepath,
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

        let path_type = if !normalized_glob.starts_with("**") && first_char(normalized_glob) != '/'
        {
            Filepath::Relative
        } else {
            Filepath::Absolute
        };
        let extension_type = if has_extension.is_match(normalized_glob) {
            Extension::Defined
        } else {
            Extension::Undefined
        };
        let path_kind = if normalized_glob.ends_with("/") {
            PathKind::Directory
        } else {
            PathKind::File
        };

        Self {
            string: String::from(normalized_glob),
            negated,
            path_type,
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
        options.require_literal_separator = true;

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

    fn prepend_root(&self, p: &str) -> String {
        let root_str = self.root.as_ref().display();

        if p.starts_with("**/") {
            return String::from(p);
        }

        if p.starts_with("/") {
            return format!("{}{}", root_str, p);
        }

        format!("{}{}", "**/", p)
    }

    pub fn includes(&self, glob: impl AsRef<Path>, target: impl AsRef<Path>) -> bool {
        let glob = Pattern::new(glob);
        let glob_dir = first_segment(&glob.string);
        let unprefixed = target.as_ref().strip_prefix(&self.root).unwrap();
        let target_dir = first_segment(&unprefixed);

        let full_path = match glob.path_kind {
            PathKind::File => self.prepend_root(&glob.string),
            PathKind::Directory => self.prepend_root(&glob.string) + "**/*",
        };
        let matcher = PatternMatcher::new(&full_path).unwrap();

        if let Extension::Defined = glob.extension_type {
            if let Filepath::Relative = glob.path_type {
                if glob_dir != target_dir {
                    if glob.negated {
                        return true;
                    }
                    return false;
                }
            }
        }

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
        let gitignore = Gitignore::default();

        assert!(gitignore.includes("**/dist/*.js", cwd.join("build/dist/lib.js")));
        assert!(gitignore.includes("/dist/**/*.js", cwd.join("dist/types/types.js")));
        assert!(gitignore.includes("/lib.js", cwd.join("lib.js")));
        assert!(gitignore.includes("lib/*.js", cwd.join("lib/module.js")));
        assert!(gitignore.includes("/lib/", cwd.join("lib/module.js")));
        assert!(gitignore.includes("lib/", cwd.join("lib/module.js")));
        assert!(gitignore.includes("lib/", cwd.join("dist/lib/module.js")));
        assert!(gitignore.includes("lib/", cwd.join("lib/nested/module.js")));
        assert!(gitignore.includes("remove-*", cwd.join("remove-items.js")));
        assert!(gitignore.includes("/*.js", cwd.join("module.js")));
        assert!(gitignore.includes("!/lib.js", cwd.join("lib/lib.js")));
        assert!(gitignore.includes("!lib/*.js", cwd.join("dist/lib/module.js")));

        assert!(!gitignore.includes("!/*.js", cwd.join("module.js")));
        assert!(!gitignore.includes("/lib.js", cwd.join("lib/lib.js")));
        assert!(!gitignore.includes("/dist/*.js", cwd.join("dist/types/types.js")));
        assert!(!gitignore.includes("lib/*.js", cwd.join("dist/lib/module.js")));
        assert!(!gitignore.includes("*.js", cwd.join("dist/module.js")));
        assert!(!gitignore.includes("*.js", cwd.join("module.js")));
        assert!(!gitignore.includes("lib", cwd.join("lib/module.js")));
    }
}
