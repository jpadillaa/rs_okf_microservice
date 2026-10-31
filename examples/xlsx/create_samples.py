"""Build three artificial but realistic XLSX workbooks for the XLSX adapter. The markup
follows the patterns Microsoft Excel writes (shared strings, cellXfs with built-in and
custom number formats, shared and array formulas, ListObject tables, pivot tables with
their cache, drawings with charts and pictures, legacy and threaded comments, external
links), so the adapter is exercised as with a real file.

muestras/ventas_trimestrales.xlsx   sales workbook: merged title over a summary table
                                    with SUM formulas (shared), percentages, a chart
                                    with cached data, conditional formatting, a legacy
                                    comment and a threaded comment with a reply,
                                    external and internal hyperlinks; a 300-row Excel
                                    table with dates, currency, booleans and a shared
                                    formula; a hidden sheet with a named range and a
                                    #DIV/0! error; a one-column notes sheet.
muestras/inventario_regiones.xlsx   several regions per sheet: two tables side by side,
                                    a caption separated by a blank row, a two-row
                                    header with merged cells, a loose note far away,
                                    hidden rows and columns; a pivot table with its
                                    cache; a sheet with date, time, currency, text,
                                    scientific, fraction and floating-point values; a
                                    picture with alt text and a chartsheet.
muestras/libro_con_perdidas.xlsx    content the converter cannot represent faithfully:
                                    external link, formulas without a stored value in a
                                    workbook that asks to recalculate, a chart without
                                    cached data, an embedded OLE object, a picture whose
                                    part is missing, an array formula and an empty
                                    sheet. Expected state: partial.

Usage: python3 -I examples/xlsx/create_samples.py
"""
from datetime import date, datetime
from pathlib import Path
from xml.sax.saxutils import escape, quoteattr
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo
import re
import struct
import zlib

OUT = Path(__file__).parent / "muestras"

S = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
XDR = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing"
A = "http://schemas.openxmlformats.org/drawingml/2006/main"
C = "http://schemas.openxmlformats.org/drawingml/2006/chart"
MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"
X14AC = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac"
TC = "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments"
XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
DATE = "2026-10-29T10:00:00Z"
SML = "application/vnd.openxmlformats-officedocument.spreadsheetml"
DML = "application/vnd.openxmlformats-officedocument.drawing"


def png(rgb, w=80, h=40):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
    rows = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


# --- Estilos ------------------------------------------------------------------------
# Índices de cellXfs que usan las hojas.
GENERAL, FECHA, PORCENTAJE, MONEDA, FECHA_HORA, HORA, TEXTO, MES, MILES, CIENTIFICO, FRACCION, NEGRITA, TITULO = range(13)
CUSTOM_FORMATS = {164: '#,##0.00\\ "€"', 165: "dd/mm/yyyy\\ hh:mm", 166: "mmmm\\ yyyy"}
XFS = [0, 14, 10, 164, 165, 21, 49, 166, 4, 11, 12, 0, 0]


def styles():
    fmts = "".join(f"<numFmt numFmtId=\"{k}\" formatCode={quoteattr(v)}/>" for k, v in CUSTOM_FORMATS.items())
    xfs = []
    for i, fmt in enumerate(XFS):
        font = 1 if i in (NEGRITA, TITULO) else 0
        apply = ' applyNumberFormat="1"' if fmt else ""
        bold = ' applyFont="1"' if font else ""
        align = '<alignment horizontal="center"/>' if i == TITULO else ""
        xfs.append(f'<xf numFmtId="{fmt}" fontId="{font}" fillId="0" borderId="0" xfId="0"{apply}{bold}>{align}</xf>'
                   if align else f'<xf numFmtId="{fmt}" fontId="{font}" fillId="0" borderId="0" xfId="0"{apply}{bold}/>')
    return (f'{XML}<styleSheet xmlns="{S}" xmlns:mc="{MC}" xmlns:x14ac="{X14AC}" mc:Ignorable="x14ac">'
            f'<numFmts count="{len(CUSTOM_FORMATS)}">{fmts}</numFmts>'
            '<fonts count="2"><font><sz val="11"/><name val="Calibri"/><family val="2"/></font>'
            '<font><b/><sz val="11"/><name val="Calibri"/><family val="2"/></font></fonts>'
            '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>'
            '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
            '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
            f'<cellXfs count="{len(xfs)}">{"".join(xfs)}</cellXfs>'
            '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>'
            '<dxfs count="1"><dxf><font><color rgb="FF9C0006"/></font><fill><patternFill><bgColor rgb="FFFFC7CE"/></patternFill></fill></dxf></dxfs>'
            '</styleSheet>')


# --- Celdas ---------------------------------------------------------------------------

def col_letter(n):
    s = ""
    while n:
        n, r = divmod(n - 1, 26)
        s = chr(65 + r) + s
    return s


def split_ref(ref):
    m = re.fullmatch(r"([A-Z]+)(\d+)", ref)
    col = 0
    for ch in m.group(1):
        col = col * 26 + ord(ch) - 64
    return col, int(m.group(2))


def serial(d):
    base = datetime(1899, 12, 30)
    if isinstance(d, datetime):
        delta = d - base
        return delta.days + delta.seconds / 86400
    return (d - base.date()).days


class Err(str):
    """Valor de error almacenado (#DIV/0!, #N/A, #REF!)."""


class Inline(str):
    """Cadena en línea (t="inlineStr") en lugar de la tabla compartida."""


class Sheet:
    def __init__(self, name, state=None):
        self.name, self.state = name, state
        self.cells = {}          # ref -> (value, style, formula)
        self.merges, self.hidden_rows, self.hidden_cols = [], set(), set()
        self.extra, self.tail = "", ""   # XML tras sheetData y al final de la hoja
        self.rels, self.cols_xml = [], ""

    def set(self, ref, value=None, style=GENERAL, formula=None):
        self.cells[ref] = (value, style, formula)

    def row(self, ref, values, style=GENERAL):
        col, row = split_ref(ref)
        for i, v in enumerate(values):
            if v is not None:
                self.set(f"{col_letter(col + i)}{row}", v, style)


