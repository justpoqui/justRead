// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Character references.
//!
//! XHTML in the wild uses HTML named entities (`&nbsp;`, `&mdash;`) that a
//! strict XML parser rejects. We resolve numeric references and a table of
//! the most common named ones; unknown names are kept literally so no text
//! is lost.

/// Common HTML named entities (name → character).
const NAMED: &[(&str, &str)] = &[
    ("amp", "&"),
    ("lt", "<"),
    ("gt", ">"),
    ("quot", "\""),
    ("apos", "'"),
    ("nbsp", "\u{a0}"),
    ("shy", "\u{ad}"),
    ("ensp", "\u{2002}"),
    ("emsp", "\u{2003}"),
    ("thinsp", "\u{2009}"),
    ("zwnj", "\u{200c}"),
    ("zwj", "\u{200d}"),
    ("lrm", "\u{200e}"),
    ("rlm", "\u{200f}"),
    ("ndash", "–"),
    ("mdash", "—"),
    ("lsquo", "‘"),
    ("rsquo", "’"),
    ("sbquo", "‚"),
    ("ldquo", "“"),
    ("rdquo", "”"),
    ("bdquo", "„"),
    ("laquo", "«"),
    ("raquo", "»"),
    ("lsaquo", "‹"),
    ("rsaquo", "›"),
    ("hellip", "…"),
    ("bull", "•"),
    ("middot", "·"),
    ("prime", "′"),
    ("Prime", "″"),
    ("dagger", "†"),
    ("Dagger", "‡"),
    ("sect", "§"),
    ("para", "¶"),
    ("copy", "©"),
    ("reg", "®"),
    ("trade", "™"),
    ("deg", "°"),
    ("plusmn", "±"),
    ("times", "×"),
    ("divide", "÷"),
    ("frac12", "½"),
    ("frac14", "¼"),
    ("frac34", "¾"),
    ("sup1", "¹"),
    ("sup2", "²"),
    ("sup3", "³"),
    ("micro", "µ"),
    ("cent", "¢"),
    ("pound", "£"),
    ("euro", "€"),
    ("yen", "¥"),
    ("iexcl", "¡"),
    ("iquest", "¿"),
    ("szlig", "ß"),
    ("aelig", "æ"),
    ("AElig", "Æ"),
    ("oelig", "œ"),
    ("OElig", "Œ"),
    ("oslash", "ø"),
    ("Oslash", "Ø"),
    ("aring", "å"),
    ("Aring", "Å"),
    ("ccedil", "ç"),
    ("Ccedil", "Ç"),
    ("ntilde", "ñ"),
    ("Ntilde", "Ñ"),
    ("agrave", "à"),
    ("aacute", "á"),
    ("acirc", "â"),
    ("atilde", "ã"),
    ("auml", "ä"),
    ("egrave", "è"),
    ("eacute", "é"),
    ("ecirc", "ê"),
    ("euml", "ë"),
    ("igrave", "ì"),
    ("iacute", "í"),
    ("icirc", "î"),
    ("iuml", "ï"),
    ("ograve", "ò"),
    ("oacute", "ó"),
    ("ocirc", "ô"),
    ("otilde", "õ"),
    ("ouml", "ö"),
    ("ugrave", "ù"),
    ("uacute", "ú"),
    ("ucirc", "û"),
    ("uuml", "ü"),
    ("yacute", "ý"),
    ("yuml", "ÿ"),
    ("Agrave", "À"),
    ("Aacute", "Á"),
    ("Acirc", "Â"),
    ("Auml", "Ä"),
    ("Egrave", "È"),
    ("Eacute", "É"),
    ("Ecirc", "Ê"),
    ("Iacute", "Í"),
    ("Oacute", "Ó"),
    ("Ouml", "Ö"),
    ("Uacute", "Ú"),
    ("Uuml", "Ü"),
];

/// Resolve the *name* of a reference (text between `&` and `;`).
/// Returns `None` for unknown or invalid references.
pub fn resolve(name: &str) -> Option<String> {
    // STEP 1: numeric references, decimal (`#160`) or hex (`#xA0`).
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse::<u32>().ok()?,
        };
        // NUL and surrogates are not valid characters.
        return char::from_u32(code)
            .filter(|c| *c != '\0')
            .map(|c| c.to_string());
    }
    // STEP 2: named references from the table.
    NAMED
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v.to_string())
}

/// Unescape a raw attribute value or text run that may contain references.
pub fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        // STEP 1: copy text up to the '&'.
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        // STEP 2: a reference ends at ';' within a sane distance.
        match after.find(';').filter(|&i| i <= 32) {
            Some(semi) => match resolve(&after[..semi]) {
                Some(v) => {
                    out.push_str(&v);
                    rest = &after[semi + 1..];
                }
                None => {
                    // Unknown: keep the literal text.
                    out.push('&');
                    rest = after;
                }
            },
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_numeric_and_named() {
        assert_eq!(resolve("#160").as_deref(), Some("\u{a0}"));
        assert_eq!(resolve("#x2014").as_deref(), Some("—"));
        assert_eq!(resolve("mdash").as_deref(), Some("—"));
        assert_eq!(resolve("#0"), None);
        assert_eq!(resolve("#xD800"), None);
        assert_eq!(resolve("bogus"), None);
    }

    #[test]
    fn unescapes_mixed_text() {
        assert_eq!(
            unescape("a &amp; b&nbsp;c &unknown; d & e"),
            "a & b\u{a0}c &unknown; d & e"
        );
    }
}
