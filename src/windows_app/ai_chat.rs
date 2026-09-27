//! The AI Assistant's conversation: connecting to a model server, sending
//! questions (with the selected code), and streaming answers into the panel.
//! lightline::ai does the HTTP; render/ai_assistant.rs paints the panel.
//!
//! Nothing runs until the user connects or sends a message: the assistant
//! is only state until then. Each request runs on its own thread, and the
//! pieces of an answer are batched so a fast model causes at most one
//! repaint per ANSWER_FLUSH, however many words it sends.

use super::markdown_view::{MarkdownSnippet, SnippetFonts};
use super::*;
use lightline::ai::{self, Message, Role};
use std::cell::Cell;

pub(super) const AI_EVENT_MESSAGE: u32 = WM_APP + 10;

const ANSWER_FLUSH: Duration = Duration::from_millis(50);
// Selected code sent along with a question: at most this much.
const MAX_CONTEXT_LINES: usize = 400;
const MAX_CONTEXT_BYTES: usize = 24 * 1024;
// Earlier messages sent along for context, newest first, up to this size.
const MAX_HISTORY_BYTES: usize = 48 * 1024;
const MAX_INPUT_BYTES: usize = 16 * 1024;

/// The model a first-time user is told to download.
pub(super) const SUGGESTED_MODEL: &str = "qwen2.5-coder:7b";

const SYSTEM_PROMPT: &str = "You are the coding assistant in LightLine, a code \
editor. Answer concisely and accurately. Use Markdown, and put code in fenced \
code blocks tagged with the language.";

enum AiEvent {
    Models(Result<Vec<String>, String>),
    Text(u64, String),
    Done(u64, Result<(), String>),
}

pub(super) struct ChatEntry {
    pub(super) role: Role,
    /// The question as typed, or the answer so far.
    pub(super) text: String,
    /// What was sent to the model for a question: the text plus any code.
    prompt: String,
    /// The code sent with a question, e.g. "app.rs, lines 40–82".
    pub(super) context: Option<String>,
    /// Why an answer ended early.
    pub(super) error: Option<String>,
    /// Bumped whenever `text` changes, so the answer is laid out again.
    pub(super) revision: u64,
    pub(super) view: MarkdownSnippet,
}

impl ChatEntry {
    fn new(role: Role, text: String, prompt: String, context: Option<String>) -> Self {
        Self {
            role,
            text,
            prompt,
            context,
            error: None,
            revision: 0,
            view: MarkdownSnippet::default(),
        }
    }
}

/// Where the panel's buttons were painted, for mouse clicks.
#[derive(Default, Clone)]
pub(super) struct AiHits {
    pub(super) connect: Option<RECT>,
    pub(super) model: Option<RECT>,
    pub(super) new_chat: Option<RECT>,
    pub(super) composer: Option<RECT>,
    pub(super) send: Option<RECT>,
    /// Copy buttons and the text each one copies.
    pub(super) copies: Vec<(RECT, String)>,
}

pub(super) struct AiChat {
    pub(super) entries: Vec<ChatEntry>,
    /// The message being typed.
    pub(super) input: String,
    /// Whether the message box has keyboard focus.
    pub(super) focused: bool,
    /// How far the conversation is scrolled, in pixels from its top.
    pub(super) scroll: Cell<i32>,
    /// Keep the newest text in view while an answer arrives, until the
    /// user scrolls up.
    pub(super) follow: Cell<bool>,
    /// The chat models the server listed on the last connect.
    pub(super) models: Vec<String>,
    pub(super) connecting: bool,
    /// Why connecting failed.
    pub(super) problem: Option<String>,
    // The answer being received: its id and its cancel flag.
    request: Option<(u64, Arc<AtomicBool>)>,
    next_request: u64,
    pub(super) fonts: SnippetFonts,
    pub(super) hits: RefCell<AiHits>,
    tx: Sender<AiEvent>,
    rx: Receiver<AiEvent>,
}

impl AiChat {
    pub(super) fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            entries: Vec::new(),
            input: String::new(),
            focused: false,
            scroll: Cell::new(0),
            follow: Cell::new(true),
            models: Vec::new(),
            connecting: false,
            problem: None,
            request: None,
            next_request: 0,
            fonts: SnippetFonts::default(),
            hits: RefCell::default(),
            tx,
            rx,
        }
    }

    /// Whether an answer is being received.
    pub(super) fn busy(&self) -> bool {
        self.request.is_some()
    }
}

