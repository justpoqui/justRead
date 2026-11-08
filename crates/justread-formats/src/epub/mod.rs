// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! EPUB 2 / EPUB 3 adapter.
//!
//! Conversion steps (each is a function below):
//! 1. open the ZIP with safety limits;
//! 2. refuse DRM-protected books;
//! 3. read `container.xml` → OPF → metadata, manifest, spine;
//! 4. plan output paths (chapters → `content/cNNNN.xhtml`, everything
//!    else → `resources/<original path>`);
//! 5. copy resources (scrubbing CSS, de-obfuscating fonts);
//! 6. read the TOC (nav, else NCX);
//! 7. normalize each spine document into a justBook chapter;
//! 8. copy the cover.

pub mod archive;
pub mod drm;
pub mod fonts;
pub mod opf;
pub mod toc;

use std::collections::HashMap;
use std::path::Path;

use justbook::{JustBookWriter, Resource, SpineItem, TocEntry};

use self::archive::SafeArchive;
use self::opf::Package;
use crate::html::{normalize, LinkContext};
use crate::{
    css, decode_text, BookContent, Confidence, DrmStatus, FormatAdapter, FormatError, FormatFamily,
    Inspection, Result,
};

/// The EPUB adapter (stateless).
pub struct EpubAdapter;

/// Media types of spine documents we can convert.
const CONTENT_TYPES: &[&str] = &["application/xhtml+xml", "text/html", "application/xml"];

impl FormatAdapter for EpubAdapter {
    fn id(&self) -> &'static str {
        "epub"
    }

    fn family(&self) -> FormatFamily {
        FormatFamily::Reflowable
    }

    fn sniff(&self, head: &[u8], ext: Option<&str>) -> Confidence {
        // OCF requires an uncompressed first entry "mimetype" containing
        // "application/epub+zip", which puts the text at byte 30.
        let is_zip = head.starts_with(b"PK\x03\x04");
        if is_zip && head.get(30..58) == Some(b"mimetypeapplication/epub+zip") {
            Confidence::High
        } else if is_zip && ext == Some("epub") {
            Confidence::Low
        } else {
            Confidence::No
        }
    }

    fn inspect(&self, src: &Path) -> Result<Inspection> {
        let mut ar = SafeArchive::open(src)?;
        let enc = drm::examine(&mut ar);
        let pkg = read_package(&mut ar)?;
        Ok(Inspection {
            metadata: pkg.metadata,
            drm: enc.status(),
        })
    }

    fn convert(&self, src: &Path, out: &mut JustBookWriter) -> Result<BookContent> {
        // STEP 1: open with limits.
        let mut ar = SafeArchive::open(src)?;
        let mut warnings = Vec::new();

        // STEP 2: DRM — stop immediately, never try to work around it.
        let enc = drm::examine(&mut ar);
        if let DrmStatus::Protected(scheme) = enc.status() {
            return Err(FormatError::DrmProtected(scheme));
        }

        // STEP 3: package document.
        let pkg = read_package(&mut ar)?;
        let mut metadata = pkg.metadata.clone();
        if metadata.title.trim().is_empty() {
            metadata.title = file_stem(src);
        }

        // STEP 4: decide where every file goes.
        let plan = plan_paths(&pkg, &ar);
        let map = |p: &str| plan.lookup(p);

        // STEP 5: resources.
        let resources = copy_resources(&mut ar, &pkg, &plan, &enc, out, &mut warnings)?;

        // STEP 6: table of contents (needed first: it supplies chapter titles).
        let raw_toc = read_toc(&mut ar, &pkg);
        let mut toc = toc::map_entries(&raw_toc, &map);
        let titles = first_titles(&toc);

        // STEP 7: chapters.
        let mut spine = Vec::new();
        for (source, target, linear) in &plan.chapters {
            let text = match ar.read(source) {
                Ok(bytes) => decode_text(&bytes),
                Err(e) => {
                    warnings.push(format!("skipped chapter {source}: {e}"));
                    continue;
                }
            };
            let ctx = LinkContext {
                source_path: source,
                out_path: target,
                map: &map,
                default_lang: metadata.language.as_deref(),
            };
            let toc_title = titles.get(target.as_str()).map(String::as_str);
            let n = normalize(&text, &ctx, toc_title.or(Some(&metadata.title)));
            warnings.extend(n.warnings);
            out.write_file(target, n.xhtml.as_bytes())?;
            let title = toc_title
                .map(str::to_string)
                .or(n.head_title.filter(|t| *t != metadata.title))
                .or(n.first_heading);
            spine.push(SpineItem {
                href: target.clone(),
                title,
                chars: n.stats.chars,
                blocks: n.stats.blocks,
                linear: *linear,
            });
        }
        if spine.is_empty() {
            return Err(FormatError::Invalid {
                format: "EPUB",
                reason: "no readable chapters".into(),
            });
        }

        // STEP 8: a TOC is always useful; synthesize one from chapter titles if missing.
        if toc.is_empty() {
            toc = spine
                .iter()
                .enumerate()
                .map(|(i, s)| TocEntry {
                    title: s
                        .title
                        .clone()
                        .unwrap_or_else(|| format!("Chapter {}", i + 1)),
                    href: Some(s.href.clone()),
                    children: vec![],
                })
                .collect();
        }

        // STEP 9: cover.
        let cover = copy_cover(&mut ar, &pkg, out);

        Ok(BookContent {
            metadata,
            spine,
            toc,
            resources,
            cover,
            warnings,
        })
    }
}

