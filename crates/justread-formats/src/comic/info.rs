// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! `ComicInfo.xml`: the de-facto comic metadata file (the "ComicRack"
//! schema, <https://anansi-project.github.io/docs/comicinfo/intro>).

use justbook::Metadata;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::entities;
use crate::html::collapse_ws;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComicInfo {
    pub title: Option<String>,
    pub series: Option<String>,
    pub number: Option<String>,
    pub summary: Option<String>,
    pub writers: Vec<String>,
    pub artists: Vec<String>,
    pub publisher: Option<String>,
    pub year: Option<String>,
    pub language: Option<String>,
    /// `<Manga>YesAndRightToLeft</Manga>`.
    pub right_to_left: bool,
}

impl ComicInfo {
    /// Parse leniently; unknown or malformed fields are ignored.
    pub fn parse(xml: &str) -> Self {
        let mut info = ComicInfo::default();
        let mut r = Reader::from_str(xml);
        r.config_mut().check_end_names = false;
        let mut field: Option<String> = None;
        let mut text = String::new();
        loop {
            match r.read_event() {
                Ok(Event::Start(e)) => {
                    field = Some(
                        String::from_utf8_lossy(e.local_name().as_ref().as_bytes()).into_owned(),
                    );
                    text.clear();
                }
                Ok(Event::Text(t)) if field.is_some() => text.push_str(&t.xml10_content()),
                Ok(Event::GeneralRef(g)) if field.is_some() => {
                    let n: &str = &g;
                    text.push_str(&entities::resolve(n).unwrap_or_default());
                }
                Ok(Event::End(_)) => {
                    // STEP 1: store the finished field.
                    if let Some(f) = field.take() {
                        let v = collapse_ws(&text);
                        if v.is_empty() {
                            continue;
                        }
                        let list = |s: &str| {
                            s.split(',')
                                .map(|x| x.trim().to_string())
                                .filter(|x| !x.is_empty())
                                .collect::<Vec<_>>()
                        };
                        match f.as_str() {
                            "Title" => info.title = Some(v),
                            "Series" => info.series = Some(v),
                            "Number" => info.number = Some(v),
                            "Summary" => info.summary = Some(v),
                            "Writer" => info.writers = list(&v),
                            "Penciller" | "Artist" => info.artists.extend(list(&v)),
                            "Publisher" => info.publisher = Some(v),
                            "Year" => info.year = Some(v),
                            "LanguageISO" => info.language = Some(v),
                            "Manga" => {
                                info.right_to_left = v.eq_ignore_ascii_case("YesAndRightToLeft")
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
        info
    }

    /// Book metadata; `fallback` (the file name) is used when there is no title.
    pub fn to_metadata(&self, fallback: &str) -> Metadata {
        // STEP 1: "Title", else "Series #Number", else the file name.
        let title = self.title.clone().or_else(|| {
            self.series.as_ref().map(|s| match &self.number {
                Some(n) => format!("{s} #{n}"),
                None => s.clone(),
            })
        });
        // STEP 2: writers first, then artists, without duplicates.
        let mut authors = self.writers.clone();
        for a in &self.artists {
            if !authors.contains(a) {
                authors.push(a.clone());
            }
        }
        Metadata {
            title: title.unwrap_or_else(|| fallback.to_string()),
            authors,
            language: self.language.clone(),
            publisher: self.publisher.clone(),
            description: self.summary.clone(),
            published: self.year.clone(),
            series: self.series.clone(),
            series_index: self.number.as_deref().and_then(|n| n.parse().ok()),
            identifiers: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comicinfo() {
        let x = r#"<?xml version="1.0"?><ComicInfo><Series>Night &amp; Day</Series><Number>3</Number>
<Writer>Ann A, Bob B</Writer><Penciller>Bob B, Cy C</Penciller><Manga>YesAndRightToLeft</Manga>
<LanguageISO>ja</LanguageISO></ComicInfo>"#;
        let i = ComicInfo::parse(x);
        assert!(i.right_to_left);
        let m = i.to_metadata("file");
        assert_eq!(m.title, "Night & Day #3");
        assert_eq!(m.authors, vec!["Ann A", "Bob B", "Cy C"]);
        assert_eq!(m.series_index, Some(3.0));
        assert_eq!(m.language.as_deref(), Some("ja"));
        assert_eq!(ComicInfo::parse("garbage<").to_metadata("f").title, "f");
    }
}
