# kotoba — Remaining Work

Everything below is **not yet implemented**. For a description of what the app currently does, see `DESCRIPTION.md`.

Completed phases: 1 (Core DB + Tokenizer), 2 (Reader + SRS), 3 (Import Sources), 4 (Library + Card Browser + Settings), 5 (LLM Integration), 5.5 (Home Activity Dashboard), 6 (Stats Screen), 7 (Theming + Configuration — partial).

---

## Phase 7 — Polish (Remaining)

**Goal**: Finish the last two items from the original Phase 7.

### 7.3 Loading States
- [ ] Global spinner overlay for any blocking operation > 200ms
  - Track `app.loading: Option<&str>` with a message like "Importing…", "Loading dictionary…"
  - Render a centered braille spinner + message when `loading` is `Some`
  - Wrap long-running sync operations (dict import, text import from CLI path, chapter open) with loading state
  - The background importer already has per-chapter spinners — this is for the remaining sync paths

### 7.4 Man Page Generation
- [ ] Restructure crate into `lib.rs` + `bin/main.rs` so `clap_mangen` can generate man pages at build time
  - Move all modules under `lib.rs` re-exports
  - `build.rs` generates man pages into `target/man/`
  - Add `kotoba.1` installation instructions to README

---

## Phase 8 — AnkiConnect Export

**Goal**: One-way sync to push vocabulary cards to Anki via the AnkiConnect local API.

### 8.1 AnkiConnect Client
- [ ] Implement `core/anki.rs`:
  - `AnkiConnectClient` struct wrapping HTTP calls to `http://localhost:8765`
  - Actions: `version`, `deckNames`, `modelNames`, `modelFieldNames`, `addNote`, `addNotes`, `findNotes`, `notesInfo`
  - Error handling: connection refused (Anki not running), permission denied (user must click Allow), invalid model
  - Configurable deck name and note type in `[anki]` TOML section

### 8.2 Note Mapping
- [ ] Define default note type mapping:
  - **Word cards** → fields: `Word`, `Reading`, `Meaning`, `Sentence`, `SentenceTranslation`, `Notes`
  - **Sentence cards** → fields: `Sentence`, `Translation`, `Notes`
  - Allow custom field mapping in config: `[anki.field_map]` section
- [ ] Duplicate detection: query AnkiConnect for existing notes by word/sentence before adding
- [ ] Tag strategy: tag with `kotoba`, source text title, vocabulary status

### 8.3 Export UI
- [ ] `kotoba export anki` CLI command — batch export all Learning/Known cards not yet synced
  - Track `anki_note_id` in `srs_cards` table (nullable, set after successful push)
  - `--dry-run` flag to preview what would be exported
  - `--deck <name>` override
- [ ] TUI integration: `A` key in Card Browser to export selected card, `Ctrl+A` to batch export filtered set
- [ ] Export summary: cards added, skipped (duplicate), failed

---

## Phase 9 — Pitch Accent Data

**Goal**: Display pitch accent patterns for words using accent dictionary data.

### 9.1 Accent Dictionary
- [ ] Data source: bundled OJAD-style accent CSV or NHK accent dictionary (evaluate licensing)
  - Fallback: parse Kanjium pitch accent data (CC-BY, widely used in Anki community)
  - Schema: `pitch_accents` table with `reading TEXT, accent_pattern TEXT, source TEXT`
  - `kotoba setup-pitch` CLI command to download and import accent data
- [ ] Lookup function: `get_pitch_accent(conn, reading) -> Vec<PitchAccent>`
  - Handle multiple accent patterns per word (common in Japanese)
  - Match by reading (hiragana), not surface form

### 9.2 Pitch Accent Display
- [ ] Visual rendering in `ui/components/pitch.rs`:
  - Horizontal pitch diagram: ＼ for downstep, lines for high/low mora
  - Color-coded: high mora in one color, low mora in another
  - Accent type label: 平板 (heiban), 頭高 (atamadaka), 中高 (nakadaka), 尾高 (odaka)
- [ ] Integration points:
  - Reader sidebar: show pitch accent below word reading
  - WordDetail popup: pitch accent section with diagram
  - Review cards: optional pitch accent display (configurable)
- [ ] Theme colors: `pitch_high`, `pitch_low`, `pitch_downstep` added to Theme struct

