# kotoba - Terminal Language Learning App

A terminal-based language learning app for Japanese, inspired by LingQ and Lute.
Built with Rust, Ratatui, SQLite, and Sudachi.

---

## Architecture Overview

```text
+-------------------------------------------------------------+
|                     Ratatui TUI Event Loop                  |
|  +-------------------------------------------------------+  |
|  | Input (crossterm) -> App State Update -> Render Frame |  |
|  +-------------------------------------------------------+  |
|        |          |            |            |          |     |
|  +------+  +---------+  +----------+  +--------+  +------+ |
|  | Home |  | Library |  | Chapter  |  | Reader |  |Review| |
|  |      |  |         |  | Select   |  |        |  |      | |
|  +------+  +---------+  +----------+  +--------+  +------+ |
|      \         |    Esc↑  ↓Enter  Esc↑   ↑  Tab↕          |
|       `---Esc--'                         Tab               |
|  +-------------------------------------------------------+  |
|  |       BackgroundImporter (thread pool, mpsc)          |  |
|  +-------------------------------------------------------+  |
+-------------------------------------------------------------+
|                      Application Layer                      |
|  +----------+ +-----------+ +------------+ +-----------+    |
|  | Importer | | lindera  | |  fsrs-rs   | | JMdict DB |    |
|  +----------+ +-----------+ +------------+ +-----------+    |
|  +---------------------+ +-----------------------------+    |
|  | tokio Async LLM API | |    AnkiConnect API          |    |
|  +---------------------+ +-----------------------------+    |
+-------------------------------------------------------------+
|  +------------+   +------------------------------------+    |
|  | rusqlite   |   | reqwest (HTTP)                     |    |
|  +------------+   +------------------------------------+    |
+-------------------------------------------------------------+
```

## Tech Stack

| Component            | Choice                  | Rationale                                                      |
| -------------------- | ----------------------- | -------------------------------------------------------------- |
| Language             | Rust                    | Native performance, single binary, strict type safety          |
| TUI Framework        | `ratatui`               | Immediate-mode UI, industry standard for modern Rust TUIs      |
| Terminal Backend     | `crossterm`             | Cross-platform input handling and terminal manipulation        |
| CLI Framework        | `clap`                  | Standard declarative CLI argument parser                       |
| Async Runtime        | `tokio`                 | Handles concurrent HTTP requests (Scraping, LLMs, APIs)        |
| Tokenizer            | `lindera` + UniDic      | Native Rust morphological analyzer with modern UniDic dictionary (maintained by NINJAL) |
| Dictionary           | JMdict (XML -> SQLite)  | Standard JP-EN dictionary, free, comprehensive                 |
| SRS Algorithm        | `fsrs`                  | Official Rust crate for the FSRS-5 spaced repetition algorithm |
| Database             | `rusqlite`              | Ergonomic and safe synchronous wrapper around SQLite3          |
| HTTP / API           | `reqwest`               | Standard HTTP client for LLM and AnkiConnect                   |
| Configuration        | `serde` + `toml`        | App settings and custom UI themes                              |

## Key Dependencies (`Cargo.toml`)

```toml
[dependencies]
# TUI / CLI
ratatui = "0.26"
crossterm = "0.27"
clap = { version = "4.5", features = ["derive"] }
unicode-width = "0.1"        # Crucial for calculating CJK cell widths in terminals

# Core / Async
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"

# Data / DB
rusqlite = { version = "0.31", features = ["bundled"] }
quick-xml = "0.31"           # For parsing JMdict XML efficiently

# Domain specific
lindera = { version = "2.2", features = ["lindera-unidic", "embed-unidic"] }  # Morphological analyzer with UniDic
fsrs = "5.2"                 # Spaced repetition engine (FSRS-5)
strsim = "0.11"              # Fuzzy string matching (Levenshtein) for typed reviews

# Utilities
anyhow = "1.0"               # Error handling
sha2 = "0.10"                # Hashing for LLM cache
chrono = { version = "0.4", features = ["serde"] }
indicatif = "0.17"           # Progress bars
dirs = "5.0"                 # XDG-compliant directory paths
```

## Project Structure

```text
kotoba/
├── Cargo.toml
├── build.rs                   # Build script (e.g., placing default configs/DBs)
├── src/
│   ├── main.rs                # App entry point & CLI routing (clap)
│   ├── app.rs                 # Central TUI App State struct
│   ├── config.rs              # TOML parsing & Theme struct
│   ├── ui/
│   │   ├── mod.rs             # Ratatui render functions
│   │   ├── events.rs          # crossterm key/tick event loop
│   │   ├── components/
│   │   │   ├── furigana.rs    # String padding/alignment logic for CJK
│   │   │   ├── text_area.rs   # Scrollable paragraph rendering
│   │   │   └── sidebar.rs     # Dictionary / context panel
│   │   └── screens/
│   │       ├── home.rs        # Landing screen: recent texts, quick actions
│   │       ├── reader.rs      # Interactive text loop
│   │       ├── review.rs      # FSRS flashcard UI
│   │       ├── library.rs     # Source list (grouped by source)
│   │       └── chapter_select.rs  # Paginated chapter list for multi-chapter sources
│   ├── core/
│   │   ├── tokenizer.rs       # lindera/UniDic wrapper handling conjugations
│   │   ├── dictionary.rs      # SQLite JMdict lookups
│   │   ├── srs.rs             # fsrs-rs state machines and diffing
│   │   └── llm.rs             # tokio/reqwest client for OpenAI/Anthropic
│   ├── db/
│   │   ├── connection.rs      # rusqlite setup & pool
│   │   ├── schema.rs          # SQL migrations
│   │   └── models.rs          # Rust definitions mapped to DB tables
│   └── import/
│       ├── text.rs            # Raw text IO
│       └── web.rs             # HTTP + readability crate
└── data/
    └── JMdict_e.xml           # Seed data
```

<details>
<summary><strong>Database Schema (Click to expand)</strong></summary>

_The SQLite schema maps 1:1 to Rust `structs` using standard SQL._

*   **`texts`**: `id`, `title`, `source_url`, `source_type`, `content`, `language`, `created_at`
*   **`paragraphs`**: `id`, `text_id`, `position`, `content`
*   **`tokens`**: `id`, `paragraph_id`, `position`, `surface`, `base_form`, `reading`, `pos`, `conjugation_form`, `conjugation_type`
*   **`vocabulary`**: `id`, `base_form`, `reading`, `pos`, `status`, `notes`, `first_seen_at`, `updated_at`
*   **`conjugation_encounters`**: `id`, `vocabulary_id`, `surface`, `conjugation_form`, `conjugation_type`, `encounter_count`, `status`, `first_seen`, `updated`
*   **`srs_cards`**: `id`, `vocabulary_id`, `conjugation_id`, `answer_mode`, `due_date`, `stability`, `difficulty`, `reps`, `lapses`, `state`, `created_at`
*   **`srs_reviews`**: `id`, `card_id`, `reviewed_at`, `rating`, `elapsed_ms`, `typed_answer`, `answer_correct`
*   **`llm_cache`**: `id`, `request_type`, `request_hash`, `request_body`, `response`, `model`, `tokens_used`, `created_at`

</details>

## Domain Concepts

### Vocabulary Status (LingQ-style)

| Status    | Value | Meaning                                       | Hiragana | Color      |
| --------- | ----- | --------------------------------------------- | -------- | ---------- |
| New       | 0     | Never seen, first encounter                   | Show     | Blue bg    |
| Learning 1| 1     | Just started learning                         | Show     | Yellow bg  |
| Learning 2| 2     | Recognized but shaky                          | Show     | Lighter bg |
| Learning 3| 3     | Mostly know it                                | Show     | Lighter bg |
| Learning 4| 4     | Almost mastered                               | Hide     | Lighter bg |
| Known     | 5     | Fully acquired, no longer highlighted         | Hide     | Default    |
| Ignored   | -1    | Particles, proper nouns, skip from SRS/counts | Hide     | Default    |

- Words start as **New** (status 0) on first encounter during reading.
- User manually sets status via number keys `1`-`5` or `i` for Ignored.
- Any word set to status 1-4 (Learning) **automatically creates SRS cards** (both word and sentence cards).
- A word set to Known (5) or Ignored (-1) retires its SRS cards.

### SRS Card Types

Two types of cards are auto-generated:

1. **Word cards** — Created per vocabulary item when status is set to Learning (1-4).
2. **Sentence cards** — Created for all sentences where there is at least one word with Learning status. A sentence card is **retired from the review pool** when all vocabulary within it reach Known status.

### SRS Review Modes

Each card can be reviewed in one of these modes (configured per session or per card):

| Mode              | Front                          | Back / Action                                    |
| ----------------- | ------------------------------ | ------------------------------------------------ |
| Meaning recall    | Word + reading                 | JMdict definitions; user self-rates              |
| Reading recall    | Word (kanji only)              | Reading revealed; user self-rates                |
| Typed reading     | Word (kanji only)              | User types hiragana; fuzzy-matched via `strsim`  |
| Sentence cloze    | Sentence with word blanked out | User recalls the missing word                    |
| Sentence full     | Full sentence                  | User recalls the reading and translation         |

**Critical**: During all SRS review modes, the sentence context is displayed with vocabulary **color-coded by status** (same colors as Reader). For Sentence full mode words are only highlighted with appropriate color, no translation and reading is provided. Unknown words in the sentence are tappable/selectable to view their JMdict definitions — solving LingQ's problem of showing sentences without respecting known word state.

### Reader Navigation Model

```text
                 Main Reader (70%)                    Sidebar (30%)
