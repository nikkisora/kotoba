# kotoba

A terminal-based Japanese language learning app. Read native content, look up words, track vocabulary, and review with spaced repetition directly in your terminal.

<p align="center">
  <img src="data/reader.png" alt="kotoba" width="100%">
</p>

## Features

- **Immersive reader** -- Read sentence-by-sentence with furigana. The sidebar shows readings, dictionary definitions, and word types.
- **Smart text splitting** -- Japanese text is automatically broken down into distinct words and grammar points.
- **Vocabulary tracking** -- Track your progress. Mark words as New, Learning (stages 1-4), Known, or Ignored.
- **Built-in flashcards** -- Create flashcards automatically as you read and review them using a spaced-repetition system.
- **Offline dictionary** -- Includes a comprehensive Japanese-English dictionary (JMdict).
- **AI sentence analysis** -- Get context-aware sentence breakdowns using your preferred AI provider.
- **Flexible imports** -- Read `.txt`, `.epub`, subtitles (`.srt`/`.ass`), web articles, Syosetu novels, or content straight from your clipboard.

## Quick Start

```bash
# 1. Download the latest release (see Installation below)
# 2. Set up the offline dictionary (downloads $\approx 25$ MB)
kotoba setup-dict

# 3. Import something to read
kotoba import my-text.txt

# 4. Launch the app
kotoba
```

---

## Installation

Download the pre-built binary for your system from the [Releases](https://github.com/youruser/kotoba/releases) page:

| Platform | File |
|----------|------|
| Linux | `kotoba-linux-x86_64.tar.gz` |
| macOS (Intel) | `kotoba-macos-x86_64.tar.gz` |
| macOS (Apple Silicon)| `kotoba-macos-aarch64.tar.gz` |
| Windows | `kotoba-windows-x86_64.zip` |

Extract the archive, and you will get a single `kotoba` executable file.

<details>
<summary><strong>Advanced: Adding `kotoba` to your PATH</strong></summary>

To run `kotoba` from anywhere, move it to a directory included in your system's `PATH`.

**Linux / macOS:**
```bash
mkdir -p ~/.local/bin
mv kotoba ~/.local/bin/
# Add to ~/.bashrc or ~/.zshrc if not already present:
export PATH="$HOME/.local/bin:$PATH"
```
*(On macOS, you might need to run `xattr -d com.apple.quarantine kotoba` to remove the download warning).*

**Windows:**
Move `kotoba.exe` to a dedicated CLI tools folder and add that folder to your Environment Variables, or drop it into an existing folder already on your PATH.

*(Note: `curl` is required to run the initial `kotoba setup-dict` command. It comes pre-installed on macOS/Windows and most Linux distributions.)*

</details>

---

## Getting Started

### 1. Dictionary Setup
Run this command once to install the Japanese-English dictionary:
```bash
kotoba setup-dict
```

### 2. Import Content
You can import files directly from your terminal or by pressing `i` inside the app.
```bash
kotoba import book.epub
kotoba import anime.srt
kotoba import --url https://example.com/article
kotoba import --clipboard
```

### 3. Read & Learn
Type `kotoba` to open the app. Select your imported text from the Library to begin reading.

## Usage Guide

**While Reading:**
*   Use your arrow keys to navigate between sentences and words.
*   The sidebar automatically translates and explains the currently highlighted word.
*   Press keys `1` through `4` to add a word to your flashcards as a "Learning" word.
*   Press `5` for words you already know, or `i` to ignore grammar particles and names you don't want to track.

**Reviewing Flashcards:**
*   Press `r` from the Home screen to start a review session.
*   You will be tested on the words you marked while reading.
*   Rate how well you remembered the word to schedule its next review.

---

<details>
<summary><strong>App Screens Overview</strong></summary>

*   **Home:** View your learning streak, vocabulary stats, and recently read texts.
*   **Library:** Browse and search all your imported content.
*   **Reader:** The main reading interface with furigana and a tracking sidebar.
*   **Review:** Flashcard testing interface.
*   **Card Browser:** Manage, edit, or delete your flashcards.
*   **Stats:** View charts of your learning progress and review accuracy.
*   **Settings:** Customize themes, reading preferences, and AI endpoints.

</details>

<details>
<summary><strong>Configuration & Theming</strong></summary>

Settings can be changed within the app or by editing `~/.config/kotoba/kotoba.toml`. Run `kotoba config` to view your current setup.

**AI Setup:**
By default, `kotoba` can connect to OpenAI-compatible APIs (like OpenRouter or local models via Ollama) to explain complex sentences. You can add your API key and model choice in the Settings screen.

**Theming:**
`kotoba` includes `tokyo-night`, `light`, `solarized-light`, and `gruvbox`. You can create custom `.toml` theme files in `~/.local/share/kotoba/themes/` to override specific colors.

</details>

<details>
<summary><strong>CLI Reference</strong></summary>

```text
kotoba                              Launch the app
kotoba import <file>                Import a text, EPUB, or subtitle file
kotoba import --url <URL>           Import a web article
kotoba import --clipboard           Import copied text
kotoba syosetu <ncode>              Import a Syosetu web novel
kotoba setup-dict                   Download the dictionary
kotoba import-dict <path>           Import a local dictionary file
kotoba config                       Show configuration info
```

</details>

<details>
<summary><strong>Keybindings</strong></summary>

**Global:** `q` (Quit), `?` (Help), `Esc` (Back)

**Reader:**
*   `Up`/`Down` -- Change sentence
*   `Left`/`Right` -- Change word
*   `1` - `4` -- Mark as Learning
*   `5` -- Mark as Known
*   `i` -- Mark as Ignored
*   `t` -- Provide translation for a word
*   `T` -- Provide translation for a sentence
*   `g` -- Look up word on Jisho
*   `G` -- Translate the sentence in deepl or google translate (will open up a browser tab)
*   `Ctrl+T` -- AI sentence analysis (requires setup)

**Review:**
*   `Space` -- Reveal answer / Rate Good
*   `1` -- Rate Again
*   `2` -- Rate Hard
*   `4` -- Rate Easy
*   `Enter` -- Extra word details

</details>

---

<details>
<summary><strong>Building from Source & Architecture</strong></summary>

### Building limits
Ensure you have the Rust toolchain (Edition 2021+) and a C compiler installed (required for bundling SQLite).

```bash
git clone https://github.com/youruser/kotoba.git
cd kotoba
cargo build --release
```
The binary will be located at `target/release/kotoba`.

### Architecture
*   **UI:** `ratatui` + `crossterm`
*   **Database:** `rusqlite` (bundled)
*   **Tokenization:** `lindera` (UniDic embedded)
*   **Scheduling:** `fsrs` algorithm
*   **Data directory:** `~/.local/share/kotoba/` (stores the local database and XML dictionary).

</details>

## License

This project uses the [JMdict](https://www.edrdg.org/wiki/index.php/JMdict-EDICT_Dictionary_Project) dictionary, property of the [Electronic Dictionary Research and Development Group](https://www.edrdg.org/), used in conformance with their [licence](https://www.edrdg.org/edrdg/licence.html).
