//! Evidence-locked tailoring for an imported DOCX resume.
//!
//! Bluey keeps the original package and changes only employment bullet text
//! that the resume generation pipeline has tied to confirmed evidence. The
//! patcher fails closed when a source bullet is missing or ambiguous.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
};

use anyhow::{anyhow, bail, Context, Result};
use quick_xml::{events::Event, Reader, Writer};
use serde_json::Value;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const DOCUMENT_XML: &str = "word/document.xml";
const MAX_REWRITES: usize = 64;
const MAX_BULLET_BYTES: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Replacement {
    before: String,
    after: String,
}

#[derive(Debug)]
enum XmlChunk {
    Raw(Event<'static>),
    Paragraph(Vec<Event<'static>>),
}

#[derive(Debug)]
struct ZipEntry {
    name: String,
    bytes: Vec<u8>,
    compression: CompressionMethod,
    unix_mode: Option<u32>,
    is_dir: bool,
}

/// Apply the evidence-backed `experience_rewrites` from a ResumeVersion diff
/// to the original DOCX package while retaining every other package part.
pub fn patch_docx(source: &[u8], diff: &Value) -> Result<Vec<u8>> {
    let replacements = replacements_from_diff(diff)?;
    if replacements.is_empty() {
        return Ok(source.to_vec());
    }

    let mut archive = ZipArchive::new(Cursor::new(source)).context("open source DOCX")?;
    if archive.by_name("[Content_Types].xml").is_err() {
        bail!("source file is not a valid DOCX package");
    }

    let mut entries = Vec::with_capacity(archive.len());
    let mut found_document = false;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).context("read DOCX entry")?;
        let name = entry.name().to_string();
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .with_context(|| format!("read DOCX entry {name}"))?;
        if name == DOCUMENT_XML {
            bytes = patch_document_xml(&bytes, &replacements)?;
            found_document = true;
        }
        entries.push(ZipEntry {
            name,
            bytes,
            compression: entry.compression(),
            unix_mode: entry.unix_mode(),
            is_dir: entry.is_dir(),
        });
    }
    if !found_document {
        bail!("source DOCX does not contain word/document.xml");
    }

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for entry in entries {
        let mut options = SimpleFileOptions::default().compression_method(entry.compression);
        if let Some(mode) = entry.unix_mode {
            options = options.unix_permissions(mode);
        }
        if entry.is_dir {
            writer
                .add_directory(&entry.name, options)
                .with_context(|| format!("write DOCX directory {}", entry.name))?;
        } else {
            writer
                .start_file(&entry.name, options)
                .with_context(|| format!("write DOCX entry {}", entry.name))?;
            writer
                .write_all(&entry.bytes)
                .with_context(|| format!("write DOCX bytes {}", entry.name))?;
        }
    }
    Ok(writer
        .finish()
        .context("finish tailored DOCX")?
        .into_inner())
}

fn replacements_from_diff(diff: &Value) -> Result<Vec<Replacement>> {
    let Some(values) = diff.get("experience_rewrites").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    if values.len() > MAX_REWRITES {
        bail!("resume diff contains too many employment rewrites");
    }

    let mut seen = BTreeSet::new();
    let mut replacements = Vec::new();
    for value in values {
        let before = value
            .get("before")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let after = value
            .get("after")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let evidence = value
            .get("source_evidence_ids")
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty())
            .ok_or_else(|| anyhow!("resume rewrite is missing evidence IDs"))?;
        if evidence
            .iter()
            .any(|item| item.as_str().map(str::trim).unwrap_or_default().is_empty())
        {
            bail!("resume rewrite contains an empty evidence ID");
        }
        if before.is_empty() || after.is_empty() || before == after {
            bail!("resume rewrite must contain distinct source and replacement text");
        }
        if before.len() > MAX_BULLET_BYTES || after.len() > MAX_BULLET_BYTES {
            bail!("resume rewrite exceeds the supported bullet length");
        }
        let key = normalize_text(before);
        if !seen.insert(key) {
            bail!("resume diff contains the same source bullet more than once");
        }
        replacements.push(Replacement {
            before: before.to_string(),
            after: after.to_string(),
        });
    }
    Ok(replacements)
}

