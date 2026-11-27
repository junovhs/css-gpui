use std::ops::Range;

use super::html_support::{find_tag_end, parse_start_tag};
use super::model::{HtmlNode, HtmlNodeKind, SourceGraphError};

pub(super) struct ParsedHtml {
    pub nodes: Vec<HtmlNode>,
    pub styles: Vec<Range<usize>>,
}

pub(super) fn parse_html_document(
    document_id: usize,
    source: &str,
) -> Result<ParsedHtml, SourceGraphError> {
    let mut builder = HtmlTreeBuilder::new(document_id);
    builder.parse(source)?;
    Ok(ParsedHtml {
        nodes: builder.nodes,
        styles: builder.styles,
    })
}

#[derive(Clone)]
struct OpenNode {
    id: usize,
    path: String,
    next_child_index: usize,
}

struct HtmlTreeBuilder {
    document_id: usize,
    nodes: Vec<HtmlNode>,
    stack: Vec<OpenNode>,
    styles: Vec<Range<usize>>,
}

impl HtmlTreeBuilder {
    fn new(document_id: usize) -> Self {
        let root = OpenNode {
            id: 0,
            path: "0".to_owned(),
            next_child_index: 0,
        };
        let nodes = vec![HtmlNode {
            id: 0,
            parent_id: None,
            kind: HtmlNodeKind::Document,
            name: None,
            text: None,
            attributes: Vec::new(),
            dom_path: root.path.clone(),
            document_id,
            start_tag_range: None,
        }];

        Self {
            document_id,
            nodes,
            stack: vec![root],
            styles: Vec::new(),
        }
    }

    fn parse(&mut self, source: &str) -> Result<(), SourceGraphError> {
        let bytes = source.as_bytes();
        let mut index = 0;

        while index < bytes.len() {
            if bytes[index] == b'<' {
                index = self.parse_tag(source, index)?;
            } else {
                index = self.parse_text(source, index);
            }
        }

        if self.stack.len() != 1 {
            return Err(SourceGraphError::UnsupportedHtml {
                reason: "unclosed HTML tags in fixture".to_owned(),
                offset: source.len(),
            });
        }

        Ok(())
    }

    fn parse_tag(&mut self, source: &str, start: usize) -> Result<usize, SourceGraphError> {
        if let Some(comment) = source[start..].strip_prefix("<!--") {
            return comment
                .find("-->")
                .map(|end| start + 4 + end + 3)
                .ok_or_else(|| SourceGraphError::UnsupportedHtml {
                    reason: "unterminated HTML comment".to_owned(),
                    offset: start,
                });
        }
        let end = find_tag_end(source, start)?;
        let raw_tag = source[start + 1..end].trim();
        if raw_tag.is_empty() || raw_tag.starts_with('!') || raw_tag.starts_with('?') {
            return Ok(end + 1);
        }

        if let Some(name) = raw_tag.strip_prefix('/') {
            self.close_tag(name.trim(), start)?;
            return Ok(end + 1);
        }

        let tag_name = raw_tag.split_ascii_whitespace().next().unwrap_or_default();
        let raw_text_tag = ["script", "style"]
            .into_iter()
            .find(|tag| tag_name.eq_ignore_ascii_case(tag));
        if let Some(tag) = raw_text_tag {
            let close = raw_text_end(source, end + 1, tag).ok_or_else(|| {
                SourceGraphError::UnsupportedHtml {
                    reason: format!("unclosed HTML `<{tag}>` element"),
                    offset: start,
                }
            })?;
            if tag == "style" {
                self.styles.push(end + 1..close);
            }
            return Ok(find_tag_end(source, close)? + 1);
        }

        let open_node = self.push_element(raw_tag, start..end + 1)?;
        if let Some(node) = open_node {
            self.stack.push(node);
        }

        Ok(end + 1)
    }

    fn parse_text(&mut self, source: &str, start: usize) -> usize {
        let end = source[start..]
            .find('<')
            .map_or(source.len(), |offset| start + offset);
        let trimmed = source[start..end].trim();
        if !trimmed.is_empty() {
            let _ = self.push_text(trimmed.to_owned());
        }
        end
    }

    fn push_element(
        &mut self,
        raw_tag: &str,
        range: Range<usize>,
    ) -> Result<Option<OpenNode>, SourceGraphError> {
        let start_tag = parse_start_tag(raw_tag, range.start)?;
        let (parent_id, dom_path) = self.next_child_location()?;
        let id = self.nodes.len();
        self.nodes.push(HtmlNode {
            id,
            parent_id: Some(parent_id),
            kind: HtmlNodeKind::Element,
            name: Some(start_tag.name),
            text: None,
            attributes: start_tag.attributes,
            dom_path: dom_path.clone(),
            document_id: self.document_id,
            start_tag_range: Some(range),
        });
        Ok((!start_tag.self_closing).then_some(OpenNode {
            id,
            path: dom_path,
            next_child_index: 0,
        }))
    }

    fn push_text(&mut self, text: String) -> Result<(), SourceGraphError> {
        let (parent_id, dom_path) = self.next_child_location()?;
        let id = self.nodes.len();
        self.nodes.push(HtmlNode {
            id,
            parent_id: Some(parent_id),
            kind: HtmlNodeKind::Text,
            name: None,
            text: Some(text),
            attributes: Vec::new(),
            dom_path,
            document_id: self.document_id,
            start_tag_range: None,
        });
        Ok(())
    }

    fn close_tag(&mut self, name: &str, offset: usize) -> Result<(), SourceGraphError> {
        let Some(open) = self.stack.pop() else {
            return Err(SourceGraphError::UnsupportedHtml {
                reason: format!("unexpected closing tag `</{name}>`"),
                offset,
            });
        };

        if open.id == 0 {
            return Err(SourceGraphError::UnsupportedHtml {
                reason: format!("unexpected closing tag `</{name}>`"),
                offset,
            });
        }

        let node = self
            .nodes
            .get(open.id)
            .ok_or_else(|| SourceGraphError::UnsupportedHtml {
                reason: "open node was missing from the HTML tree".to_owned(),
                offset,
            })?;
        if node.name.as_deref() != Some(name) {
            return Err(SourceGraphError::UnsupportedHtml {
                reason: format!(
                    "mismatched closing tag `</{name}>`, expected `</{}>`",
                    node.name.as_deref().unwrap_or("?")
                ),
                offset,
            });
        }

        Ok(())
    }

    fn next_child_location(&mut self) -> Result<(usize, String), SourceGraphError> {
        let parent = self
            .stack
            .last_mut()
            .ok_or_else(|| SourceGraphError::UnsupportedHtml {
                reason: "document root must exist while parsing HTML".to_owned(),
                offset: 0,
            })?;
        let child_index = parent.next_child_index;
        parent.next_child_index += 1;
        Ok((parent.id, format!("{}/{}", parent.path, child_index)))
    }
}

fn raw_text_end(source: &str, start: usize, tag: &str) -> Option<usize> {
    source[start..].match_indices("</").find_map(|(index, _)| {
        let close = start + index;
        let after_slash = &source.as_bytes()[close + 2..];
        let matches = after_slash
            .get(..tag.len())?
            .eq_ignore_ascii_case(tag.as_bytes());
        let ends_tag = after_slash
            .get(tag.len())
            .is_some_and(|byte| *byte == b'>' || byte.is_ascii_whitespace());
        (matches && ends_tag).then_some(close)
    })
}
