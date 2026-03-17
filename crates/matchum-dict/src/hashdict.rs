use crate::dictionary::{Dictionary, FindResult, WordSetsRef};
use crate::distance::{self, EditCosts};
use crate::repmap::RepMap;
use compact_str::CompactString;
use hashbrown::HashMap;
use hashbrown::HashSet;
use std::borrow::Cow;
use unicode_normalization::UnicodeNormalization;

const HASH_DICT_CACHE_VERSION: u8 = 12;

/// Compound word position flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundPos {
    /// `word*` — can only be the first part of a compound
    Prefix,
    /// `*word` — can only be the last part of a compound
    Suffix,
    /// `*word*` — can appear at any position (first, middle, or last)
    Middle,
}

/// A dictionary backed by hash sets for O(1) lookup.
/// Uses `CompactString` to store words inline (no heap allocation for ≤24 bytes).
pub struct HashDictionary {
    words: HashSet<CompactString>,
    non_strict_words: HashSet<CompactString>,
    forbidden: HashSet<CompactString>,
    no_suggest: HashSet<CompactString>,
    suggest_map: HashMap<CompactString, Vec<CompactString>>,
    /// Preferred suggestions: words marked with `!*` prefix in dictionary files.
    /// These are promoted to the top of suggestion results.
    preferred: HashSet<CompactString>,
    /// All compound parts (union) for quick existence checks.
    compound_parts: HashSet<CompactString>,
    /// Compound parts that can appear at the start of a compound (`word*` or `*word*`).
    compound_can_prefix: HashSet<CompactString>,
    /// Compound parts that can appear at the end of a compound (`*word` or `*word*`).
    compound_can_suffix: HashSet<CompactString>,
    /// Compound parts that can appear in the middle of a 3+ part compound (`*word*` only).
    /// In cspell's trie, this corresponds to `+word+` entries — words that are both
    /// compound continuations and compound segment endings. Separate `*word` and `word*`
    /// entries do NOT combine to create middle eligibility.
    compound_can_middle: HashSet<CompactString>,
    /// Identity words: stored with NFC but without case normalization.
    /// These match only the exact casing provided.
    identity_words: HashSet<CompactString>,
    /// Exact-case words loaded from trie dictionaries. Unlike `identity_words`,
    /// these do not implicitly validate their lowercased form.
    exact_words: HashSet<CompactString>,
    /// Lowercased/NFC shadow index for exact-case words.
    /// Used by cspell-compatible ignore-case lookup.
    folded_exact_words: HashSet<CompactString>,
    case_sensitive: bool,
    pub repmap: Option<RepMap>,
}

impl HashDictionary {
    pub fn new(case_sensitive: bool) -> Self {
        Self {
            words: HashSet::default(),
            non_strict_words: HashSet::default(),
            forbidden: HashSet::default(),
            no_suggest: HashSet::default(),
            suggest_map: HashMap::default(),
            preferred: HashSet::default(),
            compound_parts: HashSet::default(),
            compound_can_prefix: HashSet::default(),
            compound_can_suffix: HashSet::default(),
            compound_can_middle: HashSet::default(),
            identity_words: HashSet::default(),
            exact_words: HashSet::default(),
            folded_exact_words: HashSet::default(),
            case_sensitive,
            repmap: None,
        }
    }

    pub fn set_case_sensitive(&mut self, case_sensitive: bool) {
        self.case_sensitive = case_sensitive;
    }

    pub fn set_repmap(&mut self, repmap: RepMap) {
        self.repmap = Some(repmap);
    }

    pub fn add_word(&mut self, word: &str) {
        self.words
            .insert(CompactString::from(self.normalize_owned(word)));
    }

