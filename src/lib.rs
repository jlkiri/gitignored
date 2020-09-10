use gitignore::Pattern;
use std::path::{Path, PathBuf};

fn create_absolute<P: AsRef<Path>>(p: P) -> PathBuf {
    let _cwd = std::env::current_dir().unwrap();
    let cwd = _cwd.as_path();
    cwd.join(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn it_works() {
        let _cwd = std::env::current_dir().unwrap();
        let cwd = _cwd.as_path();
        let ps = "lib.js";
        let mut pattern = Pattern::new(ps, cwd).unwrap();
        println!("pattern: {:?}", pattern.anchored);

        let index = create_absolute("lib/lib.js");

        // assert_eq!(pattern.is_excluded(module.as_path(), false), false);
        assert_eq!(pattern.is_excluded(index.as_path(), false), true);
    }
}
