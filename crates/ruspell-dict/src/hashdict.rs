use crate::dictionary::{Dictionary, FindResult};
use crate::distance;
use compact_str::CompactString;
use hashbrown::HashSet;
use std::borrow::Cow;
use unicode_normalization::UnicodeNormalization;

/// A dictionary backed by hash sets for O(1) lookup.
/// Uses `CompactString` to store words inline (no heap allocation for ≤24 bytes).
pub struct HashDictionary {
    words: HashSet<CompactString>,
    forbidden: HashSet<CompactString>,
    no_suggest: HashSet<CompactString>,
    /// Compound word parts (entries like `*Word*`, `Word*`, `*Word`, `+Word`, `Word+`).
    /// Stored as lowercased base words.
    compound_parts: HashSet<CompactString>,
    /// Identity words: stored with NFC but without case normalization.
    /// These match only the exact casing provided.
    identity_words: HashSet<CompactString>,
    case_sensitive: bool,
}

impl HashDictionary {
    pub fn new(case_sensitive: bool) -> Self {
        Self {
            words: HashSet::default(),
            forbidden: HashSet::default(),
            no_suggest: HashSet::default(),
            compound_parts: HashSet::default(),
            identity_words: HashSet::default(),
            case_sensitive,
        }
    }

    pub fn add_word(&mut self, word: &str) {
        self.words
            .insert(CompactString::from(self.normalize_owned(word)));
    }

    pub fn add_forbidden(&mut self, word: &str) {
        self.forbidden
            .insert(CompactString::from(self.normalize_owned(word)));
    }

    pub fn add_no_suggest(&mut self, word: &str) {
        let normalized = self.normalize_owned(word);
        self.no_suggest.insert(CompactString::from(&normalized));
        // Still a valid word, just don't suggest it
        self.words.insert(CompactString::from(normalized));
    }

    /// Add a compound word part (stripped of `*`/`+` markers).
    pub fn add_compound_part(&mut self, word: &str) {
        self.compound_parts
            .insert(CompactString::from(self.normalize_owned(word)));
    }

    /// Add an identity word (exact-case match, NFC-only normalization).
    pub fn add_identity_word(&mut self, word: &str) {
        let nfc: String = word.nfc().collect();
        self.words
            .insert(CompactString::from(self.normalize_owned(word)));
        self.identity_words.insert(CompactString::from(nfc));
    }

    pub fn has_compound_parts(&self) -> bool {
        !self.compound_parts.is_empty()
    }

    /// Serialize dictionary to binary cache format.
    pub fn to_cache_bytes(&self, mtime: u64, size: u32) -> Vec<u8> {
        let total_words = self.words.len()
            + self.forbidden.len()
            + self.no_suggest.len()
            + self.compound_parts.len()
            + self.identity_words.len();
        // Estimate ~12 bytes per word average
        let mut buf = Vec::with_capacity(21 + total_words * 12);

        // Header
        buf.extend_from_slice(b"RSPELLD\x01"); // magic + version
        buf.extend_from_slice(&mtime.to_le_bytes());
        buf.extend_from_slice(&size.to_le_bytes());
        buf.push(self.case_sensitive as u8);

        // Write each set
        fn write_set(buf: &mut Vec<u8>, set: &HashSet<CompactString>) {
            buf.extend_from_slice(&(set.len() as u32).to_le_bytes());
            for word in set {
                let bytes = word.as_bytes();
                buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                buf.extend_from_slice(bytes);
            }
        }

        write_set(&mut buf, &self.words);
        write_set(&mut buf, &self.forbidden);
        write_set(&mut buf, &self.no_suggest);
        write_set(&mut buf, &self.compound_parts);
        write_set(&mut buf, &self.identity_words);

        buf
    }

    /// Deserialize dictionary from binary cache format.
    /// Returns None if magic/version mismatch or mtime/size don't match source.
    pub fn from_cache_bytes(bytes: &[u8], expected_mtime: u64, expected_size: u32) -> Option<Self> {
        if bytes.len() < 21 {
            return None;
        }
        // Check magic + version
        if &bytes[0..8] != b"RSPELLD\x01" {
            return None;
        }
        let mtime = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
        let size = u32::from_le_bytes(bytes[16..20].try_into().ok()?);
        if mtime != expected_mtime || size != expected_size {
            return None;
        }
        let case_sensitive = bytes[20] != 0;

        let mut pos = 21;

        fn read_set(bytes: &[u8], pos: &mut usize) -> Option<HashSet<CompactString>> {
            if *pos + 4 > bytes.len() {
                return None;
            }
            let count = u32::from_le_bytes(bytes[*pos..*pos + 4].try_into().ok()?) as usize;
            *pos += 4;
            let mut set = HashSet::with_capacity(count);
            for _ in 0..count {
                if *pos + 2 > bytes.len() {
                    return None;
                }
                let len = u16::from_le_bytes(bytes[*pos..*pos + 2].try_into().ok()?) as usize;
                *pos += 2;
                if *pos + len > bytes.len() {
                    return None;
                }
                let s = std::str::from_utf8(&bytes[*pos..*pos + len]).ok()?;
                set.insert(CompactString::from(s));
                *pos += len;
            }
            Some(set)
        }

        let words = read_set(bytes, &mut pos)?;
        let forbidden = read_set(bytes, &mut pos)?;
        let no_suggest = read_set(bytes, &mut pos)?;
        let compound_parts = read_set(bytes, &mut pos)?;
        let identity_words = read_set(bytes, &mut pos)?;

        Some(Self {
            words,
            forbidden,
            no_suggest,
            compound_parts,
            identity_words,
            case_sensitive,
        })
    }

