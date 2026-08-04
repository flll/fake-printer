//! IPP 1.1 wire format: request parsing and response building.
//!
//! Faithful port of the hand-rolled encoder/decoder in paperlessprinter
//! `server.py`. The byte layout of every response is kept identical so that
//! clients tuned against the Python implementation behave the same.

use std::collections::HashMap;

// Operation ids.
pub const OP_PRINT_JOB: u16 = 0x0002;
pub const OP_VALIDATE_JOB: u16 = 0x0004;
pub const OP_CREATE_JOB: u16 = 0x0005;
pub const OP_SEND_DOCUMENT: u16 = 0x0006;
pub const OP_GET_JOB_ATTRIBUTES: u16 = 0x0009;
pub const OP_GET_JOBS: u16 = 0x000A;
pub const OP_GET_PRINTER_ATTRIBUTES: u16 = 0x000B;

// Status codes.
pub const STATUS_OK: u16 = 0x0000;
pub const STATUS_CLIENT_ERROR_BAD_REQUEST: u16 = 0x0400;
pub const STATUS_SERVER_ERROR_OPERATION_NOT_SUPPORTED: u16 = 0x0501;

// Delimiter tags.
pub const TAG_OPERATION_ATTRIBUTES: u8 = 0x01;
pub const TAG_JOB_ATTRIBUTES: u8 = 0x02;
pub const TAG_END_OF_ATTRIBUTES: u8 = 0x03;
pub const TAG_PRINTER_ATTRIBUTES: u8 = 0x04;

const DELIMITER_TAGS: [u8; 5] = [0x01, 0x02, 0x03, 0x04, 0x05];

// Value tags.
pub const VT_TEXT_WITHOUT_LANGUAGE: u8 = 0x41;
pub const VT_NAME_WITHOUT_LANGUAGE: u8 = 0x42;
pub const VT_KEYWORD: u8 = 0x44;
pub const VT_URI: u8 = 0x45;
pub const VT_CHARSET: u8 = 0x47;
pub const VT_NATURAL_LANGUAGE: u8 = 0x48;
pub const VT_MIME_MEDIA_TYPE: u8 = 0x49;
pub const VT_BOOLEAN: u8 = 0x22;
pub const VT_INTEGER: u8 = 0x21;
pub const VT_ENUM: u8 = 0x23;

pub fn op_name(operation_id: u16) -> String {
    match operation_id {
        OP_PRINT_JOB => "Print-Job".into(),
        OP_VALIDATE_JOB => "Validate-Job".into(),
        OP_CREATE_JOB => "Create-Job".into(),
        OP_SEND_DOCUMENT => "Send-Document".into(),
        OP_GET_JOB_ATTRIBUTES => "Get-Job-Attributes".into(),
        OP_GET_JOBS => "Get-Jobs".into(),
        OP_GET_PRINTER_ATTRIBUTES => "Get-Printer-Attributes".into(),
        other => format!("op-0x{other:04x}"),
    }
}

pub fn attr(tag: u8, name: &str, value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 2 + name.len() + 2 + value.len());
    out.push(tag);
    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value);
    out
}

pub fn attr_str(tag: u8, name: &str, value: &str) -> Vec<u8> {
    attr(tag, name, value.as_bytes())
}

pub fn attr_bool(name: &str, value: bool) -> Vec<u8> {
    attr(VT_BOOLEAN, name, if value { &[0x01] } else { &[0x00] })
}

pub fn attr_i32(tag: u8, name: &str, value: i32) -> Vec<u8> {
    attr(tag, name, &value.to_be_bytes())
}

/// 1setOf integers: additional values carry a zero-length name.
pub fn attr_i32_set(tag: u8, name: &str, values: &[i32]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, v) in values.iter().enumerate() {
        if i == 0 {
            out.extend_from_slice(&attr(tag, name, &v.to_be_bytes()));
        } else {
            out.push(tag);
            out.extend_from_slice(&0u16.to_be_bytes());
            out.extend_from_slice(&4u16.to_be_bytes());
            out.extend_from_slice(&v.to_be_bytes());
        }
    }
    out
}

/// 1setOf strings: additional values carry a zero-length name.
pub fn attr_str_set(tag: u8, name: &str, values: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, v) in values.iter().enumerate() {
        if i == 0 {
            out.extend_from_slice(&attr(tag, name, v.as_bytes()));
        } else {
            out.push(tag);
            out.extend_from_slice(&0u16.to_be_bytes());
            out.extend_from_slice(&(v.len() as u16).to_be_bytes());
            out.extend_from_slice(v.as_bytes());
        }
    }
    out
}