┌─────────────────────────────────────────────────────────────────────────┐
│  kotoba — 吾輩は猫である (Chapter 1)                           [t]heme │
├───────────────────────────────────────────┬─────────────────────────────┤
│                                           │                             │
│  吾輩は猫である。名前はまだ無い。              │  Current Sentence:          │
│                                           │                             │
│  どこで生れたかとんと見当がつかぬ。             │    にんげん     みた             │
│                                           │   「人間」というものを見た。     │
│  何でも薄暗いじめじめした所でニャーニャー       │                             │
│  泣いていた事だけは記憶している。              │  ─────────────────────────  │
│                                           │                             │
│  吾輩はここで始めて人間というものを見た。        │  Unknown / Learning Words:  │
│                                           │                             │
│     にんげん     みた                      │  人間 「にんげん」             │
│  ▶「人間」というものを見た。◀                 │    noun — human, person      │
│                                           │                             │
│  しかもあとで聞くとそれは書生という            │  見た 「みた」               │
│  人間中で一番獰悪な種族であったそうだ。         │    verb (past) — saw, seen   │
│                                           │                             │
│                                           │                             │
├───────────────────────────────────────────┴─────────────────────────────┤
│  Sentence 9/42  ◂ ▸   │  New: 12  Learning: 5  Known: 340  │  3m read │
└─────────────────────────────────────────────────────────────────────────┘
```

- **Up/Down** — Move between sentences. Current sentence stays centered in main view.
- **Left/Right** — Select individual words within the current sentence.
- **Sidebar** — Shows the current sentence broken down with most common JMdict translation per word, what type of word it is, its conjugation and reading in hiragana.
- **Enter** — Opens a popup overlay with full dictionary entry (all meanings, common conjugation forms, pitch accent info — pitch accent deferred to later phases).
- **Number keys (1-5)** — Set selected word's vocabulary status.
- **`i`** — Mark word as Ignored.
- **`n`** — Add/edit a personal note on the word.
- **`l`** — Trigger LLM structured analysis of the current sentence.

### LLM Integration Model

- **Provider**: Any OpenAI-compatible API endpoint (covers OpenAI, Anthropic via proxy, Ollama, vLLM, etc.).
- **Primary use**: Structured sentence analysis triggered on demand (`l` key in Reader).
- **Output format**: JSON structured output containing:
  - Full sentence translation
  - Per-word breakdown (base form, reading, meaning in context)
  - Grammar patterns identified with explanation
  - Idioms/set phrases detected with explanation
- **Caching**: All LLM responses are cached in `llm_cache` table, keyed by `sha256(sentence + model)`. Repeat lookups hit cache instantly.
- **Async**: Requests run via `tokio::spawn`, results sent back to TUI via `mpsc` channel. Loading spinner shown in sidebar while waiting.

---

## Development Phases

### Phase 1 — Foundation (Core Library & CLI)

**Goal**: Ensure native Rust NLP and DB functionality works flawlessly before building UI.

#### 1.1 Project Scaffolding
- [x] `cargo init kotoba`, initialize project
- [x] Set up `clap` with subcommands: `import`, `tokenize`, `dict`, `import-dict`, `run`
- [x] Create module structure: `src/{app,config,ui/,core/,db/,import/}`
- [x] Add all dependencies to `Cargo.toml` (ratatui, crossterm, clap, tokio, rusqlite, lindera, etc.)
- [x] Create `config.rs` with TOML-based `AppConfig` struct (DB path, LLM endpoint, theme file path, XDG-compliant defaults)
- [x] Create a default `kotoba.toml` config file

#### 1.2 Database Layer
- [x] Implement `db/connection.rs`: `open_or_create(path)` function returning `rusqlite::Connection` (with WAL mode, foreign keys)
- [x] Implement `db/schema.rs`: Version-tracked migration system
  - Migration 001: Create `texts`, `paragraphs`, `tokens` tables
  - Migration 002: Create `vocabulary` table with status enum (New=0, 1-4=Learning, 5=Known, -1=Ignored)
  - Migration 003: Create `conjugation_encounters` table
  - Migration 004: Create `srs_cards` table with `card_type` (word/sentence), `answer_mode` enum, FSRS fields
  - Migration 005: Create `srs_reviews` table
  - Migration 006: Create `llm_cache` table with `request_hash` unique index
  - Migration 007: Create `jmdict_entries`, `jmdict_kanji`, `jmdict_readings` tables
- [x] Implement `db/models.rs`: Rust structs with `from_row` impls for each table
  - `Text`, `Paragraph`, `Token`, `Vocabulary`, `ConjugationEncounter`, `SrsCard`, `SrsReview`, `LlmCacheEntry`
  - `VocabularyStatus` enum: `New`, `Learning1`..`Learning4`, `Known`, `Ignored`
  - `CardType` enum: `Word`, `Sentence`
  - `AnswerMode` enum: `MeaningRecall`, `ReadingRecall`, `TypedReading`, `SentenceCloze`
- [x] Write CRUD functions for each model (insert, get_by_id, update_status, upsert, list_by_text, etc.)
- [x] Add indexes: `vocabulary(base_form, reading)`, `tokens(paragraph_id, position)`, `srs_cards(due_date)`, `llm_cache(request_hash)`

#### 1.3 NLP / Tokenizer Engine
- [x] Implement `core/tokenizer.rs`:
  - Initialize `lindera::tokenizer::Tokenizer` with embedded UniDic dictionary (unidic-mecab-2.1.2, maintained by NINJAL)
  - `tokenize_sentence(text: &str) -> Vec<TokenInfo>` — runs lindera in Normal mode
  - Extract from each morpheme: `surface`, `orthographic_base_form` (base form), `reading` (katakana -> hiragana conversion), `part_of_speech`
  - Map POS tags to simplified categories: Verb, Noun, Pronoun, Adjective, Adjectival_Noun, Adverb, Particle, Auxiliary, Conjunction, Symbol, Prefix, Suffix, Interjection, Other
  - Extract conjugation info: `conjugation_form` and `conjugation_type` from UniDic detail fields
  - Handle whitespace and punctuation tokens (preserve them for rendering but don't create vocabulary entries)
- [x] Implement paragraph splitting: `split_paragraphs(text: &str) -> Vec<String>`
- [x] Implement sentence splitting within paragraphs (split on `。`, `！`, `？`, `\n`)
- [x] CLI subcommand `kotoba tokenize "日本語の文章"` — prints tokens as a formatted table

#### 1.4 JMdict Dictionary
- [x] Implement `core/dictionary.rs`:
  - `import_jmdict(path: &Path, conn: &Connection)` — parses JMdict_e.xml using `quick-xml` event reader
  - Target schema for dictionary SQLite table:
    - `jmdict_entries`: `ent_seq` (int PK), `json_blob` (TEXT) — store full entry as JSON for flexible querying
    - `jmdict_kanji`: `entry_id`, `kanji_element` — for kanji form lookups
    - `jmdict_readings`: `entry_id`, `reading_element` — for kana form lookups
  - Index on `jmdict_kanji(kanji_element)` and `jmdict_readings(reading_element)`
  - `lookup(base_form: &str, reading: Option<&str>) -> Vec<DictEntry>` — search by kanji then by reading fallback
  - `DictEntry` struct: `kanji_forms`, `readings`, `senses` (each sense: `glosses: Vec<String>`, `pos: Vec<String>`, `misc: Vec<String>`)
  - Provide a `short_gloss(entry: &DictEntry) -> String` helper — returns the first English gloss of the first sense (for sidebar display)
- [x] CLI subcommand `kotoba dict "食べる"` — prints dictionary entries
- [x] Progress bar during JMdict import (~190k entries, takes a few seconds)

#### 1.5 Text Import Pipeline
- [x] Implement `import/text.rs`:
  - `import_file(path: &Path, conn: &Connection) -> Result<i64>` (returns text_id)
  - Read file -> create `texts` row -> split into paragraphs -> split into sentences -> tokenize each sentence -> bulk-insert `paragraphs` and `tokens`
  - For each token with a non-trivial POS (not punctuation/whitespace): upsert into `vocabulary` table (if base_form+reading doesn't exist, insert as New; if exists, leave status unchanged)
  - For each token: upsert into `conjugation_encounters` (increment `encounter_count`)
  - Wrap entire import in a single SQLite transaction for performance
- [x] CLI subcommand `kotoba import <file>` with progress output (paragraphs processed, tokens found, new vocabulary count)

#### 1.6 Verification & Tests
- [x] Unit tests for tokenizer: known input/output pairs for various conjugations, reading extraction, POS mapping
- [x] Unit tests for JMdict lookup: exact match, reading fallback, reading filter, missing entries
- [x] Integration test: import a sample text file, verify DB state (correct paragraph count, token positions, vocabulary entries, no duplicates)
- [ ] Benchmark: tokenize + import a medium text (~5000 chars) in < 2 seconds

---

### Phase 2 — TUI Architecture & Reader Mode

**Goal**: Get the Ratatui environment running with interactive Japanese text rendering.

#### 2.1 TUI Framework Setup
- [x] Implement `ui/events.rs`:
  - `EventLoop` struct wrapping a `crossterm` event stream
  - Tick rate: 60ms (for smooth UI updates and spinner animations)
  - `Event` enum: `Key(KeyEvent)`, `Tick`, `LlmResponse(LlmResult)`, `Resize(u16, u16)`
  - Spawn a background thread for `crossterm::event::read()`, forward events via `mpsc::channel`
- [x] Implement `app.rs` — central `App` struct:
  - `screen: Screen` enum: `Library`, `Reader`, `Review`, `Stats`
  - `db: Connection` (owned rusqlite connection)
  - `config: AppConfig`
  - `reader_state: Option<ReaderState>`
  - `review_state: Option<ReviewState>`
  - `popup: Option<PopupState>` (for overlays like detailed dictionary entry)
  - `message: Option<(String, Instant)>` (status bar messages that auto-dismiss)
- [x] Implement main TUI loop in `main.rs`:
  - `Terminal::new(CrosstermBackend::new(stdout()))`
  - Enable raw mode, alternate screen, mouse capture
  - Loop: poll event -> `app.handle_event(event)` -> `terminal.draw(|f| ui::render(f, &app))`
  - Graceful cleanup on panic (restore terminal state)
- [x] Global keybindings:
  - `q` / `Ctrl+C` — quit (with confirmation if in Reader with unsaved state)
  - `Tab` — cycle between screens (Library -> Reader -> Review -> Stats)
  - `?` — show help overlay with all keybindings for current screen

#### 2.2 Reader State Machine
- [x] `ReaderState` struct:
  - `text_id: i64` — currently loaded text
  - `paragraphs: Vec<ParagraphData>` — pre-loaded paragraphs with their tokens
  - `sentence_index: usize` — index into flat list of all sentences
  - `word_index: Option<usize>` — selected word within current sentence (None = no word selected)
  - `sentences: Vec<SentenceData>` — flattened list: each holds `paragraph_idx`, `token_range`, `text`
  - `vocabulary_cache: HashMap<(String, String), Vocabulary>` — in-memory cache of vocabulary statuses keyed by (base_form, reading)
  - `scroll_offset: usize` — vertical scroll position for main text area
  - `autopromote_enabled: bool` — per-session toggle, default `true`
  - `autopromote_history: Vec<AutopromotionBatch>` — undo stack of autopromoted word batches
  - `show_all_readings: bool` — per-session toggle to show readings for all words in sidebar, default `false`
  - `show_known_in_sidebar: bool` — per-session toggle to show Known/Ignored words in sidebar word list, default `false`
- [x] `SentenceData` struct: `paragraph_idx`, `start_token`, `end_token`, `tokens: Vec<TokenDisplay>`
- [x] `TokenDisplay` struct: `surface`, `base_form`, `reading`, `pos`, `vocabulary_status`, `is_selected`, `dict_entry: Option<ShortGloss>`
- [x] Load text on enter: query all paragraphs + tokens, build sentence list, lookup all vocabulary statuses, lookup JMdict short glosses for each unique base_form

#### 2.3 Furigana Rendering Engine
- [x] Implement `ui/components/furigana.rs`:
  - `render_token_with_furigana(token: &TokenDisplay, style: Style) -> (Vec<Span>, Vec<Span>)` — returns (furigana_line, kanji_line) as Ratatui `Span` pairs
  - Use `unicode_width::UnicodeWidthStr::width()` to calculate display width $W$ of the surface form
  - If surface == reading (all kana), render single line only (no furigana needed)
  - If surface contains kanji: center the reading (converted to hiragana) within $W$ cells on the line above
  - Handle edge cases: mixed kanji-kana tokens, single kanji, long readings that exceed kanji width (pad kanji with spaces instead)
- [x] `render_sentence_block(sentence: &SentenceData, area: Rect, buf: &mut Buffer)`:
  - Lay out tokens left-to-right, wrapping to next line-pair (furigana + kanji) when exceeding area width
  - Each token's style is determined by its `vocabulary_status` color mapping
  - Selected word gets `bg(Color::Blue)` highlight

#### 2.4 Main Reader View
- [x] Implement `ui/screens/reader.rs`:
  - Horizontal split: 70% main text, 30% sidebar (adjustable via config)
  - Main text area: Render all paragraphs as `furigana_line + kanji_line` blocks, scrolled so current sentence is vertically centered
  - Current sentence indicator: `▶` gutter marker or distinct background color
  - Paragraph boundaries: blank line between paragraphs
  - Word coloring by vocabulary status (see color table in Domain Concepts)
- [x] Scroll management:
  - Calculate total rendered height of all paragraphs (accounting for furigana doubling line count)
  - Auto-scroll to keep current sentence centered (with 3-line margin from top/bottom)
  - Don't scroll if entire text fits in view

#### 2.5 Sidebar Panel
- [x] Implement `ui/components/sidebar.rs`:
  - **Header**: Current sentence rendered with furigana (using `furigana::render_sentence`) and vocabulary status coloring, occupying up to half the sidebar height
  - **Word list**: By default shows only New and Learning words. Known/Ignored words are hidden unless toggled with `w` (or the word is currently selected). Each non-trivial token displayed as:
    ```
    食べ (たべ) = to eat     [2]
    ```
    Format: `surface (reading) = short_gloss  [status]`
  - Known/Ignored words shown dimmed (DarkGray) when revealed via toggle
  - Currently selected word highlighted with `>>` marker and bold style
  - Scroll independently if word list exceeds sidebar height
- [x] Word detail popup (triggered by `Enter`):
  - Modal overlay (centered, 60% width, 80% height)
  - Full JMdict entry: all kanji forms, all readings, all senses with POS tags and glosses
  - List of encountered conjugation forms with counts from `conjugation_encounters`
  - User notes field (if any)
  - Pitch accent info (placeholder — deferred to later phase)
  - Close with `Esc` or `Enter`

#### 2.6 Reader Keybindings & State Mutations
- [x] Keybinding implementation:
  - `↑`/`k` — previous sentence (update `sentence_index`, reset `word_index` to None, re-center scroll)
  - `↓`/`j` — next sentence
  - `←`/`h` — previous word in sentence (set or decrement `word_index`)
  - `→`/`l` — next word in sentence
  - `1`-`4` — set selected word status to Learning 1-4; triggers:
    1. UPDATE `vocabulary` SET status WHERE base_form + reading
    2. Auto-create SRS word card (if not exists) with `due_date = now`
    3. Auto-create SRS sentence card for current sentence (if not exists)
    4. Refresh `vocabulary_cache` and re-render
  - `5` — set selected word to Known; retire any active SRS cards for this word
  - `i` — set selected word to Ignored
  - `n` — open note editor popup (simple text input, saves to `vocabulary.notes`)
  - `l` — trigger LLM analysis of current sentence (show loading spinner in sidebar, display result when ready)
  - `w` — toggle showing Known/Ignored words in sidebar word list
  - `r` — toggle sidebar readings for all words (see §2.7b)
  - `a` — toggle autopromotion on/off for the current session (see §2.7a)
  - `Ctrl+Z` — undo last autopromotion batch (see §2.7a)
  - `Esc` — deselect word (set `word_index` to None)

#### 2.7a Autopromotion of New Words

When a user first starts using the app, every word is New — including common words they already know. Manually marking hundreds of words as Known is tedious and discouraging. To solve this, the Reader **automatically promotes all New words to Known** when the user advances past a sentence. The assumption: if you didn't mark a word as Learning, you already know it.

- [ ] **Default behavior**: On sentence advance (↓/j, or →/l wrapping to next sentence), all non-trivial tokens in the **departing** sentence with `vocabulary_status == New (0)` are batch-updated to `Known (5)` in the DB and in-memory cache. No SRS cards are created (Known words don't get cards).
- [ ] **Per-session toggle**: `a` key in Reader toggles autopromotion on/off for the current session. Status shown in the bottom bar (e.g. `[A]` when active, absent when off). Default: **on**. The toggle state is *not* persisted — each session starts with autopromotion on.
- [ ] **Undo stack**: `Ctrl+Z` in Reader undoes the most recent autopromotion batch (reverts those words from Known back to New in DB + cache + token display). The stack holds all autopromotion batches from the current session. Only autopromotion actions are undoable via this key — manual status changes (1-5, i) are not on this stack.
  - `AutopromotionBatch` struct: `sentence_index: usize`, `words: Vec<(String, String, i64)>` (base_form, reading, vocabulary_id) — the set of words that were promoted.
  - Stack lives on `ReaderState`: `autopromote_history: Vec<AutopromotionBatch>`
  - On undo: pop last batch, set each word back to `New` in DB, update `vocabulary_cache`, patch all matching tokens across all sentences, show message `"Undo: N words reverted to New"`
- [ ] **Interaction with manual status**: Words the user has explicitly set to Learning (1-4), Known (5), or Ignored (-1) in the current sentence are **not** touched by autopromotion — only `New (0)` words are promoted. If the user sets a word to Learning and then advances, that word stays at its Learning status.
- [ ] **Undo does not affect manual changes**: If the user manually set word X to Known (5) and autopromotion also ran on the same sentence, undoing that batch only reverts words that were *autopromoted*, not manually changed ones. The `AutopromotionBatch` only records words that autopromotion actually changed.
- [ ] **Cross-sentence word appearance**: If word X is autopromoted in sentence 5, its status updates globally (all occurrences across all sentences reflect Known). If the user later undoes sentence 5's batch, word X reverts to New globally — and may be autopromoted again when the user advances past another sentence containing it.

#### 2.7b Sidebar Readings Toggle

By default, the sidebar hides readings for Known (5) and Ignored (-1) words, matching the vocabulary status table in Domain Concepts. A per-session toggle overrides this to show readings for **all** words.

- [x] **Toggle**: `r` key in Reader toggles `show_all_readings` on/off. Default: **off** (readings hidden for Known/Ignored).
- [x] **Status bar indicator**: `[R]` (green) when active, `[r]` (gray) when off.
- [x] **Scope**: Only affects the sidebar word list. Furigana rendering in the main text area is unaffected.
- [x] **Per-session**: Not persisted — resets to off on each Reader load.
- [x] **ReaderState field**: `show_all_readings: bool`

#### 2.8 Verification
- [x] Manual test: import a sample text, open in Reader, verify furigana alignment for various token widths
- [x] Test: sentence navigation wraps correctly at text boundaries
- [x] Test: status changes persist across Reader reloads (close and reopen same text)
- [x] Visual check: all vocabulary status colors render correctly on both dark and light terminal backgrounds
- [ ] Test: advancing a sentence autopromotes New words to Known, sidebar/colors update
- [ ] Test: toggling autopromotion off with `a` prevents promotion on advance
- [ ] Test: Ctrl+Z reverts the last autopromotion batch, words return to New
- [ ] Test: manual status changes (1-4, i) are not affected by undo
- [ ] Test: words set to Learning before advancing are not autopromoted

---

### Phase 3 — Content Import Expansion

**Goal**: Support multiple import sources beyond plain text files.

#### 3.1 Clipboard Import
- [x] Add `arboard` crate for cross-platform clipboard access
- [x] CLI command `kotoba import --clipboard` or TUI action (keybinding in Library screen)
- [x] Flow: read clipboard text -> confirm with user (show first 200 chars preview) -> run through same import pipeline as text files
- [x] Auto-generate title from first line or first N characters

#### 3.2 Web Import (Generic)
- [x] Implement `import/web.rs`:
  - `import_url(url: &str, conn: &Connection) -> Result<i64>`
  - Fetch HTML via `reqwest::blocking::get(url)`
  - Extract article content via `scraper` crate (readability-style heuristic extraction)
  - Strip remaining HTML tags, normalize whitespace
  - Store `source_url` and `source_type = "web"` in `texts` table
  - Run through standard tokenization pipeline
- [x] CLI command `kotoba import --url "https://..."`
- [x] Handle errors gracefully: connection failures, non-HTML content, readability extraction failures

#### 3.3 Syosetsu (小説家になろう) Custom Source
- [x] Implement `import/syosetsu.rs`:
  - `SyosetsuNovel` struct: `ncode`, `title`, `author`, `total_chapters`, `chapters: Vec<SyosetsuChapter>`
  - `SyosetsuChapter`: `number`, `title`, `text_id` (nullable, only set if imported), `word_count`
  - Fetch novel metadata from Syosetsu table of contents page
  - Fetch chapter list from the novel's table of contents page
  - `import_chapter(ncode: &str, chapter: usize, conn: &Connection) -> Result<i64>` — fetch single chapter HTML, extract text, import
- [x] CLI command `kotoba syosetsu <ncode> --chapter <N>` for importing chapters
- [x] Store novel metadata in `web_sources` table: `id`, `source_type`, `external_id`, `title`, `metadata_json`, `last_synced`
- [x] DB migration 009: Create `web_sources` and `web_source_chapters` tables
- [x] TUI screen: `ui/screens/syosetsu.rs` — novel info display, chapter list with import status, navigate and import/open chapters

#### 3.4 Subtitle Import (.srt / .ass)
- [x] Implement `import/subtitle.rs`:
  - Parse `.srt` format: extract timed text entries, strip timing info, concatenate into paragraphs (group by scene/gap)
  - Parse `.ass`/`.ssa` format: extract `Dialogue` lines, strip style tags `{\...}`, extract text
  - Each subtitle block becomes a paragraph; individual lines become sentences
  - Store `source_type = "subtitle"` in `texts` table
- [x] CLI command `kotoba import <file.srt>` (auto-detected by extension)

#### 3.5 EPUB Import
- [x] Add `zip` crate for EPUB unarchiving
- [x] Implement `import/epub.rs`:
  - Open EPUB (it's a ZIP archive), parse `content.opf` for spine order
  - Extract XHTML chapter files in spine order
  - Strip HTML tags, extract text content per chapter
  - Each chapter becomes a separate `texts` entry (linked by a shared `source_url` = EPUB file path)
- [x] CLI command `kotoba import <book.epub>` (auto-detected by extension)
- [x] Progress bar for multi-chapter EPUB imports

#### 3.6 Library Screen
- [x] Implement `ui/screens/library.rs`:
  - List all imported texts with: title, source type icon, date imported
  - Keybindings:
    - `Enter` — open selected text in Reader
    - `d` — delete text (with confirmation popup)
    - `i` — import new text (sub-menu: clipboard / URL)
    - `/` — search/filter texts by title
  - Per-text stats queries available (total words, unique vocab, known/learning/new counts)
  - Search texts by title with live filtering
- [x] Per-text stats display in library list (word count, known/learning/new counts, completion %)
- [x] Sort by: date (desc/asc), title A-Z, completion % — cycle with `s` key
- [x] Filter by source type (text, web, syosetsu, subtitle, epub) — cycle with `f` key
- [x] Syosetsu novels shown as grouped sources in Library, with ChapterSelect screen for browsing chapters

#### 3.7 Import Progress Bar (Additional)
- [x] All imports show an `indicatif` progress bar with paragraph count, token count, and new vocab count
- [x] EPUB imports show chapter-level progress
- [x] Quiet import variants available for TUI context (no terminal output)

---

### Phase 3.5 — Screen Layout & Navigation Overhaul

**Goal**: Replace the flat Tab-cycling navigation with a hub-and-spoke model. Add a Home screen, chapter-aware source browsing, progress indicators, and background preprocessing so imports never freeze the UI.

#### 3.5.1 Database Changes

- [x] Add `last_read_at` (nullable timestamp) column to `texts` table — updated every time a text is opened or navigated in Reader
- [x] Add `total_sentences` column to `texts` table — kept in sync when navigating in Reader for persistent progress tracking
- [x] Add `is_skipped` (boolean, default false) column to `web_source_chapters` table — skipped chapters are grayed out in chapter list and excluded from word statistics/SRS
- [x] Add `chapter_group` (text) column to `web_source_chapters` table — stores the arc/group name from Syosetu chapter list
- [x] Extend EPUB import to store a `web_sources` row per book with `source_type = "epub"`, and a `web_source_chapters` row per chapter linking to its `texts.id`
- [x] DB migrations 11 (`last_read_at`, `total_sentences`, `is_skipped`) and 12 (`chapter_group`) applied

#### 3.5.2 Screen Enum & Navigation Model

- [x] New `Screen` enum: `Home`, `Library`, `ChapterSelect { source_id }`, `Reader`, `Review`
- [x] **Tab key** — two-mode toggle between **Reader** ↔ **non-Reader** (most recently read text)
- [x] **Esc key** — contextual "go back": Reader→previous, ChapterSelect→Library, Library→Home
- [x] Remove `Screen::Syosetu` — folded into generic `ChapterSelect`
- [x] Remove `Screen::Stats` — deferred to Phase 6

#### 3.5.3 Home Screen

- [x] Implement `ui/screens/home.rs` with recently-read list, progress bars, K/L/N% stats
- [x] Quick actions: `[l]` Library, `[r]` Review, `[i]` Import
- [x] Home is the default screen on app launch

#### 3.5.4 Library Screen Rework

- [x] Library shows grouped sources via `LibraryItem` enum (`Text` | `Source`) — standalone texts + grouped multi-chapter sources
- [x] Each row displays source type, title, chapter counts for sources
- [x] Existing functionality preserved: sort (`s`), filter (`f`), search (`/`), delete (`d`), import (`i`)
- [x] **Enter** behavior: plain text → Reader directly, Syosetu/EPUB → ChapterSelect
- [x] Delete web sources with `DeleteSourceConfirm` popup

#### 3.5.5 Chapter Select Screen

- [x] Implement `ui/screens/chapter_select.rs` — paginated chapter list (50/page)
- [x] Header with source info, chapter/imported/skipped counts
- [x] 5 distinct chapter states: `S` skipped, `⠋` preprocessing (animated), `—` not imported, `○` unread, `◐` in progress, `●` finished
- [x] Skip toggle (`x` key) with in-memory update, cancel preprocessing, re-queue
- [x] Chapter group headers rendered as non-selectable separator rows (magenta bold `── Group Name ──`)
- [x] Non-blocking chapter enter: `pending_open_chapter` auto-opens when import completes
- [x] Preprocessing progress display: `[⠋ 45%]` with phase/percent from `ImportEvent::Progress`
- [x] Keybindings: Enter, x (skip), ↑↓ nav, p/n (paginate), Esc (back), Tab (reader)

#### 3.5.6 Progress Indicators on Texts & Chapters

- [x] **Reading progress bar** on Home and Library screens
- [x] **Word status breakdown** `K:67% L:12% N:21%` on Home screen
- [x] Indicators update when Reader progress is saved or vocabulary statuses change

#### 3.5.7 Background Chapter Preprocessing

- [x] **Architecture**: `BackgroundImporter` with 3-thread worker pool, `mpsc` channel for `ImportEvent`s
- [x] **Two-phase import**: Phase 1 (HTTP fetch + tokenize in memory, parallelizable), Phase 2 (short DB transaction)
- [x] **Events**: `Started`, `Progress` (phase + percent), `Completed`, `Failed`, `Cancelled`, `ChaptersPageLoaded`, `NovelInfoLoaded`, `NovelInfoFailed`
- [x] **Eager preprocessing**: configurable via `config.reader.preprocess_ahead` (default 3), smart budget counting unread-imported + in-flight
- [x] **Cancellation**: cancelled set checked between stages, removed from queue on skip
- [x] **UI indicators**: animated Braille spinners (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`), progress percentage
- [x] **Syosetu novel info fetch** in background with incremental page-by-page loading — sends `ChaptersPageLoaded` per page so UI updates as chapters arrive
- [x] **Auto-refresh on revisit**: when loading ChapterSelect for an existing Syosetu source, fires background thread to check for new chapters
- [x] **Non-blocking chapter enter**: queues import and auto-opens when ready
- [x] **DB connection handling**: busy_timeout(5000) on all connections, WAL mode for concurrent access

