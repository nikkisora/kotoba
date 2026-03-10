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
pub fn is_numeric(s: &str) -> bool {
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
pub fn is_ascii_only(s: &str) -> bool {
    let trimmed = s.trim();
    !trimmed.is_empty() && trimmed.is_ascii()
}

/// Initialize a lindera tokenizer with the bundled UniDic dictionary.
/// Creating a tokenizer is expensive (loads dictionary data), so callers
/// processing multiple sentences should create one via `create_tokenizer()`
/// and reuse it with `tokenize_with()`.
pub fn create_tokenizer() -> Result<lindera::tokenizer::Tokenizer> {
    let dictionary = lindera::dictionary::load_dictionary("embedded://unidic")
        .map_err(|e| anyhow::anyhow!("Failed to load UniDic dictionary: {}", e))?;
    let segmenter =
        lindera::segmenter::Segmenter::new(lindera::mode::Mode::Normal, dictionary, None);
    Ok(lindera::tokenizer::Tokenizer::new(segmenter))
}

/// Tokenize a sentence into morphological tokens.
/// Creates a new tokenizer each call — for batch processing, use
/// `create_tokenizer()` + `tokenize_with()` instead.
pub fn tokenize_sentence(text: &str) -> Result<Vec<TokenInfo>> {
    let tokenizer = create_tokenizer()?;
    tokenize_with(&tokenizer, text)
}

/// Tokenize a sentence using an existing tokenizer instance.
/// This avoids the cost of re-loading the dictionary for every sentence.
pub fn tokenize_with(
    tokenizer: &lindera::tokenizer::Tokenizer,
    text: &str,
) -> Result<Vec<TokenInfo>> {
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
pub fn katakana_to_hiragana(kata: &str) -> String {
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

/// Convert romaji (latin alphabet) to hiragana using standard Wapuro romanization.
/// Handles common patterns: consonant+vowel pairs, double consonants (っ), n before
/// consonants (ん), long vowels, and special digraphs (sh, ch, ts, etc.).
pub fn romaji_to_hiragana(input: &str) -> String {
    // Mapping table: longest matches first within each starting letter
    static ROMAJI_MAP: &[(&str, &str)] = &[
        // Four-letter combinations
        ("xtsu", "っ"),
        // Three-letter combinations
        ("sha", "しゃ"),
        ("shi", "し"),
        ("shu", "しゅ"),
        ("sho", "しょ"),
        ("chi", "ち"),
        ("tsu", "つ"),
        ("cha", "ちゃ"),
        ("chu", "ちゅ"),
        ("cho", "ちょ"),
        ("tya", "ちゃ"),
        ("tyi", "ちぃ"),
        ("tyu", "ちゅ"),
        ("tye", "ちぇ"),
        ("tyo", "ちょ"),
        ("nya", "にゃ"),
        ("nyi", "にぃ"),
        ("nyu", "にゅ"),
        ("nye", "にぇ"),
        ("nyo", "にょ"),
        ("hya", "ひゃ"),
        ("hyi", "ひぃ"),
        ("hyu", "ひゅ"),
        ("hye", "ひぇ"),
        ("hyo", "ひょ"),
        ("mya", "みゃ"),
        ("myi", "みぃ"),
        ("myu", "みゅ"),
        ("mye", "みぇ"),
        ("myo", "みょ"),
        ("rya", "りゃ"),
        ("ryi", "りぃ"),
        ("ryu", "りゅ"),
        ("rye", "りぇ"),
        ("ryo", "りょ"),
        ("gya", "ぎゃ"),
        ("gyi", "ぎぃ"),
        ("gyu", "ぎゅ"),
        ("gye", "ぎぇ"),
        ("gyo", "ぎょ"),
        ("bya", "びゃ"),
        ("byi", "びぃ"),
        ("byu", "びゅ"),
        ("bye", "びぇ"),
        ("byo", "びょ"),
        ("pya", "ぴゃ"),
        ("pyi", "ぴぃ"),
        ("pyu", "ぴゅ"),
        ("pye", "ぴぇ"),
        ("pyo", "ぴょ"),
        ("kya", "きゃ"),
        ("kyi", "きぃ"),
        ("kyu", "きゅ"),
        ("kye", "きぇ"),
        ("kyo", "きょ"),
        ("jya", "じゃ"),
        ("jyi", "じぃ"),
        ("jyu", "じゅ"),
        ("jye", "じぇ"),
        ("jyo", "じょ"),
        ("dya", "ぢゃ"),
        ("dyi", "ぢぃ"),
        ("dyu", "ぢゅ"),
        ("dye", "ぢぇ"),
        ("dyo", "ぢょ"),
        // Two-letter combinations
        ("ka", "か"),
        ("ki", "き"),
        ("ku", "く"),
        ("ke", "け"),
        ("ko", "こ"),
        ("sa", "さ"),
        ("si", "し"),
        ("su", "す"),
        ("se", "せ"),
        ("so", "そ"),
        ("ta", "た"),
        ("ti", "ち"),
        ("tu", "つ"),
        ("te", "て"),
        ("to", "と"),
        ("na", "な"),
        ("ni", "に"),
        ("nu", "ぬ"),
        ("ne", "ね"),
        ("no", "の"),
        ("ha", "は"),
        ("hi", "ひ"),
        ("hu", "ふ"),
        ("he", "へ"),
        ("ho", "ほ"),
        ("ma", "ま"),
        ("mi", "み"),
        ("mu", "む"),
        ("me", "め"),
        ("mo", "も"),
        ("ra", "ら"),
        ("ri", "り"),
        ("ru", "る"),
        ("re", "れ"),
        ("ro", "ろ"),
        ("ya", "や"),
        ("yi", "い"),
        ("yu", "ゆ"),
        ("ye", "いぇ"),
        ("yo", "よ"),
        ("wa", "わ"),
        ("wi", "ゐ"),
        ("wu", "う"),
        ("we", "ゑ"),
        ("wo", "を"),
        ("ga", "が"),
        ("gi", "ぎ"),
        ("gu", "ぐ"),
        ("ge", "げ"),
        ("go", "ご"),
        ("za", "ざ"),
        ("zi", "じ"),
        ("zu", "ず"),
        ("ze", "ぜ"),
        ("zo", "ぞ"),
        ("da", "だ"),
        ("di", "ぢ"),
        ("du", "づ"),
        ("de", "で"),
        ("do", "ど"),
        ("ba", "ば"),
        ("bi", "び"),
        ("bu", "ぶ"),
        ("be", "べ"),
        ("bo", "ぼ"),
        ("pa", "ぱ"),
        ("pi", "ぴ"),
        ("pu", "ぷ"),
        ("pe", "ぺ"),
        ("po", "ぽ"),
        ("ja", "じゃ"),
        ("ji", "じ"),
        ("ju", "じゅ"),
        ("je", "じぇ"),
        ("jo", "じょ"),
        ("fa", "ふぁ"),
        ("fi", "ふぃ"),
        ("fu", "ふ"),
        ("fe", "ふぇ"),
        ("fo", "ふぉ"),
        ("va", "ゔぁ"),
        ("vi", "ゔぃ"),
        ("vu", "ゔ"),
        ("ve", "ゔぇ"),
        ("vo", "ゔぉ"),
        // Single vowels
        ("a", "あ"),
        ("i", "い"),
        ("u", "う"),
        ("e", "え"),
        ("o", "お"),
    ];

    let lower = input.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let len = chars.len();
    let mut result = String::new();
    let mut i = 0;

    while i < len {
        // Handle double consonant → っ (e.g., "kk" → っ + continue with "k")
        if i + 1 < len
            && chars[i] == chars[i + 1]
            && chars[i].is_ascii_alphabetic()
            && chars[i] != 'a'
            && chars[i] != 'i'
            && chars[i] != 'u'
            && chars[i] != 'e'
            && chars[i] != 'o'
            && chars[i] != 'n'
        {
            result.push('っ');
            i += 1;
            continue;
        }

        // Handle 'n' before a consonant or end of string → ん
        if chars[i] == 'n' && i + 1 < len {
            let next = chars[i + 1];
            // 'n' followed by a consonant (not a vowel or 'y') → ん
            // This includes 'nn' → ん + continue from second n
            if next != 'a'
                && next != 'i'
                && next != 'u'
                && next != 'e'
                && next != 'o'
                && next != 'y'
            {
                result.push('ん');
                i += 1;
                continue;
            }
        }

        // Try longest match first (up to 4 chars)
        let mut matched = false;
        for try_len in (1..=4.min(len - i)).rev() {
            let slice: String = chars[i..i + try_len].iter().collect();
            if let Some((_, hira)) = ROMAJI_MAP.iter().find(|(rom, _)| *rom == slice.as_str()) {
                result.push_str(hira);
                i += try_len;
                matched = true;
                break;
            }
        }

        if !matched {
            // Handle trailing 'n' → ん
            if chars[i] == 'n' && i + 1 == len {
                result.push('ん');
            } else {
                // Pass through non-romaji characters (e.g., punctuation, already-hiragana)
                result.push(chars[i]);
            }
            i += 1;
        }
    }

    result
}

/// Normalize user input for reading comparison.
/// Converts romaji to hiragana, katakana to hiragana, and lowercases.
/// If the input is already hiragana/kanji, it passes through unchanged.
pub fn normalize_reading_input(input: &str) -> String {
    let trimmed = input.trim();

    // Check if input contains any ASCII letters (romaji)
    let has_ascii = trimmed.chars().any(|c| c.is_ascii_alphabetic());

    if has_ascii {
        // Convert romaji to hiragana, then normalize any remaining katakana
        let converted = romaji_to_hiragana(trimmed);
        katakana_to_hiragana(&converted)
    } else {
        // Convert katakana to hiragana (handles pure katakana or mixed input)
        katakana_to_hiragana(trimmed)
    }
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

// ─── Conjugation Grouping ────────────────────────────────────────────

/// Minimal token info needed for conjugation grouping (avoids circular dep with app.rs).
pub struct GroupToken<'a> {
    pub pos: &'a str,
    pub base_form: &'a str,
    /// Raw UniDic conjugation form, already translated to English (e.g. "irrealis (general)").
    pub conjugation_form: &'a str,
}

/// Result of conjugation group assignment for a single group.
#[derive(Debug, Clone)]
pub struct ConjugationGroup {
    /// Unique group index within the sentence.
    pub group_id: usize,
    /// Index of the head token (verb/adjective).
    pub head_index: usize,
    /// All token indices in this group (head + auxiliaries).
    pub member_indices: Vec<usize>,
    /// Human-readable conjugation description: "verb, negative, past".
    pub description: String,
}

/// Scan tokens left-to-right and group verb/adjective heads with following auxiliaries.
///
/// Rules:
/// - Start a new group when a Verb or Adjective token is found
/// - Continue adding tokens while the next token's POS is "Auxiliary"
/// - Stop at any non-Auxiliary token
///
/// Returns a list of groups. Standalone verbs/adjectives with no auxiliaries
/// still get a group (single-member) with a base description.
pub fn assign_conjugation_groups(tokens: &[GroupToken]) -> Vec<ConjugationGroup> {
    let mut groups = Vec::new();
    let mut group_id = 0usize;
    let mut i = 0;

    while i < tokens.len() {
        let tok = &tokens[i];

        // Only verbs and adjectives can be group heads
        if tok.pos == "Verb" || tok.pos == "Adjective" {
            let head_index = i;
            let mut members = vec![i];

            // Collect following Auxiliary tokens
            let mut j = i + 1;
            while j < tokens.len() && tokens[j].pos == "Auxiliary" {
                members.push(j);
                j += 1;
            }

            // Build human-readable description
            let desc = build_conjugation_description(tokens, &members);

            groups.push(ConjugationGroup {
                group_id,
                head_index,
                member_indices: members,
                description: desc,
            });

            group_id += 1;
            i = j; // skip past the group
        } else {
            i += 1;
        }
    }

    groups
}

/// Build a human-readable conjugation description from a group of tokens.
///
/// The first token is the head (verb/adjective). Remaining tokens are auxiliaries
/// whose `base_form` is mapped to English labels. The head's `conjugation_form`
/// may add additional info (imperative, conditional, volitional).
fn build_conjugation_description(tokens: &[GroupToken], members: &[usize]) -> String {
    if members.is_empty() {
        return String::new();
    }

    let head = &tokens[members[0]];

    // Start with POS label
    let pos_label = match head.pos {
        "Verb" => "verb",
        "Adjective" => "adjective",
        _ => "word",
    };

    let mut parts: Vec<&str> = vec![pos_label];

    // Add labels for each auxiliary based on its base_form
    for &idx in &members[1..] {
        let label = auxiliary_label(tokens[idx].base_form);
        if !label.is_empty() {
            parts.push(label);
        }
    }

    // Check the conjugation_form of the *last* token in the group for
    // additional inflection info (conditional, imperative, volitional).
    let last = &tokens[*members.last().unwrap()];
    let trailing_label = trailing_form_label(last.conjugation_form);
    if let Some(lbl) = trailing_label {
        // Avoid duplicating labels already added by auxiliaries
        if !parts.contains(&lbl) {
            parts.push(lbl);
        }
    }

    // Also check if head is in imperative form with no auxiliaries
    if members.len() == 1 {
        let head_trailing = trailing_form_label(head.conjugation_form);
        if let Some(lbl) = head_trailing {
            if !parts.contains(&lbl) {
                parts.push(lbl);
            }
        }
    }

    parts.join(", ")
}

/// Map an auxiliary's base_form to a human-readable English label.
fn auxiliary_label(base_form: &str) -> &'static str {
    match base_form {
        "ない" => "negative",
        "ます" => "polite",
        "た" | "だ" => "past",
        "て" | "で" => "te-form",
        "れる" | "られる" => "passive/potential",
        "せる" | "させる" => "causative",
        "たい" => "want to",
        "う" | "よう" => "volitional",
        "ぬ" => "negative (classical)",
        "ず" => "negative (literary)",
        "まい" => "negative volitional",
        "そう" => "looks like",
        "らしい" => "seems like",
        "べし" => "should (classical)",
        "です" => "copula (polite)",
        // "だ" is already covered as "past" above — when it's an auxiliary
        // after a verb it's typically the past tense marker (食べた=食べ+た,
        // 読んだ=読ん+だ). The copula だ after na-adj/nouns is a separate case.
        _ => "",
    }
}

/// Check the conjugation_form of the last group member for trailing inflection
/// info that should be appended to the description.
/// Handles both raw UniDic forms (Japanese) and translated forms (English).
fn trailing_form_label(conjugation_form: &str) -> Option<&'static str> {
    // Translated English forms
    if conjugation_form.starts_with("conditional") {
        Some("conditional")
    } else if conjugation_form.starts_with("imperative") {
        Some("imperative")
    } else if conjugation_form.starts_with("volitional") {
        Some("volitional")
    }
    // Raw UniDic Japanese forms
    else if conjugation_form.starts_with("仮定形") {
        Some("conditional")
    } else if conjugation_form.starts_with("命令形") {
        Some("imperative")
    } else if conjugation_form.starts_with("意志推量形") {
        Some("volitional")
    } else {
        None
    }
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
    fn test_romaji_to_hiragana_basic_vowels() {
        assert_eq!(romaji_to_hiragana("a"), "あ");
        assert_eq!(romaji_to_hiragana("i"), "い");
        assert_eq!(romaji_to_hiragana("u"), "う");
        assert_eq!(romaji_to_hiragana("e"), "え");
        assert_eq!(romaji_to_hiragana("o"), "お");
    }

    #[test]
    fn test_romaji_to_hiragana_consonant_vowel() {
        assert_eq!(romaji_to_hiragana("ka"), "か");
        assert_eq!(romaji_to_hiragana("ki"), "き");
        assert_eq!(romaji_to_hiragana("ku"), "く");
        assert_eq!(romaji_to_hiragana("ke"), "け");
        assert_eq!(romaji_to_hiragana("ko"), "こ");
        assert_eq!(romaji_to_hiragana("sa"), "さ");
        assert_eq!(romaji_to_hiragana("ta"), "た");
        assert_eq!(romaji_to_hiragana("na"), "な");
        assert_eq!(romaji_to_hiragana("ha"), "は");
        assert_eq!(romaji_to_hiragana("ma"), "ま");
        assert_eq!(romaji_to_hiragana("ra"), "ら");
        assert_eq!(romaji_to_hiragana("ya"), "や");
        assert_eq!(romaji_to_hiragana("wa"), "わ");
    }

    #[test]
    fn test_romaji_to_hiragana_words() {
        assert_eq!(romaji_to_hiragana("taberu"), "たべる");
        assert_eq!(romaji_to_hiragana("nihongo"), "にほんご");
        assert_eq!(romaji_to_hiragana("arigatou"), "ありがとう");
        assert_eq!(romaji_to_hiragana("ohayou"), "おはよう");
        assert_eq!(romaji_to_hiragana("konnichiwa"), "こんにちわ");
    }

    #[test]
    fn test_romaji_to_hiragana_special() {
        // shi/chi/tsu digraphs
        assert_eq!(romaji_to_hiragana("shi"), "し");
        assert_eq!(romaji_to_hiragana("chi"), "ち");
        assert_eq!(romaji_to_hiragana("tsu"), "つ");
        assert_eq!(romaji_to_hiragana("fu"), "ふ");
        // Combo digraphs
        assert_eq!(romaji_to_hiragana("sha"), "しゃ");
        assert_eq!(romaji_to_hiragana("cho"), "ちょ");
        assert_eq!(romaji_to_hiragana("nya"), "にゃ");
    }

    #[test]
    fn test_romaji_to_hiragana_double_consonant() {
        assert_eq!(romaji_to_hiragana("kitte"), "きって");
        assert_eq!(romaji_to_hiragana("gakkou"), "がっこう");
        assert_eq!(romaji_to_hiragana("kekkon"), "けっこん");
    }

    #[test]
    fn test_romaji_to_hiragana_n() {
        // n before consonant → ん
        assert_eq!(romaji_to_hiragana("sanpo"), "さんぽ");
        assert_eq!(romaji_to_hiragana("sensei"), "せんせい");
        // nn → ん
        assert_eq!(romaji_to_hiragana("onna"), "おんな");
        // n at end → ん
        assert_eq!(romaji_to_hiragana("nihon"), "にほん");
    }

    #[test]
    fn test_romaji_to_hiragana_case_insensitive() {
        assert_eq!(romaji_to_hiragana("Taberu"), "たべる");
        assert_eq!(romaji_to_hiragana("NIHON"), "にほん");
    }

    #[test]
    fn test_normalize_reading_input_romaji() {
        assert_eq!(normalize_reading_input("taberu"), "たべる");
        assert_eq!(normalize_reading_input("  taberu  "), "たべる");
    }

    #[test]
    fn test_normalize_reading_input_katakana() {
        assert_eq!(normalize_reading_input("タベル"), "たべる");
    }

    #[test]
    fn test_normalize_reading_input_hiragana_passthrough() {
        assert_eq!(normalize_reading_input("たべる"), "たべる");
    }

    #[test]
    fn test_normalize_reading_input_kanji_passthrough() {
        assert_eq!(normalize_reading_input("食べる"), "食べる");
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

    // ─── Conjugation Grouping Tests ───

    fn make_group_token<'a>(
        pos: &'a str,
        base_form: &'a str,
        conj_form: &'a str,
    ) -> GroupToken<'a> {
        GroupToken {
            pos,
            base_form,
            conjugation_form: conj_form,
        }
    }

    #[test]
    fn test_grouping_verb_negative() {
        // 食べない → 食べ(Verb) + ない(Auxiliary)
        let tokens = vec![
            make_group_token("Verb", "食べる", "continuative (general)"),
            make_group_token("Auxiliary", "ない", "terminal (general)"),
        ];
        let groups = assign_conjugation_groups(&tokens);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].head_index, 0);
        assert_eq!(groups[0].member_indices, vec![0, 1]);
        assert_eq!(groups[0].description, "verb, negative");
    }

    #[test]
    fn test_grouping_verb_polite_past() {
        // 食べました → 食べ(Verb) + まし(Aux:ます) + た(Aux)
        let tokens = vec![
            make_group_token("Verb", "食べる", "continuative (general)"),
            make_group_token("Auxiliary", "ます", "continuative (general)"),
            make_group_token("Auxiliary", "た", "terminal (general)"),
        ];
        let groups = assign_conjugation_groups(&tokens);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].member_indices, vec![0, 1, 2]);
        assert_eq!(groups[0].description, "verb, polite, past");
    }

    #[test]
    fn test_grouping_verb_causative_want() {
        // 食べさせたい → 食べ(Verb) + させ(Aux) + たい(Aux)
        let tokens = vec![
            make_group_token("Verb", "食べる", "continuative (general)"),
            make_group_token("Auxiliary", "させる", "continuative (general)"),
            make_group_token("Auxiliary", "たい", "terminal (general)"),
        ];
        let groups = assign_conjugation_groups(&tokens);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].description, "verb, causative, want to");
    }

    #[test]
    fn test_grouping_stops_at_particle() {
        // 食べて + いる → two separate groups because て is Particle
        let tokens = vec![
            make_group_token("Verb", "食べる", "continuative (general)"),
            make_group_token("Particle", "て", ""),
            make_group_token("Verb", "いる", "terminal (general)"),
        ];
        let groups = assign_conjugation_groups(&tokens);
        assert_eq!(
            groups.len(),
            2,
            "Should be two groups separated by particle"
        );
        assert_eq!(groups[0].member_indices, vec![0]);
        assert_eq!(groups[1].member_indices, vec![2]);
    }

    #[test]
    fn test_grouping_standalone_verb() {
        // 食べる (dictionary form, no auxiliaries)
        let tokens = vec![
            make_group_token("Verb", "食べる", "terminal (general)"),
            make_group_token("Symbol", "。", ""),
        ];
        let groups = assign_conjugation_groups(&tokens);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].member_indices, vec![0]);
        assert_eq!(groups[0].description, "verb");
    }

    #[test]
    fn test_grouping_conditional_form() {
        // 行かなけれ → 行か(Verb) + なけれ(Aux:ない, conditional form)
        let tokens = vec![
            make_group_token("Verb", "行く", "irrealis (general)"),
            make_group_token("Auxiliary", "ない", "conditional (general)"),
        ];
        let groups = assign_conjugation_groups(&tokens);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].description, "verb, negative, conditional");
    }

    #[test]
    fn test_grouping_imperative() {
        // 食べろ — single verb in imperative form
        let tokens = vec![make_group_token("Verb", "食べる", "imperative (general)")];
        let groups = assign_conjugation_groups(&tokens);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].description, "verb, imperative");
    }

    #[test]
    fn test_grouping_adjective() {
        // 美しくない → 美しく(Adj) + ない(Aux)
        let tokens = vec![
            make_group_token("Adjective", "美しい", "continuative (general)"),
            make_group_token("Auxiliary", "ない", "terminal (general)"),
        ];
        let groups = assign_conjugation_groups(&tokens);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].description, "adjective, negative");
    }

    #[test]
    fn test_grouping_multiple_groups() {
        // 食べない猫が走った → verb-group1, Noun, Particle, verb-group2
        let tokens = vec![
            make_group_token("Verb", "食べる", "continuative (general)"),
            make_group_token("Auxiliary", "ない", "attributive (general)"),
            make_group_token("Noun", "猫", ""),
            make_group_token("Particle", "が", ""),
            make_group_token("Verb", "走る", "continuative (general)"),
            make_group_token("Auxiliary", "た", "terminal (general)"),
        ];
        let groups = assign_conjugation_groups(&tokens);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].member_indices, vec![0, 1]);
        assert_eq!(groups[0].description, "verb, negative");
        assert_eq!(groups[1].member_indices, vec![4, 5]);
        assert_eq!(groups[1].description, "verb, past");
    }

    #[test]
    fn test_auxiliary_label_coverage() {
        assert_eq!(auxiliary_label("ない"), "negative");
        assert_eq!(auxiliary_label("ます"), "polite");
        assert_eq!(auxiliary_label("た"), "past");
        assert_eq!(auxiliary_label("だ"), "past");
        assert_eq!(auxiliary_label("れる"), "passive/potential");
        assert_eq!(auxiliary_label("られる"), "passive/potential");
        assert_eq!(auxiliary_label("せる"), "causative");
        assert_eq!(auxiliary_label("させる"), "causative");
        assert_eq!(auxiliary_label("たい"), "want to");
        assert_eq!(auxiliary_label("う"), "volitional");
        assert_eq!(auxiliary_label("よう"), "volitional");
        assert_eq!(auxiliary_label("ぬ"), "negative (classical)");
        assert_eq!(auxiliary_label("ず"), "negative (literary)");
        assert_eq!(auxiliary_label("まい"), "negative volitional");
        assert_eq!(auxiliary_label("そう"), "looks like");
        assert_eq!(auxiliary_label("らしい"), "seems like");
        assert_eq!(auxiliary_label("べし"), "should (classical)");
        assert_eq!(auxiliary_label("です"), "copula (polite)");
        assert_eq!(auxiliary_label("unknown"), "");
    }

    /// Helper: tokenize text and run grouping, returning groups with their descriptions
    fn tokenize_and_group(text: &str) -> (Vec<TokenInfo>, Vec<ConjugationGroup>) {
        let tokens = tokenize_sentence(text).unwrap();
        let group_tokens: Vec<GroupToken> = tokens
            .iter()
            .map(|t| GroupToken {
                pos: &t.pos,
                base_form: &t.base_form,
                conjugation_form: &t.conjugation_form,
            })
            .collect();
        let groups = assign_conjugation_groups(&group_tokens);
        (tokens, groups)
    }

    #[test]
    fn test_grouping_real_tabenai() {
        // 食べない → verb, negative
        let (tokens, groups) = tokenize_and_group("食べない");
        assert!(
            !groups.is_empty(),
            "食べない should have at least one group"
        );
        assert!(
            groups[0].member_indices.len() >= 2,
            "食べない should group verb + auxiliary, tokens: {:?}",
            tokens
                .iter()
                .map(|t| (&t.surface, &t.pos))
                .collect::<Vec<_>>()
        );
        assert!(
            groups[0].description.contains("negative"),
            "Expected 'negative' in description, got: {}",
            groups[0].description
        );
    }

    #[test]
    fn test_grouping_real_tabemasu() {
        // 食べます → verb, polite
        let (_tokens, groups) = tokenize_and_group("食べます");
        assert!(!groups.is_empty());
        assert!(
            groups[0].description.contains("polite"),
            "Expected 'polite' in description, got: {}",
            groups[0].description
        );
    }

    #[test]
    fn test_grouping_real_taberareru() {
        // 食べられる → verb, passive/potential
        let (_tokens, groups) = tokenize_and_group("食べられる");
        assert!(!groups.is_empty());
        assert!(
            groups[0].description.contains("passive/potential"),
            "Expected 'passive/potential' in description, got: {}",
            groups[0].description
        );
    }

    #[test]
    fn test_grouping_real_tabeyou() {
        // 食べよう → verb, volitional
        let (_tokens, groups) = tokenize_and_group("食べよう");
        assert!(!groups.is_empty());
        assert!(
            groups[0].description.contains("volitional"),
            "Expected 'volitional' in description, got: {}",
            groups[0].description
        );
    }

    #[test]
    fn test_grouping_real_taberu() {
        // 食べる → standalone verb (dictionary form)
        let (_tokens, groups) = tokenize_and_group("食べる");
        assert!(!groups.is_empty());
        assert_eq!(
            groups[0].description, "verb",
            "食べる (dictionary form) should just be 'verb'"
        );
    }

    #[test]
    fn test_grouping_sample_sentence() {
        // From sample_hard.txt: 食べない。食べます。食べる。食べられる。食べよう。
        let full = "食べない。食べます。食べる。食べられる。食べよう。";
        let (tokens, groups) = tokenize_and_group(full);

        // Print debug info
        let token_info: Vec<_> = tokens
            .iter()
            .map(|t| format!("{}({}:{})", t.surface, t.pos, t.base_form))
            .collect();

        assert!(
            groups.len() >= 5,
            "Expected at least 5 verb groups, got {} from tokens: {:?}",
            groups.len(),
            token_info
        );

        // Check that we found negative, polite, passive/potential, and volitional
        let descriptions: Vec<&str> = groups.iter().map(|g| g.description.as_str()).collect();
        assert!(
            descriptions.iter().any(|d| d.contains("negative")),
            "Should find 'negative' among: {:?}",
            descriptions
        );
        assert!(
            descriptions.iter().any(|d| d.contains("polite")),
            "Should find 'polite' among: {:?}",
            descriptions
        );
        assert!(
            descriptions.iter().any(|d| d.contains("passive/potential")),
            "Should find 'passive/potential' among: {:?}",
            descriptions
        );
    }
}