class Book:
    def __init__(self):
        self.strings, self.index = [], {}
        self.sheets, self.parts, self.overrides = [], {}, {}
        self.defined_names, self.workbook_rels, self.calc = [], [], '<calcPr calcId="191029"/>'
        self.external_refs, self.pivot_caches = "", ""

    def sst(self, s):
        if s not in self.index:
            self.index[s] = len(self.strings)
            self.strings.append(s)
        return self.index[s]

    def cell_xml(self, ref, value, style, formula):
        s = f' s="{style}"' if style else ""
        f = ""
        if formula is not None:
            f = formula if formula.startswith("<f") else f"<f>{escape(formula)}</f>"
        if value is None:
            return f'<c r="{ref}"{s}>{f}</c>' if f else f'<c r="{ref}"{s}/>'
        if isinstance(value, Err):
            return f'<c r="{ref}"{s} t="e">{f}<v>{value}</v></c>'
        if isinstance(value, bool):
            return f'<c r="{ref}"{s} t="b">{f}<v>{int(value)}</v></c>'
        if isinstance(value, Inline):
            return f'<c r="{ref}"{s} t="inlineStr"><is><t xml:space="preserve">{escape(value)}</t></is></c>'
        if isinstance(value, str):
            if formula is not None:
                return f'<c r="{ref}"{s} t="str">{f}<v>{escape(value)}</v></c>'
            return f'<c r="{ref}"{s} t="s"><v>{self.sst(value)}</v></c>'
        if isinstance(value, (date, datetime)):
            value = serial(value)
        return f'<c r="{ref}"{s}>{f}<v>{value!r}</v></c>' if isinstance(value, float) else f'<c r="{ref}"{s}>{f}<v>{value}</v></c>'

    def sheet_xml(self, sh):
        rows = {}
        for ref, cell in sh.cells.items():
            col, row = split_ref(ref)
            rows.setdefault(row, []).append((col, ref, cell))
        out = []
        for row in sorted(rows):
            cells = "".join(self.cell_xml(ref, *cell) for _, ref, cell in sorted(rows[row]))
            hidden = ' hidden="1"' if row in sh.hidden_rows else ""
            out.append(f'<row r="{row}"{hidden}>{cells}</row>')
        for row in sorted(sh.hidden_rows - set(rows)):
            out.append(f'<row r="{row}" hidden="1"/>')
        out.sort(key=lambda r: int(re.search(r'r="(\d+)"', r).group(1)))
        refs = list(sh.cells) or ["A1"]
        cols = [split_ref(r)[0] for r in refs]
        rws = [split_ref(r)[1] for r in refs]
        dim = f"{col_letter(min(cols))}{min(rws)}:{col_letter(max(cols))}{max(rws)}"
        cols_xml = sh.cols_xml or "".join(
            f'<col min="{c}" max="{c}" width="0" hidden="1" customWidth="1"/>' for c in sorted(sh.hidden_cols))
        cols_xml = f"<cols>{cols_xml}</cols>" if cols_xml else ""
        merges = (f'<mergeCells count="{len(sh.merges)}">'
                  + "".join(f'<mergeCell ref="{m}"/>' for m in sh.merges) + "</mergeCells>") if sh.merges else ""
        return (f'{XML}<worksheet xmlns="{S}" xmlns:r="{R}" xmlns:mc="{MC}" xmlns:x14ac="{X14AC}" mc:Ignorable="x14ac">'
                f'<dimension ref="{dim}"/><sheetViews><sheetView workbookViewId="0"/></sheetViews>'
                f'<sheetFormatPr defaultRowHeight="15" x14ac:dyDescent="0.25"/>{cols_xml}'
                f'<sheetData>{"".join(out)}</sheetData>{merges}{sh.extra}'
                '<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/>'
                f'{sh.tail}</worksheet>')

    def write(self, name, title):
        parts = dict(self.parts)
        wb_rels = []
        sheets = []
        for i, sh in enumerate(self.sheets, 1):
            if isinstance(sh, tuple):  # hoja de gráfico: (nombre, xml, rels)
                cname, xml, srels = sh
                path = f"xl/chartsheets/sheet{i}.xml"
                parts[path] = xml
                parts[f"xl/chartsheets/_rels/sheet{i}.xml.rels"] = rels(srels)
                self.overrides["/" + path] = f"{SML}.chartsheet+xml"
                wb_rels.append((f"rId{i}", "chartsheet", f"chartsheets/sheet{i}.xml", False))
                sheets.append(f'<sheet name={quoteattr(cname)} sheetId="{i}" r:id="rId{i}"/>')
                continue
            path = f"xl/worksheets/sheet{i}.xml"
            parts[path] = self.sheet_xml(sh)
            if sh.rels:
                parts[f"xl/worksheets/_rels/sheet{i}.xml.rels"] = rels(sh.rels)
            self.overrides["/" + path] = f"{SML}.worksheet+xml"
            wb_rels.append((f"rId{i}", "worksheet", f"worksheets/sheet{i}.xml", False))
            state = f' state="{sh.state}"' if sh.state else ""
            sheets.append(f'<sheet name={quoteattr(sh.name)} sheetId="{i}"{state} r:id="rId{i}"/>')
        n = len(self.sheets)
        wb_rels += [(f"rId{n + 1}", "theme", "theme/theme1.xml", False),
                    (f"rId{n + 2}", "styles", "styles.xml", False),
                    (f"rId{n + 3}", "sharedStrings", "sharedStrings.xml", False)] + self.workbook_rels
        names = ("<definedNames>" + "".join(
            f'<definedName name={quoteattr(k)}{extra}>{escape(v)}</definedName>' for k, v, extra in self.defined_names)
            + "</definedNames>") if self.defined_names else ""
        parts["xl/workbook.xml"] = (
            f'{XML}<workbook xmlns="{S}" xmlns:r="{R}" xmlns:mc="{MC}" xmlns:x15="http://schemas.microsoft.com/office/spreadsheetml/2010/11/main" mc:Ignorable="x15">'
            '<fileVersion appName="xl" lastEdited="7" lowestEdited="7" rupBuild="27425"/>'
            '<workbookPr defaultThemeVersion="202300"/><bookViews><workbookView xWindow="0" yWindow="0" windowWidth="28800" windowHeight="12300"/></bookViews>'
            f'<sheets>{"".join(sheets)}</sheets>{self.external_refs}{names}{self.calc}{self.pivot_caches}</workbook>')
        parts["xl/_rels/workbook.xml.rels"] = rels(wb_rels)
        parts["xl/styles.xml"] = styles()
        parts["xl/theme/theme1.xml"] = (f'{XML}<a:theme xmlns:a="{A}" name="Office"><a:themeElements>'
                                        '<a:clrScheme name="Office"/><a:fontScheme name="Office"/><a:fmtScheme name="Office"/>'
                                        '</a:themeElements></a:theme>')
        total = sum(1 for sh in self.sheets if not isinstance(sh, tuple) for v, _, _ in sh.cells.values()
                    if isinstance(v, str) and not isinstance(v, (Err, Inline)))
        sis = "".join(
            f'<si><t xml:space="preserve">{escape(s)}</t></si>' if not s.startswith("<r>") else f"<si>{s}</si>"
            for s in self.strings)
        parts["xl/sharedStrings.xml"] = (f'{XML}<sst xmlns="{S}" count="{total}" uniqueCount="{len(self.strings)}">'
                                         f'{sis}</sst>')
        parts["docProps/core.xml"] = core(title)
        parts["docProps/app.xml"] = (f'{XML}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties">'
                                     '<Application>Microsoft Excel</Application><AppVersion>16.0300</AppVersion></Properties>')
        parts["_rels/.rels"] = rels([("rId1", "officeDocument", "xl/workbook.xml", False),
                                     ("rId2", "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties",
                                      "docProps/core.xml", False),
                                     ("rId3", "extended-properties", "docProps/app.xml", False)])
        overrides = {"/xl/workbook.xml": f"{SML}.sheet.main+xml", "/xl/styles.xml": f"{SML}.styles+xml",
                     "/xl/sharedStrings.xml": f"{SML}.sharedStrings+xml",
                     "/xl/theme/theme1.xml": "application/vnd.openxmlformats-officedocument.theme+xml",
                     "/docProps/core.xml": "application/vnd.openxmlformats-package.core-properties+xml",
                     "/docProps/app.xml": "application/vnd.openxmlformats-officedocument.extended-properties+xml",
                     **self.overrides}
        parts["[Content_Types].xml"] = (
            f'{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/>'
            '<Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/>'
            '<Default Extension="bin" ContentType="application/vnd.openxmlformats-officedocument.oleObject"/>'
            + "".join(f'<Override PartName="{p}" ContentType="{t}"/>' for p, t in overrides.items() if p[1:] in parts)
            + "</Types>")
        OUT.mkdir(exist_ok=True)
        path = OUT / name
        order = ["[Content_Types].xml", "_rels/.rels"] + sorted(p for p in parts if p not in ("[Content_Types].xml", "_rels/.rels"))
        with ZipFile(path, "w", ZIP_DEFLATED) as z:
            for p in order:
                info = ZipInfo(p, date_time=(2026, 10, 29, 10, 0, 0))
                info.compress_type = ZIP_DEFLATED
                data = parts[p]
                z.writestr(info, data if isinstance(data, bytes) else data.encode("utf-8"))
        print(f"{path} ({path.stat().st_size} bytes)")