---

## Phase 10 — Audio & TTS

**Goal**: Add pronunciation audio for words and sentences.

### 10.1 TTS Backend
- [ ] Pluggable TTS backend in `core/audio.rs`:
  - **System TTS**: Use `say` (macOS), `espeak-ng` (Linux), or `SAPI` (Windows) via `std::process::Command`
  - **Cloud TTS**: Optional OpenAI TTS API (`/v1/audio/speech`) or Google Cloud TTS
  - Backend selection in `[audio]` TOML section: `backend = "system"` | `"openai"` | `"none"`
- [ ] Audio caching: store generated audio in `$XDG_CACHE_HOME/kotoba/audio/` keyed by hash of text + voice
  - Cache lookup before TTS call
  - `kotoba cache clear --audio` to clear audio cache

### 10.2 Playback Integration
- [ ] Cross-platform audio playback via `rodio` crate (lightweight, pure Rust)
  - Play in background thread, non-blocking
  - Queue system: if already playing, queue next clip
- [ ] Keybindings:
  - Reader: `p` play current sentence, `Ctrl+P` play selected word
  - Review: `p` play card's sentence, auto-play on card reveal (configurable)
- [ ] Settings: `[audio]` section with `auto_play_review`, `voice` (for cloud TTS), `speed` (0.5-2.0)

---

## Phase 11 — Additional Web Sources

**Goal**: Expand content import with structured web source support.

### 11.1 Source Framework
- [ ] Generalize the Syosetu importer into a pluggable source trait:
  ```rust
  trait WebSource {
      fn name(&self) -> &str;
      fn matches_url(&self, url: &str) -> bool;
      fn fetch_metadata(&self, url: &str) -> Result<SourceMetadata>;
      fn fetch_chapter_list(&self, url: &str) -> Result<Vec<ChapterInfo>>;
      fn fetch_chapter_text(&self, url: &str) -> Result<String>;
  }
  ```
- [ ] Source registry: `Vec<Box<dyn WebSource>>` checked in order for URL matching
- [ ] Existing Syosetu importer refactored to implement the trait

### 11.2 NHK News Easy
- [ ] `import/nhk.rs`: Fetch article list from NHK News Easy API
  - Article metadata: title, date, category, has_audio flag
  - Ruby annotations available in HTML (extract furigana directly)
  - Article list browsable in a dedicated TUI screen or via import menu
- [ ] `kotoba nhk` CLI subcommand: list recent articles, import by ID or URL

### 11.3 Aozora Bunko
- [ ] `import/aozora.rs`: Import from Aozora Bunko (public domain Japanese literature)
  - Parse Aozora-specific HTML formatting (ruby, notes, annotations)
  - Author + title metadata extraction
  - `kotoba aozora <url>` CLI subcommand

### 11.4 Generic RSS/Atom
- [ ] `import/rss.rs`: Subscribe to Japanese RSS/Atom feeds
  - Feed management: add, remove, list feeds in config
  - Periodic refresh (manual, not background daemon)
  - `kotoba feed add <url>`, `kotoba feed list`, `kotoba feed refresh`
  - New articles appear in Library with feed source metadata

---

## Phase 12 — PDF Import

**Goal**: Extract readable text from PDF documents.

### 12.1 PDF Text Extraction
- [ ] Implement `import/pdf.rs`:
  - Use `pdf-extract` or `lopdf` crate for text layer extraction
  - Handle CJK font encoding (common issue with Japanese PDFs)
  - Preserve paragraph structure where possible (use vertical spacing heuristics)
  - Fallback: warn user if PDF has no text layer (scanned image)
- [ ] Page-based chapter splitting:
  - Default: one chapter per N pages (configurable, default 10)
  - Optional: split at detected headings or page breaks
  - `kotoba import file.pdf --pages-per-chapter 5`

### 12.2 Vertical Text Handling
- [ ] Detect vertical text layout in PDF metadata
  - Re-order extracted text for horizontal reading flow
  - Column detection for multi-column layouts
- [ ] User override: `--layout horizontal` | `vertical` flag on import

---

## Phase 13 — Cloud Sync

**Goal**: Optional sync of vocabulary and SRS state across devices.

