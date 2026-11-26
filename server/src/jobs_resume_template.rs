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
use zip::{write::SimpleFileOptions, CompressionMethod, DateTime, ZipArchive, ZipWriter};

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

#[derive(Debug, Clone)]
struct ZipEntry {
    name: String,
    bytes: Vec<u8>,
    compression: CompressionMethod,
    unix_mode: Option<u32>,
    last_modified: Option<DateTime>,
    is_dir: bool,
}

/// Apply the evidence-backed `experience_rewrites` from a ResumeVersion diff
/// to the original DOCX package while retaining every other package part.
pub fn patch_docx(source: &[u8], diff: &Value) -> Result<Vec<u8>> {
    let replacements = replacements_from_diff(diff)?;
    if replacements.is_empty() {
        return Ok(source.to_vec());
    }

    let source_entries = read_docx_entries(source)?;
    validate_source_package(&source_entries)?;
    let mut tailored_entries = source_entries.clone();
    let document = tailored_entries
        .iter_mut()
        .find(|entry| entry.name == DOCUMENT_XML)
        .ok_or_else(|| anyhow!("source DOCX does not contain word/document.xml"))?;
    document.bytes = patch_document_xml(&document.bytes, &replacements)?;

    let output = write_docx_entries(&tailored_entries)?;
    verify_tailored_package(&source_entries, &output, &replacements)?;
    Ok(output)
}

fn read_docx_entries(source: &[u8]) -> Result<Vec<ZipEntry>> {
    let mut archive = ZipArchive::new(Cursor::new(source)).context("open DOCX package")?;
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).context("read DOCX entry")?;
        let name = entry.name().to_string();
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .with_context(|| format!("read DOCX entry {name}"))?;
        entries.push(ZipEntry {
            name,
            bytes,
            compression: entry.compression(),
            unix_mode: entry.unix_mode(),
            last_modified: entry.last_modified(),
            is_dir: entry.is_dir(),
        });
    }
    Ok(entries)
}

fn validate_source_package(entries: &[ZipEntry]) -> Result<()> {
    let mut names = BTreeSet::new();
    for entry in entries {
        if !names.insert(entry.name.as_str()) {
            bail!("source DOCX contains a duplicate package entry");
        }
    }
    if !names.contains("[Content_Types].xml") {
        bail!("source file is not a valid DOCX package");
    }
    if !names.contains(DOCUMENT_XML) {
        bail!("source DOCX does not contain word/document.xml");
    }
    Ok(())
}