def rels(items):
    out = []
    for rid, kind, target, external in items:
        kind = kind if kind.startswith("http") else f"{REL}/{kind}"
        mode = ' TargetMode="External"' if external else ""
        out.append(f'<Relationship Id="{rid}" Type="{kind}" Target={quoteattr(target)}{mode}/>')
    return f'{XML}<Relationships xmlns="{PKG}">{"".join(out)}</Relationships>'


def core(title):
    return (f'{XML}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
            'xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" '
            'xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">'
            f'<dc:title>{escape(title)}</dc:title><dc:creator>Equipo de Análisis</dc:creator>'
            f'<dcterms:created xsi:type="dcterms:W3CDTF">{DATE}</dcterms:created>'
            f'<dcterms:modified xsi:type="dcterms:W3CDTF">{DATE}</dcterms:modified></cp:coreProperties>')


# --- Dibujos, gráficos y comentarios ----------------------------------------------------

def anchor(col, row, body, to=None):
    to_col, to_row = to or (col + 6, row + 14)
    return (f'<xdr:twoCellAnchor editAs="oneCell"><xdr:from><xdr:col>{col}</xdr:col><xdr:colOff>0</xdr:colOff>'
            f'<xdr:row>{row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>{to_col}</xdr:col>'
            f'<xdr:colOff>0</xdr:colOff><xdr:row>{to_row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>'
            f'{body}<xdr:clientData/></xdr:twoCellAnchor>')


def chart_frame(shape_id, rid, name):
    return (f'<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="{shape_id}" name="{name}"/>'
            '<xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm>'
            f'<a:graphic><a:graphicData uri="{C}"><c:chart r:id="{rid}"/></a:graphicData></a:graphic></xdr:graphicFrame>')


def picture(shape_id, rid, name, descr=None):
    d = f" descr={quoteattr(descr)}" if descr else ""
    return (f'<xdr:pic><xdr:nvPicPr><xdr:cNvPr id="{shape_id}" name="{name}"{d}/><xdr:cNvPicPr>'
            '<a:picLocks noChangeAspect="1"/></xdr:cNvPicPr></xdr:nvPicPr>'
            f'<xdr:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill>'
            '<xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1905000" cy="952500"/></a:xfrm>'
            '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>')


def drawing(anchors):
    return (f'{XML}<xdr:wsDr xmlns:xdr="{XDR}" xmlns:a="{A}" xmlns:r="{R}" xmlns:c="{C}">'
            f'{"".join(anchors)}</xdr:wsDr>')


