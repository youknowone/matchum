// spell-checker:ignore Anishinaabemowin
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use crate::hashdict::HashDictionary;

use super::LoadError;

/// A node in the trie being built during TrieXv3 parsing.
#[derive(Clone, Default)]
struct TrieNode {
    children: HashMap<char, usize>,
    is_word: bool,
    locked: bool,
}

struct StackEntry {
    parent_pool_idx: usize,
    child_char: char,
}

/// Builds a trie from TrieXv3 format data, mirroring cspell's BuilderCursor.
struct TrieBuilder {
    /// All trie nodes. pool[0] = root, pool[1] = shared EOW sentinel.
    pool: Vec<TrieNode>,
    /// Maps reference IDs to pool indices. Mirrors cspell's `nodes` array.
    ref_ids: Vec<usize>,
    /// Stack tracking parent info at each depth level.
    stack: Vec<StackEntry>,
    /// Current node (pool index).
    curr: usize,
    /// Current depth in the trie.
    depth: usize,
}

impl TrieBuilder {
    fn new() -> Self {
        let root = TrieNode::default();
        let eow = TrieNode {
            is_word: true,
            locked: true,
            ..Default::default()
        };

        Self {
            pool: vec![root, eow],
            ref_ids: vec![0, 1], // [root, EOW]
            stack: vec![StackEntry {
                parent_pool_idx: 0,
                child_char: '\0',
            }],
            curr: 0,
            depth: 0,
        }
    }

    fn insert_char(&mut self, c: char) {
        // If current node is locked (from a reference), clone it first
        if self.pool[self.curr].locked {
            let mut cloned = self.pool[self.curr].clone();
            cloned.locked = false;
            let new_idx = self.pool.len();
            self.pool.push(cloned);

            // Update parent's child pointer to the clone
            let entry = &self.stack[self.depth];
            self.pool[entry.parent_pool_idx]
                .children
                .insert(entry.child_char, new_idx);
            self.curr = new_idx;

            self.ref_ids.push(new_idx);
        }

        let parent = self.curr;

        // Get existing child or create new one
        let child = if let Some(&existing) = self.pool[parent].children.get(&c) {
            existing
        } else {
            let idx = self.pool.len();
            self.pool.push(TrieNode::default());
            self.pool[parent].children.insert(c, idx);
            idx
        };

        // Track for reference numbering (mirrors cspell's nodes.push(next))
        self.ref_ids.push(child);

        self.depth += 1;
        if self.depth < self.stack.len() {
            self.stack[self.depth] = StackEntry {
                parent_pool_idx: parent,
                child_char: c,
            };
        } else {
            self.stack.push(StackEntry {
                parent_pool_idx: parent,
                child_char: c,
            });
        }
        self.curr = child;
    }

    fn mark_eow(&mut self) {
        if self.pool[self.curr].children.is_empty() {
            // Leaf node: replace with shared EOW sentinel (pool[1])
            let entry = &self.stack[self.depth];
            self.pool[entry.parent_pool_idx]
                .children
                .insert(entry.child_char, 1);

            if self.ref_ids.last() == Some(&self.curr) {
                self.ref_ids.pop();
            }
            self.curr = 1; // EOW sentinel
        } else {
            self.pool[self.curr].is_word = true;
        }
    }

    fn reference(&mut self, ref_id: usize) {
        let referenced = self.ref_ids[ref_id];
        let entry = &self.stack[self.depth];
        self.pool[entry.parent_pool_idx]
            .children
            .insert(entry.child_char, referenced);
        self.ref_ids.pop();
    }

    fn back_step(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        debug_assert!(
            n <= self.depth,
            "back_step {n} exceeds depth {}",
            self.depth
        );
        self.depth -= n;
        self.curr = self.stack[self.depth + 1].parent_pool_idx;
    }

    /// Walk the trie and add words directly to the dictionary,
    /// avoiding the intermediate Vec<String> allocation.
    fn populate_dict(&self, dict: &mut HashDictionary) {
        let mut path = String::new();
        self.walk_into_dict(0, &mut path, dict);
    }