### 13.1 Sync Protocol
- [ ] Design a lightweight sync format:
  - Export: `kotoba sync export` → JSON snapshot of vocabulary + SRS cards + review logs + settings
  - Import: `kotoba sync import <file>` → merge into local DB
  - Conflict resolution: last-write-wins by `updated_at` timestamp per row
  - Add `updated_at TIMESTAMP` and `sync_id UUID` columns to vocabulary, srs_cards, srs_reviews

### 13.2 Sync Backend
- [ ] File-based sync (MVP): export/import JSON files manually (Dropbox, Google Drive, USB)
  - `kotoba sync export --output ~/Dropbox/kotoba-sync.json`
  - `kotoba sync import ~/Dropbox/kotoba-sync.json`
  - Merge log: show what was added/updated/conflicted
- [ ] Optional remote backend (future): simple REST API or S3-compatible object storage
  - `[sync]` TOML section: `backend = "file"` | `"s3"`, `endpoint`, `bucket`, `api_key`
  - `kotoba sync push` / `kotoba sync pull`

### 13.3 Selective Sync
- [ ] Choose what to sync: vocabulary only, SRS state, review history, settings
  - `kotoba sync export --include vocab,srs` flag
- [ ] Sync status indicator on home screen: last sync time, pending changes count

---

## Phase 14 — Multi-Language Support

**Goal**: Generalize beyond Japanese to support Chinese, Korean, and potentially other languages.

### 14.1 Tokenizer Abstraction
- [ ] Define `Tokenizer` trait:
  ```rust
  trait Tokenizer {
      fn tokenize(&self, text: &str) -> Vec<Token>;
      fn language(&self) -> Language;
  }
  ```
- [ ] Refactor current lindera/UniDic tokenizer to implement the trait
- [ ] Language enum: `Japanese`, `Chinese`, `Korean`, `Other`
- [ ] Per-text language tag stored in `texts` table

### 14.2 Chinese Support
- [ ] `jieba-rs` integration for Chinese word segmentation
  - POS tagging via jieba's built-in POS tagger
  - No furigana equivalent (pinyin shown in sidebar instead)
- [ ] Dictionary: CC-CEDICT (Chinese-English dictionary, similar format to JMdict)
  - `kotoba setup-dict --language chinese` to download and import
  - Pinyin display in sidebar and word detail popup

### 14.3 Korean Support
- [ ] Korean tokenizer: evaluate `mecab-ko` bindings or pure-Rust alternatives
  - Hangul syllable decomposition for reading display
- [ ] Dictionary: KENGDIC or similar Korean-English dictionary
  - `kotoba setup-dict --language korean`

### 14.4 UI Adaptations
- [ ] Language-aware furigana: Japanese (hiragana above kanji), Chinese (pinyin), Korean (none)
- [ ] Language-aware sentence splitting rules per language
- [ ] Settings: default language selection, per-text language override

---

## Phase 15 — Plugin System

**Goal**: User-extensible import sources and LLM prompt templates.

### 15.1 Import Plugins
- [ ] Plugin directory: `$XDG_CONFIG_HOME/kotoba/plugins/`
- [ ] Plugin format: TOML manifest + Lua script (via `mlua` crate)
  ```toml
  # plugin.toml
  name = "pixiv-novels"
  version = "0.1.0"
  type = "import"
  url_pattern = "https://www.pixiv.net/novel/show.php\\?id=.*"
  ```
  ```lua
  -- import.lua
  function fetch(url)
      local html = http.get(url)
      return { title = extract_title(html), text = extract_text(html) }
  end
  ```
- [ ] Sandboxed Lua environment: HTTP access (GET only), string manipulation, JSON parsing
  - No filesystem access, no command execution
- [ ] `kotoba plugin list`, `kotoba plugin install <path>`, `kotoba plugin remove <name>`

### 15.2 LLM Prompt Templates
- [ ] Custom system prompts stored in `$XDG_CONFIG_HOME/kotoba/prompts/`
  - TOML format: name, description, system_prompt, expected output fields
  - `[llm] prompt_template = "grammar-focus"` in config to select active template
- [ ] Built-in templates:
  - `default` — current translation + breakdown + explanation
  - `grammar-focus` — emphasize grammar point identification
  - `beginner` — simpler explanations, more romaji
  - `advanced` — nuanced usage notes, register analysis
- [ ] Template selection in Settings screen or via `Ctrl+T` menu in Reader