#### 3.5.8 Verification

- [x] Test: Tab from Home opens the most recently read text; Tab from Reader returns to previous screen
- [x] Test: Esc chain — Reader → ChapterSelect → Library → Home
- [x] Test: Library shows grouped Syosetu/EPUB sources, Enter opens ChapterSelect
- [x] Test: ChapterSelect pagination works with 500+ chapters
- [x] Test: skipping a chapter grays it out, excludes from stats, cancels in-progress preprocessing
- [x] Test: background preprocessing imports 3 chapters concurrently without freezing the UI
- [x] Test: progress bars and word status percentages update correctly after reading
- [x] Test: plain text / clipboard Enter from Library goes directly to Reader (no ChapterSelect)

---

### Phase 4 — Spaced Repetition (SRS)

**Goal**: Full FSRS-powered review system with multiple card types and review modes.

#### 4.1 FSRS Engine Integration
- [ ] Implement `core/srs.rs`:
  - Initialize `fsrs::FSRS` with default parameters (or user-configured via TOML)
  - `create_word_card(vocabulary_id: i64, conn: &Connection)`:
    - Insert into `srs_cards` with `card_type = Word`, `answer_mode` = user's default preference
    - Set initial FSRS state: `stability = 0`, `difficulty = 0`, `state = New`, `due_date = now`
  - `create_sentence_card(sentence_tokens: &[Token], conn: &Connection)`:
    - Insert into `srs_cards` with `card_type = Sentence`
    - Link to all vocabulary_ids in the sentence via a join table or store sentence token IDs as JSON
  - `get_due_cards(conn: &Connection, limit: usize) -> Vec<SrsCard>`:
    - Query `srs_cards WHERE due_date <= now AND state != Retired ORDER BY due_date ASC LIMIT n`
    - Filter out sentence cards where all constituent vocabulary items are Known
  - `record_review(card_id: i64, rating: Rating, elapsed_ms: u64, conn: &Connection)`:
    - Call `fsrs.next_states()` with current card state + rating (Again/Hard/Good/Easy)
    - Update `srs_cards` with new `stability`, `difficulty`, `due_date`, `reps`, `lapses`, `state`
    - Insert into `srs_reviews` log table
  - `retire_card(card_id: i64, conn: &Connection)` — set state to Retired (when word reaches Known)