def chart_xml(title, series, cats_ref, cats, cached=True, kind="bar"):
    def cache(tag, values):
        if not cached:
            return ""
        pts = "".join(f'<c:pt idx="{i}"><c:v>{escape(str(v))}</c:v></c:pt>' for i, v in enumerate(values))
        fmt = "<c:formatCode>General</c:formatCode>" if tag == "num" else ""
        return f'<c:{tag}Cache>{fmt}<c:ptCount val="{len(values)}"/>{pts}</c:{tag}Cache>'
    sers = ""
    for i, (name_ref, name, vals_ref, vals) in enumerate(series):
        sers += (f'<c:ser><c:idx val="{i}"/><c:order val="{i}"/>'
                 f'<c:tx><c:strRef><c:f>{escape(name_ref)}</c:f>{cache("str", [name])}</c:strRef></c:tx>'
                 f'<c:cat><c:strRef><c:f>{escape(cats_ref)}</c:f>{cache("str", cats)}</c:strRef></c:cat>'
                 f'<c:val><c:numRef><c:f>{escape(vals_ref)}</c:f>{cache("num", vals)}</c:numRef></c:val></c:ser>')
    head = '<c:barDir val="col"/><c:grouping val="clustered"/>' if kind == "bar" else '<c:grouping val="standard"/>'
    t = (f'<c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>{escape(title)}</a:t></a:r></a:p></c:rich></c:tx>'
         '<c:overlay val="0"/></c:title>') if title and cached else ""
    return (f'{XML}<c:chartSpace xmlns:c="{C}" xmlns:a="{A}" xmlns:r="{R}"><c:chart>{t}<c:plotArea>'
            f'<c:{kind}Chart>{head}{sers}<c:axId val="1"/><c:axId val="2"/></c:{kind}Chart>'
            '<c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/>'
            '<c:axPos val="b"/><c:crossAx val="2"/></c:catAx><c:valAx><c:axId val="2"/><c:scaling>'
            '<c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="1"/></c:valAx>'
            '</c:plotArea></c:chart></c:chartSpace>')


def legacy_comments(items):
    authors = sorted({a for _, a, _ in items})
    body = "".join(
        f'<comment ref="{ref}" authorId="{authors.index(a)}"><text><r><rPr><b/><sz val="9"/></rPr><t>{escape(a)}:</t></r>'
        f'<r><rPr><sz val="9"/></rPr><t xml:space="preserve">\n{escape(t)}</t></r></text></comment>'
        for ref, a, t in items)
    return (f'{XML}<comments xmlns="{S}"><authors>{"".join(f"<author>{escape(a)}</author>" for a in authors)}</authors>'
            f'<commentList>{body}</commentList></comments>')


def vml(refs):
    shapes = "".join(
        f'<v:shape id="_x0000_s{1025 + i}" type="#_x0000_t202" style="position:absolute;visibility:hidden" fillcolor="#ffffe1">'
        f'<x:ClientData ObjectType="Note"><x:MoveWithCells/><x:Row>{split_ref(r)[1] - 1}</x:Row>'
        f'<x:Column>{split_ref(r)[0] - 1}</x:Column></x:ClientData></v:shape>' for i, r in enumerate(refs))
    return ('<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" '
            f'xmlns:x="urn:schemas-microsoft-com:office:excel">{shapes}</xml>')


# --- Libro 1: ventas trimestrales ----------------------------------------------------------

REGIONES = ["Andina", "Caribe", "Pacífico", "Orinoquía"]
PRODUCTOS = [("Café especial", 18.5), ("Cacao en grano", 12.75), ("Panela orgánica", 4.2),
             ("Aguacate Hass", 6.9), ("Flores de corte", 22.0)]


