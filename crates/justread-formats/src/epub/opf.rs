// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! `META-INF/container.xml` and the OPF package document.
//!
//! Spec: EPUB 3.3 §4 (OCF) and §5 (package document),
//! <https://www.w3.org/TR/epub-33/>; EPUB 2 OPF 2.0.1 for legacy metadata.

use std::collections::HashMap;

use justbook::paths::resolve_reference;
use justbook::Metadata;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::entities;
use crate::html::collapse_ws;
use crate::{FormatError, Result};

/// One `<item>` of the OPF manifest.
#[derive(Debug, Clone)]
pub struct Item {
    pub id: String,
    /// Path inside the ZIP (already resolved against the OPF location).
    pub path: String,
    pub media_type: String,
    pub properties: Vec<String>,
}

/// One `<itemref>` of the spine.
#[derive(Debug, Clone)]
pub struct ItemRef {
    pub idref: String,
    pub linear: bool,
}

/// Everything we use from the package document.
#[derive(Debug, Clone, Default)]
pub struct Package {
    pub opf_path: String,
    pub metadata: Metadata,
    /// Value of the identifier named by `package@unique-identifier`.
    pub unique_identifier: Option<String>,
    pub items: Vec<Item>,
    pub spine: Vec<ItemRef>,
    /// `spine@toc`: id of the EPUB 2 NCX.
    pub ncx_id: Option<String>,
    /// EPUB 2 `<meta name="cover" content="...">`.
    pub cover_meta: Option<String>,
}

impl Package {
    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    /// EPUB 3 navigation document (`properties="nav"`).
    pub fn nav_item(&self) -> Option<&Item> {
        self.items
            .iter()
            .find(|i| i.properties.iter().any(|p| p == "nav"))
    }

    /// EPUB 2 NCX: spine@toc, falling back to the NCX media type.
    pub fn ncx_item(&self) -> Option<&Item> {
        self.ncx_id
            .as_deref()
            .and_then(|id| self.item(id))
            .or_else(|| {
                self.items
                    .iter()
                    .find(|i| i.media_type == "application/x-dtbncx+xml")
            })
    }

    /// Cover image, using the EPUB 3 property, the EPUB 2 meta, then a name heuristic.
    pub fn cover_item(&self) -> Option<&Item> {
        let is_image = |i: &&Item| i.media_type.starts_with("image/");
        self.items
            .iter()
            .filter(is_image)
            .find(|i| i.properties.iter().any(|p| p == "cover-image"))
            .or_else(|| {
                let meta = self.cover_meta.as_deref()?;
                // Some books put an href instead of an id in the meta.
                self.item(meta)
                    .or_else(|| self.items.iter().find(|i| i.path.ends_with(meta)))
            })
            .filter(|i| i.media_type.starts_with("image/"))
            .or_else(|| {
                self.items.iter().filter(is_image).find(|i| {
                    i.id.to_lowercase().contains("cover") || i.path.to_lowercase().contains("cover")
                })
            })
    }
}

