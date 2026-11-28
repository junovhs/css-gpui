use std::ops::Range;

use super::model::{CssDeclaration, CssRule, SourceGraphError, TraceSpan};

pub(super) fn parse_css_region(
    document_id: usize,
    source: &str,
    region: Range<usize>,
    viewport: Option<(f32, Option<f32>)>,
) -> Result<Vec<CssRule>, SourceGraphError> {
    let (masked, comments) = mask_css_comments(&source[region.clone()], region.start)?;
    let mut parser = Parser {
        source,
        masked: &masked,
        comments: &comments,
        document_id,
        offset: region.start,
        viewport,
        rules: Vec::new(),
    };
    parser.parse_block(0..masked.len(), None)?;
    Ok(parser.rules)
}

struct Parser<'a> {
    source: &'a str,
    masked: &'a str,
    comments: &'a [Range<usize>],
    document_id: usize,
    offset: usize,
    viewport: Option<(f32, Option<f32>)>,
    rules: Vec<CssRule>,
}

impl Parser<'_> {
    fn parse_block(
        &mut self,
        block: Range<usize>,
        parent: Option<(&[String], usize)>,
    ) -> Result<(), SourceGraphError> {
        let mut index = block.start;
        let mut declaration_start = None;
        while index < block.end {
            index = skip_css_ws(self.masked, index).min(block.end);
            if index == block.end {
                break;
            }
            let delimiter = find_delimiter(self.masked, index..block.end, b"{;}");
            let prelude_end = delimiter.unwrap_or(block.end);
            let prelude = selector_without_comments(
                &self.source[self.offset..self.offset + self.masked.len()],
                index..prelude_end,
                self.comments,
            );
            let at_rule = prelude.starts_with('@');
            match delimiter.map(|position| self.masked.as_bytes()[position]) {
                Some(b'{') => {
                    self.emit_declarations(parent, declaration_start.take(), index)?;
                    let mut depth = 1;
                    let mut close = prelude_end + 1;
                    while let Some(position) = find_delimiter(self.masked, close..block.end, b"{}")
                    {
                        close = position + 1;
                        if self.masked.as_bytes()[position] == b'{' {
                            depth += 1;
                        } else {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    if depth != 0 {
                        return Err(
                            self.error("missing closing brace for CSS rule", prelude_end + 1)
                        );
                    }
                    let inner = prelude_end + 1..close - 1;
                    if at_rule {
                        match self.media_matches(&self.masked[index..prelude_end]) {
                            Some(true) => self.parse_block(inner, parent)?,
                            Some(false) => {}
                            None => self.push_rule(
                                vec![prelude.trim().to_owned()],
                                Vec::new(),
                                index..close,
                            ),
                        }
                    } else {
                        let selectors = parse_selectors(&prelude, self.offset + index)?;
                        let selectors = match parent {
                            Some((parents, _)) => expand_nested(parents, &selectors),
                            None => selectors,
                        };
                        self.parse_block(inner, Some((&selectors, index)))?;
                    }
                    index = close;
                }
                Some(b'}') => return Err(self.error("unexpected closing brace", prelude_end)),
                Some(b';') if at_rule => {
                    self.emit_declarations(parent, declaration_start.take(), index)?;
                    self.push_rule(
                        vec![prelude.trim().to_owned()],
                        Vec::new(),
                        index..prelude_end + 1,
                    );
                    index = prelude_end + 1;
                }
                _ if parent.is_some() => {
                    declaration_start.get_or_insert(index);
                    index = (prelude_end + 1).min(block.end);
                }
                _ => return Err(self.error("missing opening brace for CSS rule", index)),
            }
        }
        self.emit_declarations(parent, declaration_start, block.end)
    }

    fn emit_declarations(
        &mut self,
        parent: Option<(&[String], usize)>,
        declaration_start: Option<usize>,
        end: usize,
    ) -> Result<(), SourceGraphError> {
        if let (Some((selectors, selector_start)), Some(start)) = (parent, declaration_start) {
            let declarations =
                parse_css_declarations(self.source, &self.masked[start..end], self.offset + start)?;
            self.push_rule(selectors.to_vec(), declarations, selector_start..end);
        }
        Ok(())
    }

    fn push_rule(
        &mut self,
        selectors: Vec<String>,
        declarations: Vec<CssDeclaration>,
        range: Range<usize>,
    ) {
        self.rules.push(CssRule {
            id: self.rules.len(),
            selectors,
            declarations,
            span: TraceSpan::from_offsets(
                self.source,
                self.offset + range.start,
                self.offset + range.end,
            ),
            document_id: self.document_id,
        });
    }

    fn media_matches(&self, prelude: &str) -> Option<bool> {
        let (width, height) = self.viewport?;
        let (at_rule, condition) = prelude.split_at_checked("@media".len())?;
        if !at_rule.eq_ignore_ascii_case("@media")
            || !condition.starts_with(|ch: char| ch.is_ascii_whitespace() || ch == '(')
        {
            return None;
        }
        let condition = condition.trim_ascii().to_ascii_lowercase();
        let mut condition = condition.as_str();
        if !condition.starts_with('(') {
            let media_type = strip_media_keyword(condition, "only").unwrap_or(condition);
            condition = strip_media_keyword(media_type, "screen")?;
            if condition.is_empty() {
                return Some(true);
            }
            condition = strip_media_keyword(condition, "and")?;
        }

        let mut matched = true;
        loop {
            let (feature, remaining) = condition.strip_prefix('(')?.split_once(')')?;
            // An unsupported feature must stay diagnosable even after a false term.
            matched &= media_feature_matches(feature, width, height)?;
            condition = remaining.trim_ascii_start();
            if condition.is_empty() {
                return Some(matched);
            }
            condition = strip_media_keyword(condition, "and")?;
        }
    }

    fn error(&self, reason: &str, index: usize) -> SourceGraphError {
        SourceGraphError::UnsupportedCss {
            reason: reason.to_owned(),
            offset: self.offset + index,
        }
    }
}

fn strip_media_keyword<'a>(condition: &'a str, keyword: &str) -> Option<&'a str> {
    let remaining = condition.strip_prefix(keyword)?;
    (remaining.is_empty() || remaining.starts_with(|ch: char| ch.is_ascii_whitespace()))
        .then(|| remaining.trim_ascii_start())
}

