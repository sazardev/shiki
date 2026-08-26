#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Edit,
    Visual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Notebooks,
    Notes,
    Preview,
}

impl Focus {
    /// Full cycle, used by `tab`.
    pub(crate) fn next(self) -> Self {
        match self {
            Focus::Notebooks => Focus::Notes,
            Focus::Notes => Focus::Preview,
            Focus::Preview => Focus::Notebooks,
        }
    }

    /// One level deeper (Yazi-style `l` / Right / Enter). Stays at the deepest level.
    pub(crate) fn forward(self) -> Self {
        match self {
            Focus::Notebooks => Focus::Notes,
            Focus::Notes | Focus::Preview => Focus::Preview,
        }
    }

    /// One level back (Yazi-style `h` / Left). Stays at the shallowest level.
    pub(crate) fn backward(self) -> Self {
        match self {
            Focus::Notebooks | Focus::Notes => Focus::Notebooks,
            Focus::Preview => Focus::Notes,
        }
    }

    /// For `general.remember_last_session` — `shiki_config::SessionState`
    /// doesn't know about this enum at all (shiki-config sits below
    /// shiki-tui in the dependency chain), so it's persisted as a plain
    /// lowercase string and translated on both ends here instead.
    pub(crate) fn as_session_str(self) -> &'static str {
        match self {
            Focus::Notebooks => "notebooks",
            Focus::Notes => "notes",
            Focus::Preview => "preview",
        }
    }

    pub(crate) fn from_session_str(s: &str) -> Option<Self> {
        match s {
            "notebooks" => Some(Focus::Notebooks),
            "notes" => Some(Focus::Notes),
            "preview" => Some(Focus::Preview),
            _ => None,
        }
    }
}

/// How the NOTES list is ordered; cycled by `Action::SortNotes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum NoteSort {
    #[default]
    Filename,
    TitleAz,
    DateNewest,
}

impl NoteSort {
    pub(crate) fn next(self) -> Self {
        match self {
            NoteSort::Filename => NoteSort::TitleAz,
            NoteSort::TitleAz => NoteSort::DateNewest,
            NoteSort::DateNewest => NoteSort::Filename,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            NoteSort::Filename => "filename",
            NoteSort::TitleAz => "title A-Z",
            NoteSort::DateNewest => "date (newest first)",
        }
    }

    /// Parses `config.general.default_note_sort` — case-insensitive,
    /// substring-tolerant match against each variant's own label, falling
    /// back to `Filename` (the existing default) for anything
    /// unrecognized, same leniency `pdf_theme` already has for a typo'd
    /// theme name rather than refusing to start.
    pub(crate) fn from_config_str(s: &str) -> Self {
        let s = s.trim().to_lowercase();
        if s.starts_with("title") {
            NoteSort::TitleAz
        } else if s.starts_with("date") {
            NoteSort::DateNewest
        } else {
            NoteSort::Filename
        }
    }
}
