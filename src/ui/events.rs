use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, KeyEventKind};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::import::background::ImportEvent;

/// Events sent from background LLM workers to the TUI event loop.
#[derive(Debug, Clone)]
pub enum LlmEvent {
    /// An LLM sentence analysis completed successfully.
    AnalysisComplete {
        text_id: i64,
        sentence_index: usize,
        sentence_text: String,
        analysis: crate::core::llm::SentenceAnalysis,
        model: String,
        tokens_used: i64,
        cached: bool,
    },
    /// An LLM request failed.
    Failed { error: String },
}

/// Application events.
#[derive(Debug)]
#[allow(dead_code)]
pub enum Event {
    /// A key was pressed.
    Key(KeyEvent),
    /// A tick event (for UI updates, spinners, etc.).
    Tick,
    /// Terminal was resized.
    Resize(u16, u16),
    /// Mouse event (reserved for future use).
    Mouse(crossterm::event::MouseEvent),
    /// A background import event completed/started/failed.
    Import(ImportEvent),
    /// A background LLM event completed/failed.
    Llm(LlmEvent),
}

/// Event loop that polls crossterm events and sends them via mpsc channel.
pub struct EventLoop {
    rx: mpsc::Receiver<Event>,
    tx: mpsc::Sender<Event>,
}

impl EventLoop {
    /// Create a new event loop with the given tick rate.
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        let event_tx = tx.clone();

        thread::spawn(move || {
            let mut last_tick = Instant::now();
            loop {
                // Calculate timeout until next tick
                let timeout = tick_rate
                    .checked_sub(last_tick.elapsed())
                    .unwrap_or(Duration::ZERO);

                // Poll for crossterm events
                if event::poll(timeout).unwrap_or(false) {
                    match event::read() {
                        Ok(CrosstermEvent::Key(key)) if key.kind == KeyEventKind::Press => {
                            if event_tx.send(Event::Key(key)).is_err() {
                                return; // Channel closed, exit thread
                            }
                        }
                        Ok(CrosstermEvent::Resize(w, h)) => {
                            if event_tx.send(Event::Resize(w, h)).is_err() {
                                return;
                            }
                        }
                        Ok(CrosstermEvent::Mouse(m)) => {
                            if event_tx.send(Event::Mouse(m)).is_err() {
                                return;
                            }
                        }
                        _ => {}
                    }
                }

                // Send tick if enough time has elapsed
                if last_tick.elapsed() >= tick_rate {
                    if event_tx.send(Event::Tick).is_err() {
                        return;
                    }
                    last_tick = Instant::now();
                }
            }
        });

        Self { rx, tx }
    }

    /// Get a sender that can be used to inject events (e.g., from background workers).
    pub fn sender(&self) -> mpsc::Sender<Event> {
        self.tx.clone()
    }

    /// Receive the next event (blocking).
    pub fn next(&self) -> Result<Event, mpsc::RecvError> {
        self.rx.recv()
    }
}