pub fn build_response(
    version_major: u8,
    version_minor: u8,
    status_code: u16,
    request_id: u32,
    attribute_bytes: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + attribute_bytes.len() + 1);
    out.push(version_major);
    out.push(version_minor);
    out.extend_from_slice(&status_code.to_be_bytes());
    out.extend_from_slice(&request_id.to_be_bytes());
    out.extend_from_slice(attribute_bytes);
    out.push(TAG_END_OF_ATTRIBUTES);
    out
}

/// Operation attributes group present in every response.
pub fn operation_attributes() -> Vec<u8> {
    let mut out = vec![TAG_OPERATION_ATTRIBUTES];
    out.extend_from_slice(&attr_str(VT_CHARSET, "attributes-charset", "utf-8"));
    out.extend_from_slice(&attr_str(
        VT_NATURAL_LANGUAGE,
        "attributes-natural-language",
        "en",
    ));
    out
}

pub fn printer_attributes(
    host_header: &str,
    ipp_path: &str,
    busy: bool,
    document_formats: &[&str],
) -> Vec<u8> {
    let host = if host_header.is_empty() { "127.0.0.1" } else { host_header };
    let printer_uri = format!("ipp://{host}{ipp_path}");

    let mut a = operation_attributes();
    a.push(TAG_PRINTER_ATTRIBUTES);
    a.extend_from_slice(&attr_str(VT_URI, "printer-uri-supported", &printer_uri));
    a.extend_from_slice(&attr_str(VT_KEYWORD, "uri-authentication-supported", "none"));
    a.extend_from_slice(&attr_str(VT_KEYWORD, "uri-security-supported", "none"));
    a.extend_from_slice(&attr_str(VT_NAME_WITHOUT_LANGUAGE, "printer-name", "ipp-to-png"));
    a.extend_from_slice(&attr_str(
        VT_TEXT_WITHOUT_LANGUAGE,
        "printer-make-and-model",
        "ipp-to-png",
    ));
    a.extend_from_slice(&attr_str(VT_KEYWORD, "ipp-versions-supported", "1.1"));
    a.extend_from_slice(&attr_i32_set(
        VT_ENUM,
        "operations-supported",
        &[
            OP_PRINT_JOB as i32,
            OP_VALIDATE_JOB as i32,
            OP_CREATE_JOB as i32,
            OP_SEND_DOCUMENT as i32,
            OP_GET_JOB_ATTRIBUTES as i32,
            OP_GET_JOBS as i32,
            OP_GET_PRINTER_ATTRIBUTES as i32,
        ],
    ));
    a.extend_from_slice(&attr_str(VT_CHARSET, "charset-configured", "utf-8"));
    a.extend_from_slice(&attr_str(VT_CHARSET, "charset-supported", "utf-8"));
    a.extend_from_slice(&attr_str(VT_NATURAL_LANGUAGE, "natural-language-configured", "en"));
    a.extend_from_slice(&attr_str(
        VT_NATURAL_LANGUAGE,
        "generated-natural-language-supported",
        "en",
    ));
    a.extend_from_slice(&attr_bool("printer-is-accepting-jobs", true));
    // Android's built-in print service decides a job finished by watching the
    // printer transition processing(4) -> idle(3) via Get-Printer-Attributes
    // polling; a printer that always reports idle makes it wait ~46s per job.
    a.extend_from_slice(&attr_i32(VT_ENUM, "printer-state", if busy { 4 } else { 3 }));
    a.extend_from_slice(&attr_str(VT_KEYWORD, "printer-state-reasons", "none"));
    a.extend_from_slice(&attr_i32(VT_INTEGER, "queued-job-count", if busy { 1 } else { 0 }));
    a.extend_from_slice(&attr_str(
        VT_MIME_MEDIA_TYPE,
        "document-format-default",
        "application/pdf",
    ));
    a.extend_from_slice(&attr_str_set(
        VT_MIME_MEDIA_TYPE,
        "document-format-supported",
        document_formats,
    ));
    a.extend_from_slice(&attr_str(VT_KEYWORD, "compression-supported", "none"));

    // Tell clients (notably macOS/CUPS) that this printer supports color.
    // Without these, macOS may default the print pipeline/preview to B/W.
    a.extend_from_slice(&attr_bool("color-supported", true));
    a.extend_from_slice(&attr_str_set(
        VT_KEYWORD,
        "print-color-mode-supported",
        &["auto", "color", "monochrome"],
    ));
    a.extend_from_slice(&attr_str(VT_KEYWORD, "print-color-mode-default", "auto"));
    a.extend_from_slice(&attr_str_set(
        VT_KEYWORD,
        "output-mode-supported",
        &["auto", "color", "monochrome"],
    ));
    a.extend_from_slice(&attr_str(VT_KEYWORD, "output-mode-default", "auto"));
    a
}

