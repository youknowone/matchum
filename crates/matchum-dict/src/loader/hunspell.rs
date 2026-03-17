use std::collections::HashMap;
use std::path::Path;

use regex::Regex;

use crate::hashdict::HashDictionary;

use super::LoadError;

/// A single affix rule (prefix or suffix).
pub struct AffixRule {
    pub strip: String,
    pub add: String,
    pub condition: Option<String>,
    pub cross_product: bool,
}

/// Parse a .aff file and extract PFX/SFX rules.
///
/// Returns (prefixes, suffixes) keyed by flag character.
pub fn parse_aff(content: &str) -> (HashMap<char, Vec<AffixRule>>, HashMap<char, Vec<AffixRule>>) {
    let mut prefixes: HashMap<char, Vec<AffixRule>> = HashMap::new();
    let mut suffixes: HashMap<char, Vec<AffixRule>> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let kind = parts[0];
        if kind != "SFX" && kind != "PFX" {
            continue;
        }

        let flag = match parts[1].chars().next() {
            Some(f) => f,
            None => continue,
        };

        // Header line: SFX/PFX flag cross_product count
        // Rule line: SFX/PFX flag strip add [condition]
        if parts.len() == 4 {
            // Could be header (count is numeric) or rule with condition
            if let Ok(_count) = parts[3].parse::<u32>() {
                // This is a header line: SFX flag Y/N count
                // Nothing to do here, rules follow
                continue;
            }
        }

        // This is a rule line: SFX/PFX flag strip add [condition]
        if parts.len() < 4 {
            continue;
        }

        let strip = if parts[2] == "0" {
            String::new()
        } else {
            parts[2].to_string()
        };

        let add = if parts[3] == "0" {
            String::new()
        } else {
            parts[3].to_string()
        };

        let condition = if parts.len() >= 5 && parts[4] != "." {
            Some(parts[4].to_string())
        } else {
            None
        };

        // Determine cross_product from the header we already parsed.
        // We look for the header by checking if a header was stored.
        // For simplicity, we determine cross_product by checking the flag's header.
        // Since we skip headers above, we need a different approach.
        // We'll parse headers in a first pass.
        let cross_product = true; // Will be corrected below

        let rule = AffixRule {
            strip,
            add,
            condition,
            cross_product,
        };

        match kind {
            "PFX" => prefixes.entry(flag).or_default().push(rule),
            "SFX" => suffixes.entry(flag).or_default().push(rule),
            _ => {}
        }
    }

    // Second pass: read cross_product flags from headers and apply to rules
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 4 {
            continue;
        }
        let kind = parts[0];
        if kind != "SFX" && kind != "PFX" {
            continue;
        }
        // Check if this is a header line (4th part is numeric count)
        if parts[3].parse::<u32>().is_err() {
            continue;
        }
        let flag = match parts[1].chars().next() {
            Some(f) => f,
            None => continue,
        };
        let cross = parts[2] == "Y";

        let map = match kind {
            "PFX" => &mut prefixes,
            "SFX" => &mut suffixes,
            _ => continue,
        };
        if let Some(rules) = map.get_mut(&flag) {
            for rule in rules.iter_mut() {
                rule.cross_product = cross;
            }
        }
    }

    (prefixes, suffixes)
}

/// Parse a .dic file and extract words with their flags.
///
/// First line is the word count (ignored). Each subsequent line is `word` or `word/flags`.
pub fn parse_dic(content: &str) -> Vec<(String, Vec<char>)> {
    let mut words = Vec::new();
    let mut first_line = true;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Skip the word count on the first non-empty line
        if first_line {
            first_line = false;
            if line.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
        }

        if let Some(slash_pos) = line.find('/') {
            let word = &line[..slash_pos];
            let flags: Vec<char> = line[slash_pos + 1..].chars().collect();
            words.push((word.to_string(), flags));
        } else {
            words.push((line.to_string(), Vec::new()));
        }
    }

    words
}

/// Check if a condition pattern matches the given word.
fn condition_matches(word: &str, condition: &str) -> bool {
    // Build a regex anchored to the full word
    let pattern = format!("^.*{}$", condition);
    match Regex::new(&pattern) {
        Ok(re) => re.is_match(word),
        Err(_) => false,
    }
}

