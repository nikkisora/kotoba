use anyhow::Result;

/// Information extracted from a single morphological token.
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub surface: String,
    pub base_form: String,
    /// Reading of the base/lemma form (for vocabulary matching and dictionary lookup).
    pub reading: String,
    /// Reading of the actual surface form (for furigana display above conjugated forms).
    pub surface_reading: String,
    pub pos: String,
    pub conjugation_form: String,
    pub conjugation_type: String,
}

impl TokenInfo {
    /// Returns true if this token represents punctuation, whitespace, symbols,
    /// numbers, or non-Japanese text that should not create vocabulary entries.
    pub fn is_trivial(&self) -> bool {
        matches!(
            self.pos.as_str(),
            "Symbol" | "Punctuation" | "Whitespace" | "BOS/EOS" | ""
        ) || self.surface.trim().is_empty()
            || is_numeric(&self.surface)
            || is_ascii_only(&self.surface)
    }
}

/// Check if a string is purely numeric (digits, decimal points, commas).
fn is_numeric(s: &str) -> bool {
    let trimmed = s.trim();
    !trimmed.is_empty()
        && trimmed.chars().all(|c| {
            c.is_ascii_digit()
                || c == '.'
                || c == ','
                || c == '０'
                || c == '１'
                || c == '２'
                || c == '３'
                || c == '４'
                || c == '５'
                || c == '６'
                || c == '７'
                || c == '８'
                || c == '９'
                || ('０'..='９').contains(&c)
        })
}

/// Check if a string contains only ASCII characters (English text, punctuation, etc.).
fn is_ascii_only(s: &str) -> bool {
    let trimmed = s.trim();
    !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii())
}

/// Initialize a lindera tokenizer with the bundled UniDic dictionary.
fn create_tokenizer() -> Result<lindera::tokenizer::Tokenizer> {
    let dictionary = lindera::dictionary::load_dictionary("embedded://unidic")
        .map_err(|e| anyhow::anyhow!("Failed to load UniDic dictionary: {}", e))?;
    let segmenter =
        lindera::segmenter::Segmenter::new(lindera::mode::Mode::Normal, dictionary, None);
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
        let get = |i: usize| -> &str { details.get(i).map(|s| s.as_str()).unwrap_or("*") };

        let major_pos = get(0);
        let conj_type = get(4);
        let conj_form = get(5);
        let lemma_reading_kata = get(6); // Index 6: lemma reading (e.g. イク for 行く)
        let surface_reading_kata = get(9); // Index 9: surface pronunciation (e.g. イッ for 行っ)

        // UniDic uses "orthographic_base_form" (index 10) for the written base form,
        // and "lexeme" (index 7) for the lemma. Prefer orthographic, fall back to lexeme.
        let base_form_raw = {
            let ortho = get(10);
            if ortho != "*" {
                ortho
            } else {
                let lexeme = get(7);
                if lexeme != "*" {
                    lexeme
                } else {
                    &surface
                }
            }
        };

        let pos = map_pos(major_pos);
        let reading = katakana_to_hiragana(lemma_reading_kata);
        let surface_reading = katakana_to_hiragana(surface_reading_kata);

        result.push(TokenInfo {
            surface: surface.clone(),
            base_form: base_form_raw.to_string(),
            reading: if reading == "*" {
                String::new()
            } else {
                reading
            },
            surface_reading: if surface_reading == "*" {
                String::new()
            } else {
                surface_reading
            },
            pos: pos.to_string(),
            conjugation_form: normalize_star(conj_form),
            conjugation_type: normalize_star(conj_type),
        });
    }

    Ok(result)
}

