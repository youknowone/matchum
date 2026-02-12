use gix_attributes::parse::{Kind, Lines};
use std::path::Path;

/// Read `.gitattributes` from `root` and return glob patterns marked as
/// `linguist-vendored` (or `linguist-generated`).
///
/// Returned strings are gitignore-style globs suitable for `globset::Glob`.
pub fn vendored_globs(root: &Path) -> Vec<String> {
    let path = root.join(".gitattributes");
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };

    let mut globs = Vec::new();
    for entry in Lines::new(&bytes) {
        let (kind, attrs, _line_no) = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };
        let pattern_text = match kind {
            Kind::Pattern(ref pat) => pat.text.to_string(),
            Kind::Macro(_) => continue,
        };
        let is_vendored = attrs.filter_map(|a| a.ok()).any(|a| {
            let name = a.name.as_str();
            name == "linguist-vendored" || name == "linguist-generated"
        });
        if is_vendored {
            globs.push(pattern_text);
        }
    }
    globs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_vendored_patterns() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = std::fs::File::create(dir.path().join(".gitattributes")).unwrap();
        writeln!(f, "vendor/** linguist-vendored").unwrap();
        writeln!(f, "generated/** linguist-generated").unwrap();
        writeln!(f, "src/** text").unwrap();
        drop(f);

        let globs = vendored_globs(dir.path());
        assert_eq!(globs, vec!["vendor/**", "generated/**"]);
    }

    #[test]
    fn no_gitattributes_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let globs = vendored_globs(dir.path());
        assert!(globs.is_empty());
    }

    /// Vendored globs must filter out absolute paths produced by WalkBuilder.
    /// This reproduces the real bug: `vendor/**` does not match
    /// `/abs/root/vendor/file.txt` unless we strip the root prefix first.
    #[test]
    fn vendored_globs_filter_absolute_walked_paths() {
        let dir = tempfile::tempdir().unwrap();
        // .gitattributes marks vendor as vendored
        std::fs::write(
            dir.path().join(".gitattributes"),
            "vendor/** linguist-vendored\n",
        )
        .unwrap();
        // Create vendor and src files
        std::fs::create_dir_all(dir.path().join("vendor/sub")).unwrap();
        std::fs::write(dir.path().join("vendor/sub/file.txt"), "hello").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

        let root = dir.path();
        let globs = vendored_globs(root);
        assert_eq!(globs, vec!["vendor/**"]);

        let mut builder = globset::GlobSetBuilder::new();
        for pattern in &globs {
            builder.add(globset::Glob::new(pattern).unwrap());
        }
        let glob_set = builder.build().unwrap();

        // WalkBuilder returns absolute paths when given an absolute root
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        for entry in ignore::WalkBuilder::new(root).build() {
            if let Ok(e) = entry {
                if e.file_type().is_some_and(|ft| ft.is_file()) {
                    files.push(e.into_path());
                }
            }
        }
        assert!(files.iter().any(|f| f.to_str().unwrap().contains("vendor")));

        // Matching absolute paths directly does NOT filter vendored files.
        // We must strip the root prefix before matching.
        let mut unstripped = files.clone();
        unstripped.retain(|f| !glob_set.is_match(f));
        assert!(
            unstripped
                .iter()
                .any(|f| f.to_str().unwrap().contains("vendor")),
            "glob_set.is_match(abs_path) should NOT filter vendor files"
        );

        // Correct: strip root prefix before matching
        files.retain(|f| {
            let rel = f.strip_prefix(root).unwrap_or(f);
            !glob_set.is_match(rel)
        });

        assert!(
            !files.iter().any(|f| f.to_str().unwrap().contains("vendor")),
            "vendor files should have been filtered out: {:?}",
            files
        );
        assert!(
            files
                .iter()
                .any(|f| f.to_str().unwrap().contains("main.rs")),
            "src/main.rs should remain: {:?}",
            files
        );
    }
}