/// Read `container.xml` and the OPF it points to.
fn read_package(ar: &mut SafeArchive) -> Result<Package> {
    let container = ar.read_text("META-INF/container.xml")?;
    let opf_path = opf::parse_container(&container)?;
    let opf_path = ar.find(&opf_path).unwrap_or(opf_path);
    let opf_xml = ar.read_text(&opf_path)?;
    opf::parse_package(&opf_xml, &opf_path)
}

/// Output locations for every source file.
struct PathPlan {
    /// (source path, package path, linear) in reading order.
    chapters: Vec<(String, String, bool)>,
    /// source path → package path for chapters and resources.
    table: HashMap<String, String>,
    /// lower-case fallback table (references with the wrong case).
    table_lower: HashMap<String, String>,
}

impl PathPlan {
    fn lookup(&self, source: &str) -> Option<String> {
        self.table
            .get(source)
            .or_else(|| self.table_lower.get(&source.to_lowercase()))
            .cloned()
    }
}

/// Assign `content/cNNNN.xhtml` to spine documents and `resources/...` to the rest.
fn plan_paths(pkg: &Package, ar: &SafeArchive) -> PathPlan {
    let mut chapters = Vec::new();
    let mut table = HashMap::new();

    // STEP 1: spine documents in reading order (each source used once).
    for itemref in &pkg.spine {
        let Some(item) = pkg.item(&itemref.idref) else {
            continue;
        };
        if !CONTENT_TYPES.contains(&item.media_type.as_str()) || table.contains_key(&item.path) {
            continue;
        }
        let Some(real) = ar.find(&item.path) else {
            continue;
        };
        let target = format!("content/c{:04}.xhtml", chapters.len() + 1);
        table.insert(item.path.clone(), target.clone());
        chapters.push((real, target, itemref.linear));
    }

    // STEP 2: resources keep their relative layout under resources/, so
    // relative url()s inside CSS keep working without rewriting.
    for item in &pkg.items {
        if is_resource(&item.media_type) && !table.contains_key(&item.path) && ar.has(&item.path) {
            table.insert(item.path.clone(), format!("resources/{}", item.path));
        }
    }

    let table_lower = table
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.clone()))
        .collect();
    PathPlan {
        chapters,
        table,
        table_lower,
    }
}

/// Images, stylesheets and fonts are copied; audio/video/scripts are not.
fn is_resource(media_type: &str) -> bool {
    media_type.starts_with("image/")
        || media_type == "text/css"
        || media_type.starts_with("font/")
        || media_type.contains("font")
        || media_type == "application/vnd.ms-opentype"
}

/// Copy resources into the package, scrubbing CSS and de-obfuscating fonts.
fn copy_resources(
    ar: &mut SafeArchive,
    pkg: &Package,
    plan: &PathPlan,
    enc: &drm::EncryptionInfo,
    out: &mut JustBookWriter,
    warnings: &mut Vec<String>,
) -> Result<Vec<Resource>> {
    let mut resources = Vec::new();
    for item in &pkg.items {
        // STEP 1: only planned resources (chapters are written separately).
        let Some(target) = plan
            .table
            .get(&item.path)
            .filter(|t| t.starts_with("resources/"))
        else {
            continue;
        };
        let mut bytes = match ar.read(&item.path) {
            Ok(b) => b,
            Err(FormatError::TooLarge(m)) => return Err(FormatError::TooLarge(m)),
            Err(e) => {
                warnings.push(format!("skipped resource {}: {e}", item.path));
                continue;
            }
        };

        // STEP 2: per-type processing.
        if item.media_type == "text/css" {
            bytes = css::scrub_stylesheet(&decode_text(&bytes)).into_bytes();
        } else if let Some(alg) = enc.obfuscated.get(&item.path) {
            if !fonts::deobfuscate(
                &mut bytes,
                alg,
                &pkg.metadata.identifiers,
                pkg.unique_identifier.as_deref(),
            ) {
                warnings.push(format!("could not de-obfuscate font {}", item.path));
            }
        }

        // STEP 3: write and record.
        let href = out.write_file(target, &bytes)?;
        resources.push(Resource {
            href,
            media_type: item.media_type.clone(),
        });
    }
    Ok(resources)
}

/// Nav document first (EPUB 3), NCX second (EPUB 2).
fn read_toc(ar: &mut SafeArchive, pkg: &Package) -> Vec<toc::RawEntry> {
    if let Some(nav) = pkg.nav_item() {
        if let Ok(x) = ar.read_text(&nav.path) {
            let t = toc::parse_nav(&x, &nav.path);
            if !t.is_empty() {
                return t;
            }
        }
    }
    if let Some(ncx) = pkg.ncx_item() {
        if let Ok(x) = ar.read_text(&ncx.path) {
            return toc::parse_ncx(&x, &ncx.path);
        }
    }
    Vec::new()
}

/// For each chapter path, the first TOC title pointing into it.
fn first_titles(toc: &[TocEntry]) -> HashMap<String, String> {
    fn walk(entries: &[TocEntry], out: &mut HashMap<String, String>) {
        for e in entries {
            if let Some(h) = &e.href {
                let path = h.split('#').next().unwrap_or(h).to_string();
                out.entry(path).or_insert_with(|| e.title.clone());
            }
            walk(&e.children, out);
        }
    }
    let mut out = HashMap::new();
    walk(toc, &mut out);
    out
}

/// Copy the cover image to `cover.<ext>`.
fn copy_cover(ar: &mut SafeArchive, pkg: &Package, out: &mut JustBookWriter) -> Option<String> {
    let item = pkg.cover_item()?;
    let bytes = ar.read(&item.path).ok()?;
    let ext = match item.media_type.as_str() {
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => "jpg",
    };
    out.write_file(&format!("cover.{ext}"), &bytes).ok()
}

/// File name without extension (title fallback).
pub(crate) fn file_stem(p: &Path) -> String {
    p.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}
