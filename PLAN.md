# kotoba — Remaining Work

Everything below is **not yet implemented**. For a description of what the app currently does, see `DESCRIPTION.md`.

---

## Phase 5 — LLM Integration ✅

**Goal**: On-demand structured sentence analysis via OpenAI-compatible API.

### 5.1 LLM Client
- [x] Implement `core/llm.rs`:
  - `analyze_sentence()` function with `LlmConfig` (endpoint, api_key, model, max_tokens)
  - Load config from TOML `[llm]` section
  - System prompt bundled at compile time via `include_str!("data/system_prompt.txt")`
  - Structured JSON output: `translation`, `component_breakdown` (japanese/romaji/meaning), `explanation`
  - POST to `{endpoint}/chat/completions` with `response_format: { type: "json_object" }`
  - Parse response into `SentenceAnalysis` struct; handles markdown code fences
  - Context support: up to 3 previous sentences included in user message
  - Default: OpenRouter endpoint, `google/gemini-3.1-flash-lite-preview` model

### 5.2 Caching Layer
- [x] Before making API call, hash `sha256(sentence + model)` and check `llm_cache` table
- [x] On cache hit: deserialize stored JSON response, return immediately
- [x] On cache miss: make API call, store response + metadata (model, tokens_used) in `llm_cache`
- [x] Cache invalidation: `kotoba cache clear` and `kotoba cache stats` CLI commands

### 5.3 Async TUI Integration
- [x] In `ui/events.rs`, `LlmEvent` enum with `AnalysisComplete` and `Failed` variants
- [x] `Ctrl+T` in Reader toggles LLM sidebar (`l` reserved for vim-style navigation):
  - Check cache first (synchronous, fast)
  - If cache miss: spawn `std::thread::spawn` (sync mpsc event loop) with blocking HTTP call
  - Show loading spinner in sidebar: rotating braille animation
  - On completion: send result through `mpsc` channel -> event loop picks it up -> update sidebar
- [x] Display LLM result in sidebar (replacing word list temporarily):
  - **Translation**: full sentence translation at top
  - **Component breakdown**: table of japanese + romaji + meaning
  - **Explanation**: free-form contextual explanation
  - Press `Esc` or `Ctrl+T` again to dismiss and return to JMdict word list view
- [x] LLM translation saved to `sentence_translations` table (same as user-provided)
- [x] LLM settings configurable in Settings screen (endpoint, api_key, model, max_tokens)

### 5.4 LLM in SRS Review
- [x] `Ctrl+L` in SRS review triggers LLM analysis of the card's source sentence
- [x] Result shown in a side panel
- [x] Same caching applies — most sentences will already be cached from Reader usage

---

## Phase 5.5 — Home Screen Activity Dashboard ✅

**Goal**: Enrich the home screen with an interactive activity heatmap and quick stats.

### 5.5.1 Daily Activity Tracking
- [x] New `daily_activity` table (migration 20): date, sentences_read, reviews_completed, words_learned
- [x] Increment counters automatically: sentence advance in Reader, review completion, word status → Learning1

### 5.5.2 Activity Queries
- [x] `get_daily_activity(conn, days)` — activity records for heatmap grid
- [x] `get_activity_streak(conn)` — consecutive days with any activity (reading or SRS)
- [x] `get_vocabulary_summary(conn)` — total known/learning word counts
- [x] `get_reviews_today(conn)` — reviews completed today
- [x] `get_recent_accuracy(conn)` — 7-day review accuracy percentage

### 5.5.3 Home Screen Redesign
- [x] Layout: Activity heatmap (65%) + Quick Stats panel (35%) above the text list
- [x] GitHub-style contribution heatmap: 26 weeks (~6 months), Mon-Sun rows
  - 5 color intensity levels: empty, low (1-19), moderate (20-79), high (80-159), max (160+)
  - Month labels along the top, day-of-week labels on the left
  - Today highlighted with underline
  - Legend with color scale
- [x] Quick Stats panel: streak (with fire emoji), words known/learning, reviews today, due now, 7-day accuracy
- [x] Two-panel focus system: Tab/BackTab switches between Heatmap and TextList
  - Heatmap: arrow keys navigate cursor, selected day shows detailed breakdown (reviews/sentences/words)
  - TextList: existing navigation (Up/Down, Enter to open, f to toggle finished)
  - Active panel border highlighted with accent color
- [x] Heatmap colors added to Theme struct: `heatmap_empty`, `heatmap_low`, `heatmap_mid`, `heatmap_high`, `heatmap_max`, `heatmap_cursor`
  - All 4 built-in themes updated with appropriate heatmap colors
  - Custom theme TOML overrides supported via `[ui]` section

