//! Referencias A1 (columnas, celdas y rangos) y desplazamiento de las fórmulas
//! compartidas.

/// Columnas y filas máximas de una hoja de Excel.
pub(crate) const MAX_COL: u32 = 16_384;
pub(crate) const MAX_ROW: u32 = 1_048_576;

/// Letras de una columna desde 1 (`1` → `A`, `28` → `AB`).
pub(crate) fn col_name(mut c: u32) -> String {
    let mut s = Vec::new();
    while c > 0 {
        let r = (c - 1) % 26;
        s.push(b'A' + r as u8);
        c = (c - 1) / 26;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

fn col_number(letters: &str) -> Option<u32> {
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let mut n = 0u32;
    for b in letters.bytes() {
        if !b.is_ascii_alphabetic() {
            return None;
        }
        n = n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1);
    }
    (n <= MAX_COL).then_some(n)
}

/// Fila y columna (desde 1) de una celda A1; admite `$`.
pub(crate) fn parse_cell(s: &str) -> Option<(u32, u32)> {
    let s = s.trim().replace('$', "");
    let split = s.find(|c: char| c.is_ascii_digit())?;
    let col = col_number(&s[..split])?;
    let row: u32 = s[split..].parse().ok()?;
    (1..=MAX_ROW).contains(&row).then_some((row, col))
}

pub(crate) fn cell_name(row: u32, col: u32) -> String {
    format!("{}{row}", col_name(col))
}

/// Rango rectangular con extremos incluidos, desde 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Rect {
    pub(crate) r0: u32,
    pub(crate) c0: u32,
    pub(crate) r1: u32,
    pub(crate) c1: u32,
}

impl Rect {
    pub(crate) fn cell(r: u32, c: u32) -> Self {
        Rect {
            r0: r,
            c0: c,
            r1: r,
            c1: c,
        }
    }
    /// `A1` o `A1:C9`, con o sin `$`.
    pub(crate) fn parse(s: &str) -> Option<Self> {
        let (a, b) = s.split_once(':').unwrap_or((s, s));
        let (r0, c0) = parse_cell(a)?;
        let (r1, c1) = parse_cell(b)?;
        Some(Rect {
            r0: r0.min(r1),
            c0: c0.min(c1),
            r1: r0.max(r1),
            c1: c0.max(c1),
        })
    }
    pub(crate) fn a1(&self) -> String {
        let a = cell_name(self.r0, self.c0);
        if self.r0 == self.r1 && self.c0 == self.c1 {
            a
        } else {
            format!("{a}:{}", cell_name(self.r1, self.c1))
        }
    }
    pub(crate) fn contains(&self, r: u32, c: u32) -> bool {
        (self.r0..=self.r1).contains(&r) && (self.c0..=self.c1).contains(&c)
    }
    pub(crate) fn contains_rect(&self, o: &Rect) -> bool {
        self.contains(o.r0, o.c0) && self.contains(o.r1, o.c1)
    }
    pub(crate) fn rows(&self) -> u32 {
        self.r1 - self.r0 + 1
    }
    pub(crate) fn cols(&self) -> u32 {
        self.c1 - self.c0 + 1
    }
    pub(crate) fn union(&self, o: &Rect) -> Rect {
        Rect {
            r0: self.r0.min(o.r0),
            c0: self.c0.min(o.c0),
            r1: self.r1.max(o.r1),
            c1: self.c1.max(o.c1),
        }
    }
    pub(crate) fn overlaps(&self, o: &Rect) -> bool {
        self.r0 <= o.r1 && o.r0 <= self.r1 && self.c0 <= o.c1 && o.c0 <= self.c1
    }
}

