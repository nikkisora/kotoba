# kotoba — Terminal Japanese Language Learning App

A terminal-based language learning app for Japanese, inspired by LingQ and Lute.
Built with Rust, Ratatui, SQLite, and lindera/UniDic.

For remaining work, see `PLAN.md`.

---

## Architecture

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
+-------------------------------------------------------------+
|  +------------+   +------------------------------------+    |
|  | rusqlite   |   | reqwest (HTTP)                     |    |
|  +------------+   +------------------------------------+    |
+-------------------------------------------------------------+
```

## Tech Stack

| Component         | Choice                  | Rationale                                                      |
| ----------------- | ----------------------- | -------------------------------------------------------------- |
| Language          | Rust                    | Native performance, single binary, strict type safety          |
| TUI Framework     | `ratatui`               | Immediate-mode UI, industry standard for modern Rust TUIs      |
| Terminal Backend  | `crossterm`             | Cross-platform input handling and terminal manipulation        |
| CLI Framework     | `clap`                  | Standard declarative CLI argument parser                       |
| Async Runtime     | `tokio`                 | Handles concurrent HTTP requests (Scraping, APIs)              |
| Tokenizer         | `lindera` + UniDic      | Native Rust morphological analyzer with modern UniDic dictionary |
| Dictionary        | JMdict (XML → SQLite)   | Standard JP-EN dictionary, free, comprehensive                 |
| SRS Algorithm     | `fsrs`                  | Official Rust crate for the FSRS-5 spaced repetition algorithm |
| Database          | `rusqlite`              | Ergonomic synchronous wrapper around SQLite3                   |
| HTTP / API        | `reqwest`               | Standard HTTP client for web imports                           |
| Configuration     | `serde` + `toml`        | App settings stored in TOML                                    |

## Project Structure

```text
kotoba/
├── Cargo.toml
├── build.rs
├── DESCRIPTION.md          # This file
├── PLAN.md                 # Remaining work
├── src/
│   ├── main.rs             # CLI routing (clap) + TUI key handlers + event loop
│   ├── app.rs              # Central App state struct, all screen states, core logic
│   ├── config.rs           # TOML config parsing, AppConfig, SrsConfig
│   ├── ui/
│   │   ├── mod.rs          # Screen dispatch (render routing)
│   │   ├── events.rs       # crossterm key/tick event loop
│   │   ├── components/
│   │   │   ├── furigana.rs # CJK-aware furigana rendering + line layout
│   │   │   ├── text_area.rs# Scrollable paragraph rendering
│   │   │   ├── sidebar.rs  # Dictionary / context panel
│   │   │   ├── popup.rs    # All popup overlay renderers
│   │   │   └── status_bar.rs
│   │   └── screens/
│   │       ├── home.rs
│   │       ├── reader.rs
│   │       ├── review.rs
│   │       ├── library.rs
│   │       ├── chapter_select.rs
│   │       ├── card_browser.rs
│   │       └── settings.rs
│   ├── core/
│   │   ├── tokenizer.rs    # lindera/UniDic wrapper + conjugation grouping
│   │   ├── dictionary.rs   # SQLite JMdict lookups
│   │   └── srs.rs          # FSRS engine, card creation, review recording
│   ├── db/
│   │   ├── connection.rs   # rusqlite setup (WAL mode, busy_timeout)
│   │   ├── schema.rs       # 19 SQL migrations
│   │   └── models.rs       # Rust structs mapped to DB tables + CRUD functions
│   └── import/
│       ├── text.rs          # Plain text import
│       ├── web.rs           # URL / HTML import
│       ├── syosetsu.rs      # Syosetsu (小説家になろう) novel import
│       ├── epub.rs          # EPUB book import
│       ├── subtitle.rs      # SRT / ASS subtitle import
│       └── clipboard.rs     # Clipboard import (arboard)
└── data/
    └── JMdict_e.xml         # Seed dictionary data