---

## Phase 6 — Stats Screen ✅

**Goal**: Visualize learning progress with terminal-based charts and metrics.

### 6.1 Stats Data Queries
- [x] Implement `db/stats.rs`:
  - `get_known_words_over_time(conn, days)` — cumulative Known word count over time with baseline
  - `get_words_by_status(conn)` — `HashMap<VocabularyStatus, usize>` breakdown of all status levels
  - `get_reading_activity(conn, days)` — daily reading activity (delegates to `daily_activity` table)
  - `get_srs_stats(conn) -> SrsStats` — due today/tomorrow, total reviews, reviews today, 7d/30d accuracy, retention rate, card counts by state
  - `get_text_coverage(conn, text_id) -> CoverageStats` — total/known/learning/new/ignored tokens + coverage %
  - `get_all_text_coverages(conn)` — coverage for all read texts
  - `get_overview_stats(conn) -> OverviewStats` — texts read, total/known/learning/new/ignored word counts

### 6.2 Stats Screen UI
- [x] Implement `ui/screens/stats.rs`:
  - **Overview panel**: texts read, total vocabulary, known/learning/new/ignored counts, activity streak
  - **Vocabulary growth chart**: ASCII block-character line chart showing cumulative Known words over time
  - **Status breakdown**: horizontal stacked color bar (Known | L1 | L2 | L3 | L4 | New | Ignored) with legend percentages
  - **SRS panel**: due now/tomorrow, reviews today/total, 7d/30d accuracy, retention rate, card counts by state
  - **Per-text coverage**: selectable list with coverage bar + percentage, detail panel for selected text
- [x] Two-panel focus: Tab switches between stats panels (left) and coverage list (right), active panel highlighted
- [x] Keybindings:
  - `Tab`/`BackTab` — switch focus between stats and coverage panels
  - `↑`/`↓` — scroll stats or navigate coverage list
  - `t` — toggle time range (7d / 30d / 90d / All) — reloads data for new range
  - `Enter` on a text in coverage list — jump to Reader for that text
  - `Esc` — back to previous screen
- [x] Accessible from Home screen via `S` key

---

## Phase 7 — Theming, Configuration & Polish

**Goal**: Customization, UX refinements, and release readiness.

### 7.1 Theming Engine
- [x] Implement theme loading from `theme.toml`:
  - Parse hex colors into `Color::Rgb(r, g, b)`
  - Provide 2-3 built-in themes: Tokyo Night (dark), Solarized Light, Gruvbox
  - Fallback to 256-color or 16-color palette if terminal doesn't support RGB
- [x] Thread theme through all 14 UI files, replacing all hardcoded colors
- [x] Theme selection in Settings screen with live preview

### 7.2 Configuration
- [x] CLI command `kotoba config` — print current config location and values
- [x] XDG-compliant paths: config in `$XDG_CONFIG_HOME/kotoba/`, data in `$XDG_DATA_HOME/kotoba/`

### 7.3 UX Polish
- [x] Consistent popup system: all popups use same border style, close with `Esc`, support scrolling
- [x] Error handling: user-friendly error messages in status bar (not panics)
- [ ] Loading states: spinner for any operation > 200ms
- [x] First-run experience: detect missing JMdict, show warning banner on home screen
- [x] Mouse support: click on words in Reader to select them, scroll wheel to navigate sentences

### 7.4 Build & Distribution
- [x] `cargo build --release` — single static binary
- [x] GitHub Actions CI: build for Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64)
- [x] Include JMdict download via `kotoba setup-dict` command
- [x] Write `--help` text for all CLI subcommands (with long_about descriptions)
- [ ] Man page generation via `clap_mangen` (requires lib+bin restructure)

---

## Phase 8 — Future Enhancements (Deferred)

- [ ] **AnkiConnect export**: One-way sync to push vocabulary cards to Anki via AnkiConnect API
- [ ] **Pitch accent data**: Integrate OJAD or NHK accent dictionary, display in word detail popup
- [ ] **Audio/TTS**: System TTS or cloud TTS API for word/sentence pronunciation
- [ ] **Additional web sources**: News sites (NHK News Easy, Asahi), custom per-source TUI screens
- [ ] **PDF import**: Extract text layers from PDFs
- [ ] **Multi-language support**: Generalize beyond Japanese (Chinese, Korean — different tokenizers)
- [ ] **Cloud sync**: Optional sync of vocabulary/SRS state across devices
- [ ] **Plugin system**: User-defined import sources or LLM prompt templates