fn patch_document_xml(source: &[u8], replacements: &[Replacement]) -> Result<Vec<u8>> {
    let chunks = parse_document_chunks(source)?;
    let replacement_map = replacements
        .iter()
        .map(|item| (normalize_text(&item.before), item.after.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut counts = replacement_map
        .keys()
        .map(|key| (key.clone(), 0_usize))
        .collect::<BTreeMap<_, _>>();

    for chunk in &chunks {
        let XmlChunk::Paragraph(events) = chunk else {
            continue;
        };
        let key = normalize_text(&paragraph_text(events)?);
        if let Some(count) = counts.get_mut(&key) {
            *count += 1;
        }
    }
    for replacement in replacements {
        let key = normalize_text(&replacement.before);
        match counts.get(&key).copied().unwrap_or_default() {
            1 => {}
            0 => bail!("source resume no longer contains a bullet selected for tailoring"),
            _ => bail!("source resume contains an ambiguous duplicate bullet"),
        }
    }

    let mut writer = Writer::new(Vec::with_capacity(source.len()));
    for chunk in chunks {
        match chunk {
            XmlChunk::Raw(event) => writer.write_event(event)?,
            XmlChunk::Paragraph(events) => {
                let key = normalize_text(&paragraph_text(&events)?);
                if let Some(replacement) = replacement_map.get(&key) {
                    write_rewritten_paragraph(&mut writer, events, replacement)?;
                } else {
                    for event in events {
                        writer.write_event(event)?;
                    }
                }
            }
        }
    }
    Ok(writer.into_inner())
}

fn parse_document_chunks(source: &[u8]) -> Result<Vec<XmlChunk>> {
    let mut reader = Reader::from_reader(source);
    reader.config_mut().trim_text(false);
    let mut chunks = Vec::new();
    let mut buffer = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buffer)
            .context("parse DOCX XML")?
        {
            Event::Eof => break,
            Event::Start(start) if start.local_name().as_ref() == b"p" => {
                let mut paragraph = vec![Event::Start(start.into_owned())];
                let mut depth = 1_usize;
                loop {
                    buffer.clear();
                    let event = reader
                        .read_event_into(&mut buffer)
                        .context("parse DOCX paragraph")?;
                    match &event {
                        Event::Start(value) if value.local_name().as_ref() == b"p" => depth += 1,
                        Event::End(value) if value.local_name().as_ref() == b"p" => depth -= 1,
                        Event::Eof => bail!("source DOCX contains an unfinished paragraph"),
                        _ => {}
                    }
                    paragraph.push(event.into_owned());
                    if depth == 0 {
                        break;
                    }
                }
                chunks.push(XmlChunk::Paragraph(paragraph));
            }
            event => chunks.push(XmlChunk::Raw(event.into_owned())),
        }
        buffer.clear();
    }
    Ok(chunks)
}

fn paragraph_text(events: &[Event<'static>]) -> Result<String> {
    let mut text_depth = 0_usize;
    let mut value = String::new();
    for event in events {
        match event {
            Event::Start(start) if start.local_name().as_ref() == b"t" => text_depth += 1,
            Event::End(end) if end.local_name().as_ref() == b"t" => {
                text_depth = text_depth.saturating_sub(1)
            }
            Event::Text(text) if text_depth > 0 => {
                value.push_str(&text.unescape().context("decode DOCX paragraph text")?);
            }
            _ => {}
        }
    }
    Ok(value)
}

fn write_rewritten_paragraph(
    writer: &mut Writer<Vec<u8>>,
    events: Vec<Event<'static>>,
    replacement: &str,
) -> Result<()> {
    let mut text_depth = 0_usize;
    let mut emitted = false;
    for event in events {
        match &event {
            Event::Start(start) if start.local_name().as_ref() == b"t" => text_depth += 1,
            Event::End(end) if end.local_name().as_ref() == b"t" => {
                text_depth = text_depth.saturating_sub(1)
            }
            Event::Text(_) if text_depth > 0 => {
                if !emitted {
                    writer
                        .write_event(Event::Text(quick_xml::events::BytesText::new(replacement)))?;
                    emitted = true;
                }
                continue;
            }
            _ => {}
        }
        writer.write_event(event)?;
    }
    if !emitted {
        bail!("matched source bullet does not contain editable DOCX text");
    }
    Ok(())
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn source_docx(paragraphs: &[&str]) -> Vec<u8> {
        let body = paragraphs
            .iter()
            .map(|value| format!("<w:p><w:r><w:t>{value}</w:t></w:r></w:p>"))
            .collect::<String>();
        let document = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
        );
        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.start_file("[Content_Types].xml", options).unwrap();
        writer
            .write_all(
                b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>",
            )
            .unwrap();
        writer.start_file(DOCUMENT_XML, options).unwrap();
        writer.write_all(document.as_bytes()).unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn diff(before: &str, after: &str) -> Value {
        json!({
            "experience_rewrites": [{
                "before": before,
                "after": after,
                "source_evidence_ids": ["employment:role-1:highlight:0"]
            }]
        })
    }

    fn document_xml(bytes: &[u8]) -> String {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut value = String::new();
        archive
            .by_name(DOCUMENT_XML)
            .unwrap()
            .read_to_string(&mut value)
            .unwrap();
        value
    }

    #[test]
    fn no_rewrites_return_exact_original_bytes() {
        let source = source_docx(&["Original bullet"]);
        assert_eq!(patch_docx(&source, &json!({})).unwrap(), source);
    }

    #[test]
    fn rewrites_only_selected_bullet_and_keeps_identity_text() {
        let source = source_docx(&[
            "Capital One",
            "Software Engineer",
            "Built reliable APIs",
            "January 2021 - Present",
        ]);
        let output = patch_docx(
            &source,
            &diff(
                "Built reliable APIs",
                "Built reliable APIs for high-volume payment workflows",
            ),
        )
        .unwrap();
        let xml = document_xml(&output);
        assert!(xml.contains("Capital One"));
        assert!(xml.contains("Software Engineer"));
        assert!(xml.contains("January 2021 - Present"));
        assert!(xml.contains("high-volume payment workflows"));
        assert!(!xml.contains(">Built reliable APIs<"));
    }

    #[test]
    fn rejects_missing_or_ambiguous_source_bullets() {
        let missing = source_docx(&["Different bullet"]);
        assert!(patch_docx(&missing, &diff("Expected bullet", "Better bullet")).is_err());

        let duplicate = source_docx(&["Repeated bullet", "Repeated bullet"]);
        assert!(patch_docx(&duplicate, &diff("Repeated bullet", "Better bullet")).is_err());
    }

    #[test]
    fn rejects_rewrite_without_evidence() {
        let source = source_docx(&["Original bullet"]);
        let diff = json!({
            "experience_rewrites": [{
                "before": "Original bullet",
                "after": "Better bullet",
                "source_evidence_ids": []
            }]
        });
        assert!(patch_docx(&source, &diff).is_err());
    }
}