fn job_attributes_group(host: &str, ipp_path: &str, job_id: i32, job_state: i32) -> Vec<u8> {
    let mut a = vec![TAG_JOB_ATTRIBUTES];
    a.extend_from_slice(&attr_i32(VT_INTEGER, "job-id", job_id));
    a.extend_from_slice(&attr_str(
        VT_URI,
        "job-uri",
        &format!("ipp://{host}{ipp_path}/job/{job_id}"),
    ));
    a.extend_from_slice(&attr_i32(VT_ENUM, "job-state", job_state));
    a.extend_from_slice(&attr_str(VT_KEYWORD, "job-state-reasons", "none"));
    a
}

pub fn get_job_attributes_response(
    host_header: &str,
    ipp_path: &str,
    job_id: i32,
    job_state: i32,
) -> Vec<u8> {
    let host = if host_header.is_empty() { "127.0.0.1" } else { host_header };
    let mut a = operation_attributes();
    if job_id > 0 {
        a.extend_from_slice(&job_attributes_group(host, ipp_path, job_id, job_state));
    }
    a
}

pub fn get_jobs_response(host_header: &str, ipp_path: &str, jobs: &[(i32, i32)]) -> Vec<u8> {
    let host = if host_header.is_empty() { "127.0.0.1" } else { host_header };
    let mut a = operation_attributes();
    for (job_id, job_state) in jobs {
        a.extend_from_slice(&job_attributes_group(host, ipp_path, *job_id, *job_state));
    }
    a
}

/// Create-Job response: job attributes with pending(3) state.
pub fn create_job_response(host_header: &str, ipp_path: &str, job_id: i32) -> Vec<u8> {
    let host = if host_header.is_empty() { "127.0.0.1" } else { host_header };
    let mut a = operation_attributes();
    a.extend_from_slice(&job_attributes_group(host, ipp_path, job_id, 3));
    a
}

/// Minimal metadata extracted from an IPP request plus the document payload.
#[derive(Debug, Default, Clone)]
pub struct ParsedRequest {
    pub version_major: u8,
    pub version_minor: u8,
    pub operation_id: u16,
    pub request_id: u32,
    /// Captured string fields (job-name, document-format, printer-uri,
    /// requesting-user-name, job-uri) and stringified job-id.
    pub meta: HashMap<String, String>,
    pub document: Vec<u8>,
}

impl ParsedRequest {
    pub fn meta_str(&self, key: &str) -> &str {
        self.meta.get(key).map(String::as_str).unwrap_or("")
    }

    pub fn job_id(&self) -> i32 {
        self.meta_str("job-id").parse().unwrap_or(0)
    }

    /// meta.json contents, matching the Python key set.
    pub fn meta_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert(
            "ipp_version".into(),
            format!("{}.{}", self.version_major, self.version_minor).into(),
        );
        map.insert("ipp_version_major".into(), self.version_major.to_string().into());
        map.insert("ipp_version_minor".into(), self.version_minor.to_string().into());
        map.insert("operation_id".into(), self.operation_id.to_string().into());
        map.insert("request_id".into(), self.request_id.to_string().into());
        for (k, v) in &self.meta {
            map.insert(k.clone(), v.clone().into());
        }
        // Python writes with sort_keys=True; serde_json::Map on a BTreeMap
        // feature would sort automatically, so sort explicitly here.
        let mut sorted = serde_json::Map::new();
        let mut keys: Vec<_> = map.keys().cloned().collect();
        keys.sort();
        for k in keys {
            sorted.insert(k.clone(), map[&k].clone());
        }
        serde_json::Value::Object(sorted)
    }
}