fn media_feature_matches(feature: &str, width: f32, height: Option<f32>) -> Option<bool> {
    let (property, value) = feature.split_once(':')?;
    let value = value.trim_ascii();
    match property.trim_ascii() {
        "orientation" => match value {
            "portrait" => Some(width <= height?),
            "landscape" => Some(width > height?),
            _ => None,
        },
        "min-width" => Some(width >= media_length(value)?),
        "max-width" => Some(width <= media_length(value)?),
        "min-height" => Some(height? >= media_length(value)?),
        "max-height" => Some(height? <= media_length(value)?),
        _ => None,
    }
}

fn media_length(value: &str) -> Option<f32> {
    let number = value.strip_suffix("px")?;
    let mantissa = number
        .split_once('e')
        .map_or(number, |(mantissa, _)| mantissa);
    if mantissa.ends_with('.') {
        return None;
    }
    let length: f32 = number.parse().ok()?;
    (length.is_finite() && length >= 0.0).then_some(length)
}

fn expand_nested(parents: &[String], selectors: &[String]) -> Vec<String> {
    selectors
        .iter()
        .flat_map(|selector| {
            parents.iter().map(move |parent| {
                if selector.contains('&') {
                    selector.replace('&', parent)
                } else {
                    format!("{parent} {selector}")
                }
            })
        })
        .collect()
}

/// Find a structural delimiter outside quoted strings, functions, and brackets.
fn find_delimiter(source: &str, range: Range<usize>, delimiters: &[u8]) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = range.start;
    let mut quote = None;
    let mut depth: usize = 0;
    while index < range.end {
        let byte = bytes[index];
        match (quote, byte) {
            (_, b'\\') => index += 1,
            (Some(delimiter), current) if delimiter == current => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'(' | b'[') => depth += 1,
            (None, b')' | b']') => depth = depth.saturating_sub(1),
            (None, current) if depth == 0 && delimiters.contains(&current) => return Some(index),
            _ => {}
        }
        index += 1;
    }
    None
}