#### 4.2 SRS Card Auto-Generation
- [ ] Hook into Reader status change flow:
  - When word status changes to Learning (1-4):
    - Check if word card exists for this vocabulary_id; if not, call `create_word_card()`
    - Check if sentence card exists for current sentence; if not, call `create_sentence_card()`
  - When word status changes to Known (5):
    - Retire the word's SRS card
    - Check all sentence cards containing this word; if all words in sentence are now Known, retire sentence card
  - When word status changes to Ignored (-1):
    - Retire any active SRS cards for this word

#### 4.3 Review Screen UI
- [ ] Implement `ui/screens/review.rs`:
  - `ReviewState` struct:
    - `queue: Vec<SrsCard>` — loaded batch of due cards
    - `current_index: usize`
    - `phase: ReviewPhase` enum: `ShowFront`, `ShowBack`, `TypingAnswer`, `ShowResult`
    - `typed_input: String` (for typed reading mode)
    - `elapsed: Instant` (track response time)
  - Layout: centered card display (60% width, 70% height), status bar at bottom
  - **Meaning recall mode**:
    - Front: word in large text + reading in parentheses + sentence context below (with vocab coloring)
    - User thinks, presses `Space` to reveal
    - Back: JMdict definitions shown
    - Rate: `1`=Again, `2`=Hard, `3`=Good, `4`=Easy
  - **Reading recall mode**:
    - Front: word in kanji only (large) + sentence context
    - User thinks, presses `Space` to reveal
    - Back: reading shown in hiragana
    - Rate: `1`-`4`
  - **Typed reading mode**:
    - Front: word in kanji + sentence context
    - Text input field appears; user types hiragana reading
    - On `Enter`: compare input to accepted readings via `strsim::levenshtein()`
    - Show diff: correct characters in green, wrong in red, missing in grey
    - Auto-rate based on edit distance: 0 = Easy, 1 = Good, 2 = Hard, 3+ = Again (overridable by user)
  - **Sentence cloze mode**:
    - Front: sentence with target word replaced by `____` (blank)
    - Sentence shown with full vocab coloring for all other words
    - User presses `Space` to reveal answer
    - Back: word shown in context, highlighted
    - Rate: `1`-`4`
