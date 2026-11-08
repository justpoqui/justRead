// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Formats we recognize but cannot open yet. Recognizing them gives users a
//! specific explanation instead of "unsupported file".

use std::path::Path;

use justbook::{JustBookWriter, Metadata};

use crate::epub::file_stem;
use crate::{
    BookContent, Confidence, DrmStatus, FormatAdapter, FormatError, FormatFamily, Inspection,
    Result,
};

/// DjVu: needs a decoder (ZP/BZZ/JB2/IW44). The reference decoder,
/// DjVuLibre, is GPL; a decoder of our own is planned (see docs/adr/0007).
pub struct DjvuAdapter;

impl FormatAdapter for DjvuAdapter {
    fn id(&self) -> &'static str {
        "djvu"
    }
    fn family(&self) -> FormatFamily {
        FormatFamily::Fixed
    }
    fn sniff(&self, head: &[u8], ext: Option<&str>) -> Confidence {
        let magic =
            head.starts_with(b"AT&TFORM") && matches!(head.get(12..16), Some(b"DJVU" | b"DJVM"));
        if magic || matches!(ext, Some("djvu" | "djv")) {
            Confidence::High
        } else {
            Confidence::No
        }
    }
    fn inspect(&self, src: &Path) -> Result<Inspection> {
        Ok(Inspection {
            metadata: Metadata {
                title: file_stem(src),
                ..Default::default()
            },
            drm: DrmStatus::None,
        })
    }
    fn convert(&self, _src: &Path, _out: &mut JustBookWriter) -> Result<BookContent> {
        Err(FormatError::NotYetSupported(
            "DjVu books can't be opened yet. Converting them to PDF works today.".into(),
        ))
    }
}

/// KFX (Amazon's newest format): almost always DRM-protected and undocumented.
pub struct KfxAdapter;

impl FormatAdapter for KfxAdapter {
    fn id(&self) -> &'static str {
        "kfx"
    }
    fn family(&self) -> FormatFamily {
        FormatFamily::Reflowable
    }
    fn sniff(&self, head: &[u8], ext: Option<&str>) -> Confidence {
        if head.starts_with(b"\xeaDRMION\xee")
            || head.starts_with(b"CONT\x02\x00")
            || ext == Some("kfx")
        {
            Confidence::High
        } else {
            Confidence::No
        }
    }
    fn inspect(&self, src: &Path) -> Result<Inspection> {
        Ok(Inspection {
            metadata: Metadata {
                title: file_stem(src),
                ..Default::default()
            },
            drm: DrmStatus::None,
        })
    }
    fn convert(&self, src: &Path, _out: &mut JustBookWriter) -> Result<BookContent> {
        let head = std::fs::read(src)
            .map(|b| b.into_iter().take(8).collect::<Vec<_>>())
            .unwrap_or_default();
        if head.starts_with(b"\xeaDRMION\xee") {
            return Err(FormatError::DrmProtected("Kindle KFX".into()));
        }
        Err(FormatError::NotYetSupported(
            "Kindle KFX books are not supported. DRM-free AZW3 or EPUB versions work.".into(),
        ))
    }
}