/// Extract minimal metadata and the document bytes from an IPP request.
///
/// For Print-Job, document data follows immediately after the
/// end-of-attributes tag (0x03). We parse enough of the attribute stream to
/// find that boundary and a couple of common fields.
pub fn parse_request(raw: &[u8]) -> Result<ParsedRequest, String> {
    if raw.len() < 8 {
        return Err("IPP request too short".into());
    }

    let mut req = ParsedRequest {
        version_major: raw[0],
        version_minor: raw[1],
        operation_id: u16::from_be_bytes([raw[2], raw[3]]),
        request_id: u32::from_be_bytes([raw[4], raw[5], raw[6], raw[7]]),
        ..Default::default()
    };

    let mut pos = 8usize;

    let read_u16 = |raw: &[u8], pos: &mut usize| -> Result<u16, String> {
        if *pos + 2 > raw.len() {
            return Err("IPP truncated (u16)".into());
        }
        let v = u16::from_be_bytes([raw[*pos], raw[*pos + 1]]);
        *pos += 2;
        Ok(v)
    };
    let read_bytes = |raw: &[u8], pos: &mut usize, n: usize| -> Result<Vec<u8>, String> {
        if *pos + n > raw.len() {
            return Err("IPP truncated (bytes)".into());
        }
        let b = raw[*pos..*pos + n].to_vec();
        *pos += n;
        Ok(b)
    };

    let mut last_name: Option<Vec<u8>> = None;

    while pos < raw.len() {
        let tag = raw[pos];
        pos += 1;

        if DELIMITER_TAGS.contains(&tag) {
            if tag == TAG_END_OF_ATTRIBUTES {
                // End of attributes: remainder is document data.
                break;
            }
            continue;
        }

        // value-tag: name-length (2), name, value-length (2), value
        let name_len = read_u16(raw, &mut pos)? as usize;
        let name = if name_len == 0 {
            last_name
                .clone()
                .ok_or_else(|| String::from("IPP additional value without previous name"))?
        } else {
            let n = read_bytes(raw, &mut pos, name_len)?;
            last_name = Some(n.clone());
            n
        };

        let value_len = read_u16(raw, &mut pos)? as usize;
        let value = read_bytes(raw, &mut pos, value_len)?;

        let name_str = String::from_utf8_lossy(&name).into_owned();
        match name_str.as_str() {
            "job-name" | "document-format" | "printer-uri" | "requesting-user-name"
            | "job-uri" => {
                req.meta
                    .insert(name_str, String::from_utf8_lossy(&value).into_owned());
            }
            "job-id" if value.len() == 4 => {
                let v = i32::from_be_bytes([value[0], value[1], value[2], value[3]]);
                req.meta.insert(name_str, v.to_string());
            }
            _ => {}
        }
    }

    req.document = raw[pos..].to_vec();
    Ok(req)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal synthetic Print-Job request.
    fn synthetic_print_job(document: &[u8]) -> Vec<u8> {
        let mut raw = vec![
            0x01, 0x01, // IPP 1.1
            0x00, 0x02, // Print-Job
            0x00, 0x00, 0x00, 0x2A, // request-id 42
        ];
        raw.push(TAG_OPERATION_ATTRIBUTES);
        raw.extend_from_slice(&attr_str(VT_CHARSET, "attributes-charset", "utf-8"));
        raw.extend_from_slice(&attr_str(VT_NAME_WITHOUT_LANGUAGE, "job-name", "テスト.pdf"));
        raw.extend_from_slice(&attr_str(VT_MIME_MEDIA_TYPE, "document-format", "application/pdf"));
        raw.push(TAG_END_OF_ATTRIBUTES);
        raw.extend_from_slice(document);
        raw
    }

    #[test]
    fn parse_synthetic_print_job() {
        let doc = b"%PDF-1.4 fake body";
        let req = parse_request(&synthetic_print_job(doc)).unwrap();
        assert_eq!(req.version_major, 1);
        assert_eq!(req.version_minor, 1);
        assert_eq!(req.operation_id, OP_PRINT_JOB);
        assert_eq!(req.request_id, 42);
        assert_eq!(req.meta_str("job-name"), "テスト.pdf");
        assert_eq!(req.meta_str("document-format"), "application/pdf");
        assert_eq!(req.document, doc);
    }

    #[test]
    fn parse_additional_values_and_job_id() {
        let mut raw = vec![0x02, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x07];
        raw.push(TAG_OPERATION_ATTRIBUTES);
        // 1setOf keyword with an additional value (zero-length name).
        raw.extend_from_slice(&attr_str_set(VT_KEYWORD, "some-set", &["a", "b"]));
        raw.push(TAG_JOB_ATTRIBUTES);
        raw.extend_from_slice(&attr_i32(VT_INTEGER, "job-id", 12345));
        raw.push(TAG_END_OF_ATTRIBUTES);
        let req = parse_request(&raw).unwrap();
        assert_eq!(req.operation_id, OP_SEND_DOCUMENT);
        assert_eq!(req.job_id(), 12345);
        assert!(req.document.is_empty());
    }

    #[test]
    fn parse_rejects_short_and_truncated() {
        assert!(parse_request(&[1, 1, 0]).is_err());
        let mut raw = vec![0x01, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01];
        raw.push(TAG_OPERATION_ATTRIBUTES);
        raw.push(VT_KEYWORD);
        raw.extend_from_slice(&10u16.to_be_bytes()); // name length beyond end
        assert!(parse_request(&raw).is_err());
    }

    #[test]
    fn additional_value_without_name_is_error() {
        let mut raw = vec![0x01, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01];
        raw.push(TAG_OPERATION_ATTRIBUTES);
        raw.push(VT_KEYWORD);
        raw.extend_from_slice(&0u16.to_be_bytes()); // zero-length name, no prior attr
        raw.extend_from_slice(&1u16.to_be_bytes());
        raw.push(b'x');
        assert!(parse_request(&raw).is_err());
    }

    #[test]
    fn response_roundtrip_layout() {
        let attrs = operation_attributes();
        let resp = build_response(2, 0, STATUS_OK, 7, &attrs);
        assert_eq!(&resp[..2], &[2, 0]); // version echo
        assert_eq!(u16::from_be_bytes([resp[2], resp[3]]), STATUS_OK);
        assert_eq!(u32::from_be_bytes([resp[4], resp[5], resp[6], resp[7]]), 7);
        assert_eq!(*resp.last().unwrap(), TAG_END_OF_ATTRIBUTES);
    }

    /// Byte-level comparison against the Python implementation's
    /// Get-Printer-Attributes response. Runs only when PY_GET_ATTRS_HEX
    /// points at a hex dump produced by the Python server code.
    #[test]
    fn printer_attributes_matches_python_golden_when_available() {
        let Ok(path) = std::env::var("PY_GET_ATTRS_HEX") else {
            eprintln!("PY_GET_ATTRS_HEX unset; skipping");
            return;
        };
        let expected = std::fs::read_to_string(path).unwrap();
        let attrs =
            printer_attributes("192.168.30.11:8631", "/ipp/print", false, &["application/pdf"]);
        let resp = build_response(2, 0, STATUS_OK, 1, &attrs);
        let hex: String = resp.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, expected.trim(), "byte-level mismatch vs Python response");
    }

    /// Golden test against real captured jobs (privacy: never committed).
    /// Points at the live deployment spool; skips silently when absent.
    #[test]
    fn parse_real_captures_when_available() {
        let root = std::env::var("FAKE_PRINTER_FIXTURES")
            .unwrap_or_else(|_| r"C:\Users\no5\OneDrive\fake-printer\spool".to_string());
        let root = std::path::Path::new(&root);
        if !root.is_dir() {
            eprintln!("fixture dir missing; skipping");
            return;
        }
        let mut checked = 0;
        for entry in std::fs::read_dir(root).unwrap().flatten() {
            let dir = entry.path();
            let request = dir.join("request.ipp");
            let meta_path = dir.join("meta.json");
            if !request.is_file() || !meta_path.is_file() {
                continue;
            }
            let raw = std::fs::read(&request).unwrap();
            let req = parse_request(&raw)
                .unwrap_or_else(|e| panic!("parse failed for {}: {e}", request.display()));
            let expected: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
            let exp = |key: &str| expected.get(key).and_then(|v| v.as_str()).unwrap_or("");
            assert_eq!(req.operation_id.to_string(), exp("operation_id"), "{}", dir.display());
            assert_eq!(req.request_id.to_string(), exp("request_id"), "{}", dir.display());
            for key in ["job-name", "document-format", "printer-uri", "requesting-user-name"] {
                let expected_value = exp(key);
                if !expected_value.is_empty() {
                    assert_eq!(req.meta_str(key), expected_value, "{} {}", dir.display(), key);
                }
            }
            // Captured document.bin must equal the parsed document payload.
            let doc_bin = dir.join("document.bin");
            if doc_bin.is_file() {
                let expected_doc = std::fs::read(&doc_bin).unwrap();
                assert_eq!(req.document, expected_doc, "document mismatch in {}", dir.display());
            }
            checked += 1;
        }
        eprintln!("verified {checked} captured requests");
        assert!(checked > 0, "fixture dir exists but no captures parsed");
    }
}