def ventas():
    b = Book()
    res = Sheet("Resumen")
    res.set("A1", "Resumen de ventas 2026", TITULO)
    res.merges.append("A1:F1")
    res.row("A3", ["Región", "T1", "T2", "T3", "T4", "Variación T4/T1"], NEGRITA)
    data = [[120500, 131200, 128900, 140250], [98300, 102100, 99800, 110400],
            [76450, 80100, 85320, 90010], [43200, 41950, 45600, 47800]]
    for i, (reg, vals) in enumerate(zip(REGIONES, data)):
        r = 4 + i
        res.set(f"A{r}", reg)
        for j, v in enumerate(vals):
            res.set(f"{col_letter(2 + j)}{r}", v, MILES)
        var = vals[3] / vals[0] - 1
        f = f'<f t="shared" ref="F4:F7" si="1">E4/B4-1</f>' if i == 0 else '<f t="shared" si="1"/>'
        res.set(f"F{r}", var, PORCENTAJE, f)
    res.set("A8", "Total", NEGRITA)
    for j in range(4):
        c = col_letter(2 + j)
        f = f'<f t="shared" ref="B8:E8" si="0">SUM(B4:B7)</f>' if j == 0 else '<f t="shared" si="0"/>'
        res.set(f"{c}8", sum(row[j] for row in data), MILES, f)
    total = [sum(row[j] for row in data) for j in range(4)]
    res.set("F8", total[3] / total[0] - 1, PORCENTAJE, "E8/B8-1")
    res.set("A10", "Fuente: sistema comercial, corte al 15 de octubre de 2026.")
    res.set("A11", "Ver detalle por operación")
    res.set("A12", "Tablero publicado")
    res.extra = ('<conditionalFormatting sqref="B4:E7"><cfRule type="cellIs" dxfId="0" priority="1" operator="lessThan">'
                 '<formula>50000</formula></cfRule></conditionalFormatting>'
                 '<hyperlinks><hyperlink ref="A11" location="\'Detalle\'!A1" display="Ver detalle por operación"/>'
                 '<hyperlink ref="A12" r:id="rId4"/></hyperlinks>')
    res.tail = '<drawing r:id="rId1"/><legacyDrawing r:id="rId3"/>'
    res.rels = [("rId1", "drawing", "../drawings/drawing1.xml", False),
                ("rId2", "comments", "../comments1.xml", False),
                ("rId3", "vmlDrawing", "../drawings/vmlDrawing1.vml", False),
                ("rId4", "hyperlink", "https://tablero.example.org/ventas-2026", True),
                ("rId5", "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment",
                 "../threadedComments/threadedComment1.xml", False)]
    b.parts["xl/drawings/drawing1.xml"] = drawing([anchor(7, 1, chart_frame(2, "rId1", "Gráfico 1"))])
    b.parts["xl/drawings/_rels/drawing1.xml.rels"] = rels([("rId1", "chart", "../charts/chart1.xml", False)])
    b.parts["xl/charts/chart1.xml"] = chart_xml(
        "Ventas por región y trimestre",
        [(f"Resumen!${col_letter(2 + j)}$3", f"T{j + 1}", f"Resumen!${col_letter(2 + j)}$4:${col_letter(2 + j)}$7",
          [row[j] for row in data]) for j in range(4)],
        "Resumen!$A$4:$A$7", REGIONES)
    # Comentario clásico en B4 y comentario encadenado con respuesta en E8; Excel escribe
    # también el comentario clásico equivalente del hilo.
    b.parts["xl/comments1.xml"] = legacy_comments([
        ("B4", "Marta Ruiz", "Incluye la venta extraordinaria a la cadena hotelera."),
        ("E8", "tc={6B1C7F1A-0D4C-4C61-9E2B-3F5A1D2C8E01}",
         "[Comentario encadenado]\n\nSu versión de Excel le permite leer este comentario encadenado; sin embargo, las ediciones que se apliquen se quitarán si el archivo se abre en una versión más reciente de Excel. Más información: https://go.microsoft.com/fwlink/?linkid=870924\n\nComentario:\n    ¿El total de T4 ya incluye las devoluciones?")])
    b.parts["xl/drawings/vmlDrawing1.vml"] = vml(["B4", "E8"])
    b.parts["xl/threadedComments/threadedComment1.xml"] = (
        f'{XML}<ThreadedComments xmlns="{TC}" xmlns:x="{S}">'
        '<threadedComment ref="E8" dT="2026-10-20T15:04:00.00" personId="{11111111-2222-3333-4444-555555555555}" '
        'id="{6B1C7F1A-0D4C-4C61-9E2B-3F5A1D2C8E01}"><text>¿El total de T4 ya incluye las devoluciones?</text></threadedComment>'
        '<threadedComment ref="E8" dT="2026-10-21T09:30:00.00" personId="{66666666-7777-8888-9999-000000000000}" '
        'id="{8D2E9A3B-1E5D-4D72-8F3C-4A6B2E3D9F02}" parentId="{6B1C7F1A-0D4C-4C61-9E2B-3F5A1D2C8E01}">'
        '<text>Sí, están descontadas desde la versión 3 del informe.</text></threadedComment></ThreadedComments>')
    b.parts["xl/persons/person.xml"] = (
        f'{XML}<personList xmlns="{TC}" xmlns:x="{S}">'
        '<person displayName="Andrés Pineda" id="{11111111-2222-3333-4444-555555555555}" userId="apineda" providerId="None"/>'
        '<person displayName="Marta Ruiz" id="{66666666-7777-8888-9999-000000000000}" userId="mruiz" providerId="None"/>'
        '</personList>')
    b.workbook_rels.append(("rIdP", "http://schemas.microsoft.com/office/2017/10/relationships/person",
                            "persons/person.xml", False))
    b.overrides.update({"/xl/drawings/drawing1.xml": f"{DML}+xml", "/xl/charts/chart1.xml": f"{DML}ml.chart+xml",
                        "/xl/comments1.xml": f"{SML}.comments+xml",
                        "/xl/threadedComments/threadedComment1.xml": "application/vnd.ms-excel.threadedcomments+xml",
                        "/xl/persons/person.xml": "application/vnd.ms-excel.person+xml"})

    det = Sheet("Detalle")
    headers = ["Fecha", "Región", "Producto", "Unidades", "Precio unitario", "Importe", "Entregado"]
    det.row("A1", headers, NEGRITA)
    rows = 300
    for i in range(rows):
        r = i + 2
        d = date(2026, 1, 2) + (date(2026, 1, 3) - date(2026, 1, 2)) * (i * 283 % 287)
        prod, price = PRODUCTOS[i * 7 % 5]
        units = 5 + (i * 37) % 120
        det.set(f"A{r}", d, FECHA)
        det.set(f"B{r}", REGIONES[i * 3 % 4])
        det.set(f"C{r}", prod)
        det.set(f"D{r}", units)
        det.set(f"E{r}", price, MONEDA)
        f = f'<f t="shared" ref="F2:F{rows + 1}" si="0">D2*E2</f>' if i == 0 else '<f t="shared" si="0"/>'
        det.set(f"F{r}", round(units * price, 2), MONEDA, f)
        det.set(f"G{r}", i % 9 != 4)
    det.tail = '<tableParts count="1"><tablePart r:id="rId1"/></tableParts>'
    det.rels = [("rId1", "table", "../tables/table1.xml", False)]
    cols = "".join(f'<tableColumn id="{i + 1}" name="{h}"/>' for i, h in enumerate(headers))
    b.parts["xl/tables/table1.xml"] = (
        f'{XML}<table xmlns="{S}" id="1" name="TablaDetalle" displayName="TablaDetalle" ref="A1:G{rows + 1}" totalsRowShown="0">'
        f'<autoFilter ref="A1:G{rows + 1}"/><tableColumns count="7">{cols}</tableColumns>'
        '<tableStyleInfo name="TableStyleMedium2" showFirstColumn="0" showLastColumn="0" showRowStripes="1" showColumnStripes="0"/></table>')
    b.overrides["/xl/tables/table1.xml"] = f"{SML}.table+xml"

    sup = Sheet("Supuestos", state="hidden")
    sup.row("A1", ["Parámetro", "Valor", "Comentario"], NEGRITA)
    sup.row("A2", ["Tasa de cambio objetivo"])
    sup.set("B2", 0.035, PORCENTAJE)
    sup.set("C2", "Usada en las proyecciones del cuarto trimestre.")
    sup.row("A3", ["Margen mínimo"])
    sup.set("B3", 0.18, PORCENTAJE)
    sup.row("A4", ["Rendimiento por hectárea"])
    sup.set("B4", Err("#DIV/0!"), GENERAL, "Detalle!D2/0")
    sup.set("C4", "Pendiente: falta el área sembrada.")
    b.defined_names = [("Tasa", "Supuestos!$B$2", ""), ("MargenMinimo", "Supuestos!$B$3", ""),
                       ("_xlnm.Print_Area", "Resumen!$A$1:$F$12", ' localSheetId="0"'),
                       ("_xlnm._FilterDatabase", "Detalle!$A$1:$G$301", ' localSheetId="1" hidden="1"')]

    notas = Sheet("Notas")
    notas.set("A1", "Notas metodológicas", NEGRITA)
    notas.set("A3", "1. Los importes están en euros e incluyen impuestos.")
    notas.set("A4", "2. Las devoluciones se descuentan en el trimestre en que se registran.")
    notas.set("A5", Inline("3. Las regiones siguen la división comercial vigente:\nAndina, Caribe, Pacífico y Orinoquía."))
    notas.set("A7", "Última revisión: comité de ventas, octubre de 2026.")
    b.sheets = [res, det, sup, notas]
    b.write("ventas_trimestrales.xlsx", "Ventas trimestrales 2026")


