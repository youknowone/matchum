use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use unidiff::PatchSet;

/// Parsed unified diff: maps each touched file to the set of added line
/// numbers (1-based, matching `ValidationIssue.line`).
#[derive(Debug)]
pub struct DiffFilter {
    added_lines: HashMap<PathBuf, HashSet<usize>>,
}

impl DiffFilter {
    /// Parse unified diff text (e.g. output of `git diff`) into a filter.
    pub fn parse(diff_text: &str) -> Self {
        let mut added_lines: HashMap<PathBuf, HashSet<usize>> = HashMap::new();

        let mut patch = PatchSet::new();
        if patch.parse(diff_text).is_err() {
            return Self { added_lines };
        }

        for patched_file in patch.files() {
            if patched_file.is_removed_file() {
                continue;
            }
            let path = PathBuf::from(patched_file.path());
            let lines = added_lines.entry(path).or_default();
            for hunk in patched_file.hunks() {
                for line in hunk.lines() {
                    if line.is_added()
                        && let Some(n) = line.target_line_no {
                            lines.insert(n);
                        }
                }
            }
        }

        Self { added_lines }
    }

    /// Iterator over file paths present in the diff.
    pub fn files(&self) -> impl Iterator<Item = &PathBuf> {
        self.added_lines.keys()
    }

    /// Returns true if the issue at (file, line) should be reported.
    pub fn should_report(&self, file: &Path, line: usize) -> bool {
        let rel = strip_to_relative(file);
        self.added_lines
            .get(rel)
            .is_some_and(|lines| lines.contains(&line))
    }

    /// Returns true if the file appears in the diff at all.
    pub fn contains_file(&self, file: &Path) -> bool {
        let rel = strip_to_relative(file);
        self.added_lines.contains_key(rel)
    }
}

/// Strip cwd prefix (or `./`) to get a relative path comparable to diff paths.
fn strip_to_relative(path: &Path) -> &Path {
    // Handle `./foo/bar` → `foo/bar`
    if let Ok(stripped) = path.strip_prefix("./") {
        return stripped;
    }
    if let Ok(stripped) = path.strip_prefix(".")
        && !stripped.as_os_str().is_empty() {
            return stripped;
        }
    if path.is_relative() {
        return path;
    }
    // Try stripping cwd; fall back to the full path
    if let Ok(cwd) = std::env::current_dir()
        && let Ok(rel) = path.strip_prefix(&cwd) {
            return rel;
        }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_add() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 line one
+added line
 line two
 line three
";
        let df = DiffFilter::parse(diff);
        assert!(df.contains_file(Path::new("src/main.rs")));
        assert!(df.should_report(Path::new("src/main.rs"), 2)); // added line
        assert!(!df.should_report(Path::new("src/main.rs"), 1)); // context
        assert!(!df.should_report(Path::new("src/main.rs"), 3)); // context
    }

    #[test]
    fn new_file() {
        let diff = "\
diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,3 @@
+first line
+second line
+third line
";
        let df = DiffFilter::parse(diff);
        assert!(df.contains_file(Path::new("new.txt")));
        assert!(df.should_report(Path::new("new.txt"), 1));
        assert!(df.should_report(Path::new("new.txt"), 2));
        assert!(df.should_report(Path::new("new.txt"), 3));
    }

    #[test]
    fn deleted_file() {
        let diff = "\
diff --git a/old.txt b/old.txt
deleted file mode 100644
--- a/old.txt
+++ /dev/null
@@ -1,3 +0,0 @@
-first line
-second line
-third line
";
        let df = DiffFilter::parse(diff);
        assert!(!df.contains_file(Path::new("old.txt")));
    }

    #[test]
    fn multiple_hunks() {
        let diff = "\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -5,3 +5,4 @@
 context
+added at line 6
 context
@@ -20,3 +21,4 @@
 context
+added at line 22
 context
";
        let df = DiffFilter::parse(diff);
        assert!(df.should_report(Path::new("src/lib.rs"), 6));
        assert!(df.should_report(Path::new("src/lib.rs"), 22));
        assert!(!df.should_report(Path::new("src/lib.rs"), 5));
        assert!(!df.should_report(Path::new("src/lib.rs"), 21));
    }

    #[test]
    fn mixed_add_remove() {
        let diff = "\
diff --git a/f.rs b/f.rs
--- a/f.rs
+++ b/f.rs
@@ -1,4 +1,4 @@
 line one
-old line two
+new line two
 line three
 line four
";
        let df = DiffFilter::parse(diff);
        // new line two is at line 2 in the new file
        assert!(df.should_report(Path::new("f.rs"), 2));
        assert!(!df.should_report(Path::new("f.rs"), 1));
        assert!(!df.should_report(Path::new("f.rs"), 3));
    }

    #[test]
    fn no_newline_marker() {
        let diff = "\
diff --git a/a.txt b/a.txt
--- a/a.txt
+++ b/a.txt
@@ -1,2 +1,2 @@
 line one
-old last
+new last
\\ No newline at end of file
";
        let df = DiffFilter::parse(diff);
        assert!(df.should_report(Path::new("a.txt"), 2));
        assert!(!df.should_report(Path::new("a.txt"), 1));
    }

    #[test]
    fn multiple_files() {
        let diff = "\
diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,2 +1,3 @@
 line
+added in a
 line
diff --git a/b.rs b/b.rs
--- a/b.rs
+++ b/b.rs
@@ -1,2 +1,3 @@
 line
+added in b
 line
";
        let df = DiffFilter::parse(diff);
        assert!(df.contains_file(Path::new("a.rs")));
        assert!(df.contains_file(Path::new("b.rs")));
        assert!(!df.contains_file(Path::new("c.rs")));
        assert!(df.should_report(Path::new("a.rs"), 2));
        assert!(df.should_report(Path::new("b.rs"), 2));
    }

    #[test]
    fn empty_diff() {
        let df = DiffFilter::parse("");
        assert_eq!(df.files().count(), 0);
        assert!(!df.should_report(Path::new("any.rs"), 1));
    }

    #[test]
    fn binary_file() {
        let diff = "\
diff --git a/image.png b/image.png
Binary files a/image.png and b/image.png differ
";
        let df = DiffFilter::parse(diff);
        assert!(!df.contains_file(Path::new("image.png")));
    }
}
