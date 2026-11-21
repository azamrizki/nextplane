//! A small XLSX writer for exports: one or more sheets of text and number cells, written as a
//! ZIP of SpreadsheetML parts (ECMA-376). Strings are stored inline, so no shared-string table
//! is needed; the first row of each sheet is bold and frozen.

use std::io::Write;

use flate2::Compression;
use flate2::write::DeflateEncoder;

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Text(String),
    Number(f64),
    Empty,
}

impl From<&str> for Cell {
    fn from(s: &str) -> Self {
        Cell::Text(s.to_string())
    }
}
impl From<String> for Cell {
    fn from(s: String) -> Self {
        Cell::Text(s)
    }
}
impl<T: Into<Cell>> From<Option<T>> for Cell {
    fn from(v: Option<T>) -> Self {
        v.map_or(Cell::Empty, Into::into)
    }
}
impl From<f64> for Cell {
    fn from(n: f64) -> Self {
        Cell::Number(n)
    }
}
impl From<i64> for Cell {
    fn from(n: i64) -> Self {
        Cell::Number(n as f64)
    }
}

pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<Cell>>,
}

/// Excel's rules for sheet names: at most 31 characters, none of `[]:*?/\`, not empty, unique
/// (case-insensitively) within the workbook.
pub fn sheet_names(wanted: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in wanted {
        let clean: String = w
            .chars()
            .map(|c| if "[]:*?/\\".contains(c) || c.is_control() { ' ' } else { c })
            .collect();
        let clean = clean.trim().trim_matches('\'').trim().to_string();
        let base = if clean.is_empty() { "Sheet".to_string() } else { clean };
        let mut name: String = base.chars().take(31).collect();
        let mut n = 2;
        while out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
            let suffix = format!(" ({n})");
            name = base.chars().take(31 - suffix.chars().count()).collect::<String>() + &suffix;
            n += 1;
        }
        out.push(name);
    }
    out
}