- [ ] Sentence context in all modes:
  - Below the card, show the source sentence
  - All words in the sentence are colored by vocabulary status
  - Left/Right arrow keys allow navigating words in the sentence context
  - Pressing Enter on a context word shows its JMdict definition in a tooltip/popup
- [ ] Session summary:
  - After all due cards reviewed, show: total reviewed, accuracy %, next review time
  - Option to continue with cards due soon (next 24h) or return to Library

#### 4.4 Review Session Configuration
- [ ] Settings (in TOML config):
  - `default_answer_mode`: which review mode to use by default
  - `new_cards_per_day`: limit on new cards introduced per session (default: 20)
  - `max_reviews_per_session`: optional cap (default: unlimited)
  - `review_order`: "due_first" (default) or "random"
- [ ] TUI review session start: show summary (X cards due, Y new) with option to adjust before starting

---

### Phase 5 — LLM Integration

**Goal**: On-demand structured sentence analysis via OpenAI-compatible API.

#### 5.1 LLM Client
- [ ] Implement `core/llm.rs`:
  - `LlmClient` struct: `endpoint: String`, `api_key: String`, `model: String`, `max_tokens: usize`
  - Load config from TOML:
    ```toml
    [llm]
    endpoint = "http://localhost:11434/v1"  # Ollama example
    api_key = ""                            # optional for local
    model = "gpt-4o"
    max_tokens = 2048
    ```
  - `async fn analyze_sentence(&self, sentence: &str) -> Result<SentenceAnalysis>`
  - Build request body with system prompt:
    ```
    You are a Japanese language tutor. Analyze the following Japanese sentence.
    Return a JSON object with:
    - "translation": full English translation
    - "words": array of { "surface", "base_form", "reading", "meaning", "pos", "notes" }
    - "grammar": array of { "pattern", "explanation", "example" }
    - "idioms": array of { "phrase", "meaning", "literal" }  (empty if none)
    ```
  - POST to `{endpoint}/chat/completions` with `response_format: { type: "json_object" }`
  - Parse response into `SentenceAnalysis` struct

