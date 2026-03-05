use anyhow::Result;

/// Information extracted from a single morphological token.
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub surface: String,
    pub base_form: String,
    pub reading: String,
    pub pos: String,
    pub conjugation_form: String,
    pub conjugation_type: String,
}

impl TokenInfo {
    /// Returns true if this token represents punctuation, whitespace, or symbols
    /// that should not create vocabulary entries.
    pub fn is_trivial(&self) -> bool {
        matches!(
            self.pos.as_str(),
            "Symbol" | "Punctuation" | "Whitespace" | "BOS/EOS" | ""
        ) || self.surface.trim().is_empty()
    }
}

/// Initialize a lindera tokenizer with the bundled UniDic dictionary.
fn create_tokenizer() -> Result<lindera::tokenizer::Tokenizer> {
    let dictionary = lindera::dictionary::load_dictionary("embedded://unidic")
        .map_err(|e| anyhow::anyhow!("Failed to load UniDic dictionary: {}", e))?;
    let segmenter = lindera::segmenter::Segmenter::new(
        lindera::mode::Mode::Normal,
        dictionary,
        None,
    );
    Ok(lindera::tokenizer::Tokenizer::new(segmenter))
}

/// Tokenize a sentence into morphological tokens.
pub fn tokenize_sentence(text: &str) -> Result<Vec<TokenInfo>> {
    let tokenizer = create_tokenizer()?;
    let mut tokens = tokenizer
        .tokenize(text)
        .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

    let mut result = Vec::new();
    for token in tokens.iter_mut() {
        let surface = token.surface.to_string();

        // UniDic detail indices (custom fields, 0-based after the 4 common fields):
        //   0: part_of_speech, 1: subcategory_1, 2: subcategory_2, 3: subcategory_3
        //   4: conjugation_type, 5: conjugation_form
        //   6: reading (katakana), 7: lexeme
        //   8: orthographic_surface_form, 9: phonological_surface_form
        //  10: orthographic_base_form, 11: phonological_base_form
        let details: Vec<String> = token.details().iter().map(|s| s.to_string()).collect();
        let get = |i: usize| -> &str {
            details.get(i).map(|s| s.as_str()).unwrap_or("*")
        };

        let major_pos = get(0);
        let conj_type = get(4);
        let conj_form = get(5);
        let reading_kata = get(6);

        // UniDic uses "orthographic_base_form" (index 10) for the written base form,
        // and "lexeme" (index 7) for the lemma. Prefer orthographic, fall back to lexeme.
        let base_form_raw = {
            let ortho = get(10);
            if ortho != "*" {
                ortho
            } else {
                let lexeme = get(7);
                if lexeme != "*" { lexeme } else { &surface }
            }
        };

        let pos = map_pos(major_pos);
        let reading = katakana_to_hiragana(reading_kata);

        result.push(TokenInfo {
            surface: surface.clone(),
            base_form: base_form_raw.to_string(),
            reading: if reading == "*" { String::new() } else { reading },
            pos: pos.to_string(),
            conjugation_form: normalize_star(conj_form),
            conjugation_type: normalize_star(conj_type),
        });
    }

    Ok(result)
}

/// Replace "*" with empty string.
fn normalize_star(s: &str) -> String {
    if s == "*" { String::new() } else { s.to_string() }
}

/// Map UniDic major POS tags to simplified categories.
/// UniDic uses the same Japanese POS names as IPADIC but with slightly
/// different sub-categories. The major categories are identical.
fn map_pos(major_pos: &str) -> &'static str {
    match major_pos {
        "名詞" => "Noun",
        "代名詞" => "Pronoun",
        "動詞" => "Verb",
        "形容詞" => "Adjective",
        "形状詞" => "Adjectival_Noun", // UniDic-specific: na-adjectives
        "副詞" => "Adverb",
        "助詞" => "Particle",
        "助動詞" => "Auxiliary",
        "接続詞" => "Conjunction",
        "記号" => "Symbol",
        "補助記号" => "Symbol",        // UniDic: supplementary symbols (。、etc.)
        "感動詞" => "Interjection",
        "連体詞" => "Adnominal",
        "接頭辞" => "Prefix",          // UniDic spelling
        "接頭詞" => "Prefix",          // IPADIC spelling
        "接尾辞" => "Suffix",          // UniDic-specific
        "空白" => "Whitespace",        // UniDic whitespace category
        "フィラー" => "Filler",
        "BOS/EOS" => "BOS/EOS",
        _ => "Other",
    }
}

