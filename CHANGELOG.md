# Changelog

## [0.1.0] - 2025-03-10

Initial release.

### Added

#### Core
- Japanese text tokenization with lindera/UniDic (morphological analysis,
  sentence splitting, conjugation grouping, multi-word expression detection)
- JMdict dictionary integration (~200k entries) with `setup-dict` auto-download
  and `import-dict` manual import
- SQLite database with 20 versioned migrations for vocabulary, texts, SRS cards,
  review history, LLM cache, and activity tracking
- TOML configuration with `~/.config/kotoba/kotoba.toml`

#### Import
- Plain text files (`.txt`)
- EPUB books with per-chapter navigation
- SRT, ASS, and SSA subtitle files
- Web URLs with article content extraction
- Syosetu (syosetu.com) web novels with paginated chapter lists
- System clipboard
- Background import with 3-thread worker pool and progress reporting

#### Reader
- Sentence-by-sentence reading with furigana above kanji
- Sidebar with word readings, POS, JMdict definitions, conjugation descriptions
- Vocabulary status tracking: New, Learning (1-4), Known, Ignored
- Auto-promotion of New words to Known on sentence advance (toggle with `a`,
  undo with `Ctrl+Z`)
- Expression marking mode for multi-word phrases
- Word and sentence translation
- Clipboard copy (word and sentence)
- Browser lookup via Jisho.org and DeepL/Google Translate
- Auto-advance to next chapter at end of text

#### Spaced Repetition
- FSRS scheduling engine with word recall and sentence cloze card types
- Optional typed reading input
- Configurable new cards per day, review cap, and cloze ratio
- Card browser with filtering (state, type) and sorting (due date, created, word)
- Session summary with accuracy stats

#### LLM Integration
- Sentence analysis via any OpenAI-compatible API (OpenRouter, Ollama, LM Studio)
- Structured breakdown: translation, component analysis, grammar explanation
- Response caching with SHA-256 deduplication
- Accessible from Reader (`Ctrl+T`) and Review (`Ctrl+L`)

#### TUI
- Home screen with activity heatmap (26 weeks), stats panel, and recently read list
- Library with sort (date, title, completion), filter (source type), and search
- Chapter select with reading state indicators and skip/preprocess controls
- Settings editor with live preview
- Stats dashboard with vocabulary growth chart, status breakdown, and per-text coverage
- 4 built-in themes: tokyo-night, light, solarized-light, gruvbox
- Custom theme support via TOML files in `~/.local/share/kotoba/themes/`
- Automatic color downgrade for 256-color and 16-color terminals
- Mouse support

#### CLI
- `kotoba run` -- launch TUI
- `kotoba import` -- import text, EPUB, subtitles, URLs, or clipboard
- `kotoba syosetu` -- import Syosetu novels
- `kotoba setup-dict` / `import-dict` -- dictionary management
- `kotoba dict` -- dictionary lookup
- `kotoba tokenize` -- debug tokenization
- `kotoba cache stats` / `clear` -- LLM cache management
- `kotoba config` -- show configuration

#### CI/CD
- GitHub Actions: check, test, clippy, format
- Cross-platform builds: Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64
- Automated GitHub Releases on version tags

[0.1.0]: https://github.com/youruser/kotoba/releases/tag/v0.1.0