    pub fn add_non_strict_word(&mut self, word: &str) {
        self.non_strict_words
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

    pub fn add_suggestions<I, S>(&mut self, word: &str, suggestions: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let normalized = CompactString::from(self.normalize_owned(word));
        let normalized_suggestions: Vec<CompactString> = suggestions
            .into_iter()
            .map(|suggestion| CompactString::from(self.normalize_owned(suggestion.as_ref())))
            .collect();
        let entry = self.suggest_map.entry(normalized).or_default();
        for suggestion in normalized_suggestions {
            if !entry.contains(&suggestion) {
                entry.push(suggestion);
            }
        }
    }

    /// Add a preferred suggestion word (marked with `!*` in dictionary files).
    /// The word is added as a regular word AND marked preferred for suggestion ranking.
    pub fn add_preferred(&mut self, word: &str) {
        let normalized = self.normalize_owned(word);
        self.preferred.insert(CompactString::from(&normalized));
        self.words.insert(CompactString::from(normalized));
    }

    /// Add a compound word part with position tracking.
    /// `pos` determines where the part can appear in compound decomposition.
    pub fn add_compound_part_with_pos(&mut self, word: &str, pos: CompoundPos) {
        let normalized = CompactString::from(self.normalize_owned(word));
        self.compound_parts.insert(normalized.clone());
        match pos {
            CompoundPos::Prefix => {
                self.compound_can_prefix.insert(normalized);
            }
            CompoundPos::Suffix => {
                self.compound_can_suffix.insert(normalized);
            }
            CompoundPos::Middle => {
                self.compound_can_prefix.insert(normalized.clone());
                self.compound_can_suffix.insert(normalized.clone());
                self.compound_can_middle.insert(normalized);
            }
        }
    }

    /// Add a compound word part with explicit capability flags.
    /// `can_first` = eligible as the first part of a compound (requires `word+` in main trie).
    /// `can_last` = eligible as the last/continuation part (requires `+word` in compound trie).
    /// `can_middle` = eligible as a middle part in 3+ part compounds (requires `+word+`).
    pub fn add_compound_part_explicit(
        &mut self,
        word: &str,
        can_first: bool,
        can_last: bool,
        can_middle: bool,
    ) {
        let normalized = CompactString::from(self.normalize_owned(word));
        self.compound_parts.insert(normalized.clone());
        if can_first {
            self.compound_can_prefix.insert(normalized.clone());
        }
        if can_last {
            self.compound_can_suffix.insert(normalized.clone());
        }
        if can_middle {
            self.compound_can_middle.insert(normalized);
        }
    }

    /// Add a compound word part without position info (legacy — allows any position).
    pub fn add_compound_part(&mut self, word: &str) {
        self.add_compound_part_with_pos(word, CompoundPos::Middle);
    }

    /// Add an identity word (exact-case match, NFC-only normalization).
    pub fn add_identity_word(&mut self, word: &str) {
        let nfc: String = word.nfc().collect();
        self.words
            .insert(CompactString::from(self.normalize_owned(word)));
        self.identity_words.insert(CompactString::from(nfc));
    }

    /// Add an exact-case word. These are valid only when the exact casing
    /// matches, mirroring plain mixed-case entries in cspell trie dictionaries.
    pub fn add_exact_word(&mut self, word: &str) {
        let nfc: String = word.nfc().collect();
        self.exact_words.insert(CompactString::from(&nfc));
        self.folded_exact_words
            .insert(CompactString::from(nfc.to_lowercase()));
    }

    pub fn has_compound_parts(&self) -> bool {
        !self.compound_parts.is_empty()
    }

    /// Serialize dictionary to binary cache format.
    pub fn to_cache_bytes(&self, mtime: u64, size: u32) -> Vec<u8> {
        let total_words = self.words.len()
            + self.non_strict_words.len()
            + self.forbidden.len()
            + self.no_suggest.len()
            + self.suggest_map.len()
            + self.preferred.len()
            + self.compound_parts.len()
            + self.identity_words.len()
            + self.exact_words.len()
            + self.folded_exact_words.len();
        // Estimate ~12 bytes per word average
        let mut buf = Vec::with_capacity(21 + total_words * 12);

        // Header: version 9 invalidates stale caches built before the trie
        // non-strict / compound loader fixes that changed lookup semantics.
        // cspell:ignore RSPELLD
        buf.extend_from_slice(b"RSPELLD");
        buf.push(HASH_DICT_CACHE_VERSION);
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
        write_set(&mut buf, &self.non_strict_words);
        write_set(&mut buf, &self.forbidden);
        write_set(&mut buf, &self.no_suggest);
        write_set(&mut buf, &self.preferred);
        write_set(&mut buf, &self.compound_parts);
        write_set(&mut buf, &self.identity_words);
        write_set(&mut buf, &self.exact_words);
        write_set(&mut buf, &self.folded_exact_words);
        // v3: compound position sets
        write_set(&mut buf, &self.compound_can_prefix);
        write_set(&mut buf, &self.compound_can_suffix);
        // v4: compound_can_middle set
        write_set(&mut buf, &self.compound_can_middle);

        buf
    }

    /// Deserialize dictionary from binary cache format.
    /// Returns None if magic/version mismatch or mtime/size don't match source.
    pub fn from_cache_bytes(bytes: &[u8], expected_mtime: u64, expected_size: u32) -> Option<Self> {
        if bytes.len() < 21 {
            return None;
        }
        // Check magic + version (v2 includes preferred set)
        // cspell:ignore RSPELLD
        if &bytes[0..7] != b"RSPELLD" {
            return None;
        }
        let version = bytes[7];
        if version != HASH_DICT_CACHE_VERSION {
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
        let non_strict_words = if version >= 9 {
            read_set(bytes, &mut pos)?
        } else {
            HashSet::default()
        };
        let forbidden = read_set(bytes, &mut pos)?;
        let no_suggest = read_set(bytes, &mut pos)?;
        // v2 added preferred set between no_suggest and compound_parts
        let preferred = if version >= 2 {
            read_set(bytes, &mut pos)?
        } else {
            HashSet::default()
        };
        let compound_parts = read_set(bytes, &mut pos)?;
        let identity_words = read_set(bytes, &mut pos)?;
        let exact_words = if version >= 7 {
            read_set(bytes, &mut pos)?
        } else {
            HashSet::default()
        };
        let folded_exact_words = if version >= 11 {
            read_set(bytes, &mut pos)?
        } else {
            exact_words.iter().map(|word| word.to_lowercase()).collect()
        };
        // v3: compound position sets
        let (compound_can_prefix, compound_can_suffix) = if version >= 3 {
            let p = read_set(bytes, &mut pos)?;
            let s = read_set(bytes, &mut pos)?;
            (p, s)
        } else {
            // Legacy: all compound parts can go anywhere
            (compound_parts.clone(), compound_parts.clone())
        };
        // v4: compound_can_middle set
        let compound_can_middle = if version >= 4 {
            read_set(bytes, &mut pos)?
        } else {
            // Legacy: assume intersection of prefix and suffix
            compound_can_prefix
                .intersection(&compound_can_suffix)
                .cloned()
                .collect()
        };

        Some(Self {
            words,
            non_strict_words,
            forbidden,
            no_suggest,
            suggest_map: HashMap::default(),
            preferred,
            compound_parts,
            compound_can_prefix,
            compound_can_suffix,
            compound_can_middle,
            identity_words,
            exact_words,
            folded_exact_words,
            case_sensitive,
            repmap: None,
        })
    }

    /// Check if a word can be decomposed into compound parts.
    /// Used for dictionary-native compounds (`word+`, `+word`, `+word+`).
    /// Legacy arbitrary compounding remains a validator concern.
    pub fn check_compound_word(&self, word: &str) -> bool {
        if self.compound_parts.is_empty() {
            return false;
        }
        let normalized = self.normalize(word);
        self.check_compound_word_normalized(normalized.as_ref())
    }

    fn check_compound_word_normalized(&self, normalized: &str) -> bool {
        if self.compound_parts.is_empty() {
            return false;
        }
        if self.decompose_compound(normalized, 0, true) {
            return true;
        }
        // Compound parts are stored in lowercase form (loaded before
        // case_sensitive is set). Try lowercased decomposition as fallback.
        if self.case_sensitive {
            let lower = normalized.to_lowercase();
            if lower != normalized {
                return self.decompose_compound(&lower, 0, true);
            }
        }
        false
    }

    /// Try to decompose a word into exactly two compound parts: prefix + suffix.
    /// cspell's trie compound mode only allows 2-part decomposition (no chaining
    /// through middle parts). Parts can be as short as 1 character
    /// (`i+call` → `icall` is valid). // cspell:ignore icall
    fn decompose_compound(&self, word: &str, _depth: usize, _is_first: bool) -> bool {
        if word.len() < 2 {
            return false;
        }

        let chars: Vec<(usize, char)> = word.char_indices().collect();
        for &(split_byte, _) in &chars[1..] {
            let left = &word[..split_byte];
            let right = &word[split_byte..];

            if self.compound_can_prefix.contains(left) && self.compound_can_suffix.contains(right) {
                return true;
            }
        }
        false
    }

    /// Direct word lookup without repMap fallback.
    /// Contains the core logic shared by `has()` and repMap alternative checks.
    fn has_direct(&self, word: &str) -> bool {
        let normalized = self.normalize(word);
        self.has_direct_normalized(word, normalized.as_ref(), false)
    }

    /// Direct word lookup using pre-normalized form, without repMap fallback.
    fn has_direct_pre_normalized(&self, word: &str, normalized: &str) -> bool {
        self.has_direct_ignore_case_normalized(word, normalized, false)
    }

    fn has_direct_with_compounds(&self, word: &str) -> bool {
        let normalized = self.normalize(word);
        self.has_direct_normalized(word, normalized.as_ref(), true)
    }

    fn has_direct_pre_normalized_with_compounds(&self, word: &str, normalized: &str) -> bool {
        self.has_direct_ignore_case_normalized(word, normalized, true)
    }

    fn has_direct_ignore_case_normalized(
        &self,
        word: &str,
        normalized: &str,
        use_compounds: bool,
    ) -> bool {
        let exact = if word.is_ascii() {
            Cow::Borrowed(word)
        } else {
            Cow::Owned(word.nfc().collect::<String>())
        };
        if self.exact_words.contains(exact.as_ref())
            || self.words.contains(normalized)
            || self.non_strict_words.contains(normalized)
        {
            return true;
        }
        if use_compounds && self.check_compound_word_normalized(normalized) {
            return true;
        }
        false
    }

    fn has_direct_normalized(&self, word: &str, normalized: &str, use_compounds: bool) -> bool {
        let exact = if word.is_ascii() {
            Cow::Borrowed(word)
        } else {
            Cow::Owned(word.nfc().collect::<String>())
        };
        if self.exact_words.contains(exact.as_ref()) {
            return true;
        }
        if self.words.contains(normalized) {
            return true;
        }
        if !self.case_sensitive && self.non_strict_words.contains(normalized) {
            return true;
        }
        if !self.case_sensitive && word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) {
            let ucfirst = uc_first(normalized);
            if ucfirst != normalized && self.words.contains(ucfirst.as_str()) {
                return true;
            }
        }
        if use_compounds && self.check_compound_word_normalized(normalized) {
            return true;
        }
        false
    }

    fn has_via_repmap_with_compounds(&self, word: &str, use_compounds: bool) -> bool {
        if let Some(ref rm) = self.repmap {
            return rm.any_alternative_matches(word, |alt| {
                let normalized = self.normalize(alt);
                self.has_direct_normalized(alt, normalized.as_ref(), use_compounds)
            });
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
        self.has_direct_with_compounds(word) || self.has_via_repmap_with_compounds(word, true)
    }

    fn has_direct_only(&self, word: &str) -> bool {
        self.has_direct(word)
    }

    fn has_with_compounds(&self, word: &str) -> bool {
        self.has_direct_with_compounds(word) || self.has_via_repmap_with_compounds(word, true)
    }

    fn is_forbidden(&self, word: &str) -> bool {
        if self.forbidden.is_empty() {
            return false;
        }
        self.forbidden.contains(self.normalize(word).as_ref())
    }

    fn suggest(&self, word: &str, limit: usize) -> Vec<String> {
        let normalized = self.normalize(word);
        let mut direct_suggestions = Vec::new();
        if let Some(mapped) = self.suggest_map.get(normalized.as_ref()) {
            for suggestion in mapped {
                if !self.no_suggest.contains(suggestion.as_str())
                    && !self.forbidden.contains(suggestion.as_str())
                {
                    direct_suggestions.push(suggestion.to_string());
                    if direct_suggestions.len() >= limit {
                        return direct_suggestions;
                    }
                }
            }
        }

        // Filter out no-suggest and forbidden words from candidates
        let candidates = self.words.iter().filter_map(|w| {
            let s: &str = w.as_str();
            if self.no_suggest.contains(s) || self.forbidden.contains(s) {
                None
            } else {
                Some(s)
            }
        });
        let exact_candidates = self.exact_words.iter().filter_map(|w| {
            let s: &str = w.as_str();
            if self.no_suggest.contains(s) || self.forbidden.contains(s) {
                None
            } else {
                Some(s)
            }
        });
        let max_edits = 2; // default max edit distance for suggestions
        let costs = EditCosts::default();
        let preferred = if self.preferred.is_empty() {
            None
        } else {
            Some(&self.preferred)
        };
        let mut results = distance::select_nearest_words_weighted(
            normalized.as_ref(),
            candidates.chain(exact_candidates),
            max_edits,
            limit,
            &costs,
            preferred,
        );

        // Inject repmap-based suggestions: generate alternative spellings and
        // check if any match a known word. If the alternative already appears in
        // the candidate list (from edit-distance search), promote it to preferred
        // with zero distance so it ranks at the top.
        if let Some(ref rm) = self.repmap {
            for alt in rm.generate_alternatives(word) {
                if self.has_direct_with_compounds(&alt) && !self.no_suggest.contains(alt.as_str()) {
                    if let Some(existing) = results.iter_mut().find(|c| c.word == alt) {
                        existing.preferred = true;
                        existing.weighted_distance = 0;
                    } else {
                        results.push(distance::SuggestionCandidate {
                            word: alt,
                            raw_distance: 0,
                            weighted_distance: 0,
                            preferred: true,
                        });
                    }
                }
            }
            // Re-sort after promoting/inserting repmap suggestions
            results.sort_by(|a, b| {
                b.preferred
                    .cmp(&a.preferred)
                    .then_with(|| a.weighted_distance.cmp(&b.weighted_distance))
                    .then_with(|| a.word.cmp(&b.word))
            });
            results.truncate(limit);
        }

        for suggestion in direct_suggestions.into_iter().rev() {
            if let Some(pos) = results
                .iter()
                .position(|candidate| candidate.word == suggestion)
            {
                results.remove(pos);
            }
            results.insert(
                0,
                distance::SuggestionCandidate {
                    word: suggestion,
                    raw_distance: 0,
                    weighted_distance: 0,
                    preferred: true,
                },
            );
        }
        results.truncate(limit);

        results.into_iter().map(|c| c.word).collect()
    }

    fn find(&self, word: &str) -> FindResult {
        let normalized = self.normalize(word);
        let mut found = self.has_direct_with_compounds(word);
        if !found {
            found = self.has_via_repmap_with_compounds(word, true);
        }
        FindResult {
            found,
            forbidden: self.forbidden.contains(normalized.as_ref()),
            no_suggest: self.no_suggest.contains(normalized.as_ref()),
        }
    }

    fn find_without_compounds(&self, word: &str) -> FindResult {
        let normalized = self.normalize(word);
        let mut found = self.has_direct(word);
        if !found {
            found = self.has_via_repmap_with_compounds(word, false);
        }
        FindResult {
            found,
            forbidden: self.forbidden.contains(normalized.as_ref()),
            no_suggest: self.no_suggest.contains(normalized.as_ref()),
        }
    }

    fn find_with_compounds(&self, word: &str) -> FindResult {
        let normalized = self.normalize(word);
        let mut found = self.has_direct_with_compounds(word);
        if !found {
            found = self.has_via_repmap_with_compounds(word, true);
        }
        FindResult {
            found,
            forbidden: self.forbidden.contains(normalized.as_ref()),
            no_suggest: self.no_suggest.contains(normalized.as_ref()),
        }
    }

    fn len(&self) -> usize {
        self.words.len()
            + self.non_strict_words.len()
            + self.exact_words.len()
            + self.suggest_map.len()
    }

    fn has_forbidden_words(&self) -> bool {
        !self.forbidden.is_empty()
    }

    fn has_no_suggest_words(&self) -> bool {
        !self.no_suggest.is_empty()
    }

    fn is_case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    fn is_no_suggest(&self, word: &str) -> bool {
        if self.no_suggest.is_empty() {
            return false;
        }
        self.no_suggest.contains(self.normalize(word).as_ref())
    }

    fn is_no_suggest_pre_normalized(&self, _word: &str, normalized: &str) -> bool {
        if self.no_suggest.is_empty() {
            return false;
        }
        self.no_suggest.contains(normalized)
    }

    fn has_pre_normalized(&self, word: &str, normalized: &str) -> bool {
        self.has_direct_pre_normalized_with_compounds(word, normalized)
            || self.has_via_repmap_with_compounds(word, true)
    }

    fn has_pre_normalized_without_compounds(&self, word: &str, normalized: &str) -> bool {
        self.has_direct_pre_normalized(word, normalized)
            || self.has_via_repmap_with_compounds(word, false)
    }

    fn has_pre_normalized_direct_only(&self, word: &str, normalized: &str) -> bool {
        self.has_direct_pre_normalized(word, normalized)
    }

    fn has_pre_normalized_with_compounds(&self, word: &str, normalized: &str) -> bool {
        self.has_direct_pre_normalized_with_compounds(word, normalized)
            || self.has_via_repmap_with_compounds(word, true)
    }

    fn has_expensive_forms(&self) -> bool {
        self.repmap.is_some() || self.has_compound_parts()
    }

    fn is_forbidden_pre_normalized(&self, _word: &str, normalized: &str) -> bool {
        if self.forbidden.is_empty() {
            return false;
        }
        self.forbidden.contains(normalized)
    }

    fn export_word_sets(&self) -> Option<WordSetsRef<'_>> {
        Some(WordSetsRef {
            words: &self.words,
            exact_words: &self.exact_words,
            non_strict_words: &self.non_strict_words,
            folded_exact_words: &self.folded_exact_words,
            case_sensitive: self.case_sensitive,
        })
    }
}

// cspell:disable
#[cfg(test)]
mod compound_debug_tests {
    use super::*;
    use crate::loader::trie_v3::load_trie_v3;
    use std::path::PathBuf;