    fn walk_into_dict(&self, node_idx: usize, path: &mut String, dict: &mut HashDictionary) {
        let node = &self.pool[node_idx];
        if node.is_word {
            add_trie_word(dict, path);
        }

        for (&c, &child_idx) in &node.children {
            path.push(c);
            self.walk_into_dict(child_idx, path, dict);
            path.pop();
        }
    }

    /// Extract all words from the built trie via depth-first walk.
    #[cfg(test)]
    fn extract_words(&self) -> Vec<String> {
        let mut words = Vec::new();
        let mut path = String::new();
        self.walk(0, &mut path, &mut words);
        words
    }

    #[cfg(test)]
    fn walk(&self, node_idx: usize, path: &mut String, words: &mut Vec<String>) {
        let node = &self.pool[node_idx];
        if node.is_word {
            words.push(path.clone());
        }

        let mut children: Vec<_> = node.children.iter().collect();
        children.sort_by_key(|(c, _)| *c);

        for (&c, &child_idx) in children {
            path.push(c);
            self.walk(child_idx, path, words);
            path.pop();
        }
    }
}

enum ParseState {
    Main,
    Back,
    Reference(String),
    EscapeStart,
    EscapeWithBackslash,
}

/// Load a TrieXv3 format dictionary (.trie or .trie.gz).
pub fn load_trie_v3(path: &Path) -> Result<HashDictionary, LoadError> {
    let file = std::fs::File::open(path)?;
    let reader: Box<dyn Read> = if path.extension().is_some_and(|ext| ext == "gz") {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };

    let content = {
        let mut s = String::new();
        let mut buf = std::io::BufReader::new(reader);
        buf.read_to_string(&mut s)?;
        s
    };

    let lines: Vec<&str> = content.lines().collect();

    // Find __DATA__ marker
    let data_start = lines
        .iter()
        .position(|l| l.contains("__DATA__"))
        .ok_or_else(|| LoadError::Format("Missing __DATA__ marker".into()))?;

    let radix = parse_header(&lines[..data_start])?;

    let mut builder = TrieBuilder::new();
    let mut state = ParseState::Main;

    for line in &lines[data_start + 1..] {
        for c in line.chars() {
            state = process_char(&mut builder, state, c, radix);
        }
    }

    let mut dict = HashDictionary::new(false);
    builder.populate_dict(&mut dict);

    Ok(dict)
}

/// Route a trie word to the correct dictionary method based on prefix.
fn add_trie_word(dict: &mut HashDictionary, word: &str) {
    if word.is_empty() {
        return;
    }
    match word.as_bytes()[0] {
        b'~' => {
            // Case-insensitive variant: add as no-suggest word
            let base = &word[1..];
            if !base.is_empty() {
                dict.add_no_suggest(base);
            }
        }
        b'!' => {
            // Forbidden word
            let base = &word[1..];
            if !base.is_empty() {
                dict.add_forbidden(base);
            }
        }
        b'+' => {
            // Compound-only word
            let base = &word[1..];
            if !base.is_empty() {
                dict.add_compound_part(base);
            }
        }
        _ => {
            dict.add_word(word);
        }
    }
}

fn parse_header(header_lines: &[&str]) -> Result<u32, LoadError> {
    let mut found_trie_v3 = false;
    let mut radix: Option<u32> = None;

    for line in header_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("#!") {
            continue;
        }
        if trimmed == "TrieXv3" {
            found_trie_v3 = true;
            continue;
        }
        if let Some(base_str) = trimmed.strip_prefix("base=") {
            radix = Some(
                base_str
                    .parse()
                    .map_err(|_| LoadError::Format(format!("Invalid radix: {base_str}")))?,
            );
        }
    }

    if !found_trie_v3 {
        return Err(LoadError::Format("Missing TrieXv3 identifier".into()));
    }

    let radix = radix.ok_or_else(|| LoadError::Format("Missing base= in header".into()))?;
    if !(2..=36).contains(&radix) {
        return Err(LoadError::Format(format!("Radix out of range: {radix}")));
    }

    Ok(radix)
}

