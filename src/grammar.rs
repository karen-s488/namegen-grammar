// Parser for .namegen grammar files.
//
// A grammar is a list of rules, one per line:
//
//   rule_name = alternative | alternative | alternative
//
// An alternative is plain text that may contain <other_rule> references,
// which get expanded recursively at generation time. An alternative may
// end in `:N` to weight it N times as heavily as an unweighted one, e.g.
// `common:5 | rare`. Every error carries the exact line and column of the
// character that caused it, plus the source line itself, so a typo points
// straight back at the file.

use std::collections::HashMap;
use std::fmt;

#[derive(Debug)]
pub struct Grammar {
    pub rules: HashMap<String, Rule>,
}

#[derive(Debug)]
pub struct Rule {
    pub alternatives: Vec<Alternative>,
    pub line: usize,
}

#[derive(Debug)]
pub struct Alternative {
    pub parts: Vec<Part>,
    pub weight: u32,
}

#[derive(Debug)]
pub enum Part {
    Literal(String),
    Reference {
        name: String,
        line: usize,
        column: usize,
    },
}

#[derive(Debug)]
pub struct GrammarError {
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub line_text: String,
}

impl fmt::Display for GrammarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "error: {}", self.message)?;
        writeln!(f, "  --> {}:{}", self.line, self.column)?;
        writeln!(f, "  |")?;
        writeln!(f, "{} | {}", self.line, self.line_text)?;
        write!(f, "  | {}^", " ".repeat(self.column.saturating_sub(1)))
    }
}

pub fn parse(source: &str) -> Result<Grammar, GrammarError> {
    let mut rules: HashMap<String, Rule> = HashMap::new();

    for (idx, raw_line) in source.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw_line.trim_start();
        let indent = raw_line.len() - trimmed.len();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let eq_pos = trimmed.find('=').ok_or_else(|| GrammarError {
            line: line_no,
            column: indent + 1,
            message: "expected '=' after the rule name".to_string(),
            line_text: raw_line.to_string(),
        })?;

        let name = trimmed[..eq_pos].trim();
        let name_col = indent + 1;

        if name.is_empty() {
            return Err(GrammarError {
                line: line_no,
                column: name_col,
                message: "rule name cannot be empty".to_string(),
                line_text: raw_line.to_string(),
            });
        }
        if !is_valid_identifier(name) {
            return Err(GrammarError {
                line: line_no,
                column: name_col,
                message: format!(
                    "invalid rule name '{name}' - use letters, digits and underscores, starting with a letter or underscore"
                ),
                line_text: raw_line.to_string(),
            });
        }
        if let Some(existing) = rules.get(name) {
            return Err(GrammarError {
                line: line_no,
                column: name_col,
                message: format!("rule '{name}' is already defined on line {}", existing.line),
                line_text: raw_line.to_string(),
            });
        }

        let body = &trimmed[eq_pos + 1..];
        let body_col_offset = indent + eq_pos + 1;
        let alternatives = parse_body(body, body_col_offset, line_no, raw_line)?;

        rules.insert(
            name.to_string(),
            Rule {
                alternatives,
                line: line_no,
            },
        );
    }

    if rules.is_empty() {
        return Err(GrammarError {
            line: 1,
            column: 1,
            message: "grammar file has no rules".to_string(),
            line_text: source.lines().next().unwrap_or("").to_string(),
        });
    }

    for rule in rules.values() {
        for alt in &rule.alternatives {
            for part in &alt.parts {
                if let Part::Reference { name, line, column } = part {
                    if !rules.contains_key(name) {
                        let line_text = source.lines().nth(*line - 1).unwrap_or("").to_string();
                        return Err(GrammarError {
                            line: *line,
                            column: *column,
                            message: format!("undefined rule '{name}'"),
                            line_text,
                        });
                    }
                }
            }
        }
    }

    Ok(Grammar { rules })
}

