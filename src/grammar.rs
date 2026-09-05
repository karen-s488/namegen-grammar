// Parser for .namegen grammar files.
//
// A grammar is a list of rules, one per line:
//
//   rule_name = alternative | alternative | alternative
//
// An alternative is plain text that may contain <other_rule> references,
// which get expanded recursively at generation time. Every error carries
// the exact line and column of the character that caused it, plus the
// source line itself, so a typo points straight back at the file.

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
        let parts = parse_parts(trimmed, trimmed_col_offset, line_no, raw_line)?;
        alternatives.push(Alternative { parts });
    }

    Ok(alternatives)
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