#### 5.2 Caching Layer
- [ ] Before making API call, hash `sha256(sentence + model)` and check `llm_cache` table
- [ ] On cache hit: deserialize stored JSON response, return immediately
- [ ] On cache miss: make API call, store response + metadata (model, tokens_used) in `llm_cache`
- [ ] Cache invalidation: provide CLI command `kotoba cache clear` and `kotoba cache stats`

#### 5.3 Async TUI Integration
- [ ] In `ui/events.rs`, add `LlmResponse(Result<SentenceAnalysis>)` event variant
- [ ] When user presses `l` in Reader:
  - Check cache first (synchronous, fast)
  - If cache miss: spawn `tokio::spawn(async { client.analyze_sentence(sentence).await })`
  - Show loading spinner in sidebar: `⠋ Analyzing...` (rotating braille animation)
  - On completion: send result through `mpsc` channel -> event loop picks it up -> update sidebar
- [ ] Display LLM result in sidebar (replacing word list temporarily):
  - **Translation**: full sentence translation at top
  - **Word breakdown**: table of words with contextual meanings
  - **Grammar**: each pattern with explanation
  - **Idioms**: if any detected
  - Press `Esc` or `l` again to dismiss and return to JMdict word list view

#### 5.4 LLM in SRS Review
- [ ] During SRS review, `l` key triggers LLM analysis of the card's source sentence
- [ ] Result shown in a side panel or popup overlay
- [ ] Same caching applies — most sentences will already be cached from Reader usage

---

### Phase 6 — Stats Screen

**Goal**: Visualize learning progress with terminal-based charts and metrics.

#### 6.1 Stats Data Queries
- [ ] Implement `db/stats.rs`:
  - `known_words_over_time(conn, days: usize) -> Vec<(Date, usize)>` — count of vocabulary with status >= Known, grouped by day
  - `words_by_status(conn) -> HashMap<VocabularyStatus, usize>` — current breakdown
  - `reading_activity(conn, days: usize) -> Vec<(Date, usize)>` — tokens read per day (from srs_reviews.reviewed_at or a separate reading_log)
  - `srs_stats(conn) -> SrsStats` — struct with: due_today, due_tomorrow, total_reviews, avg_accuracy, retention_rate
  - `text_coverage(conn, text_id: i64) -> CoverageStats` — total tokens, known tokens, learning tokens, new tokens, % coverage