fn process_char(builder: &mut TrieBuilder, state: ParseState, c: char, radix: u32) -> ParseState {
    match state {
        ParseState::Main => parse_main(builder, c),
        ParseState::Back => parse_back(builder, c),
        ParseState::Reference(ref_str) => parse_reference(builder, c, ref_str, radix),
        ParseState::EscapeStart => parse_escape_start(builder, c),
        ParseState::EscapeWithBackslash => parse_escape_mapped(builder, c),
    }
}

fn parse_main(builder: &mut TrieBuilder, c: char) -> ParseState {
    match c {
        '$' => {
            builder.mark_eow();
            builder.back_step(1);
            ParseState::Back
        }
        '<' => {
            builder.back_step(1);
            ParseState::Back
        }
        '#' => ParseState::Reference(String::new()),
        '\\' => ParseState::EscapeStart,
        '\n' | '\r' => ParseState::Main,
        _ => {
            builder.insert_char(c);
            ParseState::Main
        }
    }
}

fn parse_back(builder: &mut TrieBuilder, c: char) -> ParseState {
    match c {
        '<' => {
            builder.back_step(1);
            ParseState::Back
        }
        '2'..='9' => {
            let n = (c as u32 - b'0' as u32 - 1) as usize;
            builder.back_step(n);
            ParseState::Back
        }
        _ => parse_main(builder, c),
    }
}

fn parse_reference(
    builder: &mut TrieBuilder,
    c: char,
    mut ref_str: String,
    radix: u32,
) -> ParseState {
    if c == ';' {
        let r = usize::from_str_radix(&ref_str, radix).unwrap_or(0);
        builder.reference(r + 1);
        ParseState::Main
    } else {
        ref_str.push(c);
        ParseState::Reference(ref_str)
    }
}

fn parse_escape_start(builder: &mut TrieBuilder, c: char) -> ParseState {
    if c == '\\' {
        // Two backslashes: wait for the mapped character
        ParseState::EscapeWithBackslash
    } else {
        // \X where X != \ -> literal X
        builder.insert_char(c);
        ParseState::Main
    }
}