// Mask comments so delimiters and declaration offsets still use original byte positions.
fn mask_css_comments(
    source: &str,
    offset: usize,
) -> Result<(String, Vec<Range<usize>>), SourceGraphError> {
    let bytes = source.as_bytes();
    let mut masked = bytes.to_vec();
    let mut comments = Vec::new();
    let mut quote = None;
    let mut index = 0;
    while index < bytes.len() {
        match (quote, bytes[index]) {
            (_, b'\\') => index += 2,
            (Some(delimiter), byte) if byte == delimiter => {
                quote = None;
                index += 1;
            }
            (None, delimiter @ (b'\'' | b'"')) => {
                quote = Some(delimiter);
                index += 1;
            }
            (None, b'/') if bytes.get(index + 1) == Some(&b'*') => {
                let Some(length) = source[index + 2..].find("*/") else {
                    return Err(SourceGraphError::UnsupportedCss {
                        reason: "unterminated CSS comment".to_owned(),
                        offset: offset + index,
                    });
                };
                let end = index + length + 4;
                masked[index..end].fill(b' ');
                comments.push(index..end);
                index = end;
            }
            _ => index += 1,
        }
    }
    // The input is UTF-8 and complete comments were replaced by ASCII.
    Ok((
        String::from_utf8(masked).expect("masking comments preserves UTF-8"),
        comments,
    ))
}

fn selector_without_comments(
    source: &str,
    range: Range<usize>,
    comments: &[Range<usize>],
) -> String {
    let first = comments.partition_point(|comment| comment.end <= range.start);
    let mut selector = String::new();
    let mut start = range.start;
    for comment in &comments[first..] {
        if comment.start >= range.end {
            break;
        }
        selector.push_str(&source[start..comment.start]);
        start = comment.end;
    }
    selector.push_str(&source[start..range.end]);
    selector
}

fn parse_selectors(selector_text: &str, offset: usize) -> Result<Vec<String>, SourceGraphError> {
    let mut selectors = Vec::new();
    let mut index = 0;
    while index < selector_text.len() {
        let end = find_delimiter(selector_text, index..selector_text.len(), b",")
            .unwrap_or(selector_text.len());
        let selector = selector_text[index..end].trim();
        if !selector.is_empty() {
            selectors.push(selector.to_owned());
        }
        index = end + 1;
    }
    if selectors.is_empty() {
        return Err(SourceGraphError::UnsupportedCss {
            reason: "empty selector list".to_owned(),
            offset,
        });
    }

    Ok(selectors)
}

fn parse_css_declarations(
    full_source: &str,
    block: &str,
    block_offset: usize,
) -> Result<Vec<CssDeclaration>, SourceGraphError> {
    let mut declarations = Vec::new();
    let mut cursor = 0;

    while cursor < block.len() {
        let end = find_delimiter(block, cursor..block.len(), b";").unwrap_or(block.len());
        let segment = &block[cursor..end];
        let segment_len = segment.len();
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            cursor = end + 1;
            continue;
        }

        let Some(colon_index) = trimmed.find(':') else {
            return Err(SourceGraphError::UnsupportedCss {
                reason: "CSS declarations must use `property: value` syntax".to_owned(),
                offset: block_offset + cursor,
            });
        };

        let property = trimmed[..colon_index].trim();
        let value = trimmed[colon_index + 1..].trim();
        if property.is_empty() || value.is_empty() {
            return Err(SourceGraphError::UnsupportedCss {
                reason: "CSS declarations must include both a property and a value".to_owned(),
                offset: block_offset + cursor,
            });
        }

        let declaration_start = block_offset + cursor + segment_len - segment.trim_start().len();
        declarations.push(CssDeclaration {
            property: property.to_owned(),
            value: value.to_owned(),
            span: TraceSpan::from_offsets(
                full_source,
                declaration_start,
                declaration_start + trimmed.len(),
            ),
        });
        cursor = end + 1;
    }

    Ok(declarations)
}

fn skip_css_ws(source: &str, mut index: usize) -> usize {
    let bytes = source.as_bytes();
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}