```

---

## Screens

### Home
Dashboard with three panels: a GitHub-style **activity heatmap** (26 weeks, color-coded by daily activity intensity), a **quick stats** panel (streak, known/learning words, reviews today, due cards, 7-day accuracy), and the **recently read** list (up to 15 texts with progress bars, K/L/N% vocab breakdown). Tab switches focus between heatmap and text list; arrow keys navigate the heatmap to inspect individual days. Quick actions: `l` Library, `r` Review, `i` Import, `c` Card Browser, `s` Settings. Toggle finished texts with `f`.

### Library
Full list of all imported content — standalone texts and grouped multi-chapter sources (Syosetu, EPUB). Supports sorting (`s`: date desc/asc, title A-Z, completion %), filtering by source type (`f`), searching by title (`/`), and deletion (`d`). Enter opens text in Reader or source in ChapterSelect.

### ChapterSelect
Paginated chapter list (50/page) for multi-chapter sources. Shows chapter groups/arcs, reading state per chapter (skipped `S`, preprocessing `⠋`, not imported `—`, unread `○`, in progress `◐`, finished `●`). Skip toggle (`x`), manual preprocessing (`P`), page navigation (`p`/`n`). Non-blocking chapter open — queues import and auto-opens when ready. Auto-refreshes Syosetu chapter lists in background.

### Reader
Interactive sentence-by-sentence Japanese text reader with:
- **Main area** (70%): All text rendered with furigana above kanji, vocabulary colored by status, current sentence highlighted with `▶` marker
- **Sidebar** (30%): Current sentence breakdown with per-word readings, POS, JMdict glosses, conjugation descriptions, MWE glosses
- **Navigation**: Up/Down moves between sentences, Left/Right selects words (skips trivial tokens and non-head group members)
- **Status keys**: `1`-`4` set Learning (auto-creates SRS cards), `5` Known (retires cards), `i` Ignored
- **Translation**: `t` edit word translation, `T` edit sentence translation (creates SRS sentence_full card)
- **Expression marking**: `m` enters expression mode to mark multi-word expressions
- **Autopromotion**: New words auto-promoted to Known when advancing past a sentence; toggle with `a`, undo with `Ctrl+Z`
- **Clipboard**: `c` copy word, `C` copy sentence
- **Auto-advance**: Automatically moves to next chapter at end of text

### Review
FSRS-powered flashcard review with two card types:
- **Word Review**: Show word + context sentence (randomly chosen from all encountered sentences), recall reading and meaning. Optionally type the reading if `require_typed_input` is on.
- **Sentence Cloze** (optional variant): When `enable_sentence_cloze` is on, word cards randomly show a sentence cloze variant (word blanked, recall the word) with configurable probability.
- **Sentence Full**: Full Japanese sentence, recall English translation.

All modes show sentence context with vocabulary coloring. Context words are navigable (Left/Right) and tappable (Enter for dictionary). Rating: `1` Again, `2` Hard, `3`/Space Good, `4` Easy. Session summary shown at end with accuracy stats.

### Card Browser
Browse all SRS cards with filtering (All, Due Now, Word Cards, Sentence Cards, New, Learning, Review, Retired) and sorting (Due Date, Created Date, Word). Column-aligned display with unicode-width-aware padding. Per-card actions: `r` reset to New, `d` delete.

### Settings
Two-panel settings editor with categories (Reader, SRS). Supports Bool toggles (Enter/Space), Integer inputs (Enter to edit), and Choice cycling (Enter/Space to cycle through options like answer mode and review order). Left/Right/Tab switches categories. Auto-saves on exit.

---

## CLI Commands

| Command | Description |
| --- | --- |
| `kotoba` | Launch the TUI (default when no subcommand given) |
| `kotoba run` | Launch the TUI (explicit) |
| `kotoba import <file>` | Import content (auto-detects: .txt, .srt, .ass, .ssa, .epub) |
| `kotoba import --clipboard` | Import from system clipboard |
| `kotoba import --url <URL>` | Import from a web URL |
| `kotoba tokenize <text>` | Tokenize Japanese text (debug output) |
| `kotoba dict <word>` | Look up a word in JMdict |
| `kotoba import-dict <path>` | Import JMdict XML into the database |
| `kotoba setup-dict` | Download and set up JMdict automatically |
| `kotoba syosetu <ncode>` | Import Syosetu novel (with `--chapter N` for specific chapter) |

Global flag: `--config <path>` for custom config file location.

---

## Import Formats

| Format | Extensions / Source | Details |
| --- | --- | --- |
| Plain text | `.txt` or unrecognized | Split into paragraphs → sentences → tokenize with UniDic |
| Web / URL | `--url <URL>` | Fetch HTML, extract article text via CSS selectors |
| Syosetu | ncode or URL | Novel metadata + per-chapter import with background preprocessing |
| EPUB | `.epub` | Parse ZIP, read spine order from content.opf, per-chapter texts |
| Subtitle (SRT) | `.srt` | Strip timing/numbering, extract dialogue |
| Subtitle (ASS/SSA) | `.ass`, `.ssa` | Parse Advanced SubStation Alpha dialogue lines |
| Clipboard | `--clipboard` | Read system clipboard via arboard |

---

## Vocabulary Status System

| Value | Status     | Furigana | Color      | SRS Behavior |
| ----- | ---------- | -------- | ---------- | ------------ |
| -1    | Ignored    | Hide     | Default    | Retires cards |
| 0     | New        | Show     | Blue bg    | Autopromoted to Known on sentence advance |
| 1     | Learning 1 | Show     | Yellow bg  | Auto-creates word card, collects sentence contexts |
| 2     | Learning 2 | Show     | Lighter bg | Auto-creates word card, collects sentence contexts |
| 3     | Learning 3 | Show     | Lighter bg | Auto-creates word card, collects sentence contexts |
| 4     | Learning 4 | Hide     | Lighter bg | Auto-creates word card, collects sentence contexts |
| 5     | Known      | Hide     | Default    | Retires cards |

---

## SRS System

### FSRS Engine
- Uses `fsrs` crate with `DEFAULT_PARAMETERS`, 90% desired retention
- Card states: `new`, `learning`, `review`, `relearning`, `retired`
- Again rating: 10-minute re-study interval
- Hard/Good/Easy: FSRS-computed intervals (minimum 1 day)
- Tracks stability, difficulty, reps, lapses per card
- Review logs stored with timing, typed answer, and correctness

### Card Lifecycle
- **Creation**: Setting vocabulary to Learning (1-4) auto-creates a word card and collects the current sentence as context. Saving a sentence translation (`T`) creates a sentence_full card.
- **Sentence Collection**: When advancing past a sentence containing Learning words, the sentence is added to the word's collection. During review, a random sentence from the collection is shown as context.
- **Retirement**: Setting vocabulary to Known (5) or Ignored (-1) retires all active cards for that word.
- **Sentence Cloze**: Disabled by default. When enabled in settings, word cards randomly show a sentence cloze variant (word blanked in sentence) with configurable probability.

### Review Session
- Pre-session summary with card counts
- Configurable: `new_cards_per_day`, `max_reviews_per_session`, `review_order` (due_first or random)
- Post-session summary with accuracy stats

---

## Tokenizer & NLP

### lindera + UniDic
- Morphological analysis with embedded UniDic dictionary
- Extracts: surface, base_form, reading (hiragana), POS, conjugation_form, conjugation_type
- POS categories: Noun, Pronoun, Verb, Adjective, Adjectival_Noun, Adverb, Particle, Auxiliary, Conjunction, Symbol, Interjection, Adnominal, Prefix, Suffix, Whitespace, Filler, Other

### Sentence Splitting
- Splits on `。`, `！`, `？`, and newlines
- Bracket/quote nesting awareness — does not split inside matched `「」`, `『』`, `（）`, etc.

### Conjugation Grouping
Post-tokenization pass that merges verb/adjective stems with following auxiliaries into display groups:
- All group members share the head word's vocabulary status for consistent highlighting
- Navigation skips non-head group members
- Human-readable descriptions generated: "verb, negative, past", "adjective, causative, want to", etc.
- Auxiliary labels: ない (negative), ます (polite), た/だ (past), て/で (te-form), れる/られる (passive/potential), せる/させる (causative), たい (want to), う/よう (volitional), and more

### Multi-Word Expression (MWE) Detection
- Sliding window (2-12 tokens) checks concatenated surfaces against:
  1. User-created expressions (highest priority, stored in `user_expressions` table)
  2. JMdict kanji entries (dictionary fallback)
- Greedy longest-match strategy
- Expression marking mode in Reader: `m` to enter, Left/Right to extend range, Enter to save

---

## Background Import System

- `BackgroundImporter` with 3-thread worker pool and `mpsc` channel
- Two-phase pipeline: Phase 1 (HTTP fetch + tokenize, parallelizable), Phase 2 (short DB transaction)
- Eager preprocessing: auto-queues next N unimported chapters (configurable via `preprocess_ahead`)
- Priority queuing: user-initiated opens go to front of queue
- Cancellation support (chapters marked as skipped)
- Auto-open: when queued chapter finishes importing, automatically navigates to Reader
- Syosetu auto-refresh: checks for new chapters when ChapterSelect is opened
- Standalone imports (file, clipboard, URL) also run in background from TUI
- Progress display: Braille spinner animation with phase/percent

---

## Configuration

TOML config file. Lookup order: `--config` flag → `~/.config/kotoba/kotoba.toml` → `./kotoba.toml` → defaults.

### [reader]
| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `sidebar_width` | u16 | 30 | Sidebar width percentage (10-80) |
| `furigana` | bool | true | Show furigana above kanji |
| `preprocess_ahead` | usize | 3 | Chapters to preprocess ahead (0-20) |

### [srs]
| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `new_cards_per_day` | u32 | 20 | New card limit per session |
| `max_reviews_per_session` | u32 | 0 | Review cap (0 = unlimited) |
| `review_order` | string | `"due_first"` | Card order: `due_first` or `random` |
| `require_typed_input` | bool | false | Type reading for word cards |
| `enable_sentence_cloze` | bool | false | Enable sentence cloze variant for word cards |
| `sentence_cloze_ratio` | u32 | 50 | Probability (0-100) of showing cloze variant |

### [llm] (infrastructure only, not yet active)
| Setting | Type | Default |
| --- | --- | --- |
| `endpoint` | string | `"https://api.openai.com/v1"` |
| `api_key` | string | `""` |
| `model` | string | `"gpt-4o"` |
| `max_tokens` | usize | 2048 |

---

## Database Schema (19 migrations)

| Table | Purpose |
| --- | --- |
| `texts` | Imported texts with title, content, source metadata, reading progress |
| `paragraphs` | Paragraphs within texts |
| `tokens` | Morphological tokens (surface, base_form, reading, POS, conjugation) |
| `vocabulary` | User vocabulary entries with status, notes, custom translation |
| `vocabulary_sentences` | All sentences where a vocabulary word appears (for random context in review) |
| `conjugation_encounters` | Tracks conjugated forms encountered per vocabulary item |
| `srs_cards` | SRS flashcards with FSRS state, card type |
| `srs_reviews` | Review log with timing, rating, typed answer |
| `llm_cache` | Cache for future LLM API responses |
| `jmdict_entries` | JMdict dictionary entries (JSON blob) |
| `jmdict_kanji` | JMdict kanji element index |
| `jmdict_readings` | JMdict reading element index |
| `web_sources` | Multi-chapter sources (Syosetu, EPUB) |
| `web_source_chapters` | Chapters within sources with import/skip state |
| `user_expressions` | User-created multi-word expressions |
| `sentence_translations` | Sentence translations (for SRS sentence_full cards) |

---

## Popup Types

| Popup | Trigger | Purpose |
| --- | --- | --- |
| WordDetail | `Enter` in Reader/Review | Full dictionary entry with all senses, conjugation history, notes |
| Help | `?` anywhere | Scrollable keybinding reference |
| NoteEditor | `n` in Reader | Edit vocabulary notes |
| TranslationEditor | `t` in Reader | Edit custom word translation |
| SentenceTranslationEditor | `T` in Reader | Edit sentence translation (creates SRS card) |
| ExpressionTranslation | `Enter` after marking expression | Confirm/edit MWE reading and gloss |
| QuitConfirm | `q` in Reader | Confirm quit (saves progress) |
| DeleteConfirm | `d` in Library (text) | Confirm text deletion |
| DeleteSourceConfirm | `d` in Library (source) | Confirm source + all chapters deletion |
| DeleteCardConfirm | `d` in CardBrowser | Confirm SRS card deletion |
| ImportMenu | `i` in Home/Library | Sub-menu: clipboard, URL, file path, Syosetu |
| UrlInput | `u` from ImportMenu | URL text input |
| FilePathInput | `f` from ImportMenu | File path text input |
| SyosetuInput | `s` from ImportMenu | Syosetu ncode text input |
| SearchInput | `/` in Library | Search query text input |