    /// Check if a word can be decomposed into compound parts.
    fn check_compound(&self, word: &str) -> bool {
        if self.compound_parts.is_empty() {
            return false;
        }
        let normalized = self.normalize(word);
        self.decompose_compound(normalized.as_ref(), 0)
    }

    /// Recursively try to decompose a word into compound parts.
    fn decompose_compound(&self, word: &str, depth: usize) -> bool {
        if depth > 5 || word.len() < 2 {
            return false;
        }

        let chars: Vec<(usize, char)> = word.char_indices().collect();
        // Try all split points (minimum 1 char per part)
        for i in 1..chars.len() {
            let split_byte = chars[i].0;
            let left = &word[..split_byte];
            let right = &word[split_byte..];

            if self.compound_parts.contains(left) {
                if self.compound_parts.contains(right) {
                    return true;
                }
                if self.decompose_compound(right, depth + 1) {
                    return true;
                }
            }
        }
        false
    }

    /// Normalize for insertion (always returns owned String).
    fn normalize_owned(&self, word: &str) -> String {
        self.normalize(word).into_owned()
    }

    /// Normalize a word for lookup. Returns Cow::Borrowed when no allocation
    /// is needed (already-lowercase ASCII or case-sensitive ASCII), avoiding
    /// unnecessary String allocations on the hot path.
    fn normalize<'a>(&self, word: &'a str) -> Cow<'a, str> {
        if word.is_ascii() {
            if self.case_sensitive {
                return Cow::Borrowed(word);
            }
            // Check if already lowercase — avoid allocation
            if word.bytes().all(|b| !b.is_ascii_uppercase()) {
                return Cow::Borrowed(word);
            }
            return Cow::Owned(word.to_ascii_lowercase());
        }
        let nfc: String = word.nfc().collect();
        if self.case_sensitive {
            Cow::Owned(nfc)
        } else {
            Cow::Owned(nfc.to_lowercase())
        }
    }
}

/// Convert a lowercase word to Ucfirst form (e.g. "house" → "House").
fn uc_first(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => {
            let upper: String = c.to_uppercase().collect();
            upper + chars.as_str()
        }
    }
}

impl Dictionary for HashDictionary {
    fn has(&self, word: &str) -> bool {
        let normalized = self.normalize(word);
        if self.words.contains(normalized.as_ref()) || self.check_compound(word) {
            return true;
        }
        // ALL-CAPS fallback: e.g. "HOUSE" → lowercase "house" (already checked),
        // also try Ucfirst "House"
        if !self.case_sensitive && word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) {
            let ucfirst = uc_first(normalized.as_ref());
            if ucfirst != normalized.as_ref() && self.words.contains(ucfirst.as_str()) {
                return true;
            }
        }
        false
    }

    fn is_forbidden(&self, word: &str) -> bool {
        if self.forbidden.is_empty() {
            return false;
        }
        self.forbidden.contains(self.normalize(word).as_ref())
    }

    fn suggest(&self, word: &str, limit: usize) -> Vec<String> {
        let normalized = self.normalize(word);
        // Filter out no-suggest and forbidden words from candidates
        let candidates = self.words.iter().filter_map(|w| {
            let s: &str = w.as_str();
            if self.no_suggest.contains(s) || self.forbidden.contains(s) {
                None
            } else {
                Some(s)
            }
        });
        let max_edits = 2; // default max edit distance for suggestions
        distance::select_nearest_words(normalized.as_ref(), candidates, max_edits, limit)
            .into_iter()
            .map(|(w, _)| w)
            .collect()
    }

    fn find(&self, word: &str) -> FindResult {
        let normalized = self.normalize(word);
        let mut found = self.words.contains(normalized.as_ref()) || self.check_compound(word);
        if !found
            && !self.case_sensitive
            && word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
        {
            let ucfirst = uc_first(normalized.as_ref());
            if ucfirst != normalized.as_ref() && self.words.contains(ucfirst.as_str()) {
                found = true;
            }
        }
        FindResult {
            found,
            forbidden: self.forbidden.contains(normalized.as_ref()),
            no_suggest: self.no_suggest.contains(normalized.as_ref()),
        }
    }

    fn len(&self) -> usize {
        self.words.len()
    }

    fn has_forbidden_words(&self) -> bool {
        !self.forbidden.is_empty()
    }

    fn is_case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    fn has_pre_normalized(&self, word: &str, normalized: &str) -> bool {
        if self.words.contains(normalized) {
            return true;
        }
        // Compound check with pre-normalized word
        if !self.compound_parts.is_empty() && self.decompose_compound(normalized, 0) {
            return true;
        }
        // ALL-CAPS ucfirst fallback: "HOUSE" → try "House"
        if !self.case_sensitive && word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) {
            let ucfirst = uc_first(normalized);
            if ucfirst != normalized && self.words.contains(ucfirst.as_str()) {
                return true;
            }
        }
        false
    }

    fn is_forbidden_pre_normalized(&self, _word: &str, normalized: &str) -> bool {
        if self.forbidden.is_empty() {
            return false;
        }
        self.forbidden.contains(normalized)
    }
}