/// Expand a word by applying all matching affix rules for its flags.
///
/// Returns a list of all generated word forms (including the base word).
pub fn expand_word(
    word: &str,
    flags: &[char],
    prefixes: &HashMap<char, Vec<AffixRule>>,
    suffixes: &HashMap<char, Vec<AffixRule>>,
) -> Vec<String> {
    let mut forms = Vec::new();
    forms.push(word.to_string());

    // Collect suffix forms
    let mut suffixed_forms: Vec<(String, bool)> = Vec::new(); // (form, cross_product)
    for &flag in flags {
        if let Some(rules) = suffixes.get(&flag) {
            for rule in rules {
                if let Some(form) = apply_suffix(word, rule) {
                    suffixed_forms.push((form.clone(), rule.cross_product));
                    forms.push(form);
                }
            }
        }
    }

    // Collect prefix forms
    let mut prefixed_forms: Vec<(String, bool)> = Vec::new();
    for &flag in flags {
        if let Some(rules) = prefixes.get(&flag) {
            for rule in rules {
                if let Some(form) = apply_prefix(word, rule) {
                    prefixed_forms.push((form.clone(), rule.cross_product));
                    forms.push(form);
                }
            }
        }
    }

    // Cross-product: apply prefixes to suffixed forms and suffixes to prefixed forms
    for &flag in flags {
        if let Some(pfx_rules) = prefixes.get(&flag) {
            for pfx_rule in pfx_rules {
                if !pfx_rule.cross_product {
                    continue;
                }
                for (sfx_form, sfx_cross) in &suffixed_forms {
                    if !sfx_cross {
                        continue;
                    }
                    if let Some(form) = apply_prefix(sfx_form, pfx_rule) {
                        forms.push(form);
                    }
                }
            }
        }
    }

    forms
}

/// Apply a suffix rule to a word. Returns the new form if the condition matches.
fn apply_suffix(word: &str, rule: &AffixRule) -> Option<String> {
    // Check condition against the original word
    if let Some(ref cond) = rule.condition
        && !condition_matches(word, cond)
    {
        return None;
    }

    // Strip suffix
    let base = if rule.strip.is_empty() {
        word.to_string()
    } else if word.ends_with(&rule.strip) {
        word[..word.len() - rule.strip.len()].to_string()
    } else {
        return None;
    };

    // Add suffix
    Some(format!("{}{}", base, rule.add))
}