/// Replace "*" with empty string.
fn normalize_star(s: &str) -> String {
    if s == "*" {
        String::new()
    } else {
        s.to_string()
    }
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
        "補助記号" => "Symbol", // UniDic: supplementary symbols (。、etc.)
        "感動詞" => "Interjection",
        "連体詞" => "Adnominal",
        "接頭辞" => "Prefix",   // UniDic spelling
        "接頭詞" => "Prefix",   // IPADIC spelling
        "接尾辞" => "Suffix",   // UniDic-specific
        "空白" => "Whitespace", // UniDic whitespace category
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
/// Splits on Japanese sentence-ending punctuation: 。！？ and newlines,
/// but NOT when inside quotation marks 「」『』 or parentheses （）〈〉.
/// Closing brackets/quotes immediately after a sentence-ender are absorbed
/// into the same sentence (e.g. `！？）` stays together).
pub fn split_sentences(paragraph: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut depth: i32 = 0; // nesting depth for brackets/quotes

    let chars: Vec<char> = paragraph.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];
        current.push(c);

        // Track nesting of quotes and brackets
        match c {
            '「' | '『' | '（' | '〈' | '《' | '【' | '｛' | '(' => depth += 1,
            '」' | '』' | '）' | '〉' | '》' | '】' | '｝' | ')' => {
                depth = (depth - 1).max(0);
            }
            _ => {}
        }

        let is_sentence_end = c == '。' || c == '！' || c == '？' || c == '\n';

        // Inside quotes/brackets: only split on 。 if the quote appears unclosed
        // (i.e., no matching closing bracket exists anywhere ahead).
        // ！ and ？ never split inside quotes.
        let should_split = if !is_sentence_end {
            false
        } else if depth <= 0 {
            true
        } else if c == '。' || c == '\n' {
            // At depth > 0: split on 。 only if the bracket seems unclosed.
            // Heuristic: scan ahead for any closing bracket. If found, don't split.
            let mut close_count = 0i32;
            let mut open_count = 0i32;
            for &ahead in &chars[i + 1..] {
                if matches!(ahead, '「' | '『' | '（' | '〈' | '《' | '【' | '｛' | '(') {
                    open_count += 1;
                }
                if matches!(ahead, '」' | '』' | '）' | '〉' | '》' | '】' | '｝' | ')') {
                    if close_count < open_count {
                        // This close matches an open we saw during lookahead, not ours
                        open_count -= 1;
                    } else {
                        close_count += 1;
                    }
                }
            }
            // If there are enough closes ahead to match our current depth, bracket is closed
            close_count < depth
        } else {
            false // ！？ inside quotes — never split
        };

        if should_split {
            // Absorb any trailing closing brackets/quotes that immediately follow
            while i + 1 < len {
                let next = chars[i + 1];
                if matches!(next, '」' | '』' | '）' | '〉' | '》' | '】' | '｝' | ')') {
                    i += 1;
                    current.push(next);
                    depth = (depth - 1).max(0);
                } else {
                    break;
                }
            }

            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                sentences.push(trimmed);
            }
            current.clear();
        }

        i += 1;
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
    fn test_split_sentences_quoted() {
        // Punctuation inside quotes should NOT split
        let para = "友達が「美味しいよ！」と言っていた。次の文。";
        let sentences = split_sentences(para);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0], "友達が「美味しいよ！」と言っていた。");
        assert_eq!(sentences[1], "次の文。");
    }

    #[test]
    fn test_split_sentences_parenthesized() {
        // Punctuation inside parentheses should NOT split
        let para = "（えっ、100%オーガニック！？）と、私は少し驚いた。";
        let sentences = split_sentences(para);
        assert_eq!(sentences.len(), 1);
        assert_eq!(
            sentences[0],
            "（えっ、100%オーガニック！？）と、私は少し驚いた。"
        );
    }

    #[test]
    fn test_split_sentences_nested_quotes() {
        // Nested quotes: 「...『...！』...」
        let para = "彼は「彼女が『すごい！』と叫んだ」と言った。終わり。";
        let sentences = split_sentences(para);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0], "彼は「彼女が『すごい！』と叫んだ」と言った。");
    }

    #[test]
    fn test_split_sentences_unclosed_bracket() {
        // Unclosed bracket — should NOT swallow the entire rest of the paragraph
        let para = "彼は「すごい。次の文。最後の文。";
        let sentences = split_sentences(para);
        // The 「 is never closed — no 」 ahead before next 。, so split normally
        assert_eq!(
            sentences.len(),
            3,
            "Unclosed bracket should not prevent all splitting. Got: {:?}",
            sentences
        );
        assert_eq!(sentences[0], "彼は「すごい。");
        assert_eq!(sentences[1], "次の文。");
        assert_eq!(sentences[2], "最後の文。");
    }

    #[test]
    fn test_split_sentences_period_inside_closed_quotes() {
        // 。 inside properly closed quotes should NOT split
        let para = "「行くよ。明日だ。」と彼は言った。次の文。";
        let sentences = split_sentences(para);
        assert_eq!(
            sentences.len(),
            2,
            "Period inside closed quotes should not split. Got: {:?}",
            sentences
        );
        assert_eq!(sentences[0], "「行くよ。明日だ。」と彼は言った。");
        assert_eq!(sentences[1], "次の文。");
    }

    #[test]
    fn test_split_sentences_question_in_quotes() {
        // Question mark inside quotes should not split
        let para = "店員さんに「安全に食べられますか？」と聞いた。店員さんは答えた。";
        let sentences = split_sentences(para);
        assert_eq!(sentences.len(), 2);
        assert_eq!(
            sentences[0],
            "店員さんに「安全に食べられますか？」と聞いた。"
        );
    }

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize_sentence("食べる").unwrap();
        assert!(!tokens.is_empty());
        // UniDic should recognize 食べる as a verb
        let verb = tokens.iter().find(|t| t.pos == "Verb");
        assert!(
            verb.is_some(),
            "Should find a verb token, got: {:?}",
            tokens
        );
    }

    #[test]
    fn test_tokenize_conjugated() {
        let tokens = tokenize_sentence("食べた").unwrap();
        assert!(!tokens.is_empty());
        // "食べた" should decompose to 食べ (verb) + た (auxiliary)
        let verb = tokens.iter().find(|t| t.pos == "Verb");
        assert!(
            verb.is_some(),
            "Should find a verb token, got: {:?}",
            tokens
        );
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
