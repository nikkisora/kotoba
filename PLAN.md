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

## Phase 6 — Stats Screen

**Goal**: Visualize learning progress with terminal-based charts and metrics.

### 6.1 Stats Data Queries
- [ ] Implement `db/stats.rs`:
  - `known_words_over_time(conn, days: usize) -> Vec<(Date, usize)>`
  - `words_by_status(conn) -> HashMap<VocabularyStatus, usize>`
  - `reading_activity(conn, days: usize) -> Vec<(Date, usize)>`
  - `srs_stats(conn) -> SrsStats` — due_today, due_tomorrow, total_reviews, avg_accuracy, retention_rate
  - `text_coverage(conn, text_id: i64) -> CoverageStats` — total tokens, known tokens, learning tokens, new tokens, % coverage

### 6.2 Stats Screen UI
- [ ] Implement `ui/screens/stats.rs`:
  - **Overview panel**: Total vocabulary (known/learning/new), texts imported, total reviews
  - **Vocabulary growth chart**: ASCII/braille line chart showing known words over time (last 30/90/365 days)
  - **Status breakdown**: Horizontal stacked bar (New | L1 | L2 | L3 | L4 | Known | Ignored)
  - **Reading streak**: Calendar heatmap or simple "X days in a row" counter
  - **SRS panel**: Cards due today/tomorrow, review accuracy rate (last 7 days), retention rate
  - **Per-text coverage**: selectable list showing coverage % for each imported text
- [ ] Keybindings:
  - `↑`/`↓` — scroll between stat panels
  - `t` — toggle time range (7d / 30d / 90d / all)
  - `Enter` on a text in coverage list — jump to Reader for that text

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