fn write_docx_entries(entries: &[ZipEntry]) -> Result<Vec<u8>> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for entry in entries {
        let mut options = SimpleFileOptions::default().compression_method(entry.compression);
        if let Some(mode) = entry.unix_mode {
            options = options.unix_permissions(mode);
        }
        if let Some(last_modified) = entry.last_modified {
            options = options.last_modified_time(last_modified);
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

fn verify_tailored_package(
    source_entries: &[ZipEntry],
    output: &[u8],
    replacements: &[Replacement],
) -> Result<()> {
    let output_entries = read_docx_entries(output).context("reopen tailored DOCX")?;
    validate_source_package(&output_entries).context("validate tailored DOCX package")?;
    if output_entries.len() != source_entries.len() {
        bail!("tailored DOCX changed the package entry count");
    }

    for (source, tailored) in source_entries.iter().zip(&output_entries) {
        if source.name != tailored.name {
            bail!("tailored DOCX changed package entry order or names");
        }
        if source.compression != tailored.compression || source.is_dir != tailored.is_dir {
            bail!("tailored DOCX changed package entry metadata");
        }
        if let Some(source_mode) = source.unix_mode {
            let tailored_mode = tailored
                .unix_mode
                .ok_or_else(|| anyhow!("tailored DOCX removed package entry permissions"))?;
            if source_mode & 0o777 != tailored_mode & 0o777 {
                bail!("tailored DOCX changed package entry permissions");
            }
        }
        if source.last_modified.is_some() && source.last_modified != tailored.last_modified {
            bail!("tailored DOCX changed package entry timestamps");
        }
        if source.name != DOCUMENT_XML && source.bytes != tailored.bytes {
            bail!("tailored DOCX changed a non-document package part");
        }
    }

    let source_document = source_entries
        .iter()
        .find(|entry| entry.name == DOCUMENT_XML)
        .ok_or_else(|| anyhow!("source DOCX does not contain word/document.xml"))?;
    let tailored_document = output_entries
        .iter()
        .find(|entry| entry.name == DOCUMENT_XML)
        .ok_or_else(|| anyhow!("tailored DOCX does not contain word/document.xml"))?;
    verify_document_rewrites(
        &source_document.bytes,
        &tailored_document.bytes,
        replacements,
    )
}

fn verify_document_rewrites(
    source: &[u8],
    tailored: &[u8],
    replacements: &[Replacement],
) -> Result<()> {
    let source_paragraphs = paragraph_values(source)?;
    let tailored_paragraphs = paragraph_values(tailored)?;
    if source_paragraphs.len() != tailored_paragraphs.len() {
        bail!("tailored DOCX changed the document paragraph count");
    }

    let expected_rewrites = replacements
        .iter()
        .map(|replacement| {
            (
                normalize_text(&replacement.before),
                normalize_text(&replacement.after),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (source_value, tailored_value) in source_paragraphs.iter().zip(&tailored_paragraphs) {
        if let Some(expected) = expected_rewrites.get(source_value) {
            if tailored_value != expected {
                bail!("tailored DOCX did not apply the expected paragraph rewrite");
            }
        } else if tailored_value != source_value {
            bail!("tailored DOCX changed an unrelated paragraph");
        }
    }

    for replacement in replacements {
        let before = normalize_text(&replacement.before);
        let after = normalize_text(&replacement.after);
        if tailored_paragraphs
            .iter()
            .filter(|value| value.as_str() == before.as_str())
            .count()
            != 0
        {
            bail!("tailored DOCX retained a source bullet selected for replacement");
        }
        if tailored_paragraphs
            .iter()
            .filter(|value| value.as_str() == after.as_str())
            .count()
            != 1
        {
            bail!("tailored DOCX did not materialize one exact replacement bullet");
        }
    }
    Ok(())
}

fn paragraph_values(source: &[u8]) -> Result<Vec<String>> {
    parse_document_chunks(source)?
        .into_iter()
        .filter_map(|chunk| match chunk {
            XmlChunk::Paragraph(events) => {
                Some(paragraph_text(&events).map(|value| normalize_text(&value)))
            }
            XmlChunk::Raw(_) => None,
        })
        .collect()
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
        if before.is_empty() || after.is_empty() || normalize_text(before) == normalize_text(after)
        {
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

    fn write_entry(
        writer: &mut ZipWriter<Cursor<Vec<u8>>>,
        name: &str,
        bytes: &[u8],
        options: SimpleFileOptions,
    ) {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }

    fn package_docx(document: &str, include_full_package: bool) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        let last_modified = DateTime::from_date_and_time(2026, 7, 21, 14, 32, 10).unwrap();
        let deflated = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644)
            .last_modified_time(last_modified);
        let stored = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644)
            .last_modified_time(last_modified);

        write_entry(
            &mut writer,
            "[Content_Types].xml",
            b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>",
            deflated,
        );
        if include_full_package {
            write_entry(
                &mut writer,
                "_rels/.rels",
                b"<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"/>",
                deflated,
            );
            write_entry(
                &mut writer,
                "docProps/core.xml",
                b"<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\"/>",
                deflated,
            );
            write_entry(
                &mut writer,
                "word/_rels/document.xml.rels",
                b"<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"/>",
                deflated,
            );
            write_entry(
                &mut writer,
                "word/styles.xml",
                b"<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:style w:styleId=\"ResumeBody\"/></w:styles>",
                deflated,
            );
            write_entry(
                &mut writer,
                "word/numbering.xml",
                b"<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"/>",
                deflated,
            );
            write_entry(
                &mut writer,
                "word/header1.xml",
                b"<w:hdr xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:p><w:r><w:t>Candidate</w:t></w:r></w:p></w:hdr>",
                deflated,
            );
            write_entry(
                &mut writer,
                "word/footer1.xml",
                b"<w:ftr xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:p><w:r><w:t>Page 1</w:t></w:r></w:p></w:ftr>",
                deflated,
            );
            write_entry(
                &mut writer,
                "word/media/image1.png",
                b"\x89PNG\r\n\x1a\nbluey-resume-fixture",
                stored,
            );
        }
        write_entry(&mut writer, DOCUMENT_XML, document.as_bytes(), deflated);
        writer.finish().unwrap().into_inner()
    }

    fn source_docx(paragraphs: &[&str]) -> Vec<u8> {
        let body = paragraphs
            .iter()
            .map(|value| format!("<w:p><w:r><w:t>{value}</w:t></w:r></w:p>"))
            .collect::<String>();
        let document = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
        );
        package_docx(&document, false)
    }

    fn rich_source_docx() -> Vec<u8> {
        let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Experience</w:t></w:r></w:p>
    <w:p>
      <w:pPr><w:pStyle w:val="ListBullet"/><w:numPr><w:ilvl w:val="0"/></w:numPr></w:pPr>
      <w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">Built reliable </w:t></w:r>
      <w:r><w:rPr><w:i/></w:rPr><w:t>APIs</w:t></w:r>
    </w:p>
    <w:p><w:r><w:t>January 2021 - Present</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        package_docx(document, true)
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

    #[test]
    fn preserves_full_package_and_split_run_formatting() {
        let source = rich_source_docx();
        let source_entries = read_docx_entries(&source).unwrap();
        let output = patch_docx(
            &source,
            &diff(
                "Built reliable APIs",
                "Built reliable APIs & event pipelines for payment workloads",
            ),
        )
        .unwrap();
        let output_entries = read_docx_entries(&output).unwrap();

        assert_eq!(source_entries.len(), output_entries.len());
        for (before, after) in source_entries.iter().zip(&output_entries) {
            assert_eq!(before.name, after.name);
            assert_eq!(before.compression, after.compression);
            assert_eq!(before.is_dir, after.is_dir);
            assert_eq!(before.last_modified, after.last_modified);
            assert_eq!(
                before.unix_mode.map(|mode| mode & 0o777),
                after.unix_mode.map(|mode| mode & 0o777)
            );
            if before.name != DOCUMENT_XML {
                assert_eq!(before.bytes, after.bytes, "changed {}", before.name);
            }
        }

        let xml = document_xml(&output);
        assert!(xml.contains("w:val=\"ListBullet\""));
        assert!(xml.contains("<w:b"));
        assert!(xml.contains("<w:i"));
        assert!(xml.contains("payment workloads"));
        assert!(xml.contains("&amp;"));
        assert_eq!(
            paragraph_values(xml.as_bytes()).unwrap(),
            vec![
                "Experience",
                "Built reliable APIs & event pipelines for payment workloads",
                "January 2021 - Present"
            ]
        );
    }

    #[test]
    fn rejects_normalized_no_op_rewrite() {
        let source = source_docx(&["Built reliable APIs"]);
        assert!(patch_docx(
            &source,
            &diff("Built reliable APIs", "  Built   reliable APIs  "),
        )
        .is_err());
    }

    #[test]
    fn verifier_rejects_unrelated_paragraph_changes() {
        let source = source_docx(&["Capital One", "Software Engineer", "Built reliable APIs"]);
        let source_xml = document_xml(&source);
        let tailored_xml = source_xml
            .replace(">Software Engineer<", ">Data Engineer<")
            .replace(">Built reliable APIs<", ">Built reliable payment APIs<");
        let replacements =
            replacements_from_diff(&diff("Built reliable APIs", "Built reliable payment APIs"))
                .unwrap();

        let error = verify_document_rewrites(
            source_xml.as_bytes(),
            tailored_xml.as_bytes(),
            &replacements,
        )
        .unwrap_err();
        assert!(error.to_string().contains("unrelated paragraph"));
    }
}
