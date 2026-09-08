//! Lightweight, dependency-free source diagnostics.
//!
//! This module is the root of the `diagnostic` module tree.
//!
//! The language is ASCII-oriented (see README), so positions are byte offsets
//! and columns are byte columns.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn empty(pos: usize) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}

/// Maps byte offsets to 1-based line/column positions.
#[derive(Debug, Clone)]
pub struct SourceMap {
    line_starts: Vec<usize>,
}

impl SourceMap {
    pub fn new(src: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (index, byte) in src.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(index + 1);
            }
        }
        Self { line_starts }
    }

    pub fn location(&self, offset: usize) -> Location {
        let line_index = self
            .line_starts
            .iter()
            .rposition(|&start| start <= offset)
            .unwrap_or(0);
        Location {
            line: line_index + 1,
            column: offset - self.line_starts[line_index] + 1,
        }
    }

    pub fn line_text<'a>(&self, src: &'a str, line: usize) -> &'a str {
        src.lines()
            .nth(line.saturating_sub(1))
            .unwrap_or("")
            .trim_end_matches('\r')
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
}

impl Severity {
    fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Option<Span>,
    pub label: Option<String>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span,
            label: None,
            help: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn render(&self, filename: &str, src: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("{}: {}\n", self.severity.label(), self.message));

        if let Some(span) = self.span {
            let map = SourceMap::new(src);
            let location = map.location(span.start);
            let source_line = map.line_text(src, location.line);
            let underline_start = location.column.saturating_sub(1);
            let source_len = source_line.len().max(1);
            let underline_end = if span.start == span.end {
                underline_start.saturating_add(1).min(source_len)
            } else {
                let end_col = span
                    .end
                    .min(span.start + source_len.saturating_sub(underline_start))
                    .saturating_sub(span.start);
                underline_start
                    .saturating_add(end_col.max(1))
                    .min(source_len)
            };
            let underline_len = underline_end.saturating_sub(underline_start).max(1);

            out.push_str(&format!(
                " --> {}:{}:{}\n",
                filename, location.line, location.column
            ));
            out.push_str("  |\n");
            out.push_str(&format!("{} | {}\n", location.line, source_line));
            out.push_str(&format!(
                "{} | {}{}",
                " ".repeat(location.line.to_string().len()),
                " ".repeat(underline_start),
                "^".repeat(underline_len)
            ));
            if let Some(label) = &self.label {
                out.push(' ');
                out.push_str(label);
            }
            out.push('\n');
        }

        if let Some(help) = &self.help {
            out.push_str(&format!("  = help: {help}\n"));
        }

        out
    }
}