/// Apply a prefix rule to a word. Returns the new form if the condition matches.
fn apply_prefix(word: &str, rule: &AffixRule) -> Option<String> {
    // Check condition against the original word
    if let Some(ref cond) = rule.condition {
        // For prefix conditions, anchor at start
        let pattern = format!("^{}", cond);
        match Regex::new(&pattern) {
            Ok(re) => {
                if !re.is_match(word) {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }

    // Strip prefix
    let base = if rule.strip.is_empty() {
        word.to_string()
    } else if word.starts_with(&rule.strip) {
        word[rule.strip.len()..].to_string()
    } else {
        return None;
    };

    // Add prefix
    Some(format!("{}{}", rule.add, base))
}

/// Load a Hunspell dictionary pair (.dic + .aff) into a HashDictionary.
///
/// The .aff file is located by replacing the .dic extension with .aff.
pub fn load_hunspell(dic_path: &Path, aff_path: &Path) -> Result<HashDictionary, LoadError> {
    let aff_content = std::fs::read_to_string(aff_path)?;
    let dic_content = std::fs::read_to_string(dic_path)?;

    let (prefixes, suffixes) = parse_aff(&aff_content);
    let base_words = parse_dic(&dic_content);

    let mut dict = HashDictionary::new(false);

    for (word, flags) in &base_words {
        let forms = expand_word(word, flags, &prefixes, &suffixes);
        for form in forms {
            dict.add_word(&form);
        }
    }

    Ok(dict)
}

// cspell:disable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_aff_basic_suffix() {
        let aff = "\
SET UTF-8
SFX S Y 1
SFX S 0 s .
";
        let (prefixes, suffixes) = parse_aff(aff);
        assert!(prefixes.is_empty());
        assert_eq!(suffixes.len(), 1);
        let rules = &suffixes[&'S'];
        assert_eq!(rules.len(), 1);
        assert!(rules[0].strip.is_empty());
        assert_eq!(rules[0].add, "s");
        assert!(rules[0].cross_product);
    }

    #[test]
    fn test_parse_aff_basic_prefix() {
        let aff = "\
SET UTF-8
PFX U Y 1
PFX U 0 un .
";
        let (prefixes, suffixes) = parse_aff(aff);
        assert!(suffixes.is_empty());
        assert_eq!(prefixes.len(), 1);
        let rules = &prefixes[&'U'];
        assert_eq!(rules.len(), 1);
        assert!(rules[0].strip.is_empty());
        assert_eq!(rules[0].add, "un");
        assert!(rules[0].cross_product);
    }

    #[test]
    fn test_parse_dic_basic() {
        let dic = "\
3
hello/S
world
run/SU
";
        let words = parse_dic(dic);
        assert_eq!(words.len(), 3);
        assert_eq!(words[0], ("hello".to_string(), vec!['S']));
        assert_eq!(words[1], ("world".to_string(), vec![]));
        assert_eq!(words[2], ("run".to_string(), vec!['S', 'U']));
    }

    #[test]
    fn test_expand_word_suffix() {
        let mut suffixes = HashMap::new();
        suffixes.insert(
            'S',
            vec![AffixRule {
                strip: String::new(),
                add: "s".to_string(),
                condition: None,
                cross_product: true,
            }],
        );
        let prefixes = HashMap::new();

        let forms = expand_word("run", &['S'], &prefixes, &suffixes);
        assert!(forms.contains(&"run".to_string()));
        assert!(forms.contains(&"runs".to_string()));
        assert_eq!(forms.len(), 2);
    }

    #[test]
    fn test_expand_word_prefix() {
        let suffixes = HashMap::new();
        let mut prefixes = HashMap::new();
        prefixes.insert(
            'U',
            vec![AffixRule {
                strip: String::new(),
                add: "un".to_string(),
                condition: None,
                cross_product: true,
            }],
        );

        let forms = expand_word("happy", &['U'], &prefixes, &suffixes);
        assert!(forms.contains(&"happy".to_string()));
        assert!(forms.contains(&"unhappy".to_string()));
        assert_eq!(forms.len(), 2);
    }

    #[test]
    fn test_expand_word_cross_product() {
        let mut suffixes = HashMap::new();
        suffixes.insert(
            'S',
            vec![AffixRule {
                strip: String::new(),
                add: "s".to_string(),
                condition: None,
                cross_product: true,
            }],
        );
        let mut prefixes = HashMap::new();
        prefixes.insert(
            'U',
            vec![AffixRule {
                strip: String::new(),
                add: "un".to_string(),
                condition: None,
                cross_product: true,
            }],
        );

        let forms = expand_word("run", &['S', 'U'], &prefixes, &suffixes);
        assert!(forms.contains(&"run".to_string()));
        assert!(forms.contains(&"runs".to_string()));
        assert!(forms.contains(&"unrun".to_string()));
        assert!(forms.contains(&"unruns".to_string()));
        assert_eq!(forms.len(), 4);
    }

    #[test]
    fn test_expand_word_with_strip() {
        let mut suffixes = HashMap::new();
        suffixes.insert(
            'D',
            vec![AffixRule {
                strip: "e".to_string(),
                add: "ing".to_string(),
                condition: Some("e".to_string()),
                cross_product: true,
            }],
        );
        let prefixes = HashMap::new();

        let forms = expand_word("make", &['D'], &prefixes, &suffixes);
        assert!(forms.contains(&"make".to_string()));
        assert!(forms.contains(&"making".to_string()));
        assert_eq!(forms.len(), 2);
    }

    #[test]
    fn test_condition_no_match() {
        let mut suffixes = HashMap::new();
        suffixes.insert(
            'D',
            vec![AffixRule {
                strip: "e".to_string(),
                add: "ing".to_string(),
                condition: Some("e".to_string()),
                cross_product: true,
            }],
        );
        let prefixes = HashMap::new();

        // "run" does not end with "e", so the D rule should not apply
        let forms = expand_word("run", &['D'], &prefixes, &suffixes);
        assert_eq!(forms, vec!["run".to_string()]);
    }

    #[test]
    fn test_no_cross_product() {
        let mut suffixes = HashMap::new();
        suffixes.insert(
            'S',
            vec![AffixRule {
                strip: String::new(),
                add: "s".to_string(),
                condition: None,
                cross_product: false, // N = no cross product
            }],
        );
        let mut prefixes = HashMap::new();
        prefixes.insert(
            'U',
            vec![AffixRule {
                strip: String::new(),
                add: "un".to_string(),
                condition: None,
                cross_product: true,
            }],
        );

        let forms = expand_word("run", &['S', 'U'], &prefixes, &suffixes);
        // Should have: run, runs, unrun, but NOT unruns (since S has cross_product=false)
        assert!(forms.contains(&"run".to_string()));
        assert!(forms.contains(&"runs".to_string()));
        assert!(forms.contains(&"unrun".to_string()));
        assert!(!forms.contains(&"unruns".to_string()));
        assert_eq!(forms.len(), 3);
    }
}