/// The XLSX file for these sheets. Sheet names are taken as given; pass them through
/// [`sheet_names`] first.
pub fn workbook(sheets: &[Sheet]) -> Vec<u8> {
    let mut zip = Zip::default();
    let n = sheets.len().max(1);
    let mut types = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#,
    );
    for i in 1..=n {
        types.push_str(&format!(r#"<Override PartName="/xl/worksheets/sheet{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#));
    }
    types.push_str("</Types>");
    zip.add("[Content_Types].xml", types.as_bytes());
    zip.add(
        "_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
    );
    let mut book = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>"#,
    );
    let mut rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    );
    let empty = [Sheet {
        name: "Sheet".into(),
        rows: vec![],
    }];
    let sheets = if sheets.is_empty() { &empty[..] } else { sheets };
    for (i, s) in sheets.iter().enumerate() {
        let id = i + 1;
        book.push_str(&format!(
            r#"<sheet name="{}" sheetId="{id}" r:id="rId{id}"/>"#,
            esc(&s.name)
        ));
        rels.push_str(&format!(r#"<Relationship Id="rId{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{id}.xml"/>"#));
        zip.add(&format!("xl/worksheets/sheet{id}.xml"), sheet_xml(s).as_bytes());
    }
    book.push_str("</sheets></workbook>");
    rels.push_str(&format!(r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#, sheets.len() + 1));
    zip.add("xl/workbook.xml", book.as_bytes());
    zip.add("xl/_rels/workbook.xml.rels", rels.as_bytes());
    zip.add(
        "xl/styles.xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="3"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0" applyAlignment="1"><alignment wrapText="1" vertical="top"/></xf></cellXfs></styleSheet>"#,
    );
    zip.finish()
}

fn sheet_xml(s: &Sheet) -> String {
    let mut x = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    );
    if !s.rows.is_empty() {
        x.push_str(r#"<sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>"#);
    }
    x.push_str("<sheetData>");
    for (r, row) in s.rows.iter().enumerate() {
        let rn = r + 1;
        x.push_str(&format!(r#"<row r="{rn}">"#));
        for (c, cell) in row.iter().enumerate() {
            let at = format!("{}{rn}", col_name(c));
            let style = if r == 0 { r#" s="1""# } else { "" };
            match cell {
                Cell::Empty => {}
                Cell::Number(n) if n.is_finite() => {
                    x.push_str(&format!(r#"<c r="{at}"{style}><v>{n}</v></c>"#));
                }
                Cell::Number(_) => {}
                Cell::Text(t) => {
                    // Excel refuses cells over 32,767 characters.
                    let t: String = t.chars().take(32_767).collect();
                    let style = if r > 0 && t.contains('\n') { r#" s="2""# } else { style };
                    x.push_str(&format!(
                        r#"<c r="{at}" t="inlineStr"{style}><is><t xml:space="preserve">{}</t></is></c>"#,
                        esc(&t)
                    ));
                }
            }
        }
        x.push_str("</row>");
    }
    x.push_str("</sheetData></worksheet>");
    x
}

/// `A`, `B`, … `Z`, `AA`, … for a zero-based column.
fn col_name(mut c: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (c % 26) as u8);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// XML text: escaped, with characters XML 1.0 does not allow dropped.
fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\t' | '\n' | '\r' => o.push(ch),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => o.push(c),
        }
    }
    o
}

/// Just enough ZIP (PKWARE APPNOTE 6.3) for a workbook: deflated entries, no ZIP64.
#[derive(Default)]
struct Zip {
    out: Vec<u8>,
    central: Vec<u8>,
    count: u16,
}

impl Zip {
    fn add(&mut self, name: &str, data: &[u8]) {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let crc = crc.sum();
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        let _ = enc.write_all(data);
        let packed = enc.finish().unwrap_or_default();
        let offset = self.out.len() as u32;
        // 1980-01-01 00:00, the earliest DOS time.
        let (time, date) = (0u16, 0x21u16);
        let n = name.as_bytes();
        let mut h = Vec::with_capacity(30 + n.len());
        h.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        h.extend_from_slice(&20u16.to_le_bytes()); // version needed
        h.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
        h.extend_from_slice(&8u16.to_le_bytes()); // deflate
        h.extend_from_slice(&time.to_le_bytes());
        h.extend_from_slice(&date.to_le_bytes());
        h.extend_from_slice(&crc.to_le_bytes());
        h.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        h.extend_from_slice(&(data.len() as u32).to_le_bytes());
        h.extend_from_slice(&(n.len() as u16).to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes());
        h.extend_from_slice(n);
        self.out.extend_from_slice(&h);
        self.out.extend_from_slice(&packed);

        let c = &mut self.central;
        c.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        c.extend_from_slice(&20u16.to_le_bytes()); // made by
        c.extend_from_slice(&20u16.to_le_bytes()); // needed
        c.extend_from_slice(&0x0800u16.to_le_bytes());
        c.extend_from_slice(&8u16.to_le_bytes());
        c.extend_from_slice(&time.to_le_bytes());
        c.extend_from_slice(&date.to_le_bytes());
        c.extend_from_slice(&crc.to_le_bytes());
        c.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        c.extend_from_slice(&(data.len() as u32).to_le_bytes());
        c.extend_from_slice(&(n.len() as u16).to_le_bytes());
        c.extend_from_slice(&[0; 12]); // extra, comment, disk, internal and external attributes
        c.extend_from_slice(&offset.to_le_bytes());
        c.extend_from_slice(n);
        self.count += 1;
    }

    fn finish(mut self) -> Vec<u8> {
        let cd_offset = self.out.len() as u32;
        let cd_size = self.central.len() as u32;
        self.out.extend_from_slice(&self.central);
        self.out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        self.out.extend_from_slice(&[0; 4]); // this disk, disk with the directory
        self.out.extend_from_slice(&self.count.to_le_bytes());
        self.out.extend_from_slice(&self.count.to_le_bytes());
        self.out.extend_from_slice(&cd_size.to_le_bytes());
        self.out.extend_from_slice(&cd_offset.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::DeflateDecoder;
    use std::io::Read;

    /// Reads the entries back through the central directory, checking every CRC.
    fn unzip(z: &[u8]) -> Vec<(String, String)> {
        let eocd = z.len() - 22;
        assert_eq!(&z[eocd..eocd + 4], &0x0605_4b50u32.to_le_bytes());
        let u16_at = |i: usize| u16::from_le_bytes([z[i], z[i + 1]]) as usize;
        let u32_at = |i: usize| u32::from_le_bytes([z[i], z[i + 1], z[i + 2], z[i + 3]]) as usize;
        let count = u16_at(eocd + 10);
        let mut at = u32_at(eocd + 16);
        let mut out = Vec::new();
        for _ in 0..count {
            assert_eq!(u32_at(at), 0x0201_4b50);
            let (crc, csize, nlen, off) = (u32_at(at + 16), u32_at(at + 20), u16_at(at + 28), u32_at(at + 42));
            let name = String::from_utf8(z[at + 46..at + 46 + nlen].to_vec()).unwrap();
            let data_at = off + 30 + u16_at(off + 26) + u16_at(off + 28);
            let mut s = String::new();
            DeflateDecoder::new(&z[data_at..data_at + csize])
                .read_to_string(&mut s)
                .unwrap();
            let mut c = flate2::Crc::new();
            c.update(s.as_bytes());
            assert_eq!(c.sum() as usize, crc, "crc of {name}");
            out.push((name, s));
            at += 46 + nlen;
        }
        out
    }

    #[test]
    fn writes_a_readable_workbook() {
        let names = sheet_names(&["Web: App".into(), "web: app".into(), "x".repeat(40)]);
        assert_eq!(names[0], "Web  App");
        assert_eq!(names[1], "web  app (2)");
        assert_eq!(names[2].chars().count(), 31);
        let book = workbook(&[Sheet {
            name: names[0].clone(),
            rows: vec![
                vec!["Key".into(), "Title".into(), "Points".into()],
                vec!["WEB-1".into(), "A <b> & \"c\"\u{1}".into(), Cell::Number(3.0)],
                vec!["WEB-2".into(), Cell::Empty, Cell::Number(f64::NAN)],
            ],
        }]);
        let files = unzip(&book);
        let names: Vec<_> = files.iter().map(|f| f.0.as_str()).collect();
        assert!(names.contains(&"[Content_Types].xml"));
        assert!(names.contains(&"xl/workbook.xml"));
        let sheet = &files.iter().find(|f| f.0 == "xl/worksheets/sheet1.xml").unwrap().1;
        assert!(sheet.contains(r#"<c r="B2" t="inlineStr"><is><t xml:space="preserve">A &lt;b&gt; &amp; &quot;c&quot;</t></is></c>"#));
        assert!(sheet.contains(r#"<c r="C2"><v>3</v></c>"#));
        assert!(!sheet.contains("C3"), "NaN is left out");
        assert!(sheet.contains(r#"<c r="A1" t="inlineStr" s="1">"#));
        assert_eq!(col_name(0), "A");
        assert_eq!(col_name(25), "Z");
        assert_eq!(col_name(26), "AA");
        assert_eq!(col_name(701), "ZZ");
        assert_eq!(col_name(702), "AAA");
    }
}
