// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only completion for the application APIs already registered with Rhai.
//! This provider uses descriptors, never an engine or filesystem queries.

use crate::scripting::api::{CONSTANTS, FUNCTIONS};

pub struct Suggestion {
    pub label: String,
    pub insertion: String,
}

/// An edit anchored to the original input so cycling cannot accumulate text.
pub struct Edit {
    pub prefix: String,
    pub suffix: String,
}

impl Edit {
    pub fn apply(&self, insertion: &str) -> (String, usize) {
        if let Some(name) = insertion.strip_suffix("()") {
            let following = self.suffix.trim_start();
            if following.starts_with('(') {
                let cursor =
                    self.prefix.len() + name.len() + self.suffix.len() - following.len() + 1;
                return (format!("{}{name}{}", self.prefix, self.suffix), cursor);
            }
            return (
                format!("{}{insertion}{}", self.prefix, self.suffix),
                self.prefix.len() + insertion.len() - 1,
            );
        }
        (
            format!("{}{insertion}{}", self.prefix, self.suffix),
            self.prefix.len() + insertion.len(),
        )
    }
}

pub fn candidates(input: &str, cursor: usize) -> Option<(Edit, Vec<Suggestion>)> {
    let source = input.trim_start().strip_prefix('=')?;
    let start = input.len() - source.len();
    if cursor < start || !input.is_char_boundary(cursor) {
        return None;
    }
    if !in_code(&input[start..cursor]) {
        return None;
    }
    let symbol_char = |c: char| c.is_alphanumeric() || c == '_' || c == ':';
    let token_start = input[start..cursor]
        .char_indices()
        .rev()
        .find(|(_, c)| !symbol_char(*c))
        .map_or(start, |(index, c)| start + index + c.len_utf8());
    // Member access is not a request for a global function or namespace.
    if input[..token_start].trim_end().ends_with('.') {
        return None;
    }
    let token_end = input[cursor..]
        .char_indices()
        .find(|(_, c)| !symbol_char(*c))
        .map_or(input.len(), |(index, _)| cursor + index);
    let prefix = &input[token_start..cursor];
    let mut suggestions: Vec<Suggestion> = FUNCTIONS
        .iter()
        .filter(|function| function.name.starts_with(prefix))
        .map(|function| Suggestion {
            label: function.signature(),
            insertion: format!("{}()", function.name),
        })
        .collect();
    let mut namespaces: Vec<_> = CONSTANTS
        .iter()
        .map(|constant| constant.namespace)
        .collect();
    namespaces.sort_unstable();
    namespaces.dedup();
    suggestions.extend(namespaces.into_iter().filter_map(|namespace| {
        let name = format!("{namespace}::");
        (name.starts_with(prefix) && !prefix.contains("::")).then(|| Suggestion {
            label: name.clone(),
            insertion: name,
        })
    }));
    if prefix.contains("::") {
        suggestions.extend(CONSTANTS.iter().filter_map(|constant| {
            let name = constant.qualified_name();
            name.starts_with(prefix).then(|| Suggestion {
                label: name.clone(),
                insertion: name,
            })
        }));
    }
    suggestions.sort_by(|a, b| a.insertion.cmp(&b.insertion));
    Some((
        Edit {
            prefix: input[..token_start].to_owned(),
            suffix: input[token_end..].to_owned(),
        },
        suggestions,
    ))
}

/// Only distinguish code from literals/comments; incomplete source need not parse.
/// Backtick strings are deliberately treated as opaque, including interpolation.
fn in_code(source: &str) -> bool {
    let mut quote = None;
    let mut escaped = false;
    let mut comment_depth = 0usize;
    let mut line_comment = false;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if line_comment {
            line_comment = c != '\n';
        } else if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == delimiter {
                quote = None;
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            comment_depth += 1;
        } else if comment_depth > 0 {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                comment_depth -= 1;
            }
        } else if c == '/' && chars.peek() == Some(&'/') {
            chars.next();
            line_comment = true;
        } else if matches!(c, '"' | '\'' | '`') {
            quote = Some(c);
        }
    }
    quote.is_none() && comment_depth == 0 && !line_comment
}
