//! Comment-preserving Vibra/YAML document helpers for the `code` stdlib.

use anyhow::{bail, Context, Result};
use std::str::FromStr;
use yaml_edit::{Document, YamlNode};

pub fn parse(source: &str) -> Result<String> {
    let _ = Document::from_str(source).context("parse Vibra code document")?;
    Ok(source.to_string())
}

pub fn emit(document: &str) -> Result<String> {
    let doc = Document::from_str(document).context("parse Vibra code document")?;
    Ok(restore_leading_prefix(document, doc.to_string()))
}

pub fn get(document: &str, pointer: &str) -> Result<String> {
    let doc = Document::from_str(document).context("parse Vibra code document")?;
    let node = get_node(&doc, pointer)?;
    Ok(node.to_string())
}

pub fn set(document: &str, pointer: &str, value: &str) -> Result<String> {
    if is_root_pointer(pointer) {
        return emit(value);
    }
    let doc = Document::from_str(document).context("parse Vibra code document")?;
    let value_doc = Document::from_str(value).context("parse replacement Vibra code value")?;
    let mut segments = pointer_segments(pointer)?;
    let leaf = segments
        .pop()
        .context("JSON Pointer must identify a value to set")?;
    let parent = node_at_segments(&doc, &segments)?;
    match parent {
        YamlNode::Mapping(map) => {
            map.set(leaf, value_doc);
            Ok(restore_leading_prefix(document, doc.to_string()))
        }
        YamlNode::Sequence(seq) => {
            if leaf == "-" {
                bail!("JSON Pointer append marker `-` is only valid for `code.append`");
            }
            let index = parse_index(&leaf)?;
            if !seq.set(index, value_doc) {
                bail!("JSON Pointer index `{index}` is out of bounds");
            }
            Ok(restore_leading_prefix(document, doc.to_string()))
        }
        other => bail!("JSON Pointer parent is not a mapping or sequence: {other:?}"),
    }
}

pub fn remove(document: &str, pointer: &str) -> Result<String> {
    if is_root_pointer(pointer) {
        bail!("JSON Pointer root cannot be removed");
    }
    let doc = Document::from_str(document).context("parse Vibra code document")?;
    let mut segments = pointer_segments(pointer)?;
    let leaf = segments
        .pop()
        .context("JSON Pointer must identify a value to remove")?;
    let parent = node_at_segments(&doc, &segments)?;
    match parent {
        YamlNode::Mapping(map) => {
            map.remove(&leaf)
                .with_context(|| format!("JSON Pointer key `{leaf}` does not exist"))?;
            Ok(restore_leading_prefix(document, doc.to_string()))
        }
        YamlNode::Sequence(seq) => {
            let index = parse_index(&leaf)?;
            seq.remove(index)
                .with_context(|| format!("JSON Pointer index `{index}` is out of bounds"))?;
            Ok(restore_leading_prefix(document, doc.to_string()))
        }
        other => bail!("JSON Pointer parent is not a mapping or sequence: {other:?}"),
    }
}

pub fn append(document: &str, pointer: &str, value: &str) -> Result<String> {
    let mut segments = pointer_segments(pointer)?;
    let Some(leaf) = segments.pop() else {
        bail!("JSON Pointer append path must end in `/-`");
    };
    if leaf != "-" {
        bail!("JSON Pointer append path must end in `/-`");
    }
    let doc = Document::from_str(document).context("parse Vibra code document")?;
    let value_doc = Document::from_str(value).context("parse appended Vibra code value")?;
    let parent = node_at_segments(&doc, &segments)?;
    let YamlNode::Sequence(seq) = parent else {
        bail!("JSON Pointer append target is not a sequence");
    };
    seq.push(value_doc);
    Ok(restore_leading_prefix(document, doc.to_string()))
}

fn restore_leading_prefix(original: &str, rendered: String) -> String {
    let mut prefix = String::new();
    for line in original.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.trim().is_empty() {
            prefix.push_str(line);
        } else {
            break;
        }
    }
    if prefix.is_empty() || rendered.starts_with(&prefix) {
        rendered
    } else {
        format!("{prefix}{rendered}")
    }
}

fn get_node(doc: &Document, pointer: &str) -> Result<YamlNode> {
    if is_root_pointer(pointer) {
        return root_node(doc);
    }
    let segments = pointer_segments(pointer)?;
    node_at_segments(doc, &segments)
}

fn node_at_segments(doc: &Document, segments: &[String]) -> Result<YamlNode> {
    let mut node = root_node(doc)?;
    for segment in segments {
        node = match node {
            YamlNode::Mapping(map) => map
                .get(segment)
                .with_context(|| format!("JSON Pointer key `{segment}` does not exist"))?,
            YamlNode::Sequence(seq) => {
                let index = parse_index(segment)?;
                seq.get(index)
                    .with_context(|| format!("JSON Pointer index `{index}` is out of bounds"))?
            }
            other => bail!("JSON Pointer cannot descend through non-container node: {other:?}"),
        };
    }
    Ok(node)
}

fn root_node(doc: &Document) -> Result<YamlNode> {
    if let Some(map) = doc.as_mapping() {
        return Ok(YamlNode::Mapping(map));
    }
    if let Some(seq) = doc.as_sequence() {
        return Ok(YamlNode::Sequence(seq));
    }
    if let Some(scalar) = doc.as_scalar() {
        return Ok(YamlNode::Scalar(scalar));
    }
    bail!("Vibra code document has no root node")
}

fn pointer_segments(pointer: &str) -> Result<Vec<String>> {
    if is_root_pointer(pointer) {
        return Ok(Vec::new());
    }
    if !pointer.starts_with('/') {
        bail!("JSON Pointer must start with `/`");
    }
    pointer.split('/').skip(1).map(unescape_segment).collect()
}

fn is_root_pointer(pointer: &str) -> bool {
    pointer.is_empty()
}

fn unescape_segment(segment: &str) -> Result<String> {
    let mut out = String::new();
    let mut chars = segment.chars();
    while let Some(ch) = chars.next() {
        if ch != '~' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('0') => out.push('~'),
            Some('1') => out.push('/'),
            Some(other) => bail!("invalid JSON Pointer escape `~{other}`"),
            None => bail!("invalid trailing `~` in JSON Pointer"),
        }
    }
    Ok(out)
}

fn parse_index(segment: &str) -> Result<usize> {
    if segment.is_empty() {
        bail!("JSON Pointer sequence index must not be empty");
    }
    segment
        .parse::<usize>()
        .with_context(|| format!("JSON Pointer segment `{segment}` is not a sequence index"))
}
