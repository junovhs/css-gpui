use super::model::SourceGraphError;

pub struct StartTag {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub self_closing: bool,
}

pub fn find_tag_end(source: &str, start: usize) -> Result<usize, SourceGraphError> {
    let bytes = source.as_bytes();
    let mut index = start + 1;
    let mut quote = None;

    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if byte == b'>' {
            return Ok(index);
        }
        index += 1;
    }

    Err(SourceGraphError::UnsupportedHtml {
        reason: "unterminated HTML tag".to_owned(),
        offset: start,
    })
}

pub fn parse_start_tag(tag_body: &str, offset: usize) -> Result<StartTag, SourceGraphError> {
    let name_end = tag_body
        .find(|ch: char| ch.is_whitespace() || ch == '/')
        .unwrap_or(tag_body.len());
    let name = &tag_body[..name_end];
    if name.is_empty() {
        return Err(SourceGraphError::UnsupportedHtml {
            reason: "HTML element tag names must not be empty".to_owned(),
            offset,
        });
    }

    let (attributes, self_closing) = parse_attributes(tag_body[name_end..].trim(), offset)?;
    Ok(StartTag {
        name: name.to_owned(),
        attributes,
        self_closing: self_closing || is_void_element(name),
    })
}

type Attributes = Vec<(String, String)>;

fn parse_attributes(input: &str, offset: usize) -> Result<(Attributes, bool), SourceGraphError> {
    let mut attributes = Vec::new();
    let mut index = 0;
    loop {
        let rest = input[index..].trim_start();
        index = input.len() - rest.len();
        match rest {
            "" => return Ok((attributes, false)),
            "/" => return Ok((attributes, true)),
            _ => {}
        }

        let length = rest
            .find(|ch: char| ch.is_whitespace() || ch == '=' || ch == '/')
            .unwrap_or(rest.len());
        if length == 0 {
            return Err(SourceGraphError::UnsupportedHtml {
                reason: "expected an HTML attribute name".to_owned(),
                offset,
            });
        }
        let name = &rest[..length];
        index += length;
        let value = parse_attribute_value(input, &mut index, name, offset)?;
        attributes.push((name.to_owned(), value));
    }
}

fn parse_attribute_value(
    input: &str,
    index: &mut usize,
    name: &str,
    offset: usize,
) -> Result<String, SourceGraphError> {
    let bytes = input.as_bytes();
    while *index < bytes.len() && bytes[*index].is_ascii_whitespace() {
        *index += 1;
    }
    if *index >= bytes.len() || bytes[*index] != b'=' {
        return Ok(String::new());
    }
    *index += 1;
    while *index < bytes.len() && bytes[*index].is_ascii_whitespace() {
        *index += 1;
    }
    let Some(&first) = bytes.get(*index) else {
        return Err(SourceGraphError::UnsupportedHtml {
            reason: format!("missing value for attribute `{name}`"),
            offset,
        });
    };
    if first != b'"' && first != b'\'' {
        let start = *index;
        while *index < bytes.len() && !bytes[*index].is_ascii_whitespace() {
            if matches!(bytes[*index], b'"' | b'\'' | b'<' | b'=' | b'`') {
                return Err(SourceGraphError::UnsupportedHtml {
                    reason: format!("invalid unquoted value for attribute `{name}`"),
                    offset,
                });
            }
            *index += 1;
        }
        return Ok(input[start..*index].to_owned());
    }
    let quote = first;
    *index += 1;
    let value_start = *index;
    while *index < bytes.len() && bytes[*index] != quote {
        *index += 1;
    }
    if *index >= bytes.len() {
        return Err(SourceGraphError::UnsupportedHtml {
            reason: format!("unterminated value for attribute `{name}`"),
            offset,
        });
    }
    let value = input[value_start..*index].to_owned();
    *index += 1;
    Ok(value)
}

fn is_void_element(name: &str) -> bool {
    matches!(name, "meta" | "link" | "img" | "br" | "hr") || name.eq_ignore_ascii_case("input")
}