    fn de_de_dict_path() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        let path = PathBuf::from(home)
            .join(".matchum_cache/packages/node_modules/@cspell/dict-de-de/de_DE.trie.gz");
        path.exists().then_some(path)
    }

    fn en_us_dict_path() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        let path = PathBuf::from(home)
            .join(".matchum_cache/packages/node_modules/@cspell/dict-en_us/en_US.trie.gz");
        path.exists().then_some(path)
    }

    #[test]
    fn test_compound_short_parts() {
        let mut dict = HashDictionary::new(false);
        dict.add_compound_part_with_pos("tmp", CompoundPos::Middle);
        dict.add_word("s");
        dict.add_compound_part_with_pos("s", CompoundPos::Suffix);
        assert!(dict.check_compound_word("tmps"));
    }

    #[test]
    fn test_compound_single_char_prefix() {
        let mut dict = HashDictionary::new(false);
        dict.add_compound_part_with_pos("i", CompoundPos::Middle);
        dict.add_compound_part_with_pos("call", CompoundPos::Middle);
        assert!(dict.check_compound_word("icall"));
    }

    #[test]
    fn test_compound_separate_prefix_suffix_not_middle() {
        // `*p` (Suffix) + `p*` (Prefix) should NOT allow p as middle part.
        // In cspell's trie, `+p+` only exists from `*p*`, not from separate entries.
        let mut dict = HashDictionary::new(false);
        dict.add_compound_part_with_pos("str", CompoundPos::Middle);
        dict.add_compound_part_with_pos("p", CompoundPos::Suffix); // *p
        dict.add_compound_part_with_pos("p", CompoundPos::Prefix); // p*
        dict.add_compound_part_with_pos("printf", CompoundPos::Middle);
        // 2-part: str+printf should work
        assert!(dict.check_compound_word("strprintf"));
        // 3-part: str+p+printf should NOT work (p is not Middle)
        assert!(!dict.check_compound_word("strpprintf"));
    }

    #[test]
    fn test_compound_trie_only_allows_2part() {
        // cspell's trie compound mode only allows prefix+suffix (2-part).
        // Middle markers exist in the trie format but 3-part chains are not valid.
        let mut dict = HashDictionary::new(false);
        dict.add_compound_part_with_pos("str", CompoundPos::Middle);
        dict.add_compound_part_with_pos("p", CompoundPos::Middle);
        dict.add_compound_part_with_pos("printf", CompoundPos::Middle);
        // 2-part: str+printf is valid (prefix+suffix)
        assert!(dict.check_compound_word("strprintf"));
        // 3-part: str+p+printf is NOT valid
        assert!(!dict.check_compound_word("strpprintf"));
    }

    #[test]
    fn test_compound_plus_prefix_not_first_part() {
        // `+computer*` can be last but NOT first part.
        let mut dict = HashDictionary::new(false);
        // +computer* → can_first=false, can_last=true, can_middle=true
        dict.add_compound_part_explicit("computer", false, true, true);
        // *output* → can_first=true, can_last=true, can_middle=true
        dict.add_compound_part_explicit("output", true, true, true);
        // computeroutput should NOT decompose: computer can't be first part
        assert!(!dict.check_compound_word("computeroutput"));
        // output+computer should work (output=prefix, computer=suffix)
        assert!(dict.check_compound_word("outputcomputer"));
        // 3-part is never valid in trie compound mode
        dict.add_compound_part_explicit("data", true, true, true);
        assert!(dict.check_compound_word("datacomputer"));
        assert!(!dict.check_compound_word("datacomputeroutput"));
    }

    /// Test that `*word+` entries cannot be the final compound part.
    /// In cspell's trie, `*authorization+` creates `authorization+` and
    /// `+authorization+` but NOT `+authorization`, so it can't be the last part.
    #[test]
    fn test_compound_plus_end_not_suffix() {
        use crate::loader::txt::load_txt_from_reader;
        let data =
            b"# cspell-tools: keep-case no-split\n*binary*\n*authorization+\n*long*\n*running*\n";
        let dict = load_txt_from_reader(std::io::BufReader::new(&data[..])).unwrap();
        // longrunning works: *long* and *running* both have can_last=true
        assert!(
            dict.check_compound_word("longrunning"),
            "longrunning should decompose when compounds are enabled"
        );
        // binaryauthorization fails: *authorization+ has can_last=false
        assert!(
            !dict.check_compound_word("binaryauthorization"),
            "binaryauthorization should NOT decompose"
        );
        // authorization is standalone (star_start makes it standalone)
        assert!(
            dict.has("authorization"),
            "authorization should be standalone"
        );
    }

    #[test]
    fn test_dictionary_native_compounds_are_enabled_by_default() {
        let mut dict = HashDictionary::new(false);
        dict.add_compound_part_explicit("iscsi", true, true, true);
        dict.add_compound_part_explicit("servers", true, true, true);

        assert!(dict.check_compound_word("iscsiservers"));
        assert!(
            dict.has("iscsiservers"),
            "direct lookup should recognize dictionary-native compounds"
        );
        assert!(
            dict.has_with_compounds("iscsiservers"),
            "compound-enabled lookup should recognize dictionary-native compounds"
        );
        assert!(
            dict.has_pre_normalized("iscsiservers", "iscsiservers"),
            "pre-normalized direct lookup should recognize compounds"
        );
        assert!(
            dict.has_pre_normalized_with_compounds("iscsiservers", "iscsiservers"),
            "compound-enabled pre-normalized lookup should recognize compounds"
        );
        assert!(
            dict.find("iscsiservers").found,
            "default detailed lookup should recognize compounds"
        );
        assert!(
            dict.find_with_compounds("iscsiservers").found,
            "compound-enabled detailed lookup should recognize compounds"
        );
    }

    #[test]
    fn test_dictionary_native_compounds_can_be_checked_without_compounds() {
        let mut dict = HashDictionary::new(false);
        dict.add_compound_part_explicit("iscsi", true, true, true);
        dict.add_compound_part_explicit("servers", true, true, true);

        assert!(
            !dict.find_without_compounds("iscsiservers").found,
            "cspell compat needs a non-compound detailed lookup"
        );
        assert!(
            !dict.has_pre_normalized_without_compounds("iscsiservers", "iscsiservers"),
            "cspell compat needs a non-compound pre-normalized lookup"
        );
    }

    #[test]
    fn test_direct_only_lookup_skips_native_compounds() {
        let mut dict = HashDictionary::new(false);
        dict.add_compound_part_explicit("iscsi", true, true, true);
        dict.add_compound_part_explicit("servers", true, true, true);

        assert!(
            !<HashDictionary as Dictionary>::has_direct_only(&dict, "iscsiservers"),
            "direct-only lookup should not pay for compound decomposition"
        );
        assert!(
            !<HashDictionary as Dictionary>::has_pre_normalized_direct_only(
                &dict,
                "iscsiservers",
                "iscsiservers",
            ),
            "pre-normalized direct-only lookup should not use compounds"
        );
        assert!(dict.has("iscsiservers"));
    }

    #[test]
    fn test_repmap_affects_direct_lookup_like_cspell() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("column");
        dict.set_repmap(RepMap::new(vec![("colum".into(), "column".into())]));

        assert!(
            dict.has("colum"),
            "repMap alternatives should participate in direct lookup"
        );
        assert!(
            dict.has_pre_normalized("colum", "colum"),
            "pre-normalized lookup must match direct lookup behavior"
        );
        assert!(
            dict.suggest("colum", 5).iter().any(|s| s == "column"),
            "repMap alternatives should still contribute suggestions"
        );
    }

    #[test]
    fn test_direct_only_lookup_skips_repmap_forms() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("column");
        dict.set_repmap(RepMap::new(vec![("colum".into(), "column".into())]));

        assert!(
            !<HashDictionary as Dictionary>::has_direct_only(&dict, "colum"),
            "direct-only lookup should avoid repMap fallback"
        );
        assert!(
            !<HashDictionary as Dictionary>::has_pre_normalized_direct_only(
                &dict, "colum", "colum"
            ),
            "pre-normalized direct-only lookup should avoid repMap fallback"
        );
        assert!(dict.has("colum"));
    }

    #[test]
    fn exact_words_remain_case_sensitive_in_ignore_case_pre_normalized_lookup() {
        let mut dict = HashDictionary::new(true);
        dict.add_exact_word("TensorFlow");
        dict.add_exact_word("Satz");

        assert!(
            !dict.has("tensorflow"),
            "strict case-sensitive lookup should still reject lowercase variants"
        );
        assert!(
            !<HashDictionary as Dictionary>::has_pre_normalized_direct_only(
                &dict,
                "tensorflow",
                "tensorflow"
            ),
            "ignore-case lookup should not synthesize lowercase matches for exact-case trie words"
        );
        assert!(
            !<HashDictionary as Dictionary>::has_pre_normalized_direct_only(&dict, "satz", "satz"),
            "ignore-case lookup should not synthesize lowercase matches for titlecased exact words"
        );
    }

    #[test]
    fn de_de_compound_fragments_are_not_standalone_words() {
        let Some(path) = de_de_dict_path() else {
            eprintln!("Skipping: de_DE dictionary not found");
            return;
        };
        let mut dict = load_trie_v3(&path).expect("load de_DE trie");
        dict.set_case_sensitive(true);

        for word in [
            "kenn",
            "Aktualisierungs",
            "navigations",
            "Verteidigungs",
            "sicherheits",
            "Sicherheits",
            "Adress",
        ] {
            let normalized = if dict.case_sensitive {
                word.to_string()
            } else {
                dict.normalize(word).into_owned()
            };
            let in_words = dict.words.contains(normalized.as_str());
            let in_exact = dict.exact_words.contains(word);
            let in_compounds = dict.compound_parts.contains(normalized.as_str());
            let compound = dict.check_compound_word(word);
            assert!(
                !dict.has(word),
                "{word} unexpectedly valid: in_words={in_words} in_exact={in_exact} in_compounds={in_compounds} compound={compound}"
            );
        }
    }

    #[test]
    fn cache_version_bump_invalidates_stale_entries() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("sample");

        let mut bytes = dict.to_cache_bytes(123, 456);
        bytes[7] = HASH_DICT_CACHE_VERSION - 1;

        assert!(
            HashDictionary::from_cache_bytes(&bytes, 123, 456).is_none(),
            "older cache versions must be rejected after parser/cache semantics change"
        );
    }

    #[test]
    fn ignore_case_pre_normalized_lookup_uses_non_strict_branch_for_case_sensitive_trie_dicts() {
        let mut dict = HashDictionary::new(true);
        dict.add_exact_word("England");
        dict.add_non_strict_word("england");
        dict.add_exact_word("Colum");

        assert!(
            <HashDictionary as Dictionary>::has_pre_normalized_direct_only(
                &dict, "England", "england"
            ),
            "exact-case original word should remain valid in ignore-case lookup"
        );
        assert!(
            <HashDictionary as Dictionary>::has_pre_normalized_direct_only(
                &dict, "england", "england"
            ),
            "non-strict trie branch should be searched even when the dictionary itself is case-sensitive"
        );
        assert!(
            <HashDictionary as Dictionary>::has_pre_normalized_direct_only(&dict, "Colum", "colum"),
            "exact-case trie words should still match their original casing"
        );
        assert!(
            !<HashDictionary as Dictionary>::has_pre_normalized_direct_only(
                &dict, "colum", "colum"
            ),
            "exact-case trie words without a non-strict branch must stay case-sensitive"
        );
    }

    #[test]
    fn real_en_us_trie_keeps_colum_exact_only_during_ignore_case_lookup() {
        let Some(path) = en_us_dict_path() else {
            eprintln!("Skipping: en_US dictionary not found");
            return;
        };
        let mut dict = load_trie_v3(&path).expect("load en_US trie");
        dict.set_case_sensitive(true);

        assert!(
            <HashDictionary as Dictionary>::has_pre_normalized_direct_only(&dict, "Colum", "colum"),
            "cspell trace expects Colum to be found in en_us"
        );
        assert!(
            !<HashDictionary as Dictionary>::has_pre_normalized_direct_only(
                &dict, "colum", "colum"
            ),
            "cspell trace expects lowercase colum to remain unknown in en_us"
        );
    }
}