fn parse_escape_mapped(builder: &mut TrieBuilder, c: char) -> ParseState {
    let mapped = match c {
        'n' => '\n',
        'r' => '\r',
        '\\' => '\\',
        _ => c,
    };
    builder.insert_char(mapped);
    ParseState::Main
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictionary::Dictionary;

    fn parse_trie_str(data: &str, radix: u32) -> Vec<String> {
        let mut builder = TrieBuilder::new();
        let mut state = ParseState::Main;

        for c in data.chars() {
            state = process_char(&mut builder, state, c, radix);
        }

        let mut words = builder.extract_words();
        words.sort();
        words
    }

    #[test]
    fn test_single_word() {
        let words = parse_trie_str("hello$5", 10);
        assert_eq!(words, vec!["hello"]);
    }

    #[test]
    fn test_shared_prefix() {
        // "cat", "car" share prefix "ca"
        let words = parse_trie_str("car$t$3", 10);
        assert_eq!(words, vec!["car", "cat"]);
    }

    #[test]
    fn test_three_words_shared_prefix() {
        // "the", "them", "they" - 'e' is both a word-end and has children
        // Export: children visited first, then parent marked as word
        // them$y$$3
        // them$ -> "them", back to e. y$ -> "they", back to e. $3 -> mark e as word ("the"), back to root.
        let words = parse_trie_str("them$y$$3", 10);
        assert_eq!(words, vec!["the", "them", "they"]);
    }

    #[test]
    fn test_eow_with_multi_back_step() {
        // "$3" = markEOW + backStep(1), then backStep(2) = total 3
        let words = parse_trie_str("abc$3def$3", 10);
        assert_eq!(words, vec!["abc", "def"]);
    }

    #[test]
    fn test_back_operator() {
        // "ab", "ac" - $ already includes backStep(1)
        // ab$ -> "ab", back to 'a'. c$ -> "ac", back to 'a'. 2 -> back to root.
        let words = parse_trie_str("ab$c$2", 10);
        assert_eq!(words, vec!["ab", "ac"]);
    }

    #[test]
    fn test_reference_shared_suffix() {
        // "cat", "bat" sharing suffix "at" via reference
        //
        // Trace:
        // c: pool[2], ref_ids=[0,1,2]
        // a: pool[3], ref_ids=[0,1,2,3]
        // t: pool[4], ref_ids=[0,1,2,3,4]
        // $: markEOW (leaf, pop 4), backStep(1). ref_ids=[0,1,2,3]
        // 3: backStep(2) -> root. ref_ids=[0,1,2,3]
        // b: pool[5], ref_ids=[0,1,2,3,5]
        // a: pool[6], ref_ids=[0,1,2,3,5,6]
        // #2;: reference(3) -> pool[5].child['a'] = pool[3] (the 'a' node with child 't')
        //      pop 6. ref_ids=[0,1,2,3,5]
        // <: backStep(1)
        // <: backStep(1) -> root
        let words = parse_trie_str("cat$3ba#2;<<", 10);
        assert_eq!(words, vec!["bat", "cat"]);
    }

    #[test]
    fn test_escape_special_chars() {
        // Word "cost$" (literal dollar sign)
        let words = parse_trie_str("cost\\$$5", 10);
        assert_eq!(words, vec!["cost$"]);
    }

    #[test]
    fn test_escape_backslash() {
        // Word "a\" (literal backslash)
        // Data: a\\\$2
        // a: insert 'a' (depth 1)
        // \: EscapeStart
        // \: EscapeWithBackslash
        // \: map('\\') -> '\\', insert '\\' (depth 2)
        // $: markEOW + backStep(1) (depth 1)
        // 2: backStep(1) -> root
        let words = parse_trie_str("a\\\\\\$2", 10);
        assert_eq!(words, vec!["a\\"]);
    }

    #[test]
    fn test_escape_hash() {
        // Word "ref #" (literal hash)
        let words = parse_trie_str("ref \\#$5", 10);
        assert_eq!(words, vec!["ref #"]);
    }

    #[test]
    fn test_escape_less_than() {
        // Word "arrow <" (literal less-than)
        let words = parse_trie_str("arrow \\<$7", 10);
        assert_eq!(words, vec!["arrow <"]);
    }

    #[test]
    fn test_escape_newline() {
        // Word "eol \n" (literal newline embedded in word)
        // \\n in data = backslash + n -> mapped to newline
        let words = parse_trie_str("eol \\\\n$5", 10);
        assert_eq!(words, vec!["eol \n"]);
    }

    #[test]
    fn test_base_16() {
        // Reference using base 16: #a; = parseInt("a", 16) = 10, ref_id = 11
        // Simple test: just ensure radix parsing works
        let words = parse_trie_str("hello$5", 16);
        assert_eq!(words, vec!["hello"]);
    }

    #[test]
    fn test_multiline() {
        // Lines are just formatting; newlines are ignored
        let words = parse_trie_str("hel\nlo$5", 10);
        assert_eq!(words, vec!["hello"]);
    }

    #[test]
    fn test_complex_shared_subtree() {
        // "walk", "walked", "walker", "walking", "walks"
        // Trie: w->a->l->k(EOW)->e->d(EOW), e->r(EOW), k->i->n->g(EOW), k->s(EOW)
        // Export: walked$r$<ing$3s$$4
        // walked$ -> "walked", back to e
        // r$ -> "walker", back to e
        // < -> back to k
        // ing$ -> "walking", back to n
        // 3 -> back(2) to k
        // s$ -> "walks", back to k
        // $ -> mark k as EOW ("walk"), back to l
        // 4 -> back(3) to root
        let words = parse_trie_str("walked$r$<ing$3s$$4", 10);
        assert_eq!(words, vec!["walk", "walked", "walker", "walking", "walks"]);
    }

    #[test]
    fn test_load_sample_trie() {
        let sample_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("vendor/cspell/packages/cspell-trie-lib/Samples/sampleV3.trie");

        if !sample_path.exists() {
            eprintln!(
                "Skipping: sample trie not found at {}",
                sample_path.display()
            );
            return;
        }

        let dict = load_trie_v3(&sample_path).expect("Failed to load sample trie");

        // Verify some expected words from sampleData.ts
        assert!(dict.has("journal"), "should have 'journal'");
        assert!(dict.has("journalism"), "should have 'journalism'");
        assert!(dict.has("journalist"), "should have 'journalist'");
        assert!(dict.has("journey"), "should have 'journey'");
        assert!(dict.has("joy"), "should have 'joy'");
        assert!(dict.has("joyful"), "should have 'joyful'");
        assert!(dict.has("joyfulness"), "should have 'joyfulness'");
        assert!(dict.has("Big Apple"), "should have 'Big Apple'");
        assert!(dict.has("New York"), "should have 'New York'");
        assert!(dict.has("apple"), "should have 'apple'");
        assert!(dict.has("walk"), "should have 'walk'");
        assert!(dict.has("walked"), "should have 'walked'");
        assert!(dict.has("walking"), "should have 'walking'");
        assert!(dict.has("talk"), "should have 'talk'");
        assert!(dict.has("chalk"), "should have 'chalk'");

        // Special characters
        assert!(dict.has("arrow <"), "should have 'arrow <'");
        assert!(dict.has("eol \n"), "should have 'eol \\n'");
        assert!(dict.has("eow $"), "should have 'eow $'");
        assert!(dict.has("ref #"), "should have 'ref #'");
        assert!(dict.has("escape \\"), "should have 'escape \\\\'");
        assert!(
            dict.has("Numbers 0123456789"),
            "should have 'Numbers 0123456789'"
        );
        assert!(dict.has("Braces: {}[]()"), "should have 'Braces: {{}}[]()'");

        // Non-latin scripts
        assert!(dict.has("ᐊᓂᔑᓈᐯᒧᐎᓐ"), "should have Anishinaabemowin");
        assert!(dict.has("ᓀᐦᐃᔭᐍᐏᐣ"), "should have Nehiyawewin");
    }

    #[test]
    fn test_load_en_us_trie() {
        // Try multiple known locations
        let candidates = [
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("dictionaries/node_modules/@cspell/dict-en_us/en_US.trie.gz"),
            {
                let home = std::env::var("HOME").unwrap_or_default();
                std::path::PathBuf::from(home)
                    .join(".matchum_cache/packages/node_modules/@cspell/dict-en_us/en_US.trie.gz")
            },
        ];
        let dict_path = match candidates.iter().find(|p| p.exists()) {
            Some(p) => p.clone(),
            None => {
                eprintln!("Skipping: en_US dictionary not found");
                return;
            }
        };

        let dict = load_trie_v3(&dict_path).expect("Failed to load en_US trie");

        // Should have a substantial number of words
        assert!(
            dict.len() > 50_000,
            "en_US dict should have >50K words, got {}",
            dict.len()
        );

        // Verify common English words
        assert!(dict.has("hello"), "should have 'hello'");
        assert!(dict.has("world"), "should have 'world'");
        assert!(dict.has("the"), "should have 'the'");
        assert!(dict.has("and"), "should have 'and'");
        assert!(dict.has("computer"), "should have 'computer'");
        assert!(dict.has("programming"), "should have 'programming'");
        assert!(dict.has("dictionary"), "should have 'dictionary'");
        assert!(dict.has("active"), "should have 'active'");

        // Should not have random gibberish
        assert!(!dict.has("xyzzyplugh"), "should not have gibberish");
    }
}