/// «E2:E301, G4 y H7»: agrupa en rangos verticales las celdas consecutivas de cada
/// columna; las celdas llegan en cualquier orden.
pub(crate) fn cells_phrase(cells: &[(u32, u32)], max: usize) -> String {
    let mut sorted = cells.to_vec();
    sorted.sort_by_key(|&(r, c)| (c, r));
    sorted.dedup();
    let mut parts: Vec<Rect> = Vec::new();
    for (r, c) in sorted {
        match parts.last_mut() {
            Some(p) if p.c0 == c && p.r1 + 1 == r => p.r1 = r,
            _ => parts.push(Rect::cell(r, c)),
        }
    }
    parts.sort_by_key(|p| (p.r0, p.c0));
    let mut names: Vec<String> = parts.iter().map(Rect::a1).collect();
    let extra = names.len().saturating_sub(max);
    names.truncate(max);
    if extra > 0 {
        return format!("{} y {extra} más", names.join(", "));
    }
    let last = names.pop().unwrap_or_default();
    if names.is_empty() {
        last
    } else {
        format!("{} y {last}", names.join(", "))
    }
}

/// Fórmula de una celda que depende de una fórmula compartida: las referencias
/// relativas de la celda maestra se desplazan `dr` filas y `dc` columnas, como hace
/// Excel al rellenar. `None` si alguna referencia saldría de la hoja.
pub(crate) fn shift_formula(f: &str, dr: i64, dc: i64) -> Option<String> {
    let chars: Vec<char> = f.chars().collect();
    let mut out = String::with_capacity(f.len());
    let mut i = 0;
    let shift_col = |s: &str, out: &mut String| -> Option<()> {
        let (abs, letters) = s.strip_prefix('$').map_or((false, s), |l| (true, l));
        let mut c = i64::from(col_number(letters)?);
        if !abs {
            c += dc;
        }
        if !(1..=i64::from(MAX_COL)).contains(&c) {
            return None;
        }
        if abs {
            out.push('$');
        }
        out.push_str(&col_name(c as u32));
        Some(())
    };
    let shift_row = |s: &str, out: &mut String| -> Option<()> {
        let (abs, digits) = s.strip_prefix('$').map_or((false, s), |d| (true, d));
        let mut r: i64 = digits.parse().ok()?;
        if !abs {
            r += dr;
        }
        if !(1..=i64::from(MAX_ROW)).contains(&r) {
            return None;
        }
        if abs {
            out.push('$');
        }
        out.push_str(&r.to_string());
        Some(())
    };
    // Una referencia de celda: `$?LETRAS$?DÍGITOS`.
    let split_cell = |t: &str| -> Option<(String, String)> {
        let body = t.strip_prefix('$').unwrap_or(t);
        let letters: String = body.chars().take_while(char::is_ascii_alphabetic).collect();
        let rest = &body[letters.len()..];
        let rest_digits = rest.strip_prefix('$').unwrap_or(rest);
        if letters.is_empty()
            || rest_digits.is_empty()
            || !rest_digits.chars().all(|c| c.is_ascii_digit())
            || col_number(&letters).is_none()
        {
            return None;
        }
        let col = format!("{}{letters}", if t.starts_with('$') { "$" } else { "" });
        let row = format!(
            "{}{rest_digits}",
            if rest.starts_with('$') { "$" } else { "" }
        );
        Some((col, row))
    };
    let is_col = |t: &str| col_number(t.strip_prefix('$').unwrap_or(t)).is_some();
    let is_row = |t: &str| {
        let d = t.strip_prefix('$').unwrap_or(t);
        !d.is_empty() && d.chars().all(|c| c.is_ascii_digit())
    };
    let token_end = |start: usize| {
        let mut j = start;
        while j < chars.len()
            && (chars[j].is_alphanumeric() || matches!(chars[j], '_' | '.' | '$' | '\\'))
        {
            j += 1;
        }
        j
    };
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '"' | '\'' => {
                // Cadena literal o nombre de hoja entre comillas; la comilla doble escapa.
                out.push(ch);
                i += 1;
                while i < chars.len() {
                    out.push(chars[i]);
                    if chars[i] == ch {
                        if chars.get(i + 1) == Some(&ch) {
                            out.push(ch);
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            '[' => {
                // Referencia estructurada o índice de libro externo: se copia entera.
                let mut depth = 0;
                while i < chars.len() {
                    out.push(chars[i]);
                    match chars[i] {
                        '[' => depth += 1,
                        ']' => {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            c if c.is_alphanumeric() || matches!(c, '$' | '_' | '\\') => {
                let prev = out.chars().last();
                let end = token_end(i);
                let token: String = chars[i..end].iter().collect();
                let next = chars.get(end).copied();
                // Parte de un número decimal o exponente (`1.5E+3`): se copia.
                if prev.is_some_and(|p| p.is_ascii_digit() || p == '.')
                    || next == Some('(')
                    || next == Some('!')
                {
                    out.push_str(&token);
                    i = end;
                    continue;
                }
                if let Some((col, row)) = split_cell(&token) {
                    shift_col(&col, &mut out)?;
                    shift_row(&row, &mut out)?;
                    i = end;
                    continue;
                }
                // Columnas o filas completas: `A:C`, `$2:$5`.
                if next == Some(':') {
                    let end2 = token_end(end + 1);
                    let second: String = chars[end + 1..end2].iter().collect();
                    if is_col(&token) && is_col(&second) {
                        shift_col(&token, &mut out)?;
                        out.push(':');
                        shift_col(&second, &mut out)?;
                        i = end2;
                        continue;
                    }
                    if is_row(&token) && is_row(&second) {
                        shift_row(&token, &mut out)?;
                        out.push(':');
                        shift_row(&second, &mut out)?;
                        i = end2;
                        continue;
                    }
                }
                out.push_str(&token);
                i = end;
            }
            _ => {
                out.push(ch);
                i += 1;
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_round_trip() {
        assert_eq!(col_name(1), "A");
        assert_eq!(col_name(28), "AB");
        assert_eq!(col_name(16_384), "XFD");
        assert_eq!(parse_cell("$AB$12"), Some((12, 28)));
        assert_eq!(parse_cell("XFE1"), None);
        assert_eq!(Rect::parse("C9:A1").unwrap().a1(), "A1:C9");
        assert_eq!(Rect::parse("B2").unwrap().a1(), "B2");
    }

    #[test]
    fn shared_formulas_shift_relative_references_only() {
        assert_eq!(shift_formula("D2*E2", 1, 0).unwrap(), "D3*E3");
        assert_eq!(shift_formula("SUM(B4:B7)", 0, 2).unwrap(), "SUM(D4:D7)");
        assert_eq!(shift_formula("$B$2*C$1+$A3", 2, 1).unwrap(), "$B$2*D$1+$A5");
        assert_eq!(
            shift_formula("'Hoja 1'!A1&\"B2\"&LOG10(C3)", 1, 0).unwrap(),
            "'Hoja 1'!A2&\"B2\"&LOG10(C4)"
        );
        assert_eq!(
            shift_formula("SUM(TablaDetalle[Importe])+Tasa*1.5E+3", 4, 4).unwrap(),
            "SUM(TablaDetalle[Importe])+Tasa*1.5E+3"
        );
        assert_eq!(
            shift_formula("SUM(A:A)+SUM(2:3)", 1, 1).unwrap(),
            "SUM(B:B)+SUM(3:4)"
        );
        assert_eq!(
            shift_formula("[1]Tarifas!B2", 1, 0).unwrap(),
            "[1]Tarifas!B3"
        );
        assert_eq!(shift_formula("A1", -1, 0), None);
    }

    #[test]
    fn cells_are_grouped_into_vertical_ranges() {
        let cells = [(3, 3), (2, 3), (4, 3), (7, 8), (2, 1)];
        assert_eq!(cells_phrase(&cells, 10), "A2, C2:C4 y H7");
        assert_eq!(cells_phrase(&cells, 1), "A2 y 2 más");
    }
}
