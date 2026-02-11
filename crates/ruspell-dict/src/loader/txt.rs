use crate::hashdict::HashDictionary;
use std::path::Path;

use super::LoadError;

/// Load a plain text dictionary (.txt or .txt.gz).
pub fn load_txt(path: &Path) -> Result<HashDictionary, LoadError> {
    let file = std::fs::File::open(path)?;
    let reader: Box<dyn std::io::Read> =
        if path.extension().is_some_and(|ext| ext == "gz") {
            Box::new(flate2::read::GzDecoder::new(file))
        } else {
            Box::new(file)
        };

    let mut dict = HashDictionary::new(false);
    let buf = std::io::BufReader::new(reader);
    for line in std::io::BufRead::lines(buf) {
        let line = line?;
        let word = line.trim();
        if word.is_empty() || word.starts_with('#') {
            continue;
        }
        if let Some(rest) = word.strip_prefix('!') {
            dict.add_forbidden(rest);
        } else if let Some(rest) = word.strip_prefix('~') {
            dict.add_no_suggest(rest);
        } else {
            dict.add_word(word);
        }
    }
    Ok(dict)
}