/// Convert katakana string to hiragana.
fn katakana_to_hiragana(kata: &str) -> String {
    kata.chars()
        .map(|c| {
            if ('\u{30A1}'..='\u{30F6}').contains(&c) {
                // Katakana to hiragana: subtract 0x60
                char::from_u32(c as u32 - 0x60).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

/// Split text into paragraphs (by double newlines or single newlines with content).
pub fn split_paragraphs(text: &str) -> Vec<String> {
    text.split("\n\n")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Split a paragraph into sentences.
/// Splits on Japanese sentence-ending punctuation: 。！？ and newlines.
pub fn split_sentences(paragraph: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for c in paragraph.chars() {
        current.push(c);
        if c == '。' || c == '！' || c == '？' || c == '\n' {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                sentences.push(trimmed);
            }
            current.clear();
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }

    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_katakana_to_hiragana() {
        assert_eq!(katakana_to_hiragana("タベル"), "たべる");
        assert_eq!(katakana_to_hiragana("ニホンゴ"), "にほんご");
        assert_eq!(katakana_to_hiragana("abc"), "abc");
    }

    #[test]
    fn test_split_paragraphs() {
        let text = "First paragraph.\n\nSecond paragraph.\n\nThird.";
        let paras = split_paragraphs(text);
        assert_eq!(paras.len(), 3);
        assert_eq!(paras[0], "First paragraph.");
    }

    #[test]
    fn test_split_sentences() {
        let para = "吾輩は猫である。名前はまだ無い。";
        let sentences = split_sentences(para);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0], "吾輩は猫である。");
        assert_eq!(sentences[1], "名前はまだ無い。");
    }

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize_sentence("食べる").unwrap();
        assert!(!tokens.is_empty());
        // UniDic should recognize 食べる as a verb
        let verb = tokens.iter().find(|t| t.pos == "Verb");
        assert!(verb.is_some(), "Should find a verb token, got: {:?}", tokens);
    }

    #[test]
    fn test_tokenize_conjugated() {
        let tokens = tokenize_sentence("食べた").unwrap();
        assert!(!tokens.is_empty());
        // "食べた" should decompose to 食べ (verb) + た (auxiliary)
        let verb = tokens.iter().find(|t| t.pos == "Verb");
        assert!(verb.is_some(), "Should find a verb token, got: {:?}", tokens);
    }

    #[test]
    fn test_tokenize_reading() {
        let tokens = tokenize_sentence("猫").unwrap();
        let neko = tokens.iter().find(|t| t.surface == "猫").unwrap();
        assert_eq!(neko.reading, "ねこ", "猫 should have reading ねこ");
    }

    #[test]
    fn test_tokenize_unidic_pos_categories() {
        // Test UniDic-specific POS: 補助記号 for punctuation
        let tokens = tokenize_sentence("東京。").unwrap();
        let period = tokens.iter().find(|t| t.surface == "。");
        assert!(period.is_some());
        assert_eq!(period.unwrap().pos, "Symbol");
    }

    #[test]
    fn test_map_pos() {
        assert_eq!(map_pos("名詞"), "Noun");
        assert_eq!(map_pos("動詞"), "Verb");
        assert_eq!(map_pos("形状詞"), "Adjectival_Noun");
        assert_eq!(map_pos("補助記号"), "Symbol");
        assert_eq!(map_pos("unknown"), "Other");
    }
}
