use matchum_dict::dictionary::Dictionary;
use matchum_dict::hashdict::HashDictionary;
use matchum_dict::repmap::RepMap;

#[test]
fn repmap_german_umlaut() {
    let mut dict = HashDictionary::new(false);
    dict.add_word("stra\u{00df}e");
    dict.set_repmap(RepMap::new(vec![("ss".into(), "\u{00df}".into())]));
    assert!(dict.has("strasse")); // should match via repMap
    assert!(dict.has("stra\u{00df}e")); // direct match
}

#[test]
fn repmap_french_accent() {
    let mut dict = HashDictionary::new(false);
    dict.add_word("caf\u{00e9}");
    dict.set_repmap(RepMap::new(vec![("e".into(), "\u{00e9}".into())]));
    assert!(dict.has("cafe")); // should match via repMap
}

#[test]
fn repmap_no_false_positive() {
    let mut dict = HashDictionary::new(false);
    dict.add_word("hello");
    dict.set_repmap(RepMap::new(vec![("ss".into(), "\u{00df}".into())]));
    assert!(dict.has("hello")); // direct match
    assert!(!dict.has("xyzzy")); // no match, repMap doesn't help
}

#[test]
fn repmap_disabled() {
    let mut dict = HashDictionary::new(false);
    dict.add_word("stra\u{00df}e");
    // No repMap set
    assert!(!dict.has("strasse")); // should NOT match without repMap
    assert!(dict.has("stra\u{00df}e")); // direct match
}
