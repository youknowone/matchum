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
    parent_idx: usize,
    child_char: char,
}

/// Builds a trie from TrieXv3 format data, mirroring cspell's TrieBlobBuilder
/// cursor semantics.
struct TrieBuilder {
    /// Unique trie nodes. `nodes[0]` = root, `nodes[1]` = shared EOW sentinel.
    nodes: Vec<TrieNode>,
    /// History of node pushes as tracked by cspell's TrieNodeBuilder cursor.
    /// Reference ids in TrieXv3 point into this history, not into unique nodes.
    history: Vec<usize>,
    /// Stack tracking parent info at each character depth level.
    stack: Vec<StackEntry>,
    /// Current unique node index.
    curr: usize,
    /// Current character depth in the trie.
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
            nodes: vec![root, eow],
            history: vec![0, 1],
            stack: vec![StackEntry {
                parent_idx: 0,
                child_char: '\0',
            }],
            curr: 0,
            depth: 0,
        }
    }

    fn insert_char(&mut self, c: char) {
        if self.nodes[self.curr].locked {
            let mut cloned = self.nodes[self.curr].clone();
            cloned.locked = false;
            let new_idx = self.nodes.len();
            self.nodes.push(cloned);

            let entry = &self.stack[self.depth];
            self.nodes[entry.parent_idx]
                .children
                .insert(entry.child_char, new_idx);
            self.history.push(new_idx);
            self.curr = new_idx;
        }

        let parent = self.curr;
        let child = if let Some(existing) = self.nodes[parent].children.get(&c).copied() {
            existing
        } else {
            let idx = self.nodes.len();
            self.nodes.push(TrieNode::default());
            self.nodes[parent].children.insert(c, idx);
            idx
        };
        self.history.push(child);

        self.depth += 1;
        if self.depth < self.stack.len() {
            self.stack[self.depth] = StackEntry {
                parent_idx: parent,
                child_char: c,
            };
        } else {
            self.stack.push(StackEntry {
                parent_idx: parent,
                child_char: c,
            });
        }
        self.curr = child;
    }

    fn mark_eow(&mut self) {
        if self.curr == 1 {
            return;
        }

        if self.nodes[self.curr].children.is_empty() {
            let entry = &self.stack[self.depth];
            self.nodes[entry.parent_idx]
                .children
                .insert(entry.child_char, 1);
            if self.history.last().copied() == Some(self.curr) {
                self.history.pop();
            }
            self.curr = 1;
        } else {
            self.nodes[self.curr].is_word = true;
        }
    }

    fn reference(&mut self, ref_id: usize) {
        let referenced = self.history[ref_id];
        let entry = &self.stack[self.depth];
        self.curr = entry.parent_idx;
        self.nodes[self.curr]
            .children
            .insert(entry.child_char, referenced);
        self.history.pop();
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
        self.curr = self.stack[self.depth + 1].parent_idx;
    }

    /// Walk the trie and add words directly to the dictionary,
    /// avoiding the intermediate Vec<String> allocation.
    fn populate_dict(&self, dict: &mut HashDictionary) {
        let mut path = Vec::new();
        self.walk_into_dict(0, &mut path, dict);
    }

    fn walk_into_dict(&self, node_idx: usize, path: &mut Vec<u8>, dict: &mut HashDictionary) {
        let Some(node) = self.nodes.get(node_idx) else {
            return;
        };
        if node.is_word {
            let word = std::str::from_utf8(path).expect("TrieXv3 path must remain valid UTF-8");
            add_trie_word(dict, word);
        }

        for (&ch, &child_idx) in &node.children {
            let mut buf = [0; 4];
            let bytes = ch.encode_utf8(&mut buf).as_bytes();
            path.extend_from_slice(bytes);
            self.walk_into_dict(child_idx, path, dict);
            for _ in 0..bytes.len() {
                path.pop();
            }
        }
    }

    /// Extract all words from the built trie via depth-first walk.
    #[cfg(test)]
    fn extract_words(&self) -> Vec<String> {
        let mut words = Vec::new();
        let mut path = Vec::new();
        self.walk(0, &mut path, &mut words);
        words
    }

    #[cfg(test)]
    fn walk(&self, node_idx: usize, path: &mut Vec<u8>, words: &mut Vec<String>) {
        let Some(node) = self.nodes.get(node_idx) else {
            return;
        };
        if node.is_word {
            words.push(
                std::str::from_utf8(path)
                    .expect("TrieXv3 path must remain valid UTF-8")
                    .to_owned(),
            );
        }

        let mut children: Vec<_> = node.children.iter().collect();
        children.sort_by_key(|(ch, _)| **ch);

        for (&ch, &child_idx) in children {
            let mut buf = [0; 4];
            let bytes = ch.encode_utf8(&mut buf).as_bytes();
            path.extend_from_slice(bytes);
            self.walk(child_idx, path, words);
            for _ in 0..bytes.len() {
                path.pop();
            }
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

/// Load a TrieXv3 format dictionary (.trie or .trie.gz) from a file path.
pub fn load_trie_v3(path: &Path) -> Result<HashDictionary, LoadError> {
    let file = std::fs::File::open(path)?;
    let reader: Box<dyn Read> = if path.extension().is_some_and(|ext| ext == "gz") {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    load_trie_v3_from_reader(reader)
}

/// Load a TrieXv3 format dictionary from a reader.
pub fn load_trie_v3_from_reader(reader: impl Read) -> Result<HashDictionary, LoadError> {
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
    let mut rest = word;
    let mut non_strict = false;
    if let Some(base) = rest.strip_prefix('~') {
        non_strict = true;
        rest = base;
    }

    if let Some(rest) = rest.strip_prefix(':') {
        // Suggestion metadata is encoded as `:word` / `:word:0:suggestion`.
        // The underlying word itself is emitted separately, so it is safe to
        // skip these entries here.
        let _ = rest;
        return;
    }

    let mut forbidden = false;
    if let Some(base) = rest.strip_prefix('!') {
        forbidden = true;
        rest = base;
    }

    if rest.starts_with('+') || rest.ends_with('+') {
        let plus_start = rest.starts_with('+');
        let plus_end = rest.ends_with('+');
        let base = rest.trim_matches('+');
        if base.is_empty() {
            return;
        }

        if forbidden {
            dict.add_forbidden(base);
        } else if !non_strict {
            if rest.chars().any(|c| c.is_alphabetic() && c.is_uppercase()) {
                dict.add_exact_word(rest);
            } else {
                dict.add_word(rest);
            }
        }

        dict.add_compound_part_explicit(
            base,
            plus_end && !plus_start,
            plus_start && !plus_end,
            plus_start && plus_end,
        );
        return;
    }

    if rest.is_empty() {
        return;
    }

    if forbidden {
        dict.add_forbidden(rest);
        return;
    }

    // `~word` entries belong to cspell's non-strict trie branch. They should
    // only participate in non-strict searches, not become standalone words.
    if non_strict {
        dict.add_non_strict_word(rest);
        return;
    }

    if rest.chars().any(|c| c.is_alphabetic() && c.is_uppercase()) {
        dict.add_exact_word(rest);
    } else {
        dict.add_word(rest);
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

// cspell:disable
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

    fn en_us_dict_path() -> Option<std::path::PathBuf> {
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
        candidates.iter().find(|p| p.exists()).cloned()
    }

    fn java_dict_path() -> Option<std::path::PathBuf> {
        let candidates = [{
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(home)
                .join(".matchum_cache/packages/node_modules/@cspell/dict-java/dict/java.trie")
        }];
        candidates.iter().find(|p| p.exists()).cloned()
    }

    fn parse_en_us_words() -> Option<Vec<String>> {
        let dict_path = en_us_dict_path()?;
        parse_trie_words(&dict_path)
    }

    fn parse_trie_words(dict_path: &std::path::Path) -> Option<Vec<String>> {
        let file = std::fs::File::open(&dict_path).ok()?;
        let reader: Box<dyn Read> = if dict_path.extension().is_some_and(|ext| ext == "gz") {
            Box::new(flate2::read::GzDecoder::new(file))
        } else {
            Box::new(file)
        };

        let mut content = String::new();
        let mut buf = std::io::BufReader::new(reader);
        buf.read_to_string(&mut content).ok()?;

        let lines: Vec<&str> = content.lines().collect();
        let data_start = lines.iter().position(|l| l.contains("__DATA__"))?;
        let radix = parse_header(&lines[..data_start]).ok()?;

        let mut builder = TrieBuilder::new();
        let mut state = ParseState::Main;
        for line in &lines[data_start + 1..] {
            for c in line.chars() {
                state = process_char(&mut builder, state, c, radix);
            }
        }

        Some(builder.extract_words())
    }

    #[test]
    fn test_en_us_raw_words_do_not_include_colum() {
        let words = match parse_en_us_words() {
            Some(words) => words,
            None => {
                eprintln!("Skipping: en_US dictionary not found");
                return;
            }
        };
        assert!(
            !words.iter().any(|w| w == "colum"),
            "raw trie should not contain 'colum'"
        );
    }

    #[test]
    fn test_en_us_add_trie_word_does_not_create_colum() {
        let words = match parse_en_us_words() {
            Some(words) => words,
            None => {
                eprintln!("Skipping: en_US dictionary not found");
                return;
            }
        };

        let mut dict = HashDictionary::new(false);
        for raw_word in &words {
            add_trie_word(&mut dict, raw_word);
            assert!(
                !dict.has("colum"),
                "add_trie_word unexpectedly made 'colum' valid after processing {:?}",
                raw_word
            );
        }
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
    fn test_add_trie_word_compound_markers_stay_literal() {
        let mut dict = HashDictionary::new(false);

        add_trie_word(&mut dict, "error+");
        add_trie_word(&mut dict, "+code");
        add_trie_word(&mut dict, "+msg+");

        assert!(dict.has("error+"));
        assert!(dict.has("+code"));
        assert!(dict.has("+msg+"));
        assert!(dict.has("errorcode"));
        assert!(!dict.has("codeerror"));
        assert!(!dict.has("errormsgcode"));
    }

    #[test]
    fn test_add_trie_word_non_strict_prefix_is_not_no_suggest() {
        let mut dict = HashDictionary::new(false);

        add_trie_word(&mut dict, "~cafe");

        assert!(dict.has("cafe"));
        assert!(!dict.find("cafe").no_suggest);
    }

    #[test]
    fn test_add_trie_word_non_strict_prefix_is_not_standalone_in_case_sensitive_dicts() {
        let mut dict = HashDictionary::new(true);

        add_trie_word(&mut dict, "~kenn");

        assert!(!dict.has("kenn"));
        assert!(!dict.find("kenn").found);
    }

    #[test]
    fn test_add_trie_word_non_strict_compound_markers_survive_prefix_parsing() {
        let mut dict = HashDictionary::new(false);

        add_trie_word(&mut dict, "~multi+");
        add_trie_word(&mut dict, "~+api");
        add_trie_word(&mut dict, "~+api+");
        add_trie_word(&mut dict, "~+script");

        assert!(
            !dict.has("multi"),
            "compound-only non-strict prefixes must not become standalone words"
        );
        // Trie compound decomposition only allows 2-part (prefix+suffix).
        assert!(dict.has("multiapi"));
        // 3-part chain (multi+api+script) is NOT valid in trie compounds.
        assert!(!dict.has("multiapiscript"));
    }

    #[test]
    fn test_add_trie_word_non_strict_forbidden_remains_forbidden() {
        let mut dict = HashDictionary::new(false);

        add_trie_word(&mut dict, "~!bluecode");

        let found = dict.find("bluecode");
        assert!(!found.found);
        assert!(found.forbidden);
    }

    #[test]
    fn test_add_trie_word_mixed_case_plain_word_stays_exact_case() {
        let mut dict = HashDictionary::new(false);

        add_trie_word(&mut dict, "Colum");

        assert!(dict.has("Colum"));
        assert!(!dict.has("colum"));
        assert!(
            <HashDictionary as crate::dictionary::Dictionary>::has_pre_normalized_direct_only(
                &dict, "Colum", "colum",
            ),
            "pre-normalized lookup should preserve exact mixed-case entries"
        );
        assert!(
            !<HashDictionary as crate::dictionary::Dictionary>::has_pre_normalized_direct_only(
                &dict, "colum", "colum",
            ),
            "pre-normalized lookup should not invent lowercase variants for exact mixed-case entries"
        );
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
        let dict_path = match en_us_dict_path() {
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
        assert!(!dict.has("colum"), "should not have 'colum'");

        // Should not have random gibberish
        assert!(!dict.has("xyzzyplugh"), "should not have gibberish");
    }

    #[test]
    fn test_load_en_us_trie_rejects_unprefixed_derivations_without_entries() {
        let dict_path = match en_us_dict_path() {
            Some(p) => p.clone(),
            None => {
                eprintln!("Skipping: en_US dictionary not found");
                return;
            }
        };

        let dict = load_trie_v3(&dict_path).expect("Failed to load en_US trie");

        assert!(
            dict.has("enumerable"),
            "sanity check: should have 'enumerable'"
        );
        assert!(
            dict.has("overridden"),
            "sanity check: should have 'overridden'"
        );
        assert!(
            !dict.has("unenumerable"),
            "en_US trie should not synthesize 'unenumerable'"
        );
        assert!(
            !dict.has("unoverridden"),
            "en_US trie should not synthesize 'unoverridden'"
        );
    }

    #[test]
    fn test_load_en_us_trie_matches_cspell_raw_lookup_semantics() {
        let dict_path = match en_us_dict_path() {
            Some(p) => p.clone(),
            None => {
                eprintln!("Skipping: en_US dictionary not found");
                return;
            }
        };

        let dict = load_trie_v3(&dict_path).expect("Failed to load en_US trie");

        assert!(dict.has("multi"), "en_US trie should recognize 'multi'");
        assert!(dict.has("api"), "en_US trie should recognize 'api'");
        assert!(dict.has("script"), "en_US trie should recognize 'script'");
        assert!(
            !dict.has("multiapi"),
            "raw en_US trie should not synthesize 'multiapi' without allowCompoundWords"
        );
        assert!(
            !dict.has("multiapiscript"),
            "raw en_US trie should not synthesize 'multiapiscript' without allowCompoundWords"
        );
        assert!(
            !dict.has("splashscreen"),
            "en_US trie should not synthesize unrelated compound 'splashscreen'"
        );
    }

    #[test]
    fn test_load_java_trie_does_not_accept_splashscreen_without_explicit_entry() {
        let dict_path = match java_dict_path() {
            Some(p) => p.clone(),
            None => {
                eprintln!("Skipping: Java dictionary not found");
                return;
            }
        };

        let mut dict = load_trie_v3(&dict_path).expect("Failed to load java trie");
        dict.set_case_sensitive(true);
        let matches = parse_trie_words(&dict_path)
            .unwrap_or_default()
            .into_iter()
            .filter(|word| {
                let lower = word.to_lowercase();
                lower.contains("splash") || lower.contains("screen")
            })
            .take(20)
            .collect::<Vec<_>>();

        assert!(
            !<HashDictionary as crate::dictionary::Dictionary>::has_pre_normalized_direct_only(
                &dict,
                "splashscreen",
                "splashscreen",
            ),
            "java trie should not accept lowercase splashscreen via direct ignore-case lookup; nearby parsed words={matches:?}"
        );
        assert!(
            !<HashDictionary as crate::dictionary::Dictionary>::find_without_compounds(
                &dict,
                "splashscreen",
            )
            .found,
            "java trie should not accept splashscreen without compounds; nearby parsed words={matches:?}"
        );
    }

    #[test]
    fn test_non_strict_words_excluded_in_case_sensitive_mode() {
        let mut dict = HashDictionary::new(false);
        add_trie_word(&mut dict, "~cafe");

        // case-insensitive (default): non-strict words should be found
        assert!(
            dict.has("cafe"),
            "non-strict word should be found case-insensitively"
        );

        // case-sensitive: non-strict words should NOT be found (matches cspell)
        dict.set_case_sensitive(true);
        let found = <HashDictionary as crate::dictionary::Dictionary>::find_without_compounds(
            &dict, "cafe",
        )
        .found;
        assert!(
            !found,
            "non-strict word should NOT be found in case-sensitive mode"
        );
    }

    #[test]
    fn test_debug_en_us_raw_words_for_multiapi() {
        let words = match parse_en_us_words() {
            Some(words) => words,
            None => {
                eprintln!("Skipping: en_US dictionary not found");
                return;
            }
        };

        let multi: Vec<_> = words
            .iter()
            .filter(|w| w.contains("multi"))
            .take(30)
            .cloned()
            .collect();
        let api: Vec<_> = words
            .iter()
            .filter(|w| w.contains("api"))
            .take(30)
            .cloned()
            .collect();
        let script: Vec<_> = words
            .iter()
            .filter(|w| w.contains("script"))
            .take(30)
            .cloned()
            .collect();
        let splash: Vec<_> = words
            .iter()
            .filter(|w| w.contains("splash"))
            .take(30)
            .cloned()
            .collect();
        let compound_like: Vec<_> = words
            .iter()
            .filter(|w| {
                w.contains('+')
                    && (w.contains("multi") || w.contains("api") || w.contains("script"))
            })
            .take(50)
            .cloned()
            .collect();
        eprintln!("multi raw words: {multi:?}");
        eprintln!("api raw words: {api:?}");
        eprintln!("script raw words: {script:?}");
        eprintln!("splash raw words: {splash:?}");
        eprintln!("compound-like raw words: {compound_like:?}");
    }
}