#### 6.2 Stats Screen UI
- [ ] Implement `ui/screens/stats.rs`:
  - **Overview panel**: Total vocabulary (known/learning/new), texts imported, total reviews
  - **Vocabulary growth chart**: ASCII/braille line chart showing known words over time (last 30/90/365 days)
    - Use `ratatui::widgets::canvas::Canvas` or a simple bar chart with `BarChart` widget
  - **Status breakdown**: Horizontal stacked bar or pie-style breakdown (New | L1 | L2 | L3 | L4 | Known | Ignored)
  - **Reading streak**: Calendar heatmap or simple "X days in a row" counter
  - **SRS panel**: Cards due today/tomorrow, review accuracy rate (last 7 days), retention rate
  - **Per-text coverage**: selectable list showing coverage % for each imported text
- [ ] Keybindings:
  - `↑`/`↓` — scroll between stat panels
  - `t` — toggle time range (7d / 30d / 90d / all)
  - `Enter` on a text in coverage list — jump to Reader for that text

---

### Phase 7 — Theming, Configuration & Polish

**Goal**: Customization, UX refinements, and release readiness.

#### 7.1 Theming Engine
- [ ] Implement `config.rs` theme loading:
  - Load `theme.toml` with color definitions for every UI element:
    ```toml
    [theme]
    bg = "#1a1b26"
    fg = "#c0caf5"
    status_new = "#7aa2f7"
    status_learning1 = "#fde25d"
    status_learning2 = "#fde25dbf"
    status_learning3 = "#fde25d6f"
    status_learning4 = "#fde25d2a"
    status_known = "#c0caf5"
    status_ignored = "#565f89"
    highlight_bg = "#33467c"
    sidebar_bg = "#1f2335"
    popup_border = "#7aa2f7"
    ```
  - Parse hex colors into `Color::Rgb(r, g, b, a)`
  - Provide 2-3 built-in themes: Tokyo Night (dark), Solarized Light, Gruvbox
  - Fallback to 256-color or 16-color palette if terminal doesn't support RGB

#### 7.2 Configuration
- [ ] Full `kotoba.toml` config file:
  ```toml
  [general]
  db_path = "~/.local/share/kotoba/kotoba.db"
  jmdict_path = "~/.local/share/kotoba/JMdict_e.xml"
  theme = "tokyo-night"  # or path to custom theme.toml

  [reader]
  sidebar_width = 30  # percentage
  furigana = true     # toggle furigana display
  font_size = "normal" # not applicable in terminal, but kept for future GUI

  [srs]
  default_answer_mode = "meaning_recall"
  new_cards_per_day = 20
  max_reviews_per_session = 0  # 0 = unlimited

  [llm]
  endpoint = "https://api.openai.com/v1"
  api_key = ""
  model = "gpt-4o"
  max_tokens = 2048
  ```
- [ ] CLI command `kotoba config` — print current config location and values
- [ ] XDG-compliant paths: config in `$XDG_CONFIG_HOME/kotoba/`, data in `$XDG_DATA_HOME/kotoba/`

#### 7.3 UX Polish
- [ ] Status bar at bottom of every screen: current screen name, keybinding hints, notification messages
- [ ] Consistent popup system: all popups use same border style, close with `Esc`, support scrolling
- [ ] Error handling: user-friendly error messages in status bar (not panics)
- [ ] Loading states: spinner for any operation > 200ms (DB queries on large datasets, LLM calls, web imports)
- [ ] First-run experience:
  - Detect missing DB / JMdict: prompt to run `kotoba init` or auto-initialize
  - `kotoba init` command: create DB, download JMdict XML (or prompt for path), run import
- [ ] Mouse support (optional): click on words in Reader to select them

#### 7.4 Build & Distribution
- [ ] `cargo build --release` — single static binary
- [ ] GitHub Actions CI: build for Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64)
- [ ] Include JMdict download script or bundle instructions
- [ ] Write `--help` text for all CLI subcommands
- [ ] Man page generation via `clap_mangen` (optional)

---

### Phase 8 — Future Enhancements (Deferred)

Items identified but explicitly deferred to keep scope manageable.

- [ ] **AnkiConnect export**: One-way sync to push vocabulary cards to Anki via AnkiConnect API
- [ ] **Pitch accent data**: Integrate OJAD or NHK accent dictionary, display in word detail popup
- [ ] **Audio/TTS**: System TTS or cloud TTS API for word/sentence pronunciation
- [ ] **Additional web sources**: News sites (NHK News Easy, Asahi), custom per-source TUI screens
- [ ] **PDF import**: Extract text layers from PDFs
- [ ] **Multi-language support**: Generalize beyond Japanese (Chinese, Korean — different tokenizers)
- [ ] **Cloud sync**: Optional sync of vocabulary/SRS state across devices
- [ ] **Plugin system**: User-defined import sources or LLM prompt templates

---

### Enhancement: Conjugation Grouping, Multi-Word Expressions & Conjugation Descriptions

**Problem Statement**

Currently, lindera/UniDic tokenizes text into minimal morphemes. This causes two user-facing problems:

1. **Conjugated forms are split**: `食べない` → `食べ`(Verb) + `ない`(Auxiliary). Only the verb stem `食べ` is highlighted; the auxiliary `ない` appears unhighlighted (auto-ignored). The user sees a broken-looking partial highlight instead of the full conjugated word.

2. **Idiomatic expressions are atomized**: `猫の手も借りたい` → `猫` + `の` + `手` + `も` + `借り` + `たい`. Each morpheme is treated independently, losing the idiomatic meaning ("so busy you'd even borrow a cat's paws").

3. **Conjugation labels are raw UniDic**: The sidebar shows cryptic labels like `continuative (general)` instead of learner-friendly descriptions like `verb, negative, past`.

**Test Data**: `data/sample_hard.txt`

```
昨日はとても忙しくて、猫の手も借りたいほどでした。新しい自動車運転免許証を受け取りに行かなければならなかったからです。複雑な手続きに気が遠くなりそうでしたが、なんとか終わらせて安堵しています。
食べない。食べます。食べる。食べられる。食べよう。
```

**Approach chosen**: Keep lindera for morphological analysis (embedded UniDic, no external dictionary files). Add two post-processing layers at display time.

**Why NOT sudachi.rs**:
- Not on crates.io (git dependency only)
- Requires external dictionary files (~70-100MB), cannot embed in binary — breaks the current zero-config single-binary distribution
- Dictionary baking feature listed as "un-implemented and does not work currently"
- Still splits conjugations the same way (morphological analyzers decompose inflections by design)
- Only helps with compound nouns (Mode C), not idioms containing particles/verbs
- Conclusion: significant cost for marginal benefit over lindera

**How yomitan was used as reference**: Yomitan's `japanese-transforms.js` contains comprehensive deinflection rules mapping conjugated suffix patterns → dictionary forms. These rules were used as a reference to build the auxiliary chain → human-readable label mapping (e.g., `ない` → "negative", `ます` → "polite", `た` → "past"). Yomitan's actual approach (substring scanning + deinflection) is designed for hover-lookup, not full-text tokenization, so it was used for linguistic reference only.

---

#### Layer 1 — Conjugation Group Merging

**Concept**: After lindera tokenizes and `build_sentences()` creates `TokenDisplay` items, a grouping pass merges verb/adjective stems with their following auxiliaries into "conjugation groups". All tokens in a group share the head word's `vocabulary_status`, so the entire conjugated form highlights as one unit.

**New `TokenDisplay` fields**:

```rust
pub group_id: Option<usize>,      // Shared group index within the sentence (None = standalone)
pub is_group_head: bool,          // True for the vocabulary-bearing head token
pub conjugation_desc: String,     // Human-readable: "verb, negative, past"
pub mwe_gloss: String,            // For MWE groups: the expression's English meaning
```

**Grouping rules** (`assign_conjugation_groups`):

1. Scan tokens left-to-right
2. When a **Verb** or **Adjective** token is found, start a new group
3. Continue adding tokens while the next token's POS is **Auxiliary** (助動詞)
4. Stop at any non-Auxiliary token (Noun, Verb, Particle, Symbol, etc.)
5. All group members inherit the head token's `vocabulary_status`
6. Navigation skips non-head group members (Left/Right jumps between group heads)
7. Selection highlights the entire group (all members get `is_selected = true`)