// `body` is everything after the '=' on a rule line. `body_col_offset` is
// the byte offset of `body`'s first character within `raw_line`, so every
// position we find inside `body` can be turned back into a real column.
fn parse_body(
    body: &str,
    body_col_offset: usize,
    line_no: usize,
    raw_line: &str,
) -> Result<Vec<Alternative>, GrammarError> {
    let mut alternatives = Vec::new();
    let mut offset = 0usize;

    for segment in body.split('|') {
        let seg_col_offset = body_col_offset + offset;
        offset += segment.len() + 1; // account for the '|' we split on

        let leading_ws = segment.len() - segment.trim_start().len();
        let trimmed = segment.trim();

        if trimmed.is_empty() {
            return Err(GrammarError {
                line: line_no,
                column: seg_col_offset + leading_ws + 1,
                message: "empty alternative - remove the extra '|' or add text".to_string(),
                line_text: raw_line.to_string(),
            });
        }

        let trimmed_col_offset = seg_col_offset + leading_ws;
        let (text, weight) = split_weight(trimmed, trimmed_col_offset, line_no, raw_line)?;
        let parts = parse_parts(text, trimmed_col_offset, line_no, raw_line)?;
        alternatives.push(Alternative { parts, weight });
    }

    Ok(alternatives)
}

// An alternative may end in `:N` to make it N times as likely as a plain
// (unweighted) alternative, e.g. `common:5 | rare`. The ':' only counts as
// a weight separator when it sits outside any `<...>` reference and is
// followed by nothing but digits, so ordinary text and reference names
// containing ':' are left alone.
fn split_weight<'a>(
    text: &'a str,
    col_offset: usize,
    line_no: usize,
    raw_line: &str,
) -> Result<(&'a str, u32), GrammarError> {
    let bytes = text.as_bytes();
    let mut digit_start = text.len();
    while digit_start > 0 && bytes[digit_start - 1].is_ascii_digit() {
        digit_start -= 1;
    }

    if digit_start == text.len() || digit_start == 0 || bytes[digit_start - 1] != b':' {
        return Ok((text, 1));
    }
    let colon_pos = digit_start - 1;

    let prefix = &text[..colon_pos];
    if prefix.matches('<').count() != prefix.matches('>').count() {
        // The ':' is inside an unterminated reference, not a weight.
        return Ok((text, 1));
    }

    let digits = &text[digit_start..];
    let weight: u32 = digits.parse().map_err(|_| GrammarError {
        line: line_no,
        column: col_offset + digit_start + 1,
        message: format!("alternative weight '{digits}' is too large"),
        line_text: raw_line.to_string(),
    })?;
    if weight == 0 {
        return Err(GrammarError {
            line: line_no,
            column: col_offset + digit_start + 1,
            message: "alternative weight must be at least 1".to_string(),
            line_text: raw_line.to_string(),
        });
    }

    Ok((&text[..colon_pos], weight))
}

fn parse_parts(
    text: &str,
    col_offset: usize,
    line_no: usize,
    raw_line: &str,
) -> Result<Vec<Part>, GrammarError> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;

    while i < chars.len() {
        let (byte_pos, c) = chars[i];
        let col = col_offset + byte_pos + 1;

        if c == '<' {
            if !literal.is_empty() {
                parts.push(Part::Literal(std::mem::take(&mut literal)));
            }
            let mut j = i + 1;
            let mut name = String::new();
            while j < chars.len() && chars[j].1 != '>' {
                name.push(chars[j].1);
                j += 1;
            }
            if j >= chars.len() {
                return Err(GrammarError {
                    line: line_no,
                    column: col,
                    message: "unterminated reference - missing closing '>'".to_string(),
                    line_text: raw_line.to_string(),
                });
            }
            let name_trimmed = name.trim();
            if name_trimmed.is_empty() {
                return Err(GrammarError {
                    line: line_no,
                    column: col,
                    message: "empty reference '<>' - name a rule between the brackets".to_string(),
                    line_text: raw_line.to_string(),
                });
            }
            if !is_valid_identifier(name_trimmed) {
                return Err(GrammarError {
                    line: line_no,
                    column: col + 1,
                    message: format!("invalid rule name '{name_trimmed}' in reference"),
                    line_text: raw_line.to_string(),
                });
            }
            parts.push(Part::Reference {
                name: name_trimmed.to_string(),
                line: line_no,
                column: col,
            });
            i = j + 1;
            continue;
        }

        if c == '>' {
            return Err(GrammarError {
                line: line_no,
                column: col,
                message: "unexpected '>' without matching '<'".to_string(),
                line_text: raw_line.to_string(),
            });
        }

        literal.push(c);
        i += 1;
    }

    if !literal.is_empty() {
        parts.push(Part::Literal(literal));
    }

    Ok(parts)
}

fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