# --- Libro 2: inventario con varias regiones ------------------------------------------------

def inventario():
    b = Book()
    alm = Sheet("Almacenes")
    alm.set("A1", "Almacén Norte", TITULO)
    alm.merges.append("A1:C1")
    alm.row("A2", ["Referencia", "Descripción", "Existencias"], NEGRITA)
    norte = [("N-001", "Tornillo hexagonal M8", 1250), ("N-002", "Arandela plana 8 mm", 3400),
             ("N-003", "Tuerca autoblocante M8", 980), ("N-004", "Pasador elástico 4×20", 410)]
    for i, row in enumerate(norte):
        alm.row(f"A{3 + i}", list(row))
    alm.set("A7", "Total", NEGRITA)
    alm.set("C7", sum(r[2] for r in norte), GENERAL, "SUM(C3:C6)")
    alm.set("E1", "Almacén Sur", TITULO)
    alm.merges.append("E1:G1")
    alm.row("E2", ["Referencia", "Descripción", "Existencias"], NEGRITA)
    sur = [("S-101", "Brida de acero DN50", 64), ("S-102", "Junta de goma DN50", 220)]
    for i, row in enumerate(sur):
        alm.row(f"E{3 + i}", list(row))
    alm.set("A10", "Pedidos pendientes")
    alm.row("A12", ["Pedido", "Proveedor", "Fecha prevista", "Importe"], NEGRITA)
    pedidos = [("P-2026-118", "Ferretería Central", date(2026, 11, 4), 1830.5),
               ("P-2026-121", "Suministros del Valle", date(2026, 11, 12), 642.0),
               ("P-2026-125", "Ferretería Central", date(2026, 11, 20), 2210.75)]
    for i, (p, prov, d, imp) in enumerate(pedidos):
        r = 13 + i
        alm.set(f"A{r}", p)
        alm.set(f"B{r}", prov)
        alm.set(f"C{r}", d, FECHA)
        alm.set(f"D{r}", imp, MONEDA)
    alm.hidden_rows = {14}
    alm.hidden_cols = {6}
    alm.set("J20", "Inventario físico pendiente de conciliar con el sistema.")
    alm.rels = [("rId1", "drawing", "../drawings/drawing1.xml", False)]
    alm.tail = '<drawing r:id="rId1"/>'
    b.parts["xl/drawings/drawing1.xml"] = drawing([anchor(9, 1, picture(2, "rId1", "Imagen 1", "Plano de distribución del almacén Norte"), (12, 8))])
    b.parts["xl/drawings/_rels/drawing1.xml.rels"] = rels([("rId1", "image", "../media/image1.png", False)])
    b.parts["xl/media/image1.png"] = png((70, 130, 180))
    b.overrides["/xl/drawings/drawing1.xml"] = f"{DML}+xml"

    din = Sheet("Dinámica")
    din.set("A1", "Existencias por proveedor (tabla dinámica)", NEGRITA)
    din.row("A3", ["Etiquetas de fila", "Suma de Importe"], NEGRITA)
    din.row("A4", ["Ferretería Central", 4041.25])
    din.row("A5", ["Suministros del Valle", 642.0])
    din.row("A6", ["Total general", 4683.25], NEGRITA)
    din.rels = [("rId1", "pivotTable", "../pivotTables/pivotTable1.xml", False)]
    b.parts["xl/pivotTables/pivotTable1.xml"] = (
        f'{XML}<pivotTableDefinition xmlns="{S}" name="TablaDinámica1" cacheId="1" applyNumberFormats="0" '
        'applyBorderFormats="0" applyFontFormats="0" applyPatternFormats="0" applyAlignmentFormats="0" '
        'applyWidthHeightFormats="1" dataCaption="Valores" updatedVersion="8" minRefreshableVersion="3" '
        'createdVersion="8" indent="0" outline="1" outlineData="1" multipleFieldFilters="0">'
        '<location ref="A3:B6" firstHeaderRow="1" firstDataRow="1" firstDataCol="1"/>'
        '<pivotFields count="4"><pivotField showAll="0"/><pivotField axis="axisRow" showAll="0"><items count="3">'
        '<item x="0"/><item x="1"/><item t="default"/></items></pivotField><pivotField showAll="0"/>'
        '<pivotField dataField="1" showAll="0"/></pivotFields><rowFields count="1"><field x="1"/></rowFields>'
        '<rowItems count="3"><i><x/></i><i><x v="1"/></i><i t="grand"><x/></i></rowItems><colItems count="1"><i/></colItems>'
        '<dataFields count="1"><dataField name="Suma de Importe" fld="3" baseField="0" baseItem="0"/></dataFields>'
        '</pivotTableDefinition>')
    b.parts["xl/pivotTables/_rels/pivotTable1.xml.rels"] = rels(
        [("rId1", "pivotCacheDefinition", "../pivotCache/pivotCacheDefinition1.xml", False)])
    b.parts["xl/pivotCache/pivotCacheDefinition1.xml"] = (
        f'{XML}<pivotCacheDefinition xmlns="{S}" xmlns:r="{R}" r:id="rId1" refreshedBy="Equipo de Análisis" '
        'refreshedDate="46323.5" createdVersion="8" refreshedVersion="8" minRefreshableVersion="3" recordCount="3">'
        '<cacheSource type="worksheet"><worksheetSource ref="A12:D15" sheet="Almacenes"/></cacheSource>'
        '<cacheFields count="4"><cacheField name="Pedido" numFmtId="0"><sharedItems/></cacheField>'
        '<cacheField name="Proveedor" numFmtId="0"><sharedItems count="2"><s v="Ferretería Central"/><s v="Suministros del Valle"/></sharedItems></cacheField>'
        '<cacheField name="Fecha prevista" numFmtId="14"><sharedItems containsSemiMixedTypes="0" containsNonDate="0" containsDate="1"/></cacheField>'
        '<cacheField name="Importe" numFmtId="164"><sharedItems containsSemiMixedTypes="0" containsString="0" containsNumber="1"/></cacheField>'
        '</cacheFields></pivotCacheDefinition>')
    b.parts["xl/pivotCache/_rels/pivotCacheDefinition1.xml.rels"] = rels(
        [("rId1", "pivotCacheRecords", "pivotCacheRecords1.xml", False)])
    b.parts["xl/pivotCache/pivotCacheRecords1.xml"] = (
        f'{XML}<pivotCacheRecords xmlns="{S}" count="3"><r><s v="P-2026-118"/><x v="0"/><d v="2026-11-04T00:00:00"/><n v="1830.5"/></r>'
        '<r><s v="P-2026-121"/><x v="1"/><d v="2026-11-12T00:00:00"/><n v="642"/></r>'
        '<r><s v="P-2026-125"/><x v="0"/><d v="2026-11-20T00:00:00"/><n v="2210.75"/></r></pivotCacheRecords>')
    b.workbook_rels.append(("rIdC1", "pivotCacheDefinition", "pivotCache/pivotCacheDefinition1.xml", False))
    b.pivot_caches = '<pivotCaches><pivotCache cacheId="1" r:id="rIdC1"/></pivotCaches>'
    b.overrides.update({"/xl/pivotTables/pivotTable1.xml": f"{SML}.pivotTable+xml",
                        "/xl/pivotCache/pivotCacheDefinition1.xml": f"{SML}.pivotCacheDefinition+xml",
                        "/xl/pivotCache/pivotCacheRecords1.xml": f"{SML}.pivotCacheRecords+xml"})

    val = Sheet("Formatos")
    val.row("A1", ["Tipo", "Valor", "Formato aplicado"], NEGRITA)
    casos = [
        ("Fecha", date(2026, 3, 15), FECHA, "dd/mm/aaaa (integrado 14)"),
        ("Fecha y hora", datetime(2026, 3, 15, 14, 30), FECHA_HORA, "dd/mm/yyyy hh:mm"),
        ("Hora", 0.6041666666666666, HORA, "h:mm:ss (integrado 21)"),
        ("Mes y año", date(2026, 7, 1), MES, "mmmm yyyy"),
        ("Moneda", 1234.5, MONEDA, '#,##0.00 "€"'),
        ("Porcentaje", 0.125, PORCENTAJE, "0.00%"),
        ("Miles", 9876543.21, MILES, "#,##0.00"),
        ("Científico", 0.000123, CIENTIFICO, "0.00E+00"),
        ("Fracción", 2.75, FRACCION, "# ?/?"),
        ("Texto con ceros", "00123", TEXTO, "@"),
        ("Suma flotante", 0.30000000000000004, GENERAL, "General; 0,1 + 0,2"),
        ("Booleano", True, GENERAL, "VERDADERO"),
        ("Fórmula de texto", "Inventario Norte", GENERAL, "CONCAT"),
        ("Celda con | barra", "a | b", GENERAL, "texto con barra vertical"),
    ]
    for i, (tipo, v, st, fmt) in enumerate(casos):
        r = 2 + i
        val.set(f"A{r}", tipo)
        formula = 'CONCAT("Inventario ",Almacenes!A1)' if tipo == "Fórmula de texto" else (
            "0.1+0.2" if tipo == "Suma flotante" else None)
        if tipo == "Fórmula de texto":
            formula = 'CONCAT("Inventario ",MID(Almacenes!A1,9,5))'
        val.set(f"B{r}", v, st, formula)
        val.set(f"C{r}", fmt)

    chart_rels = [("rId1", "drawing", "../drawings/drawing2.xml", False)]
    chartsheet = (f'{XML}<chartsheet xmlns="{S}" xmlns:r="{R}"><sheetPr/><sheetViews><sheetView zoomScale="115" workbookViewId="0" zoomToFit="1"/></sheetViews>'
                  '<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><drawing r:id="rId1"/></chartsheet>')
    b.parts["xl/drawings/drawing2.xml"] = (
        f'{XML}<xdr:wsDr xmlns:xdr="{XDR}" xmlns:a="{A}" xmlns:r="{R}" xmlns:c="{C}"><xdr:absoluteAnchor>'
        '<xdr:pos x="0" y="0"/><xdr:ext cx="8670000" cy="6300000"/>'
        f'{chart_frame(2, "rId1", "Gráfico 1")}<xdr:clientData/></xdr:absoluteAnchor></xdr:wsDr>')
    b.parts["xl/drawings/_rels/drawing2.xml.rels"] = rels([("rId1", "chart", "../charts/chart1.xml", False)])
    b.parts["xl/charts/chart1.xml"] = chart_xml(
        "Existencias del almacén Norte", [("Almacenes!$C$2", "Existencias", "Almacenes!$C$3:$C$6", [r[2] for r in norte])],
        "Almacenes!$B$3:$B$6", [r[1] for r in norte])
    b.overrides.update({"/xl/drawings/drawing2.xml": f"{DML}+xml", "/xl/charts/chart1.xml": f"{DML}ml.chart+xml"})
    b.sheets = [alm, din, val, ("Gráfico existencias", chartsheet, chart_rels)]
    b.write("inventario_regiones.xlsx", "Inventario por almacén")


