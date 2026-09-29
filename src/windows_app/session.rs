use super::*;

impl App {
    // Snapshot the current workspace and every path-backed tab (skipping the
    // transient untitled buffer) into a serializable session.
    fn capture_session(&self) -> workflow::Session {
        let to_view = |view: &EditorView| workflow::SessionView {
            cursor: (view.cursor.line, view.cursor.byte),
            anchor: view.selection_anchor.map(|pos| (pos.line, pos.byte)),
            first_line: view.first_line,
        };
        let mut tabs = Vec::new();
        let mut active = 0;
        for (index, tab) in self.tabs.iter().enumerate() {
            let Some(path) = tab.document.path.clone() else {
                continue;
            };
            if index == self.active {
                active = tabs.len();
            }
            let views = [to_view(&tab.views[0]), to_view(&tab.views[1])];
            tabs.push(workflow::SessionTab { path, views });
        }
        workflow::Session {
            root: self.workspace_root.clone(),
            active,
            tabs,
        }
    }

    pub(super) fn save_session(&self) {
        if self.restoring {
            return;
        }
        workflow::save_session(&self.capture_session());
    }

    // Reopen the last session: the workspace, and a tab for every file that
    // still exists, in the same order and with each tab's cursor, selection
    // and scroll position. Only the file that was showing is read now; the
    // others are read when first shown (Tab::unloaded). Reading them all
    // first made startup wait for every tab: with 20 tabs the window took
    // twice as long to respond.
    pub(super) fn restore_session(&mut self, hwnd: HWND) {
        let session = workflow::load_session();
        if session.root.is_none() && session.tabs.is_empty() {
            return;
        }
        self.restoring = true;
        if let Some(root) = session.root.clone() {
            self.set_workspace(hwnd, root);
        }
        let view = |saved: &workflow::SessionView| EditorView {
            cursor: Pos {
                line: saved.cursor.0,
                byte: saved.cursor.1,
            },
            selection_anchor: saved.anchor.map(|(line, byte)| Pos { line, byte }),
            first_line: saved.first_line,
            first_row: 0,
        };
        let shown = session.active.min(session.tabs.len().saturating_sub(1));
        let mut active = None;
        for (position, saved) in session.tabs.iter().enumerate() {
            if !saved.path.is_file() {
                continue;
            }
            if position == shown {
                self.open(hwnd, Some(saved.path.clone()));
                // `open` activates the tab it just opened, so the active
                // index is the tab to position.
                let index = self.active;
                let doc = &self.tabs[index].document;
                let views = saved.views.clone().map(|saved| {
                    let restored = view(&saved);
                    EditorView {
                        cursor: doc.clamp(restored.cursor),
                        selection_anchor: restored.selection_anchor.map(|pos| doc.clamp(pos)),
                        first_line: restored.first_line.min(doc.line_count().saturating_sub(1)),
                        first_row: 0,
                    }
                });
                self.tabs[index].views = views;
                active = Some(index);
            } else {
                let tab = Tab::unloaded(
                    saved.path.clone(),
                    saved.views.clone().map(|saved| view(&saved)),
                );
                // Until a file opens, the editor's one tab is an empty stand-in.
                if self.tabs.len() == 1 && self.tabs[0].is_placeholder() {
                    self.tabs[0] = tab;
                } else {
                    self.tabs.push(tab);
                }
            }
        }
        if self.tabs.iter().all(Tab::is_placeholder) {
            self.restoring = false;
            return;
        }
        // Normally the file that was showing; if it's gone, the first tab.
        self.welcome = false;
        self.activate_tab(hwnd, active.unwrap_or(0));
        self.keep_cursor_visible(hwnd);
        self.status = "Restored previous session".into();
        self.restoring = false;
        unsafe { InvalidateRect(hwnd, null(), 0) };
    }
}