// The language tag for a fenced code block, from the file's extension.
fn fence_language(path: Option<&Path>) -> String {
    path.and_then(Path::extension)
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

// `text` cut to MAX_CONTEXT_LINES lines and MAX_CONTEXT_BYTES bytes.
fn clip_context(text: &str) -> (String, bool) {
    let mut clipped = String::new();
    for (index, line) in text.split('\n').enumerate() {
        if index >= MAX_CONTEXT_LINES || clipped.len() + line.len() + 1 > MAX_CONTEXT_BYTES {
            return (clipped, true);
        }
        if index > 0 {
            clipped.push('\n');
        }
        clipped.push_str(line);
    }
    (clipped, false)
}

impl App {
    /// Whether a model is chosen, i.e. the assistant is on.
    pub(super) fn ai_ready(&self) -> bool {
        self.settings.ai_model.is_some()
    }

    /// Whether typing goes to the assistant's message box.
    pub(super) fn ai_typing(&self) -> bool {
        self.ai.focused
            && self.ai_assistant_visible
            && self.ai_ready()
            && !self.welcome
            && !self.quick_open
    }

    pub(super) fn focus_ai_input(&mut self) {
        self.ai.focused = true;
        self.terminal_focus = false;
        self.panel_focus = false;
    }

    /// Asks the server which models it has: the Connect button, and the
    /// model menu's refresh.
    pub(super) fn ai_connect(&mut self, hwnd: HWND) {
        if self.ai.connecting {
            return;
        }
        self.ai.connecting = true;
        self.ai.problem = None;
        let endpoint = self.settings.ai_endpoint.clone();
        let tx = self.ai.tx.clone();
        let hwnd_value = hwnd as isize;
        std::thread::spawn(move || {
            let _ = tx.send(AiEvent::Models(ai::list_models(&endpoint)));
            unsafe { PostMessageW(hwnd_value as HWND, AI_EVENT_MESSAGE, 0, 0) };
        });
        unsafe { InvalidateRect(hwnd, null(), 0) };
    }

    /// Takes in what the worker threads sent (AI_EVENT_MESSAGE).
    pub(super) fn poll_ai(&mut self, hwnd: HWND) {
        let mut changed = false;
        while let Ok(event) = self.ai.rx.try_recv() {
            changed = true;
            match event {
                AiEvent::Models(result) => self.ai_models_listed(result),
                AiEvent::Text(id, text) => {
                    if self.ai_current(id)
                        && let Some(entry) = self.ai.entries.last_mut()
                    {
                        entry.text.push_str(&text);
                        entry.revision += 1;
                    }
                }
                AiEvent::Done(id, result) => {
                    if self.ai_current(id) {
                        self.ai.request = None;
                        if let (Err(error), Some(entry)) = (result, self.ai.entries.last_mut()) {
                            entry.error = Some(error);
                        }
                    }
                }
            }
        }
        if changed {
            unsafe { InvalidateRect(hwnd, null(), 0) };
        }
    }

    fn ai_current(&self, id: u64) -> bool {
        self.ai
            .request
            .as_ref()
            .is_some_and(|(current, _)| *current == id)
    }

    fn ai_models_listed(&mut self, result: Result<Vec<String>, String>) {
        self.ai.connecting = false;
        let models = match result {
            Ok(models) => ai::chat_models(&models),
            Err(error) => {
                // Nothing running at Ollama's address: say how to start it.
                let hint = if error.starts_with(ai::NOTHING_ANSWERED)
                    && self.settings.ai_endpoint == ai::DEFAULT_ENDPOINT
                {
                    " Is Ollama running? Open the Ollama app (or run: ollama serve), then try again."
                } else {
                    ""
                };
                self.ai.problem = Some(format!("{error}{hint}"));
                return;
            }
        };
        self.ai.models = models;
        if self.ai.models.is_empty() {
            self.ai.problem = Some(format!(
                "The server has no chat models yet. In a terminal, run: ollama pull {SUGGESTED_MODEL}"
            ));
            return;
        }
        self.ai.problem = None;
        let current = self.settings.ai_model.clone();
        if current.is_some_and(|model| self.ai.models.contains(&model)) {
            return;
        }
        match ai::pick_model(&self.ai.models) {
            Some(model) => {
                self.set_ai_model(Some(model));
                if self.ai_assistant_visible {
                    self.focus_ai_input();
                }
            }
            // Only cloud models: none is picked for the user, since their
            // questions and code would leave the PC.
            None => {
                self.ai.problem = Some(format!(
                    "Only cloud models are installed. They run on ollama.com, not on your \
                     PC. For a local model, run in a terminal: ollama pull {SUGGESTED_MODEL}"
                ));
            }
        }
    }

    fn set_ai_model(&mut self, model: Option<String>) {
        self.settings.ai_model = model;
        let label = self.settings.ai_model.as_deref().map_or_else(
            || "AI Assistant turned off".to_string(),
            |model| format!("AI model: {model}"),
        );
        self.status = match self.settings.save() {
            Ok(()) => label,
            Err(error) => format!("{label} (not saved: {error})"),
        };
    }

    /// The selected code the next question includes: its label and text.
    fn ai_selection(&self) -> Option<(String, String)> {
        let label = self.ai_selection_label()?;
        let (start, end) = self.selection_range()?;
        let text = self.doc().text_range(start, end);
        (!text.trim().is_empty()).then_some((label, text))
    }

    /// What the next question will include, e.g. "app.rs, lines 40–82";
    /// None without a selection. Cheap enough for every repaint: the
    /// selected text itself is only read when a message is sent.
    pub(super) fn ai_selection_label(&self) -> Option<String> {
        if self.welcome || self.tab().read_only() || self.tab().is_placeholder() {
            return None;
        }
        let (start, end) = self.selection_range()?;
        // A selection ending at the start of a line doesn't include it.
        let last = if end.byte == 0 && end.line > start.line {
            end.line - 1
        } else {
            end.line
        };
        let name = self
            .doc()
            .path
            .as_deref()
            .and_then(Path::file_name)
            .map_or_else(
                || "Untitled".to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
        Some(if last == start.line {
            format!("{name}, line {}", start.line + 1)
        } else {
            format!("{name}, lines {}\u{2013}{}", start.line + 1, last + 1)
        })
    }

    /// Sends the typed message, with the selected code, and starts
    /// receiving the answer.
    pub(super) fn ai_send(&mut self, hwnd: HWND) {
        let question = self.ai.input.trim().to_string();
        if question.is_empty() || self.ai.busy() {
            return;
        }
        let Some(model) = self.settings.ai_model.clone() else {
            return;
        };
        let selection = self.ai_selection();
        let prompt = match &selection {
            Some((label, code)) => {
                let (code, clipped) = clip_context(code);
                let note = if clipped { " (cut short)" } else { "" };
                let language = fence_language(self.doc().path.as_deref());
                format!("{question}\n\nSelected code ({label}){note}:\n```{language}\n{code}\n```")
            }
            None => question.clone(),
        };

        // Earlier messages give the model the conversation so far.
        let mut history = Vec::new();
        let mut budget = MAX_HISTORY_BYTES;
        for entry in self.ai.entries.iter().rev() {
            let content = match entry.role {
                Role::Assistant => ai::visible_answer(&entry.text).0,
                _ => entry.prompt.as_str(),
            };
            if content.is_empty() {
                continue;
            }
            if content.len() > budget {
                break;
            }
            budget -= content.len();
            history.push(Message::new(entry.role, content));
        }
        history.reverse();
        let mut messages = vec![Message::new(Role::System, SYSTEM_PROMPT)];
        messages.extend(history);
        messages.push(Message::new(Role::User, prompt.clone()));

        self.ai.entries.push(ChatEntry::new(
            Role::User,
            question,
            prompt,
            selection.map(|(label, _)| label),
        ));
        self.ai.entries.push(ChatEntry::new(
            Role::Assistant,
            String::new(),
            String::new(),
            None,
        ));
        self.ai.input.clear();
        self.ai.follow.set(true);

        self.ai.next_request += 1;
        let id = self.ai.next_request;
        let cancel = Arc::new(AtomicBool::new(false));
        self.ai.request = Some((id, Arc::clone(&cancel)));
        let endpoint = self.settings.ai_endpoint.clone();
        let tx = self.ai.tx.clone();
        let hwnd_value = hwnd as isize;
        std::thread::spawn(move || {
            let wake = move || unsafe {
                PostMessageW(hwnd_value as HWND, AI_EVENT_MESSAGE, 0, 0);
            };
            let mut pending = String::new();
            let mut flushed = Instant::now();
            let result = ai::stream_chat(&endpoint, &model, &messages, &cancel, |text| {
                pending.push_str(text);
                if flushed.elapsed() >= ANSWER_FLUSH {
                    let _ = tx.send(AiEvent::Text(id, std::mem::take(&mut pending)));
                    wake();
                    flushed = Instant::now();
                }
            });
            if !pending.is_empty() {
                let _ = tx.send(AiEvent::Text(id, pending));
            }
            let _ = tx.send(AiEvent::Done(id, result));
            wake();
        });
        unsafe { InvalidateRect(hwnd, null(), 0) };
    }

    /// Stops the answer being received; what arrived so far stays.
    pub(super) fn ai_stop(&mut self, hwnd: HWND) {
        if let Some((_, cancel)) = self.ai.request.take() {
            cancel.store(true, Ordering::Relaxed);
            if let Some(entry) = self.ai.entries.last_mut() {
                entry.error = Some("Stopped".into());
            }
        }
        unsafe { InvalidateRect(hwnd, null(), 0) };
    }

    pub(super) fn ai_new_chat(&mut self, hwnd: HWND) {
        self.ai_stop(hwnd);
        self.ai.entries.clear();
        self.ai.scroll.set(0);
        self.ai.follow.set(true);
        self.focus_ai_input();
        unsafe { InvalidateRect(hwnd, null(), 0) };
    }

    /// The model menu: every chat model the server listed (the current one
    /// checked), refreshing that list, and turning the assistant off.
    pub(super) fn ai_model_menu(&mut self, hwnd: HWND, x: i32, y: i32) {
        use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
        const REFRESH: usize = 1;
        const TURN_OFF: usize = 2;
        const FIRST_MODEL: usize = 10;
        let current = self.settings.ai_model.clone().unwrap_or_default();
        let mut models = self.ai.models.clone();
        if !models.contains(&current) {
            models.insert(0, current.clone());
        }
        let chosen = unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return;
            }
            for (index, model) in models.iter().enumerate() {
                let flags = if *model == current {
                    MF_STRING | MF_CHECKED
                } else {
                    MF_STRING
                };
                let label = if ai::is_cloud_model(model) {
                    format!("{model}  (cloud: runs on ollama.com)")
                } else {
                    model.clone()
                };
                AppendMenuW(menu, flags, FIRST_MODEL + index, wide(&label).as_ptr());
            }
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            AppendMenuW(
                menu,
                MF_STRING,
                REFRESH,
                wide("Refresh model list").as_ptr(),
            );
            AppendMenuW(
                menu,
                MF_STRING,
                TURN_OFF,
                wide("Turn off AI Assistant").as_ptr(),
            );
            let mut point = POINT { x, y };
            ClientToScreen(hwnd, &mut point);
            let chosen = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_LEFTALIGN,
                point.x,
                point.y,
                0,
                hwnd,
                null(),
            ) as usize;
            DestroyMenu(menu);
            chosen
        };
        match chosen {
            0 => {}
            REFRESH => self.ai_connect(hwnd),
            TURN_OFF => {
                self.ai_stop(hwnd);
                self.ai.focused = false;
                self.set_ai_model(None);
            }
            index => {
                if let Some(model) = models.get(index - FIRST_MODEL) {
                    self.set_ai_model(Some(model.clone()));
                }
            }
        }
        unsafe { InvalidateRect(hwnd, null(), 0) };
    }

    /// A click inside the panel.
    pub(super) fn ai_click(&mut self, hwnd: HWND, x: i32, y: i32) {
        let inside =
            |rect: &RECT| x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom;
        let mut client = RECT::default();
        unsafe { GetClientRect(hwnd, &mut client) };
        let chrome_top = self.chrome_top();
        if y >= chrome_top
            && y <= chrome_top + self.scale(AI_HEADER)
            && x >= client.right - self.chrome_gap() - self.scale(34)
        {
            self.ai.focused = false;
            self.toggle_ai_assistant(hwnd);
            return;
        }
        let hits = self.ai.hits.borrow().clone();
        if hits.new_chat.as_ref().is_some_and(inside) {
            self.ai_new_chat(hwnd);
        } else if let Some(chip) = hits.model.filter(inside) {
            self.ai_model_menu(hwnd, chip.left, chip.bottom);
        } else if hits.connect.as_ref().is_some_and(inside) {
            self.ai_connect(hwnd);
        } else if hits.send.as_ref().is_some_and(inside) {
            if self.ai.busy() {
                self.ai_stop(hwnd);
            } else {
                self.ai_send(hwnd);
            }
            self.focus_ai_input();
        } else if hits.composer.as_ref().is_some_and(inside) {
            self.focus_ai_input();
        } else if let Some((_, text)) = hits.copies.iter().find(|(rect, _)| inside(rect)) {
            self.status = match clipboard::copy(hwnd, text) {
                Ok(()) => "Copied".into(),
                Err(error) => format!("Couldn't copy: {error}"),
            };
        }
        unsafe { InvalidateRect(hwnd, null(), 0) };
    }

    /// The mouse wheel over the conversation.
    pub(super) fn ai_scroll(&mut self, hwnd: HWND, delta: i32) {
        let step = self.scale(48) * delta / 120;
        let scroll = (self.ai.scroll.get() - step).max(0);
        self.ai.scroll.set(scroll);
        // Scrolling up stops following the answer; painting turns it back
        // on when the view reaches the bottom again.
        if delta > 0 {
            self.ai.follow.set(false);
        }
        unsafe { InvalidateRect(hwnd, null(), 0) };
    }

    /// A key while the message box has focus. Keys that would edit the
    /// document behind it are kept here; app-wide shortcuts (Ctrl+P, Ctrl+S,
    /// F5...) still work.
    pub(super) fn ai_key(&mut self, hwnd: HWND, key: u32, ctrl: bool, shift: bool) -> bool {
        match key {
            k if k == VK_RETURN as u32 && !ctrl => {
                if shift {
                    self.ai_insert(hwnd, "\n");
                } else {
                    self.ai_send(hwnd);
                }
                true
            }
            k if k == VK_ESCAPE as u32 => {
                if self.ai.busy() {
                    self.ai_stop(hwnd);
                } else {
                    self.ai.focused = false;
                    unsafe { InvalidateRect(hwnd, null(), 0) };
                }
                true
            }
            k if k == VK_BACK as u32 => {
                if ctrl {
                    let kept = self.ai.input.trim_end().rfind(char::is_whitespace);
                    self.ai.input.truncate(kept.map_or(0, |index| index + 1));
                } else {
                    self.ai.input.pop();
                }
                unsafe { InvalidateRect(hwnd, null(), 0) };
                true
            }
            0x56 if ctrl => {
                if let Ok(Some(text)) = clipboard::paste(hwnd) {
                    self.ai_insert(hwnd, &text.replace("\r\n", "\n"));
                }
                true
            }
            _ if (VK_F1 as u32..=VK_F24 as u32).contains(&key) => false,
            // Ctrl+P, Ctrl+S, Ctrl+O, Ctrl+N, Ctrl+W, Ctrl+Tab, Ctrl+`, Ctrl+,.
            _ if ctrl => ![
                0x50,
                0x53,
                0x4f,
                0x4e,
                0x57,
                VK_TAB as u32,
                VK_OEM_3 as u32,
                VK_OEM_COMMA as u32,
            ]
            .contains(&key),
            _ => true,
        }
    }

    /// A typed character for the message box.
    pub(super) fn ai_char(&mut self, hwnd: HWND, unit: u16) {
        if unit < 32 || unit == 127 {
            return;
        }
        let ch = if (0xd800..=0xdbff).contains(&unit) {
            self.pending_high_surrogate = Some(unit);
            None
        } else if (0xdc00..=0xdfff).contains(&unit) {
            self.pending_high_surrogate.take().and_then(|high| {
                char::from_u32(0x10000 + ((high as u32 - 0xd800) << 10) + (unit as u32 - 0xdc00))
            })
        } else {
            self.pending_high_surrogate = None;
            char::from_u32(unit as u32)
        };
        if let Some(ch) = ch {
            self.ai_insert(hwnd, ch.encode_utf8(&mut [0; 4]));
        }
    }

    fn ai_insert(&mut self, hwnd: HWND, text: &str) {
        if self.ai.input.len() + text.len() <= MAX_INPUT_BYTES {
            self.ai.input.push_str(text);
        }
        unsafe { InvalidateRect(hwnd, null(), 0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_blocks_are_tagged_by_extension() {
        assert_eq!(fence_language(Some(Path::new("src/app.RS"))), "rs");
        assert_eq!(fence_language(Some(Path::new("Makefile"))), "");
        assert_eq!(fence_language(None), "");
    }

    #[test]
    fn long_selections_are_cut_short() {
        assert_eq!(clip_context("a\nb"), ("a\nb".to_string(), false));
        let many = "x\n".repeat(MAX_CONTEXT_LINES + 50);
        let (clipped, cut) = clip_context(&many);
        assert!(cut);
        assert_eq!(clipped.lines().count(), MAX_CONTEXT_LINES);
        let wide = "y".repeat(MAX_CONTEXT_BYTES + 10);
        assert_eq!(clip_context(&wide), (String::new(), true));
    }
}
