//! Lectura de párrafos, tablas, notas, comentarios y cuadros de texto de WordprocessingML
//! hacia el modelo intermedio del adaptador.
use super::styles::{Counters, ListLabel, Numbering, RunProps, Styles, num_pr};
use super::xml::{Child, Node};
use crate::bundle::{escape_inline, escape_line_start};
use crate::omml::{MathNode, linear};
use crate::ooxml::{meaningful_alt, resolve};
use std::collections::HashMap;

/// Requisitos de `mc:Choice` que el lector interpreta (formas, grupos, lienzos y
/// extensiones de Word y DrawingML); con cualquier otro se usa `mc:Fallback`.
const SUPPORTED_CHOICES: &[&str] = &[
    "wps", "wpg", "wpc", "wpi", "wp14", "w14", "w15", "w16se", "w16cid", "a14", "m",
];

#[derive(Clone, Debug)]
pub(crate) struct Image {
    /// Ruta de la imagen dentro del paquete.
    pub(crate) target: String,
    pub(crate) alt: Option<String>,
    /// Marcada como decorativa en el original (`adec:decorative`).
    pub(crate) decorative: bool,
}

/// Contenido no textual y auxiliar de un párrafo o celda, en orden de aparición.
#[derive(Clone, Debug, Default)]
pub(crate) struct Aux {
    pub(crate) images: Vec<Image>,
    /// Partes de gráficos dentro del paquete.
    pub(crate) charts: Vec<String>,
    /// Párrafos Markdown de cada cuadro de texto anclado.
    pub(crate) textboxes: Vec<Vec<String>>,
    pub(crate) footnotes: Vec<String>,
    pub(crate) endnotes: Vec<String>,
    pub(crate) comments: Vec<String>,
    pub(crate) hidden: Vec<String>,
    /// Revisiones pendientes: tipo («inserción», «eliminación», «traslado (origen)»…) y texto.
    pub(crate) revisions: Vec<(&'static str, String)>,
    pub(crate) equations: usize,
    /// Parte de datos de cada SmartArt (vacía si no se pudo resolver).
    pub(crate) smartart: Vec<String>,
    /// Programa de cada objeto incrustado (`ProgID`).
    pub(crate) objects: Vec<String>,
    pub(crate) linked_images: usize,
    pub(crate) links: usize,
}

impl Aux {
    fn absorb(&mut self, o: Aux) {
        self.images.extend(o.images);
        self.charts.extend(o.charts);
        self.textboxes.extend(o.textboxes);
        self.footnotes.extend(o.footnotes);
        self.endnotes.extend(o.endnotes);
        self.comments.extend(o.comments);
        self.hidden.extend(o.hidden);
        self.revisions.extend(o.revisions);
        self.equations += o.equations;
        self.smartart.extend(o.smartart);
        self.objects.extend(o.objects);
        self.linked_images += o.linked_images;
        self.links += o.links;
    }
}

/// Fragmento de texto con formato homogéneo.
#[derive(Clone, Debug)]
struct Seg {
    text: String,
    bold: bool,
    italic: bool,
    link: Option<String>,
    /// Ecuación linealizada: se escribe como código en línea.
    code: bool,
}

/// Último tipo de texto emitido, para unir fragmentos contiguos ocultos o revisados.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Last {
    #[default]
    Visible,
    Hidden,
    Revision(&'static str),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Para {
    /// Nivel de esquema efectivo (0 = encabezado de nivel 1), directo o del estilo.
    pub(crate) outline: Option<u8>,
    /// El estilo del párrafo es «Title».
    pub(crate) title_style: bool,
    /// Nivel de esquema 9 declarado: texto corriente, nunca un encabezado inferido.
    pub(crate) body_level: bool,
    /// Pertenece a una tabla de contenido generada (campo `TOC` o bloque de galería).
    pub(crate) toc: bool,
    /// Estilo de código: se escribe como bloque de código literal.
    pub(crate) code: bool,
    /// Etiqueta de lista, si el párrafo está numerado.
    pub(crate) list: Option<ListLabel>,
    segs: Vec<Seg>,
    /// Tamaño efectivo del texto visible, si es uniforme (medios puntos).
    size: Option<u32>,
    seen_text: bool,
    mixed_size: bool,
    last: Last,
    pub(crate) aux: Aux,
}

impl Para {
    /// Texto visible literal.
    pub(crate) fn plain(&self) -> String {
        self.segs.iter().map(|s| s.text.as_str()).collect()
    }
    /// Texto visible en una sola línea con espacios normalizados.
    pub(crate) fn flat(&self) -> String {
        self.plain()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
    pub(crate) fn uniform_size(&self) -> Option<u32> {
        if self.mixed_size { None } else { self.size }
    }
    /// Sin texto visible ni contenido no textual.
    pub(crate) fn is_empty(&self) -> bool {
        self.flat().is_empty()
            && self.aux.images.iter().all(|i| i.decorative)
            && self.aux.charts.is_empty()
            && self.aux.textboxes.is_empty()
            && self.aux.smartart.is_empty()
            && self.aux.objects.is_empty()
            && self.aux.linked_images == 0
    }
    /// Markdown en línea: negrita, cursiva, enlaces y ecuaciones; los saltos de línea
    /// manuales son saltos forzados.
    pub(crate) fn md(&self) -> String {
        let mut groups: Vec<Seg> = Vec::new();
        for s in &self.segs {
            match groups.last_mut() {
                Some(g)
                    if !g.code
                        && !s.code
                        && g.bold == s.bold
                        && g.italic == s.italic
                        && g.link == s.link =>
                {
                    g.text.push_str(&s.text)
                }
                _ => groups.push(s.clone()),
            }
        }
        let mut out = String::new();
        for g in groups {
            if g.code {
                let fence = if g.text.contains('`') { "``" } else { "`" };
                out.push_str(&format!("{fence}{}{fence}", g.text));
                continue;
            }
            let piece = g
                .text
                .split('\n')
                .map(|line| emphasize(line, g.bold, g.italic))
                .collect::<Vec<_>>()
                .join("\n");
            match g.link {
                Some(dest) if !g.text.trim().is_empty() => out.push_str(&format!(
                    "[{}]({})",
                    piece.replace('\n', " "),
                    link_dest(&dest)
                )),
                _ => out.push_str(&piece),
            }
        }
        let lines: Vec<String> = out
            .split('\n')
            .map(|l| escape_line_start(l.trim()))
            .collect();
        let start = lines
            .iter()
            .position(|l| !l.is_empty())
            .unwrap_or(lines.len());
        let end = lines
            .iter()
            .rposition(|l| !l.is_empty())
            .map_or(start, |e| e + 1);
        lines[start..end].join("\\\n")
    }
    /// Agrega texto oculto o revisado, uniendo fragmentos contiguos del mismo tipo.
    fn piece(&mut self, kind: Last, text: &str) {
        let same = self.last == kind;
        match kind {
            Last::Hidden => match self.aux.hidden.last_mut() {
                Some(last) if same => last.push_str(text),
                _ => self.aux.hidden.push(text.to_string()),
            },
            Last::Revision(k) => match self.aux.revisions.last_mut() {
                Some((_, last)) if same => last.push_str(text),
                _ => self.aux.revisions.push((k, text.to_string())),
            },
            Last::Visible => {}
        }
        self.last = kind;
    }
    /// Texto literal, sin formato Markdown, para un bloque de código.
    pub(crate) fn code_text(&self) -> String {
        self.segs
            .iter()
            .map(|s| s.text.as_str())
            .collect::<String>()
            .trim_end()
            .to_string()
    }
}

/// Negrita o cursiva alrededor del texto, con los espacios de los extremos fuera de
/// las marcas para que CommonMark las reconozca.
fn emphasize(text: &str, bold: bool, italic: bool) -> String {
    let core = text.trim();
    let mark = match (bold, italic) {
        (true, true) => "***",
        (true, false) => "**",
        (false, true) => "*",
        (false, false) => "",
    };
    if core.is_empty() || mark.is_empty() {
        return escape_inline(text);
    }
    let lead = &text[..text.len() - text.trim_start().len()];
    let trail = &text[text.trim_end().len()..];
    format!("{lead}{mark}{}{mark}{trail}", escape_inline(core))
}

pub(crate) fn link_dest(dest: &str) -> String {
    if dest.contains([' ', '(', ')']) {
        format!("<{dest}>")
    } else {
        dest.to_string()
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Cell {
    /// Columnas de la cuadrícula que ocupa (`w:gridSpan`).
    pub(crate) span: usize,
    /// Continúa una combinación vertical; su contenido está en la celda de arriba.
    pub(crate) merged: bool,
    pub(crate) paras: Vec<Para>,
    pub(crate) nested: Vec<Table>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Table {
    pub(crate) rows: Vec<Vec<Cell>>,
}

impl Table {
    /// Párrafos de la tabla, incluidas las tablas anidadas, en orden.
    pub(crate) fn paras(&self) -> Vec<&Para> {
        let mut out = Vec::new();
        for cell in self.rows.iter().flatten() {
            out.extend(cell.paras.iter());
            for t in &cell.nested {
                out.extend(t.paras());
            }
        }
        out
    }
    pub(crate) fn has_merged_cells(&self) -> bool {
        self.rows.iter().flatten().any(|c| c.merged || c.span > 1)
    }
    pub(crate) fn nested_tables(&self) -> usize {
        self.rows.iter().flatten().map(|c| c.nested.len()).sum()
    }
}

/// Campo complejo (`w:fldChar`) o simple (`w:fldSimple`) abierto.
struct Field {
    instr: String,
    /// Ya pasó el separador: lo que sigue es el resultado visible.
    result: bool,
    link: Option<String>,
    /// Resultado dependiente de la paginación (`PAGE`, `NUMPAGES`, `PAGEREF`), que no es
    /// canónica: se sustituye por `placeholder`.
    hide: bool,
    placeholder: Option<&'static str>,
    /// Tabla de contenido generada.
    toc: bool,
}

impl Field {
    fn new(instr: &str, result: bool) -> Self {
        let mut f = Field {
            instr: instr.to_string(),
            result,
            link: None,
            hide: false,
            placeholder: None,
            toc: false,
        };
        if result {
            f.classify();
        }
        f
    }
    fn classify(&mut self) {
        let instr = self.instr.trim();
        let word = instr
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_uppercase();
        match word.as_str() {
            "HYPERLINK" if !instr.contains("\\l") => {
                self.link = instr
                    .split('"')
                    .nth(1)
                    .map(str::to_string)
                    .filter(|l| !l.is_empty())
            }
            "PAGE" | "SECTIONPAGES" | "PAGEREF" => {
                self.hide = true;
                self.placeholder = Some("[página]");
            }
            "NUMPAGES" => {
                self.hide = true;
                self.placeholder = Some("[total de páginas]");
            }
            "TOC" => self.toc = true,
            _ => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    /// Revisión visible en la vista final (inserción, destino de un traslado, cambio de
    /// formato).
    Added(&'static str),
    /// Revisión ausente de la vista final (eliminación, origen de un traslado).
    Removed(&'static str),
}

/// Rama elegida de un `mc:AlternateContent`.
fn choose(n: &Node) -> Option<&Node> {
    n.nodes()
        .find(|c| {
            c.is(b"Choice")
                && c.attr(b"Requires")
                    .is_some_and(|r| r.split_whitespace().all(|p| SUPPORTED_CHOICES.contains(&p)))
        })
        .or_else(|| n.child(b"Fallback"))
}

/// Contenedores en línea sin significado propio.
fn transparent(n: &Node) -> bool {
    matches!(
        n.name.as_slice(),
        b"smartTag" | b"customXml" | b"sdt" | b"sdtContent" | b"bdo" | b"dir"
    )
}

fn properties(n: &Node) -> bool {
    matches!(
        n.name.as_slice(),
        b"sdtPr" | b"sdtEndPr" | b"smartTagPr" | b"customXmlPr"
    )
}

fn math(n: &Node) -> MathNode {
    MathNode {
        name: n.name.clone(),
        val: n.attr(b"val").map(str::to_string),
        text: if n.is(b"t") { n.text() } else { String::new() },
        children: n.nodes().map(math).collect(),
    }
}

/// Lector de una parte WordprocessingML con el estado que cruza párrafos: campos
/// abiertos, comentarios abiertos, contadores de listas y numeración de notas.
pub(crate) struct Reader<'a> {
    styles: &'a Styles,
    numbering: &'a Numbering,
    part: String,
    rels: HashMap<String, String>,
    fields: Vec<Field>,
    counters: Counters,
    open_comments: Vec<String>,
    /// Texto anclado de cada comentario (entre `commentRangeStart` y `commentRangeEnd`).
    pub(crate) anchors: HashMap<String, String>,
    /// Identificadores de notas al pie y finales en orden de primera referencia.
    pub(crate) footnote_order: Vec<String>,
    pub(crate) endnote_order: Vec<String>,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(styles: &'a Styles, numbering: &'a Numbering) -> Self {
        Reader {
            styles,
            numbering,
            part: String::new(),
            rels: HashMap::new(),
            fields: Vec::new(),
            counters: Counters::default(),
            open_comments: Vec::new(),
            anchors: HashMap::new(),
            footnote_order: Vec::new(),
            endnote_order: Vec::new(),
        }
    }
    /// Cambia la parte que se lee y sus relaciones; los campos y los rangos de
    /// comentarios no cruzan partes.
    pub(crate) fn set_part(&mut self, part: &str, rels: HashMap<String, String>) {
        self.part = part.to_string();
        self.rels = rels;
        self.fields.clear();
        self.open_comments.clear();
    }
    fn target(&self, rid: &str) -> Option<String> {
        self.rels.get(rid).map(|t| resolve(&self.part, t))
    }
    fn suppressed(&self) -> bool {
        self.fields.iter().any(|f| !f.result || f.hide)
    }
    /// Al empezar el resultado de un campo de paginación se escribe su marcador, salvo
    /// que un campo exterior lo oculte.
    fn field_placeholder(&mut self, para: &mut Para, props: &RunProps, mode: Mode) {
        let Some((last, outer)) = self.fields.split_last() else {
            return;
        };
        if let Some(text) = last.placeholder
            && !outer.iter().any(|f| !f.result || f.hide)
        {
            self.visible(para, text, props, None, mode);
        }
    }

    pub(crate) fn paragraph(&mut self, p: &Node) -> Para {
        let ppr = p.child(b"pPr");
        let style = ppr.and_then(|x| x.val(b"pStyle")).map(str::to_string);
        let style = style.as_deref();
        let direct_outline = ppr
            .and_then(|x| x.val(b"outlineLvl"))
            .and_then(|v| v.parse::<u8>().ok());
        let declared = direct_outline.or_else(|| self.styles.outline(style));
        let outline = declared.filter(|l| *l < 9);
        let num = match num_pr(ppr) {
            Some((id, lvl)) => Some((id, lvl)),
            None => self.styles.numbering(style),
        }
        .filter(|(id, _)| id != "0");
        let mut para = Para {
            outline,
            body_level: declared == Some(9),
            code: self.styles.is_code(style),
            title_style: self.styles.is_title(style),
            ..Para::default()
        };
        let base = self.styles.para_run(style);
        for n in p.nodes() {
            self.inline(n, &mut para, &base, None, Mode::Normal);
        }
        // Un párrafo numerado sin texto no muestra su número ni avanza la lista.
        if let Some((id, lvl)) = num
            && !para.flat().is_empty()
        {
            para.list = Some(self.counters.next(self.numbering, &id, lvl));
        }
        para
    }

    pub(crate) fn table(&mut self, t: &Node) -> Table {
        let mut rows = Vec::new();
        for tr in children(t, b"tr") {
            let row = children(tr, b"tc")
                .into_iter()
                .map(|tc| self.cell(tc))
                .collect();
            rows.push(row);
        }
        Table { rows }
    }

    fn cell(&mut self, tc: &Node) -> Cell {
        let pr = tc.child(b"tcPr");
        let mut cell = Cell {
            span: pr
                .and_then(|p| p.val(b"gridSpan"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(1usize)
                .max(1),
            merged: pr
                .and_then(|p| p.child(b"vMerge"))
                .is_some_and(|v| v.attr(b"val") != Some("restart")),
            ..Cell::default()
        };
        self.cell_content(tc, &mut cell);
        cell
    }

    fn cell_content(&mut self, n: &Node, cell: &mut Cell) {
        for c in n.nodes() {
            if c.is(b"p") {
                let p = self.paragraph(c);
                cell.paras.push(p);
            } else if c.is(b"tbl") {
                let t = self.table(c);
                cell.nested.push(t);
            } else if transparent(c) && !properties(c) {
                self.cell_content(c, cell);
            }
        }
    }

    fn inline(
        &mut self,
        n: &Node,
        para: &mut Para,
        base: &RunProps,
        link: Option<&str>,
        mode: Mode,
    ) {
        match n.name.as_slice() {
            b"r" => self.run(n, para, base, link, mode),
            b"hyperlink" => {
                let target = n.rel(b"id").and_then(|rid| self.rels.get(rid)).map(|t| {
                    match n.attr(b"anchor") {
                        Some(a) => format!("{t}#{a}"),
                        None => t.clone(),
                    }
                });
                if target.is_some() {
                    para.aux.links += 1;
                }
                let l = target.as_deref().or(link);
                for c in n.nodes() {
                    self.inline(c, para, base, l, mode);
                }
            }
            b"ins" | b"moveTo" | b"del" | b"moveFrom" => {
                let revision = match n.name.as_slice() {
                    b"ins" => Mode::Added("inserción"),
                    b"moveTo" => Mode::Added("traslado (destino)"),
                    b"del" => Mode::Removed("eliminación"),
                    _ => Mode::Removed("traslado (origen)"),
                };
                for c in n.nodes() {
                    self.inline(c, para, base, link, revision);
                }
            }
            b"fldSimple" => {
                self.fields
                    .push(Field::new(n.attr(b"instr").unwrap_or_default(), true));
                self.field_placeholder(para, base, mode);
                for c in n.nodes() {
                    self.inline(c, para, base, link, mode);
                }
                self.fields.pop();
            }
            b"oMathPara" => {
                for c in n.nodes() {
                    self.inline(c, para, base, link, mode);
                }
            }
            b"oMath" => {
                let eq = linear(&math(n));
                if !matches!(mode, Mode::Removed(_)) && !self.suppressed() && !eq.is_empty() {
                    para.aux.equations += 1;
                    para.last = Last::Visible;
                    para.segs.push(Seg {
                        text: eq,
                        bold: false,
                        italic: false,
                        link: None,
                        code: true,
                    });
                }
            }
            b"commentRangeStart" => {
                if let Some(id) = n.attr(b"id") {
                    self.open_comments.push(id.to_string());
                    self.anchors.entry(id.to_string()).or_default();
                }
            }
            b"commentRangeEnd" => {
                if let Some(id) = n.attr(b"id") {
                    self.open_comments.retain(|c| c != id);
                }
            }
            b"AlternateContent" => {
                if let Some(branch) = choose(n) {
                    for c in branch.nodes() {
                        self.inline(c, para, base, link, mode);
                    }
                }
            }
            _ if transparent(n) => {
                for c in n.nodes().filter(|c| !properties(c)) {
                    self.inline(c, para, base, link, mode);
                }
            }
            _ => {}
        }
    }

    fn run(&mut self, r: &Node, para: &mut Para, base: &RunProps, link: Option<&str>, mode: Mode) {
        let rpr = r.child(b"rPr");
        let mut props = base.clone();
        if let Some(rs) = rpr.and_then(|x| x.val(b"rStyle")) {
            props = props.over(&self.styles.char_run(rs));
        }
        props = props.over(&RunProps::from_rpr(rpr));
        // Un cambio de formato registrado deja el texto en la vista final.
        let mode = match mode {
            Mode::Normal if rpr.is_some_and(|x| x.child(b"rPrChange").is_some()) => {
                Mode::Added("cambio de formato")
            }
            m => m,
        };
        for c in r.nodes() {
            self.run_child(c, para, &props, link, mode);
        }
    }

    fn run_child(
        &mut self,
        c: &Node,
        para: &mut Para,
        props: &RunProps,
        link: Option<&str>,
        mode: Mode,
    ) {
        match c.name.as_slice() {
            b"t" | b"delText" => self.emit(para, &c.text(), props, link, mode),
            b"instrText" => {
                if let Some(f) = self.fields.last_mut()
                    && !f.result
                {
                    f.instr.push_str(&c.text());
                }
            }
            b"fldChar" => match c.attr(b"fldCharType") {
                Some("begin") => self.fields.push(Field::new("", false)),
                Some("separate") => {
                    if let Some(f) = self.fields.last_mut() {
                        f.result = true;
                        f.classify();
                    }
                    self.field_placeholder(para, props, mode);
                }
                Some("end") => {
                    self.fields.pop();
                }
                _ => {}
            },
            b"tab" | b"ptab" => self.emit(para, " ", props, link, mode),
            b"br" if !matches!(c.attr(b"type"), Some("page" | "column")) => {
                self.emit(para, "\n", props, link, mode)
            }
            b"cr" => self.emit(para, "\n", props, link, mode),
            b"noBreakHyphen" => self.emit(para, "-", props, link, mode),
            b"drawing" if !matches!(mode, Mode::Removed(_)) && !self.suppressed() => {
                self.drawing(c, para)
            }
            b"pict" | b"object" if !matches!(mode, Mode::Removed(_)) && !self.suppressed() => {
                self.vml(c, para)
            }
            b"footnoteReference" | b"endnoteReference"
                if !matches!(mode, Mode::Removed(_)) && props.vanish != Some(true) =>
            {
                let Some(id) = c.attr(b"id").map(str::to_string) else {
                    return;
                };
                let foot = c.is(b"footnoteReference");
                let order = if foot {
                    &mut self.footnote_order
                } else {
                    &mut self.endnote_order
                };
                let n = match order.iter().position(|x| *x == id) {
                    Some(i) => i + 1,
                    None => {
                        order.push(id.clone());
                        order.len()
                    }
                };
                let text = if foot {
                    para.aux.footnotes.push(id);
                    format!("[nota {n}]")
                } else {
                    para.aux.endnotes.push(id);
                    format!("[nota final {n}]")
                };
                let plain = RunProps {
                    bold: None,
                    italic: None,
                    ..props.clone()
                };
                self.emit(para, &text, &plain, None, mode);
            }
            b"commentReference" => {
                if let Some(id) = c.attr(b"id") {
                    para.aux.comments.push(id.to_string());
                }
            }
            b"AlternateContent" => {
                if let Some(branch) = choose(c) {
                    for x in branch.nodes() {
                        self.run_child(x, para, props, link, mode);
                    }
                }
            }
            _ => {}
        }
    }

    fn emit(
        &mut self,
        para: &mut Para,
        text: &str,
        props: &RunProps,
        link: Option<&str>,
        mode: Mode,
    ) {
        if self.fields.iter().any(|f| f.toc && f.result) {
            para.toc = true;
        }
        if !self.suppressed() {
            self.visible(para, text, props, link, mode);
        }
    }

    /// Texto que no oculta ningún campo: va a la vista final, al texto oculto o a las
    /// revisiones según su formato y su modo.
    fn visible(
        &mut self,
        para: &mut Para,
        text: &str,
        props: &RunProps,
        link: Option<&str>,
        mode: Mode,
    ) {
        if text.is_empty() {
            return;
        }
        if let Mode::Removed(kind) = mode {
            para.piece(Last::Revision(kind), text);
            return;
        }
        if props.vanish == Some(true) {
            para.piece(Last::Hidden, text);
            return;
        }
        match mode {
            Mode::Added(kind) => para.piece(Last::Revision(kind), text),
            _ => para.last = Last::Visible,
        }
        for id in &self.open_comments {
            if let Some(a) = self.anchors.get_mut(id) {
                a.push_str(text);
            }
        }
        if !text.trim().is_empty() {
            if !para.seen_text {
                para.seen_text = true;
                para.size = props.size;
            } else if para.size != props.size {
                para.mixed_size = true;
            }
        }
        let link = link
            .map(str::to_string)
            .or_else(|| self.fields.iter().rev().find_map(|f| f.link.clone()));
        para.segs.push(Seg {
            text: text.to_string(),
            bold: props.bold == Some(true),
            italic: props.italic == Some(true),
            link,
            code: false,
        });
    }

    fn drawing(&mut self, d: &Node, para: &mut Para) {
        let docpr = d.find(b"docPr");
        let alt = docpr.and_then(|p| {
            meaningful_alt(p.attr(b"descr")).or_else(|| meaningful_alt(p.attr(b"title")))
        });
        let decorative = docpr.is_some_and(|p| {
            p.find(b"decorative")
                .is_some_and(|x| !matches!(x.attr(b"val"), Some("0" | "false")))
        });
        let mut found = Vec::new();
        d.find_all(b"graphicData", b"txbxContent", &mut found);
        for gd in found {
            let uri = gd.attr(b"uri").unwrap_or_default();
            if uri.contains("/diagram") {
                let data = gd
                    .find(b"relIds")
                    .and_then(|r| r.rel(b"dm"))
                    .and_then(|rid| self.target(rid));
                para.aux.smartart.push(data.unwrap_or_default());
            } else if uri.contains("/chart")
                && let Some(target) = gd
                    .find(b"chart")
                    .and_then(|c| c.rel(b"id"))
                    .and_then(|rid| self.target(rid))
            {
                para.aux.charts.push(target);
            }
        }
        let mut blips = Vec::new();
        d.find_all(b"blip", b"txbxContent", &mut blips);
        for b in blips {
            let svg = b.find(b"svgBlip").and_then(|s| s.rel(b"embed"));
            match svg.or(b.rel(b"embed")).and_then(|rid| self.target(rid)) {
                Some(target) => para.aux.images.push(Image {
                    target,
                    alt: alt.clone(),
                    decorative,
                }),
                None if b.rel(b"link").is_some() => para.aux.linked_images += 1,
                None => {}
            }
        }
        let mut boxes = Vec::new();
        d.find_all(b"txbxContent", b"", &mut boxes);
        for tb in boxes {
            self.textbox(tb, para);
        }
    }

    /// Forma VML (`w:pict`) u objeto incrustado (`w:object`): imagen, cuadro de texto
    /// u objeto OLE con su vista previa.
    fn vml(&mut self, v: &Node, para: &mut Para) {
        let shape_alt = v
            .find(b"shape")
            .and_then(|s| meaningful_alt(s.attr(b"alt")));
        let mut images = Vec::new();
        v.find_all(b"imagedata", b"txbxContent", &mut images);
        for i in images {
            if let Some(target) = i.rel(b"id").or(i.rel(b"pict")).and_then(|r| self.target(r)) {
                para.aux.images.push(Image {
                    target,
                    alt: shape_alt
                        .clone()
                        .or_else(|| meaningful_alt(i.attr(b"title"))),
                    decorative: false,
                });
            }
        }
        if let Some(ole) = v.find(b"OLEObject") {
            para.aux
                .objects
                .push(ole.attr(b"ProgID").unwrap_or("desconocido").to_string());
        }
        let mut boxes = Vec::new();
        v.find_all(b"txbxContent", b"", &mut boxes);
        for tb in boxes {
            self.textbox(tb, para);
        }
    }

    fn textbox(&mut self, tb: &Node, para: &mut Para) {
        let mut lines = Vec::new();
        self.box_content(tb, &mut lines, para);
        if !lines.is_empty() {
            para.aux.textboxes.push(lines);
        }
    }

    fn box_content(&mut self, n: &Node, lines: &mut Vec<String>, para: &mut Para) {
        for c in n.nodes() {
            if c.is(b"p") {
                let mut inner = self.paragraph(c);
                let mut md = inner.md();
                if let Some(l) = &inner.list
                    && !md.is_empty()
                {
                    md = match (l.ordinal, &l.label) {
                        (Some(n), _) => format!("{n}. {md}"),
                        (None, Some(label)) => format!("- {} {md}", escape_inline(label)),
                        _ => format!("- {md}"),
                    };
                }
                para.aux.absorb(std::mem::take(&mut inner.aux));
                if !md.is_empty() {
                    lines.push(md);
                }
            } else if c.is(b"tbl") {
                let t = self.table(c);
                for p in t.paras() {
                    let md = p.md();
                    para.aux.absorb(p.aux.clone());
                    if !md.is_empty() {
                        lines.push(md);
                    }
                }
            } else if transparent(c) && !properties(c) {
                self.box_content(c, lines, para);
            }
        }
    }

    /// Notas al pie o finales de una parte, por identificador; se omiten los separadores.
    pub(crate) fn notes(&mut self, root: &Node) -> HashMap<String, Vec<Para>> {
        let mut out = HashMap::new();
        for n in root
            .nodes()
            .filter(|n| n.is(b"footnote") || n.is(b"endnote"))
        {
            if n.attr(b"type").is_some_and(|t| t != "normal") {
                continue;
            }
            let Some(id) = n.attr(b"id") else { continue };
            let paras = self.flow(n);
            out.insert(id.to_string(), paras);
        }
        out
    }

    /// Párrafos de un contenedor (nota, comentario, encabezado), con las tablas
    /// aplanadas en sus párrafos.
    pub(crate) fn flow(&mut self, n: &Node) -> Vec<Para> {
        let mut out = Vec::new();
        for c in n.nodes() {
            if c.is(b"p") {
                let p = self.paragraph(c);
                out.push(p);
            } else if c.is(b"tbl") {
                let t = self.table(c);
                out.extend(t.paras().into_iter().cloned());
            } else if transparent(c) && !properties(c) {
                out.extend(self.flow(c));
            }
        }
        out
    }
}

/// Hijos con ese nombre, atravesando controles de contenido y XML personalizado.
fn children<'n>(n: &'n Node, name: &[u8]) -> Vec<&'n Node> {
    let mut out = Vec::new();
    for c in n.nodes() {
        if c.is(name) {
            out.push(c);
        } else if transparent(c) && !properties(c) {
            out.extend(children(c, name));
        }
    }
    out
}

/// Texto directo de un nodo de la estructura (sin formato), p. ej. `dc:title`.
pub(crate) fn node_text(n: &Node) -> String {
    let mut s = n.text();
    for c in &n.children {
        if let Child::Node(c) = c {
            s.push_str(&node_text(c));
        }
    }
    s
}