/// Find the OPF path from `META-INF/container.xml`.
pub fn parse_container(xml: &str) -> Result<String> {
    let mut r = Reader::from_str(xml);
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if local(&e) == "rootfile" => {
                // The first OPF rootfile is the default rendition (EPUB 3.3 §4.2.6.3.1).
                let mt = attr(&e, "media-type").unwrap_or_default();
                if mt.is_empty() || mt == "application/oebps-package+xml" {
                    if let Some(p) = attr(&e, "full-path") {
                        return Ok(p);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(invalid(format!("container.xml: {e}"))),
            _ => {}
        }
    }
    Err(invalid("container.xml has no rootfile".into()))
}

/// Which metadata field text is currently being read into.
#[derive(Clone, PartialEq)]
enum Field {
    Title,
    Creator(Option<String>, Option<String>), // (id, opf:role)
    Language,
    Publisher,
    Description,
    Date,
    Identifier(Option<String>),
    Meta3 {
        property: String,
        refines: Option<String>,
        id: Option<String>,
    },
}

/// Parse the OPF located at `opf_path`.
pub fn parse_package(xml: &str, opf_path: &str) -> Result<Package> {
    // STEP 1: state for the event walk.
    let mut r = Reader::from_str(xml);
    r.config_mut().check_end_names = false;
    let mut pkg = Package {
        opf_path: opf_path.to_string(),
        ..Default::default()
    };
    let mut unique_id_ref: Option<String> = None;
    let mut field: Option<Field> = None;
    let mut text = String::new();
    let mut creators: Vec<(Option<String>, Option<String>, String)> = Vec::new();
    let mut identifiers: Vec<(Option<String>, String)> = Vec::new();
    let mut metas3: Vec<(String, Option<String>, Option<String>, String)> = Vec::new();
    let mut metas2: HashMap<String, String> = HashMap::new();

    // STEP 2: walk the document.
    loop {
        let ev = r.read_event().map_err(|e| invalid(format!("OPF: {e}")))?;
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                let name = local(e);
                let qname = qualified(e);
                match name.as_str() {
                    "package" => unique_id_ref = attr(e, "unique-identifier"),
                    "item" => {
                        let (Some(id), Some(href)) = (attr(e, "id"), attr(e, "href")) else {
                            continue;
                        };
                        // Items with unsafe paths are ignored rather than failing the book.
                        if let Ok(path) = resolve_reference(opf_path, &href) {
                            pkg.items.push(Item {
                                id,
                                path,
                                media_type: attr(e, "media-type")
                                    .unwrap_or_default()
                                    .to_ascii_lowercase(),
                                properties: attr(e, "properties")
                                    .unwrap_or_default()
                                    .split_whitespace()
                                    .map(str::to_string)
                                    .collect(),
                            });
                        }
                    }
                    "spine" => pkg.ncx_id = attr(e, "toc"),
                    "itemref" => {
                        if let Some(idref) = attr(e, "idref") {
                            let linear = attr(e, "linear").map(|l| l != "no").unwrap_or(true);
                            pkg.spine.push(ItemRef { idref, linear });
                        }
                    }
                    "meta" => {
                        // EPUB 2 style: <meta name="..." content="..."/>
                        if let (Some(n), Some(c)) = (attr(e, "name"), attr(e, "content")) {
                            metas2.insert(n, c);
                        } else if let Some(property) = attr(e, "property").filter(|_| !empty) {
                            field = Some(Field::Meta3 {
                                property,
                                refines: attr(e, "refines"),
                                id: attr(e, "id"),
                            });
                            text.clear();
                        }
                    }
                    _ if !empty && (qname.starts_with("dc:") || is_dc(e)) => {
                        field = match name.as_str() {
                            "title" => Some(Field::Title),
                            "creator" => Some(Field::Creator(
                                attr(e, "id"),
                                attr(e, "opf:role").or_else(|| attr(e, "role")),
                            )),
                            "language" => Some(Field::Language),
                            "publisher" => Some(Field::Publisher),
                            "description" => Some(Field::Description),
                            "date" => Some(Field::Date),
                            "identifier" => Some(Field::Identifier(attr(e, "id"))),
                            _ => None,
                        };
                        text.clear();
                    }
                    _ => {}
                }
            }
            Event::Text(ref t) if field.is_some() => text.push_str(&t.xml10_content()),
            Event::CData(ref t) if field.is_some() => {
                text.push_str(&String::from_utf8_lossy(t.as_ref().as_bytes()))
            }
            Event::GeneralRef(ref g) if field.is_some() => {
                let n: &str = g;
                text.push_str(&entities::resolve(n).unwrap_or_else(|| format!("&{n};")));
            }
            Event::End(_) => {
                // STEP 3: a field ended — store its collected text.
                if let Some(f) = field.take() {
                    let value = collapse_ws(&text);
                    let m = &mut pkg.metadata;
                    match f {
                        Field::Title if m.title.is_empty() => m.title = value,
                        Field::Creator(id, role) if !value.is_empty() => {
                            creators.push((id, role, value))
                        }
                        Field::Language if m.language.is_none() => m.language = Some(value),
                        Field::Publisher if m.publisher.is_none() => m.publisher = Some(value),
                        Field::Description if m.description.is_none() => {
                            m.description = Some(strip_tags(&value))
                        }
                        Field::Date if m.published.is_none() => m.published = Some(value),
                        Field::Identifier(id) => identifiers.push((id, value)),
                        Field::Meta3 {
                            property,
                            refines,
                            id,
                        } => metas3.push((property, refines, id, value)),
                        _ => {}
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    // STEP 4: authors — prefer creators explicitly marked as authors ("aut").
    let role_of = |id: &Option<String>, role: &Option<String>| -> Option<String> {
        role.clone().or_else(|| {
            let id = id.as_ref()?;
            metas3
                .iter()
                .find(|(p, rf, _, _)| p == "role" && rf.as_deref() == Some(&format!("#{id}")))
                .map(|(_, _, _, v)| v.clone())
        })
    };
    let any_aut = creators
        .iter()
        .any(|(id, role, _)| role_of(id, role).as_deref() == Some("aut"));
    pkg.metadata.authors = creators
        .iter()
        .filter(|(id, role, _)| !any_aut || role_of(id, role).as_deref() == Some("aut"))
        .map(|(_, _, n)| n.clone())
        .collect();

    // STEP 5: identifiers and the unique identifier (needed for font de-obfuscation).
    pkg.unique_identifier = unique_id_ref
        .as_ref()
        .and_then(|uid| {
            identifiers
                .iter()
                .find(|(id, _)| id.as_deref() == Some(uid))
        })
        .or(identifiers.first())
        .map(|(_, v)| v.clone());
    pkg.metadata.identifiers = identifiers.into_iter().map(|(_, v)| v).collect();

    // STEP 6: series — EPUB 3 collections, then the common calibre EPUB 2 metas.
    if let Some((_, _, id, name)) = metas3
        .iter()
        .find(|(p, rf, _, _)| p == "belongs-to-collection" && rf.is_none())
    {
        pkg.metadata.series = Some(name.clone());
        if let Some(id) = id {
            pkg.metadata.series_index = metas3
                .iter()
                .find(|(p, rf, _, _)| {
                    p == "group-position" && rf.as_deref() == Some(&format!("#{id}"))
                })
                .and_then(|(_, _, _, v)| v.parse().ok());
        }
    } else if let Some(s) = metas2.get("calibre:series") {
        pkg.metadata.series = Some(s.clone());
        pkg.metadata.series_index = metas2
            .get("calibre:series_index")
            .and_then(|v| v.parse().ok());
    }
    pkg.cover_meta = metas2.get("cover").cloned();

    // STEP 7: sanity.
    if pkg.spine.is_empty() {
        return Err(invalid("the spine is empty".into()));
    }
    Ok(pkg)
}

/// Remove HTML tags from descriptions that embed markup.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    collapse_ws(&out)
}

fn local(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.local_name().as_ref().as_bytes()).into_owned()
}

fn qualified(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref().as_bytes()).into_owned()
}

/// Dublin Core element written with a default namespace instead of a prefix.
fn is_dc(e: &BytesStart<'_>) -> bool {
    attr(e, "xmlns")
        .map(|ns| ns.contains("purl.org/dc/elements"))
        .unwrap_or(false)
}

pub(crate) fn attr(e: &BytesStart<'_>, key: &str) -> Option<String> {
    e.html_attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .map(|a| entities::unescape(&a.value))
}

fn invalid(reason: String) -> FormatError {
    FormatError::Invalid {
        format: "EPUB",
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_rootfile() {
        let xml = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        assert_eq!(parse_container(xml).unwrap(), "OEBPS/content.opf");
        assert!(parse_container("<container/>").is_err());
    }

    #[test]
    fn epub3_package() {
        let xml = r##"<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
 <dc:identifier id="uid">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
 <dc:title>The  Title &amp; More</dc:title>
 <dc:creator id="c1">Ada Author</dc:creator><meta refines="#c1" property="role">aut</meta>
 <dc:creator id="c2">Ian Illustrator</dc:creator><meta refines="#c2" property="role">ill</meta>
 <dc:language>en</dc:language>
 <meta property="belongs-to-collection" id="col">Saga</meta>
 <meta refines="#col" property="group-position">2</meta>
</metadata>
<manifest>
 <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
 <item id="c" href="text/ch%201.xhtml" media-type="application/xhtml+xml"/>
 <item id="img" href="img/cover.jpg" media-type="image/jpeg" properties="cover-image"/>
 <item id="bad" href="../../escape.xhtml" media-type="application/xhtml+xml"/>
</manifest>
<spine><itemref idref="c"/><itemref idref="nav" linear="no"/></spine></package>"##;
        let p = parse_package(xml, "OEBPS/content.opf").unwrap();
        assert_eq!(p.metadata.title, "The Title & More");
        assert_eq!(p.metadata.authors, vec!["Ada Author"]);
        assert_eq!(p.metadata.language.as_deref(), Some("en"));
        assert_eq!(p.metadata.series.as_deref(), Some("Saga"));
        assert_eq!(p.metadata.series_index, Some(2.0));
        assert_eq!(
            p.unique_identifier.as_deref(),
            Some("urn:uuid:12345678-1234-1234-1234-123456789abc")
        );
        assert_eq!(p.item("c").unwrap().path, "OEBPS/text/ch 1.xhtml");
        assert!(p.item("bad").is_none());
        assert_eq!(p.nav_item().unwrap().id, "nav");
        assert_eq!(p.cover_item().unwrap().id, "img");
        assert!(!p.spine[1].linear);
    }

    #[test]
    fn epub2_package() {
        let xml = r#"<package version="2.0" unique-identifier="id"><metadata xmlns:opf="http://www.idpf.org/2007/opf">
<dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">Old</dc:title>
<dc:creator xmlns:dc="http://purl.org/dc/elements/1.1/" opf:role="aut">A</dc:creator>
<dc:creator xmlns:dc="http://purl.org/dc/elements/1.1/" opf:role="edt">E</dc:creator>
<dc:description xmlns:dc="http://purl.org/dc/elements/1.1/">&lt;p&gt;Nice&lt;/p&gt;</dc:description>
<meta name="cover" content="cov"/><meta name="calibre:series" content="S"/><meta name="calibre:series_index" content="3"/>
</metadata><manifest><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
<item id="cov" href="c.png" media-type="image/png"/><item id="t" href="t.html" media-type="application/xhtml+xml"/></manifest>
<spine toc="ncx"><itemref idref="t"/></spine></package>"#;
        let p = parse_package(xml, "content.opf").unwrap();
        assert_eq!(p.metadata.authors, vec!["A"]);
        assert_eq!(p.metadata.description.as_deref(), Some("Nice"));
        assert_eq!(p.metadata.series.as_deref(), Some("S"));
        assert_eq!(p.metadata.series_index, Some(3.0));
        assert_eq!(p.ncx_item().unwrap().path, "toc.ncx");
        assert_eq!(p.cover_item().unwrap().path, "c.png");
    }
}