# --- Libro 3: contenido no representable -----------------------------------------------------

def perdidas():
    b = Book()
    b.calc = '<calcPr calcId="191029" fullCalcOnLoad="1"/>'
    pre = Sheet("Precios")
    pre.row("A1", ["Producto", "Precio 2025", "Precio 2026", "Incremento"], NEGRITA)
    items = [("Café especial", 17.2), ("Cacao en grano", 11.9), ("Panela orgánica", 4.0)]
    for i, (p, v) in enumerate(items):
        r = 2 + i
        pre.set(f"A{r}", p)
        pre.set(f"B{r}", v, MONEDA, f"[1]Tarifas!B{r}")
        pre.set(f"C{r}", None, MONEDA, f"B{r}*(1+Tasa)")
        pre.set(f"D{r}", None, PORCENTAJE, f"C{r}/B{r}-1")
    pre.set("A6", "Suma de cuadrados")
    pre.set("B6", 495.05, GENERAL, '<f t="array" ref="B6">SUM(B2:B4*B2:B4)</f>')
    pre.set("A8", "Ficha técnica incrustada")
    pre.tail = ('<drawing r:id="rId1"/><legacyDrawing r:id="rId2"/>'
                '<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006">'
                '<mc:Choice Requires="x14"><oleObjects><mc:AlternateContent><mc:Choice Requires="x14">'
                '<oleObject progId="Word.Document.12" shapeId="1025" r:id="rId3"><objectPr defaultSize="0" r:id="rId4">'
                '<anchor moveWithCells="1"><from><xdr:col xmlns:xdr="' + XDR + '">1</xdr:col><xdr:colOff xmlns:xdr="' + XDR + '">0</xdr:colOff>'
                '<xdr:row xmlns:xdr="' + XDR + '">7</xdr:row><xdr:rowOff xmlns:xdr="' + XDR + '">0</xdr:rowOff></from>'
                '<to><xdr:col xmlns:xdr="' + XDR + '">3</xdr:col><xdr:colOff xmlns:xdr="' + XDR + '">0</xdr:colOff>'
                '<xdr:row xmlns:xdr="' + XDR + '">12</xdr:row><xdr:rowOff xmlns:xdr="' + XDR + '">0</xdr:rowOff></to></anchor>'
                '</objectPr></oleObject></mc:Choice><mc:Fallback><oleObject progId="Word.Document.12" shapeId="1025" r:id="rId3"/>'
                '</mc:Fallback></mc:AlternateContent></oleObjects></mc:Choice></mc:AlternateContent>')
    pre.rels = [("rId1", "drawing", "../drawings/drawing1.xml", False),
                ("rId2", "vmlDrawing", "../drawings/vmlDrawing1.vml", False),
                ("rId3", "package", "../embeddings/Microsoft_Word_Document.docx", False),
                ("rId4", "image", "../media/image2.emf", False)]
    b.parts["xl/embeddings/Microsoft_Word_Document.docx"] = b"PK\x05\x06" + b"\x00" * 18
    b.parts["xl/media/image2.emf"] = b"\x01\x00\x00\x00" + b"\x00" * 80
    b.parts["xl/drawings/vmlDrawing1.vml"] = vml([])
    b.parts["xl/drawings/drawing1.xml"] = drawing([
        anchor(5, 0, chart_frame(2, "rId1", "Gráfico 1")),
        anchor(5, 16, picture(3, "rId2", "Imagen 2", "Sello de calidad del proveedor"), (8, 22))])
    b.parts["xl/drawings/_rels/drawing1.xml.rels"] = rels([("rId1", "chart", "../charts/chart1.xml", False),
                                                           ("rId2", "image", "../media/image9.png", False)])
    b.parts["xl/charts/chart1.xml"] = chart_xml(
        "Precios 2026", [("Precios!$C$1", "Precio 2026", "Precios!$C$2:$C$4", [])], "Precios!$A$2:$A$4", [], cached=False)
    b.overrides.update({"/xl/drawings/drawing1.xml": f"{DML}+xml", "/xl/charts/chart1.xml": f"{DML}ml.chart+xml"})
    b.defined_names = [("Tasa", "0.04", "")]
    b.external_refs = '<externalReferences><externalReference r:id="rIdE1"/></externalReferences>'
    b.workbook_rels.append(("rIdE1", "externalLink", "externalLinks/externalLink1.xml", False))
    b.parts["xl/externalLinks/externalLink1.xml"] = (
        f'{XML}<externalLink xmlns="{S}" xmlns:r="{R}"><externalBook r:id="rId1"><sheetNames><sheetName val="Tarifas"/></sheetNames>'
        '<sheetDataSet><sheetData sheetId="0"><row r="2"><cell r="B2"><v>17.2</v></cell></row><row r="3"><cell r="B3"><v>11.9</v></cell></row>'
        '<row r="4"><cell r="B4"><v>4</v></cell></row></sheetData></sheetDataSet></externalBook></externalLink>')
    b.parts["xl/externalLinks/_rels/externalLink1.xml.rels"] = rels(
        [("rId1", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath",
          "file:///C:/Compartido/Tarifas/precios_2025.xlsx", True)])
    b.overrides["/xl/externalLinks/externalLink1.xml"] = f"{SML}.externalLink+xml"
    vacia = Sheet("Borrador")
    b.sheets = [pre, vacia]
    b.write("libro_con_perdidas.xlsx", "")


if __name__ == "__main__":
    ventas()
    inventario()
    perdidas()