**Why only Auxiliary and not Particle**: UniDic tags `て`, `ば` as Particle (接続助詞). These particles genuinely separate grammatical clauses (e.g., `食べている` = `食べ` + `て` + `いる` — three separate concepts). Grouping past a Particle would incorrectly merge separate clauses. The Auxiliary-only rule correctly groups:

| Input | Tokens | Group |
|---|---|---|
| `食べない` | `食べ`(V) + `ない`(Aux) | `[食べない]` → "verb, negative" |
| `食べます` | `食べ`(V) + `ます`(Aux) | `[食べます]` → "verb, polite" |
| `食べた` | `食べ`(V) + `た`(Aux) | `[食べた]` → "verb, past" |
| `食べられる` | `食べ`(V) + `られる`(Aux) | `[食べられる]` → "verb, passive/potential" |
| `食べさせたい` | `食べ`(V) + `させ`(Aux) + `たい`(Aux) | `[食べさせたい]` → "verb, causative, want to" |
| `行かなければ` | `行か`(V) + `なけれ`(Aux) | `[行かなけれ]` + `ば`(Particle standalone) |

**Conjugation description generation** (`build_conjugation_description`):

Map auxiliary `base_form` values to human-readable English labels:

```rust
match base_form {
    "ない"           => "negative",
    "ます"           => "polite",
    "た" | "だ"      => "past",
    "て" | "で"      => "te-form",
    "れる" | "られる" => "passive/potential",
    "せる" | "させる" => "causative",
    "たい"           => "want to",
    "う" | "よう"     => "volitional",
    "ぬ"             => "negative (classical)",
    "ず"             => "negative (literary)",
    "まい"           => "negative volitional",
    "そう"           => "looks like",
    "らしい"         => "seems like",
    "べし"           => "should (classical)",
    "です"           => "copula (polite)",
    "だ"             => "copula",
    _                => raw base_form as fallback,
}
```

Head POS is prepended: `"verb, "` or `"adjective, "`.
Chain is comma-separated: `"verb, causative, negative, past"`.

Also use the **conjugation_form** of the head token to detect additional inflection info:
- 未然形 (irrealis) on the head with no auxiliary → note the stem form
- 仮定形 (conditional) on the last auxiliary → append "conditional"
- 命令形 (imperative) on the head → append "imperative"
- 意志推量形 (volitional) on the head → append "volitional"

**Example full descriptions**:

| Surface | Description |
|---|---|
| 食べない | verb, negative |
| 食べました | verb, polite, past |
| 食べられなかった | verb, passive/potential, negative, past |
| 食べさせたい | verb, causative, want to |
| 行かなけれ | verb, negative, conditional |
| 食べよう | verb, volitional |

---

#### Layer 2 — Multi-Word Expression (MWE) Detection

Two sub-systems: automatic JMdict matching and manual user expressions.

##### 2A — Automatic JMdict MWE Detection

**Concept**: After tokenization, scan sliding windows of consecutive tokens and check if their concatenated surface matches a JMdict kanji element. JMdict already contains many multi-word expressions (idioms, set phrases, compound verbs).

**Algorithm** (in `detect_sentence_mwes`):

```
For each starting token position i in the sentence:
    combined = ""
    For j = i to min(i+12, len):     # max window of 12 tokens
        combined += tokens[j].surface
        if j > i:                     # skip single-token matches (already handled)
            if has_jmdict_kanji_entry(combined):
                record match (start=i, end=j+1, surface=combined)
                keep only the longest match at each position (greedy)
    advance i past the longest match (no overlaps)
```

**Performance**: Uses a lightweight `SELECT 1 FROM jmdict_kanji WHERE kanji_element = ? LIMIT 1` existence check (indexed, ~10μs per query). For a 20-token sentence with max window 12, worst case ~200 queries = ~2ms. Acceptable for interactive use.

**MWE groups in display**:
- All tokens in an MWE match share a single `group_id`
- The first non-trivial token is the `group_head` (navigation target)
- `mwe_gloss` is set to the JMdict short_gloss for the expression
- `vocabulary_status` is looked up for the full expression surface (allowing the user to set status on the entire idiom)
- MWE groups take priority over conjugation groups (if tokens overlap, MWE wins)

**Caching**: MWE matches are computed once during `load_text()` and stored in `ReaderState.mwe_matches: Vec<Vec<MweMatch>>` (per-sentence). On `refresh_reader_display()`, the cached matches are re-applied without DB queries.

```rust
#[derive(Debug, Clone)]
pub struct MweMatch {
    pub start: usize,       // First token index in the sentence
    pub end: usize,         // One past last token index
    pub surface: String,    // Concatenated surface text
    pub reading: String,    // From JMdict
    pub gloss: String,      // English meaning from JMdict
}
```

##### 2B — Manual User Expressions

**DB migration (version 16)**: New `user_expressions` table:

```sql
CREATE TABLE IF NOT EXISTS user_expressions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    surface TEXT NOT NULL,
    reading TEXT NOT NULL DEFAULT '',
    gloss TEXT NOT NULL DEFAULT '',
    status INTEGER NOT NULL DEFAULT 0,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_user_expr_surface ON user_expressions(surface);
```

**UI for creating expressions**:
- In the reader, user navigates to a word, then enters "expression mode" (keybinding: `m` for "mark expression")
- Left/Right extends the selection range visually (all tokens in range highlight)
- Enter confirms → popup to edit reading and gloss → saved to `user_expressions`
- Expression is immediately detected in all sentences going forward

**User expressions take highest priority**: user expression > JMdict MWE > conjugation group.

**Integration into MWE detection**: During `load_text()`, user expressions are loaded from DB into `ReaderState.user_expression_cache: HashMap<String, UserExpression>`. The MWE detection loop checks user expressions FIRST (before JMdict), using the same sliding-window approach.

---

#### Files Modified

| File | Changes |
|---|---|
| `src/app.rs` | Add fields to `TokenDisplay`, add `MweMatch`/caches to `ReaderState`, modify `build_sentences()`, add `apply_groups()`, modify `load_text()` and `refresh_reader_display()` to call grouping, modify `set_word_status()` to propagate to group members |
| `src/core/tokenizer.rs` | Add `assign_conjugation_groups()`, `build_conjugation_description()`, `auxiliary_label()` functions |
| `src/ui/screens/reader.rs` | Modify selection: when `word_index` points to a group head, mark all group members as `is_selected` |
| `src/ui/components/sidebar.rs` | Show `conjugation_desc` instead of raw conjugation form/type; show `mwe_gloss` for MWE groups; show grouped surface in word list |
| `src/ui/components/furigana.rs` | No changes needed (status propagation handles highlighting automatically) |
| `src/main.rs` | Modify Left/Right navigation to skip non-head group members; add `m` keybinding for manual MWE creation |
| `src/db/schema.rs` | Add migration 16: `user_expressions` table |
| `src/db/models.rs` | Add `UserExpression` struct, CRUD functions, `MweMatch` persistence helpers |
| `src/core/dictionary.rs` | Add `has_jmdict_entry()` fast existence check, add `lookup_gloss_only()` for MWE detection |

#### Order of Implementation

1. Add new fields to `TokenDisplay` + `MweMatch` struct
2. Implement conjugation grouping logic + description generation in `tokenizer.rs`
3. Integrate grouping into `build_sentences()` / `load_text()` / `refresh_reader_display()`
4. Update word navigation in `main.rs` to skip group members
5. Update selection display in `reader.rs` for group highlighting
6. Update `sidebar.rs` to show conjugation descriptions and MWE glosses
7. Update `set_word_status()` to propagate status to group members
8. Add DB migration + models for `user_expressions` table
9. Implement automatic JMdict MWE detection during `load_text()`
10. Add manual MWE creation keybinding and UI
11. Build and verify with `data/sample_hard.txt`
