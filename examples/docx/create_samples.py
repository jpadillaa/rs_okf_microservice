"""Build three artificial but realistic DOCX documents with the elements the real corpus
(datos_de_prueba/d1-d3) does not contain. The markup follows the patterns Microsoft Word
writes (styles linked to numbering, TOC inside a content control, comments with
commentsExtended, tracked changes, wps text boxes with VML fallback, charts with cached
values), so the adapter is exercised as with a real file.

muestras/informe_revisado.docx      report reviewed by several people: numbered headings,
                                    table of contents, tables with merged cells and a
                                    nested table, footnotes (one inside a table) and
                                    endnotes, comments with replies, tracked insertions,
                                    deletions, moves and format changes, hidden text,
                                    first-page and default headers, page-number footer.
muestras/guia_tecnica.docx          technical guide: subtitle, inline and display
                                    equations, text box, figures with alt text and SEQ
                                    captions, decorative image, chart with cached data,
                                    letter and roman sublists, a list that continues after
                                    a paragraph and one that restarts, external, internal
                                    and field hyperlinks, REF cross-reference, content
                                    controls, symbols, line breaks and a callout table.
muestras/anexos_con_perdidas.docx   content the converter cannot represent faithfully:
                                    SmartArt, embedded OLE object with preview, linked
                                    image, altChunk, chart without cached data and an
                                    image whose part is missing. Expected state: partial.
muestras/formato_directo.docx       document without heading styles, as written by
                                    converters such as macOS textutil: headings are bold
                                    runs with a larger size, list markers are literal text
                                    followed by a tab with a hanging indent, plus a bold
                                    label, a large non-bold sentence and a Quote paragraph.

Usage: python3 -I examples/docx/create_samples.py
"""
from pathlib import Path
from xml.sax.saxutils import escape
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo
import struct
import zlib

OUT = Path(__file__).parent / "muestras"

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
WP = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
A = "http://schemas.openxmlformats.org/drawingml/2006/main"
PIC = "http://schemas.openxmlformats.org/drawingml/2006/picture"
C = "http://schemas.openxmlformats.org/drawingml/2006/chart"
DGM = "http://schemas.openxmlformats.org/drawingml/2006/diagram"
M = "http://schemas.openxmlformats.org/officeDocument/2006/math"
MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"
WPS = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape"
W14 = "http://schemas.microsoft.com/office/word/2010/wordml"
W15 = "http://schemas.microsoft.com/office/word/2012/wordml"
V = "urn:schemas-microsoft-com:vml"
O = "urn:schemas-microsoft-com:office:office"
ADEC = "http://schemas.microsoft.com/office/drawing/2017/decorative"
NS = (f'xmlns:w="{W}" xmlns:r="{R}" xmlns:wp="{WP}" xmlns:a="{A}" xmlns:pic="{PIC}" '
      f'xmlns:c="{C}" xmlns:dgm="{DGM}" xmlns:m="{M}" xmlns:mc="{MC}" xmlns:wps="{WPS}" '
      f'xmlns:w14="{W14}" xmlns:w15="{W15}" xmlns:v="{V}" xmlns:o="{O}" mc:Ignorable="w14 w15 wps"')
XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
DATE = "2026-10-28T09:00:00Z"


def png(rgb, w=80, h=40):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
    rows = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


# --- Piezas WordprocessingML ------------------------------------------------------

def run(text, *props, rstyle=None):
    rpr = (f'<w:rStyle w:val="{rstyle}"/>' if rstyle else "") + "".join(props)
    rpr = f"<w:rPr>{rpr}</w:rPr>" if rpr else ""
    return f'<w:r>{rpr}<w:t xml:space="preserve">{escape(text)}</w:t></w:r>'


B, I, U = "<w:b/>", "<w:i/>", '<w:u w:val="single"/>'
HIDDEN = "<w:vanish/>"


def para(*content, style=None, num=None, jc=None, ppr="", pid=None):
    inner = ""
    if style:
        inner += f'<w:pStyle w:val="{style}"/>'
    if num:
        inner += f'<w:numPr><w:ilvl w:val="{num[1]}"/><w:numId w:val="{num[0]}"/></w:numPr>'
    if jc:
        inner += f'<w:jc w:val="{jc}"/>'
    inner += ppr
    attrs = f' w14:paraId="{pid}" w14:textId="77777777"' if pid else ""
    return f'<w:p{attrs}>{f"<w:pPr>{inner}</w:pPr>" if inner else ""}{"".join(content)}</w:p>'


def field(instr, result_runs):
    """Campo complejo: begin, instrucción, separador, resultado y fin."""
    return ('<w:r><w:fldChar w:fldCharType="begin"/></w:r>'
            f'<w:r><w:instrText xml:space="preserve"> {escape(instr)} </w:instrText></w:r>'
            '<w:r><w:fldChar w:fldCharType="separate"/></w:r>'
            f'{result_runs}<w:r><w:fldChar w:fldCharType="end"/></w:r>')


def hyperlink(rid, text):
    return f'<w:hyperlink r:id="{rid}" w:history="1">{run(text, rstyle="Hipervnculo")}</w:hyperlink>'


def ins(content, author="Laura Gómez", rid=100):
    return f'<w:ins w:id="{rid}" w:author="{author}" w:date="{DATE}">{content}</w:ins>'


def dele(text, author="Laura Gómez", rid=101):
    return (f'<w:del w:id="{rid}" w:author="{author}" w:date="{DATE}">'
            f'<w:r><w:delText xml:space="preserve">{escape(text)}</w:delText></w:r></w:del>')


def comment_range(cid, content):
    return (f'<w:commentRangeStart w:id="{cid}"/>{content}<w:commentRangeEnd w:id="{cid}"/>'
            f'<w:r><w:rPr><w:rStyle w:val="Refdecomentario"/></w:rPr><w:commentReference w:id="{cid}"/></w:r>')


def note_ref(kind, nid):
    style = "Refdenotaalpie" if kind == "footnote" else "Refdenotaalfinal"
    return f'<w:r><w:rPr><w:rStyle w:val="{style}"/></w:rPr><w:{kind}Reference w:id="{nid}"/></w:r>'


def cell(*paras, span=1, vmerge=None, width=2400, shade=None):
    pr = f'<w:tcW w:w="{width * span}" w:type="dxa"/>'
    if span > 1:
        pr += f'<w:gridSpan w:val="{span}"/>'
    if vmerge == "restart":
        pr += '<w:vMerge w:val="restart"/>'
    elif vmerge == "continue":
        pr += "<w:vMerge/>"
    if shade:
        pr += f'<w:shd w:val="clear" w:color="auto" w:fill="{shade}"/>'
    body = "".join(paras) or "<w:p/>"
    return f"<w:tc><w:tcPr>{pr}</w:tcPr>{body}</w:tc>"


def table(rows, cols, header=True, style="Tablaconcuadrcula"):
    grid = "".join(f'<w:gridCol w:w="{w}"/>' for w in cols)
    out = (f'<w:tbl><w:tblPr><w:tblStyle w:val="{style}"/><w:tblW w:w="0" w:type="auto"/>'
           f'<w:tblLook w:val="04A0" w:firstRow="1" w:lastRow="0" w:firstColumn="1" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/>'
           f"</w:tblPr><w:tblGrid>{grid}</w:tblGrid>")
    for i, row in enumerate(rows):
        trpr = "<w:trPr><w:tblHeader/></w:trPr>" if header and i == 0 else ""
        out += f"<w:tr>{trpr}{''.join(row)}</w:tr>"
    return out + "</w:tbl>"


def drawing(rid, docpr_id, name, descr=None, decorative=False, inline=True, cx=2743200, cy=1371600):
    descr_attr = f' descr="{escape(descr)}"' if descr else ""
    ext = ('<a:extLst><a:ext uri="{C183D7F6-B498-43B3-948B-1728B52AA6E4}">'
           f'<adec:decorative xmlns:adec="{ADEC}" val="1"/></a:ext></a:extLst>') if decorative else ""
    graphic = (f'<a:graphic><a:graphicData uri="{PIC}"><pic:pic><pic:nvPicPr>'
               f'<pic:cNvPr id="0" name="{escape(name)}"/><pic:cNvPicPr/></pic:nvPicPr>'
               f'<pic:blipFill><a:blip {rid}/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
               f'<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
               '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>')
    docpr = f'<wp:docPr id="{docpr_id}" name="{escape(name)}"{descr_attr}>{ext}</wp:docPr>'
    if inline:
        frame = (f'<wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/>'
                 f'{docpr}<wp:cNvGraphicFramePr/>{graphic}</wp:inline>')
    else:
        frame = (f'<wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="2" '
                 f'behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/>'
                 f'<wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH>'
                 f'<wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>'
                 f'<wp:extent cx="{cx}" cy="{cy}"/><wp:wrapSquare wrapText="bothSides"/>{docpr}<wp:cNvGraphicFramePr/>'
                 f'{graphic}</wp:anchor>')
    return f"<w:r><w:drawing>{frame}</w:drawing></w:r>"


def graphic_frame(docpr_id, name, uri, body, cx=4572000, cy=2743200):
    return (f'<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/>'
            f'<wp:docPr id="{docpr_id}" name="{escape(name)}"/><wp:cNvGraphicFramePr/>'
            f'<a:graphic><a:graphicData uri="{uri}">{body}</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>')


def textbox(docpr_id, paragraphs):
    content = f"<w:txbxContent>{''.join(paragraphs)}</w:txbxContent>"
    return ('<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing>'
            '<wp:anchor distT="45720" distB="45720" distL="114300" distR="114300" simplePos="0" relativeHeight="3" '
            'behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/>'
            '<wp:positionH relativeFrom="margin"><wp:align>right</wp:align></wp:positionH>'
            '<wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>'
            '<wp:extent cx="2360930" cy="1404620"/><wp:wrapSquare wrapText="bothSides"/>'
            f'<wp:docPr id="{docpr_id}" name="Cuadro de texto {docpr_id}"/><wp:cNvGraphicFramePr/>'
            f'<a:graphic><a:graphicData uri="{WPS}"><wps:wsp><wps:cNvSpPr txBox="1"/>'
            '<wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2360930" cy="1404620"/></a:xfrm>'
            '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr>'
            f'<wps:txbx>{content}</wps:txbx><wps:bodyPr rot="0" vert="horz" wrap="square"/></wps:wsp>'
            '</a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice>'
            '<mc:Fallback><w:pict><v:shape id="_x0000_s1026" type="#_x0000_t202" '
            'style="position:absolute;width:185.9pt;height:110.6pt;z-index:2">'
            f'<v:textbox>{content}</v:textbox></v:shape></w:pict></mc:Fallback>'
            '</mc:AlternateContent></w:r>')


def math_run(text):
    return f"<m:r><w:rPr><w:rFonts w:ascii='Cambria Math' w:hAnsi='Cambria Math'/></w:rPr><m:t>{escape(text)}</m:t></m:r>"


def frac(num, den):
    return f"<m:f><m:num>{num}</m:num><m:den>{den}</m:den></m:f>"


def sup(base, s):
    return f"<m:sSup><m:e>{base}</m:e><m:sup>{s}</m:sup></m:sSup>"


def sub(base, s):
    return f"<m:sSub><m:e>{base}</m:e><m:sub>{s}</m:sub></m:sSub>"


def nary(op, lo, hi, body):
    return (f'<m:nary><m:naryPr><m:chr m:val="{op}"/></m:naryPr><m:sub>{lo}</m:sub>'
            f"<m:sup>{hi}</m:sup><m:e>{body}</m:e></m:nary>")


def rad(body):
    return f'<m:rad><m:radPr><m:degHide m:val="1"/></m:radPr><m:deg/><m:e>{body}</m:e></m:rad>'


def rels(items):
    body = ""
    for rid, kind, target, external in items:
        mode = ' TargetMode="External"' if external else ""
        rtype = kind if kind.startswith("http") else f"{REL}/{kind}"
        body += f'<Relationship Id="{rid}" Type="{rtype}" Target="{escape(target)}"{mode}/>'
    return f'{XML}<Relationships xmlns="{PKG}">{body}</Relationships>'


WML = "application/vnd.openxmlformats-officedocument.wordprocessingml"
CONTENT = {
    "word/document.xml": f"{WML}.document.main+xml",
    "word/styles.xml": f"{WML}.styles+xml",
    "word/numbering.xml": f"{WML}.numbering+xml",
    "word/settings.xml": f"{WML}.settings+xml",
    "word/footnotes.xml": f"{WML}.footnotes+xml",
    "word/endnotes.xml": f"{WML}.endnotes+xml",
    "word/comments.xml": f"{WML}.comments+xml",
    "word/commentsExtended.xml": f"{WML}.commentsExtended+xml",
    "word/header1.xml": f"{WML}.header+xml",
    "word/header2.xml": f"{WML}.header+xml",
    "word/footer1.xml": f"{WML}.footer+xml",
    "word/charts/chart1.xml": "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
    "word/charts/chart2.xml": "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
    "word/diagrams/data1.xml": "application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml",
    "docProps/core.xml": "application/vnd.openxmlformats-package.core-properties+xml",
}


def content_types(parts):
    over = "".join(f'<Override PartName="/{p}" ContentType="{CONTENT[p]}"/>' for p in parts if p in CONTENT)
    return (f'{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Default Extension="png" ContentType="image/png"/>'
            '<Default Extension="htm" ContentType="application/xhtml+xml"/>'
            '<Default Extension="xlsx" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"/>'
            f"{over}</Types>")


def core(title):
    return (f'{XML}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
            'xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" '
            'xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">'
            f"<dc:title>{escape(title)}</dc:title><dc:creator>Equipo OKF (documento artificial)</dc:creator>"
            '<dcterms:created xsi:type="dcterms:W3CDTF">2026-10-28T09:00:00Z</dcterms:created></cp:coreProperties>')


def style(kind, sid, name, based="Normal", ppr="", rpr="", extra=""):
    based_xml = f'<w:basedOn w:val="{based}"/>' if based else ""
    return (f'<w:style w:type="{kind}" w:styleId="{sid}"><w:name w:val="{name}"/>{based_xml}{extra}'
            f'{f"<w:pPr>{ppr}</w:pPr>" if ppr else ""}{f"<w:rPr>{rpr}</w:rPr>" if rpr else ""}</w:style>')


def styles(heading_num=None):
    def num(level):
        return f'<w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="{heading_num}"/></w:numPr>' if heading_num else ""
    return (f'{XML}<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr>'
            '<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="es-CO"/>'
            '</w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/>'
            '</w:pPr></w:pPrDefault></w:docDefaults>'
            '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>'
            '<w:style w:type="character" w:default="1" w:styleId="Fuentedeprrafopredeter"><w:name w:val="Default Paragraph Font"/></w:style>'
            + style("paragraph", "Ttulo", "Title", ppr='<w:contextualSpacing/>', rpr='<w:sz w:val="56"/>')
            + style("paragraph", "Subttulo", "Subtitle", rpr='<w:i/><w:color w:val="5A5A5A"/><w:sz w:val="28"/>')
            + style("paragraph", "Ttulo1", "heading 1", ppr=f'<w:keepNext/>{num(0)}<w:outlineLvl w:val="0"/>',
                    rpr='<w:b/><w:color w:val="2F5496"/><w:sz w:val="32"/>')
            + style("paragraph", "Ttulo2", "heading 2", ppr=f'<w:keepNext/>{num(1)}<w:outlineLvl w:val="1"/>',
                    rpr='<w:b/><w:color w:val="2F5496"/><w:sz w:val="26"/>')
            + style("paragraph", "TtuloTDC", "TOC Heading", based="Ttulo1", ppr='<w:numPr><w:ilvl w:val="0"/><w:numId w:val="0"/></w:numPr><w:outlineLvl w:val="9"/>')
            + style("paragraph", "TDC1", "toc 1", ppr='<w:spacing w:after="100"/>')
            + style("paragraph", "TDC2", "toc 2", ppr='<w:spacing w:after="100"/><w:ind w:left="220"/>')
            + style("paragraph", "Descripcin", "caption", rpr='<w:i/><w:color w:val="44546A"/><w:sz w:val="18"/>')
            + style("paragraph", "Prrafodelista", "List Paragraph", ppr='<w:ind w:left="720"/><w:contextualSpacing/>')
            + style("paragraph", "Cdigo", "Código", rpr='<w:rFonts w:ascii="Consolas" w:hAnsi="Consolas"/><w:sz w:val="19"/>')
            + style("paragraph", "Textonotapie", "footnote text", rpr='<w:sz w:val="20"/>')
            + style("paragraph", "Textonotaalfinal", "endnote text", rpr='<w:sz w:val="20"/>')
            + style("paragraph", "Textocomentario", "annotation text", rpr='<w:sz w:val="20"/>')
            + style("paragraph", "Encabezado", "header")
            + style("paragraph", "Piedepgina", "footer")
            + style("character", "Refdenotaalpie", "footnote reference", based="Fuentedeprrafopredeter", rpr='<w:vertAlign w:val="superscript"/>')
            + style("character", "Refdenotaalfinal", "endnote reference", based="Fuentedeprrafopredeter", rpr='<w:vertAlign w:val="superscript"/>')
            + style("character", "Refdecomentario", "annotation reference", based="Fuentedeprrafopredeter", rpr='<w:sz w:val="16"/>')
            + style("character", "Hipervnculo", "Hyperlink", based="Fuentedeprrafopredeter", rpr='<w:color w:val="0563C1"/><w:u w:val="single"/>')
            + style("character", "Textoennegrita", "Strong", based="Fuentedeprrafopredeter", rpr="<w:b/>")
            + style("table", "Tablaconcuadrcula", "Table Grid", based=None)
            + "</w:styles>")


def lvl(ilvl, fmt, text, start=1, indent=720):
    return (f'<w:lvl w:ilvl="{ilvl}"><w:start w:val="{start}"/><w:numFmt w:val="{fmt}"/>'
            f'<w:lvlText w:val="{text}"/><w:lvlJc w:val="left"/>'
            f'<w:pPr><w:ind w:left="{indent * (ilvl + 1)}" w:hanging="360"/></w:pPr></w:lvl>')


def numbering(abstracts, nums):
    body = "".join(f'<w:abstractNum w:abstractNumId="{aid}"><w:multiLevelType w:val="hybridMultilevel"/>{levels}</w:abstractNum>'
                   for aid, levels in abstracts)
    for nid, aid, override in nums:
        body += f'<w:num w:numId="{nid}"><w:abstractNumId w:val="{aid}"/>{override}</w:num>'
    return f'{XML}<w:numbering xmlns:w="{W}">{body}</w:numbering>'


BULLETS = "".join(lvl(i, "bullet", ch) for i, ch in enumerate(["\uf0b7", "o", "\uf0a7"]))
SETTINGS = f'{XML}<w:settings xmlns:w="{W}"><w:trackRevisions/><w:defaultTabStop w:val="708"/></w:settings>'


def document(body, sect=""):
    return (f"{XML}<w:document {NS}><w:body>{body}<w:sectPr>{sect}"
            '<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1417" w:right="1701" w:bottom="1417" w:left="1701" '
            'w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>')


def notes(kind, items):
    """Partes de notas con los separadores que Word escribe siempre."""
    style = "Textonotapie" if kind == "footnote" else "Textonotaalfinal"
    ref = "Refdenotaalpie" if kind == "footnote" else "Refdenotaalfinal"
    out = (f'{XML}<w:{kind}s {NS}>'
           f'<w:{kind} w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:{kind}>'
           f'<w:{kind} w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:{kind}>')
    for nid, paragraphs in items:
        body = ""
        for i, text in enumerate(paragraphs):
            mark = f'<w:r><w:rPr><w:rStyle w:val="{ref}"/></w:rPr><w:{kind}Ref/></w:r>' if i == 0 else ""
            body += para(mark, run((" " if i == 0 else "") + text), style=style)
        out += f'<w:{kind} w:id="{nid}">{body}</w:{kind}>'
    return out + f"</w:{kind}s>"


def write(name, parts):
    OUT.mkdir(parents=True, exist_ok=True)
    with ZipFile(OUT / name, "w", ZIP_DEFLATED) as z:
        for path, data in parts:
            info = ZipInfo(path, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            z.writestr(info, data)


ROOT_RELS = rels([("rId1", "officeDocument", "word/document.xml", False),
                  ("rId2", "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties",
                   "docProps/core.xml", False)])


# --- 1. Informe revisado ----------------------------------------------------------

def informe():
    toc_entries = [("_Toc1", "1. Introducción", "1", "TDC1"), ("_Toc2", "1.1. Alcance de la evaluación", "1", "TDC2"),
                   ("_Toc3", "2. Resultados", "2", "TDC1"), ("_Toc4", "3. Conclusiones", "3", "TDC1")]
    toc = ""
    for i, (anchor, text, page, st) in enumerate(toc_entries):
        begin = ('<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> TOC \\o "1-3" \\h \\z \\u </w:instrText></w:r>'
                 '<w:r><w:fldChar w:fldCharType="separate"/></w:r>') if i == 0 else ""
        entry = (f'<w:hyperlink w:anchor="{anchor}" w:history="1">{run(text)}<w:r><w:tab/></w:r>'
                 f'{field(f"PAGEREF {anchor} \\h", run(page))}</w:hyperlink>')
        end = '<w:r><w:fldChar w:fldCharType="end"/></w:r>' if i == len(toc_entries) - 1 else ""
        toc += para(begin + entry + end, style=st)
    toc_block = ('<w:sdt><w:sdtPr><w:id w:val="-1"/><w:docPartObj><w:docPartGallery w:val="Table of Contents"/>'
                 '<w:docPartUnique/></w:docPartObj></w:sdtPr><w:sdtContent>'
                 + para(run("Contenido"), style="TtuloTDC") + toc + "</w:sdtContent></w:sdt>")

    def bm(name, content):
        return f'<w:bookmarkStart w:id="{name[-1]}" w:name="{name}"/>{content}<w:bookmarkEnd w:id="{name[-1]}"/>'

    head = lambda text, level, toc_id: para(bm(toc_id, run(text)), style=f"Ttulo{level}")
    shade = "D9E2F3"
    metrics = table([
        [cell(para(run("Componente", B)), vmerge="restart", shade=shade),
         cell(para(run("Métrica", B)), vmerge="restart", shade=shade),
         cell(para(run("Valor por corpus", B), jc="center"), span=2, shade=shade)],
        [cell(vmerge="continue", shade=shade), cell(vmerge="continue", shade=shade),
         cell(para(run("Normativo", B)), shade=shade), cell(para(run("Técnico", B)), shade=shade)],
        [cell(para(run("Recuperador")), vmerge="restart"), cell(para(run("Precisión del contexto"))),
         cell(para(run("0,82"))), cell(para(run("0,74")))],
        [cell(para(run("")), vmerge="continue"), cell(para(run("Exhaustividad del contexto"), note_ref("footnote", 2))),
         cell(para(run("0,77"))), cell(para(run("0,69")))],
        [cell(para(run("Generador")), vmerge="restart"), cell(para(run("Fidelidad"))),
         cell(para(run("0,91"))), cell(para(run("0,88")))],
        [cell(para(run("")), vmerge="continue"), cell(para(run("Relevancia de la respuesta"))),
         cell(para(run("0,86"))), cell(para(run("0,81")))],
    ], [2200, 2600, 1900, 1900])
    nested = table([
        [cell(para(run("Riesgo", B))), cell(para(run("Mitigación", B)))],
        [cell(para(run("Alucinaciones en respuestas largas"))),
         cell(para(run("Verificación por fragmentos:")),
              table([[cell(para(run("Paso", B)), width=1200), cell(para(run("Responsable", B)), width=1800)],
                     [cell(para(run("Citas obligatorias")), width=1200), cell(para(run("Generador")), width=1800)],
                     [cell(para(run("Juez LLM")), width=1200), cell(para(run("Evaluación")), width=1800)]],
                    [1200, 1800], header=False))],
        [cell(para(run("Documentos desactualizados"))), cell(para(run("Versionado de la base y filtros por fecha")))],
    ], [3600, 4500])

    body = (para(run("Informe de evaluación del asistente RAG"), style="Ttulo")
            + para(run("Versión revisada por el comité académico, octubre de 2026"), style="Subttulo")
            + toc_block
            + head("Introducción", 1, "_Toc1")
            + para(run("Este informe resume la evaluación del asistente de preguntas sobre los reglamentos y la "
                       "documentación técnica de la maestría. La evaluación combinó métricas automáticas con una "
                       "revisión manual de ciento veinte preguntas reales"), note_ref("footnote", 1),
                   run(". Los resultados orientan las decisiones de la siguiente iteración del sistema."))
            + para(run("El comité solicitó "), ins(run("explícitamente ")), run("que el informe distinguiera entre "
                   "fallas de recuperación y fallas de generación, porque cada una exige una intervención distinta."))
            + head("Alcance de la evaluación", 2, "_Toc2")
            + para(run("Se evaluaron dos corpus: el normativo, con "), dele("cuarenta y dos", rid=102),
                   ins(run("cuarenta y cinco"), rid=103), run(" documentos vigentes, y el técnico, con guías de "
                   "laboratorio y notas de clase. Las preguntas se tomaron del foro del curso"),
                   note_ref("endnote", 1), run("."))
            + para(comment_range(0, run("La muestra excluye preguntas en inglés")),
                   run(", que representan menos del cinco por ciento del total."))
            + para(run("Nota interna: repetir el muestreo con el corpus de 2027.", HIDDEN))
            + head("Resultados", 1, "_Toc3")
            + para(run("La tabla siguiente presenta las métricas por componente y corpus. Los valores del corpus "
                       "técnico son menores en todas las métricas, en especial en exhaustividad."))
            + metrics
            + para(run("Tabla 1. Métricas por componente y corpus.", I), style="Descripcin")
            + para(run("Los principales riesgos identificados y su mitigación se resumen a continuación:"))
            + nested
            + para(f'<w:moveFromRangeStart w:id="200" w:author="Andrés Ruiz" w:date="{DATE}" w:name="move1"/>'
                   f'<w:moveFrom w:id="201" w:author="Andrés Ruiz" w:date="{DATE}">{run("La latencia media fue de 2,4 segundos. ")}</w:moveFrom>'
                   '<w:moveFromRangeEnd w:id="200"/>'
                   + run("El costo por consulta se mantuvo por debajo del presupuesto previsto."))
            + para(comment_range(1, run("El costo por consulta")), run(" se calculó con las tarifas publicadas en "
                   "septiembre de 2026 y no incluye la infraestructura de observabilidad."),
                   f'<w:moveToRangeStart w:id="202" w:author="Andrés Ruiz" w:date="{DATE}" w:name="move1"/>'
                   f'<w:moveTo w:id="203" w:author="Andrés Ruiz" w:date="{DATE}">{run(" La latencia media fue de 2,4 segundos.")}</w:moveTo>'
                   '<w:moveToRangeEnd w:id="202"/>')
            + para(f'<w:r><w:rPr><w:b/><w:rPrChange w:id="204" w:author="Laura Gómez" w:date="{DATE}"><w:rPr/></w:rPrChange></w:rPr>'
                   '<w:t xml:space="preserve">Hallazgo principal:</w:t></w:r>',
                   run(" la mayoría de las respuestas incorrectas se originan en fragmentos mal recuperados."))
            + para(ins(run("Se añadió este párrafo durante la revisión para dejar constancia del cambio de criterio."), rid=105),
                   ppr=f'<w:rPr><w:ins w:id="106" w:author="Laura Gómez" w:date="{DATE}"/></w:rPr>')
            + head("Conclusiones", 1, "_Toc4")
            + para(run("El asistente es apto para el corpus normativo. Para el técnico se recomienda mejorar la "
                       "fragmentación y añadir reordenamiento antes de su despliegue"), note_ref("endnote", 2), run("."))
            + para(run("Se recomienda:"))
            + para(run("Ajustar el tamaño de los fragmentos del corpus técnico."), num=(1, 0))
            + para(run("Incorporar un reordenador entrenado con preguntas del curso."), num=(1, 0))
            + para(run("Repetir la evaluación con el nuevo corpus."), num=(1, 0)))
    sect = ('<w:headerReference w:type="default" r:id="rIdH1"/><w:headerReference w:type="first" r:id="rIdH2"/>'
            '<w:footerReference w:type="default" r:id="rIdF1"/><w:titlePg/>')
    header1 = f'{XML}<w:hdr {NS}>{para(run("Informe de evaluación — uso interno"), style="Encabezado", jc="right")}</w:hdr>'
    header2 = (f'{XML}<w:hdr {NS}>{para(drawing('r:embed="rIdLogo"', 10, "Logo", descr="Logotipo del programa de maestría"), style="Encabezado")}</w:hdr>')
    footer1 = (f'{XML}<w:ftr {NS}>' + para(run("Página "), field("PAGE \\* MERGEFORMAT", run("2")), run(" de "),
                                           field("NUMPAGES", run("3")), style="Piedepgina", jc="center") + "</w:ftr>")
    footnotes = notes("footnote", [
        (1, ["Las preguntas se anonimizaron antes del análisis.", "El protocolo de anonimización está en el anexo B."]),
        (2, ["Calculada sobre los cinco primeros fragmentos recuperados."]),
    ])
    endnotes = notes("endnote", [
        (1, ["Foro del curso MISW4411, periodo 2026-20."]),
        (2, ["Véase el plan de trabajo de la iteración 4."]),
    ])
    comments = (f'{XML}<w:comments {NS}>'
                f'<w:comment w:id="0" w:author="Marta Díaz" w:date="2026-10-27T15:10:00Z" w:initials="MD">'
                + para('<w:r><w:rPr><w:rStyle w:val="Refdecomentario"/></w:rPr><w:annotationRef/></w:r>',
                       run("¿Por qué se excluyen? Conviene justificarlo."), style="Textocomentario", pid="0A000001")
                + '</w:comment><w:comment w:id="1" w:author="Andrés Ruiz" w:date="2026-10-27T16:00:00Z" w:initials="AR">'
                + para('<w:r><w:rPr><w:rStyle w:val="Refdecomentario"/></w:rPr><w:annotationRef/></w:r>',
                       run("Indicar la fuente de las tarifas."), style="Textocomentario", pid="0A000002")
                + '</w:comment><w:comment w:id="2" w:author="Laura Gómez" w:date="2026-10-28T08:30:00Z" w:initials="LG">'
                + para('<w:r><w:rPr><w:rStyle w:val="Refdecomentario"/></w:rPr><w:annotationRef/></w:r>',
                       run("Añadida en la nota del anexo de costos."), style="Textocomentario", pid="0A000003")
                + "</w:comment></w:comments>")
    comments_ext = (f'{XML}<w15:commentsEx {NS}><w15:commentEx w15:paraId="0A000001" w15:done="0"/>'
                    '<w15:commentEx w15:paraId="0A000002" w15:done="1"/>'
                    '<w15:commentEx w15:paraId="0A000003" w15:paraIdParent="0A000002" w15:done="0"/></w15:commentsEx>')
    nums = numbering([(0, lvl(0, "decimal", "%1.") + lvl(1, "decimal", "%1.%2.")), (1, BULLETS)],
                     [(1, 1, ""), (5, 0, "")])
    parts = ["word/document.xml", "word/styles.xml", "word/numbering.xml", "word/settings.xml", "word/footnotes.xml",
             "word/endnotes.xml", "word/comments.xml", "word/commentsExtended.xml", "word/header1.xml",
             "word/header2.xml", "word/footer1.xml", "docProps/core.xml"]
    write("informe_revisado.docx", [
        ("[Content_Types].xml", content_types(parts)),
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", document(body, sect)),
        ("word/_rels/document.xml.rels", rels([
            ("rId1", "styles", "styles.xml", False), ("rId2", "numbering", "numbering.xml", False),
            ("rId3", "settings", "settings.xml", False), ("rId4", "footnotes", "footnotes.xml", False),
            ("rId5", "endnotes", "endnotes.xml", False), ("rId6", "comments", "comments.xml", False),
            ("rId7", "http://schemas.microsoft.com/office/2011/relationships/commentsExtended", "commentsExtended.xml", False),
            ("rIdH1", "header", "header1.xml", False), ("rIdH2", "header", "header2.xml", False),
            ("rIdF1", "footer", "footer1.xml", False)])),
        ("word/_rels/header2.xml.rels", rels([("rIdLogo", "image", "media/image1.png", False)])),
        ("word/styles.xml", styles(heading_num=5)),
        ("word/numbering.xml", nums),
        ("word/settings.xml", SETTINGS),
        ("word/footnotes.xml", footnotes),
        ("word/endnotes.xml", endnotes),
        ("word/comments.xml", comments),
        ("word/commentsExtended.xml", comments_ext),
        ("word/header1.xml", header1),
        ("word/header2.xml", header2),
        ("word/footer1.xml", footer1),
        ("word/media/image1.png", png((20, 60, 120))),
        ("docProps/core.xml", core("Informe de evaluación del asistente RAG")),
    ])


# --- 2. Guía técnica --------------------------------------------------------------

def chart_xml(cached=True):
    def cache(kind, values):
        if not cached:
            return ""
        pts = "".join(f'<c:pt idx="{i}"><c:v>{v}</c:v></c:pt>' for i, v in enumerate(values))
        fmt = "<c:formatCode>General</c:formatCode>" if kind == "num" else ""
        return f'<c:{kind}Cache>{fmt}<c:ptCount val="{len(values)}"/>{pts}</c:{kind}Cache>'
    title = ('<c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>Precisión por tamaño de fragmento</a:t></a:r></a:p>'
             '</c:rich></c:tx><c:overlay val="0"/></c:title>') if cached else ""
    return (f'{XML}<c:chartSpace xmlns:c="{C}" xmlns:a="{A}" xmlns:r="{R}"><c:chart>{title}<c:plotArea><c:barChart>'
            '<c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/>'
            f'<c:tx><c:strRef><c:f>Hoja1!$B$1</c:f>{cache("str", ["Precisión"])}</c:strRef></c:tx>'
            f'<c:cat><c:strRef><c:f>Hoja1!$A$2:$A$4</c:f>{cache("str", ["256 tokens", "512 tokens", "1024 tokens"])}</c:strRef></c:cat>'
            f'<c:val><c:numRef><c:f>Hoja1!$B$2:$B$4</c:f>{cache("num", ["0.71", "0.82", "0.76"])}</c:numRef></c:val>'
            '</c:ser><c:axId val="1"/><c:axId val="2"/></c:barChart></c:plotArea></c:chart></c:chartSpace>')


def chart_frame(docpr_id, rid):
    return graphic_frame(docpr_id, f"Gráfico {docpr_id}", C, f'<c:chart r:id="{rid}"/>')


def guia():
    inline_eq = "<m:oMath>" + sub(math_run("F"), math_run("1")) + math_run("=") + frac(math_run("2PR"), math_run("P+R")) + "</m:oMath>"
    display_eq = ("<m:oMathPara><m:oMath>" + math_run("cos(q,d)=")
                  + frac(nary("∑", math_run("i=1"), math_run("n"), sub(math_run("q"), math_run("i")) + sub(math_run("d"), math_run("i"))),
                         rad(nary("∑", math_run("i=1"), math_run("n"), sup(sub(math_run("q"), math_run("i")), math_run("2"))))
                         + rad(nary("∑", math_run("i=1"), math_run("n"), sup(sub(math_run("d"), math_run("i")), math_run("2")))))
                  + "</m:oMath></m:oMathPara>")
    def caption(n, text, bookmark):
        return para(f'<w:bookmarkStart w:id="9" w:name="{bookmark}"/>', run("Figura "),
                    field("SEQ Figura \\* ARABIC", run(str(n))), '<w:bookmarkEnd w:id="9"/>', run(f". {text}"),
                    style="Descripcin")
    callout = table([[cell(para(run("Importante: ", B), run("los índices deben reconstruirse cada vez que cambia el modelo de "
                                                              "embeddings; los vectores de modelos distintos no son comparables.")),
                           width=8400, shade="FFF2CC")]], [8400], header=False)
    body = (para(run("Guía técnica de indexación vectorial"), style="Ttulo")
            + para(run("Laboratorio 3 · Recuperación densa"), style="Subttulo")
            + para(run("Esta guía acompaña el laboratorio 3. Explica cómo se construye un índice vectorial, cómo se mide su "
                       "calidad y qué decisiones de configuración afectan más los resultados del sistema de recuperación."))
            + para(run("Fundamentos"), style="Ttulo1")
            + para(run("La similitud entre la consulta y cada fragmento se mide con el coseno de sus vectores:"))
            + para(display_eq)
            + para(run("La calidad del recuperador se resume con la medida "), inline_eq,
                   run(", que combina precisión (P) y exhaustividad (R)."))
            + para(textbox(20, [para(run("Recuerde", B)),
                                para(run("Normalice los vectores antes de indexarlos si usa producto punto.")),
                                para(run("Use el mismo modelo para la consulta y los documentos."), num=(3, 0))]),
                   run("Los vectores se almacenan en un índice aproximado que permite búsquedas rápidas a costa de una "
                       "pequeña pérdida de exhaustividad. La figura 1 muestra el flujo completo."))
            + para(drawing('r:embed="rIdImg1"', 21, "Imagen 21", descr="Flujo de indexación: documentos, fragmentación, "
                           "embeddings e índice vectorial"), jc="center")
            + caption(1, "Flujo de indexación.", "_Ref1")
            + para(drawing('r:embed="rIdImg2"', 22, "Imagen 22", decorative=True, cx=914400, cy=91440), jc="center")
            + para(run("Configuración del índice"), style="Ttulo1")
            + para(run("Siga estos pasos para crear el índice:"))
            + para(run("Prepare el corpus."), num=(1, 0))
            + para(run("Elimine duplicados."), num=(1, 1))
            + para(run("Normalice la codificación a UTF-8."), num=(1, 1))
            + para(run("Verifique los metadatos obligatorios."), num=(1, 2))
            + para(run("Fragmente los documentos."), num=(1, 0))
            + para(run("Antes de continuar, revise que cada fragmento conserve su documento de origen."))
            + para(run("Calcule los embeddings."), num=(1, 0))
            + para(run("Cree el índice y verifique su tamaño."), num=(1, 0))
            + para(run("Para la verificación, repita la numeración desde el principio:"))
            + para(run("Ejecute las consultas de prueba."), num=(2, 0))
            + para(run("Compare los resultados con la referencia."), num=(2, 0))
            + para(run("El tamaño del fragmento influye en la precisión, como muestra el gráfico:"))
            + para(chart_frame(23, "rIdChart1"))
            + para(run("El código mínimo para crear la colección es:"))
            + para(run("coleccion = cliente.create_collection("), '<w:r><w:br/></w:r>',
                   run('    name="laboratorio3", metric="cosine")'), style="Cdigo")
            + callout
            + para(run("Referencias y enlaces"), style="Ttulo1")
            + para(run("La documentación oficial está en "), hyperlink("rIdLink1", "el sitio del proveedor"),
                   run(". Véase también la "), f'<w:hyperlink w:anchor="_Ref1" w:history="1">{run("Figura 1")}</w:hyperlink>',
                   run(" y el "), field('HYPERLINK "https://example.org/okf/laboratorio3" \\o "Enunciado"', run("enunciado del laboratorio", rstyle="Hipervnculo")),
                   run(", donde se repite el flujo de la "), field("REF _Ref1 \\h", run("Figura 1")), run("."))
            + para(run("Responsable del laboratorio: "),
                   '<w:sdt><w:sdtPr><w:alias w:val="Responsable"/><w:tag w:val="responsable"/><w:text/></w:sdtPr>'
                   f'<w:sdtContent>{run("Carolina Pérez")}</w:sdtContent></w:sdt>',
                   '<w:r><w:tab/></w:r>', run("Entrega: semana 6 "),
                   '<w:r><w:sym w:font="Wingdings" w:char="F0FC"/></w:r>')
            + '<w:sdt><w:sdtPr><w:alias w:val="Observaciones"/><w:tag w:val="observaciones"/></w:sdtPr><w:sdtContent>'
            + para(run("Las observaciones del laboratorio se registran en el repositorio del curso."))
            + para(run("Cada grupo entrega un informe breve con sus métricas."))
            + "</w:sdtContent></w:sdt>")
    nums = numbering([(0, lvl(0, "decimal", "%1.") + lvl(1, "lowerLetter", "%2)") + lvl(2, "lowerRoman", "%3.")),
                      (1, BULLETS)],
                     [(1, 0, ""), (2, 0, '<w:lvlOverride w:ilvl="0"><w:startOverride w:val="1"/></w:lvlOverride>'),
                      (3, 1, "")])
    parts = ["word/document.xml", "word/styles.xml", "word/numbering.xml", "word/settings.xml",
             "word/charts/chart1.xml", "docProps/core.xml"]
    write("guia_tecnica.docx", [
        ("[Content_Types].xml", content_types(parts)),
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", document(body)),
        ("word/_rels/document.xml.rels", rels([
            ("rId1", "styles", "styles.xml", False), ("rId2", "numbering", "numbering.xml", False),
            ("rId3", "settings", "settings.xml", False),
            ("rIdImg1", "image", "media/image1.png", False), ("rIdImg2", "image", "media/image2.png", False),
            ("rIdChart1", "chart", "charts/chart1.xml", False),
            ("rIdLink1", "hyperlink", "https://example.org/docs/indices-vectoriales", True)])),
        ("word/styles.xml", styles()),
        ("word/numbering.xml", nums),
        ("word/settings.xml", SETTINGS.replace("<w:trackRevisions/>", "")),
        ("word/charts/chart1.xml", chart_xml(cached=True)),
        ("word/media/image1.png", png((40, 140, 90))),
        ("word/media/image2.png", png((200, 200, 200), 120, 4)),
        ("docProps/core.xml", core("Guía técnica de indexación vectorial")),
    ])


# --- 3. Anexos con pérdidas -------------------------------------------------------

def anexos():
    smartart = graphic_frame(30, "Diagrama 30", DGM,
                             '<dgm:relIds r:dm="rIdDm" r:lo="rIdLo" r:qs="rIdQs" r:cs="rIdCs"/>')
    ole = ('<w:r><w:object w:dxaOrig="2880" w:dyaOrig="1440">'
           '<v:shape id="_x0000_i1025" type="#_x0000_t75" style="width:144pt;height:72pt" o:ole="">'
           '<v:imagedata r:id="rIdOlePreview" o:title=""/></v:shape>'
           '<o:OLEObject Type="Embed" ProgID="Excel.Sheet.12" ShapeID="_x0000_i1025" DrawAspect="Content" '
           'ObjectID="_1790000001" r:id="rIdOle"/></w:object></w:r>')
    filler = ("El anexo reúne material de apoyo de la evaluación. Algunos elementos se crearon con herramientas "
              "externas y se insertaron en el documento sin convertirlos a texto, por lo que su contenido depende "
              "de la aplicación que los creó. ")
    body = (para(run("Anexos de la evaluación"), style="Ttulo")
            + para(run("Anexo A. Proceso de evaluación"), style="Ttulo1")
            + para(run(filler * 3))
            + para(run("El proceso de evaluación se resume en el siguiente diagrama:"))
            + para(smartart)
            + para(run("Los datos de costos se adjuntan como hoja de cálculo incrustada:"))
            + para(ole)
            + para(run("Anexo B. Material externo"), style="Ttulo1")
            + para(run(filler * 2))
            + para(run("El diagrama de arquitectura se enlaza desde la carpeta compartida:"))
            + para(drawing('r:link="rIdLinked"', 31, "Imagen vinculada", descr="Arquitectura del sistema"))
            + '<w:altChunk r:id="rIdAlt"/>'
            + para(run("El gráfico de latencias perdió sus datos al copiarse desde otra aplicación:"))
            + para(chart_frame(32, "rIdChart2"))
            + para(run("La captura de la consola de observabilidad no se conservó en el archivo:"))
            + para(drawing('r:embed="rIdMissing"', 33, "Captura", descr="Consola de observabilidad")))
    data = (f'{XML}<dgm:dataModel xmlns:dgm="{DGM}" xmlns:a="{A}"><dgm:ptLst>'
            + "".join(f'<dgm:pt modelId="{i}"><dgm:t><a:bodyPr/><a:p><a:r><a:t>{t}</a:t></a:r></a:p></dgm:t></dgm:pt>'
                      for i, t in enumerate(["Preguntas", "Recuperación", "Generación", "Juicio"], 1))
            + "</dgm:ptLst></dgm:dataModel>")
    parts = ["word/document.xml", "word/styles.xml", "word/settings.xml", "word/charts/chart2.xml",
             "word/diagrams/data1.xml", "docProps/core.xml"]
    diagram = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/diagram"
    write("anexos_con_perdidas.docx", [
        ("[Content_Types].xml", content_types(parts)),
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", document(body)),
        ("word/_rels/document.xml.rels", rels([
            ("rId1", "styles", "styles.xml", False), ("rId3", "settings", "settings.xml", False),
            ("rIdDm", f"{diagram}Data", "diagrams/data1.xml", False),
            ("rIdLo", f"{diagram}Layout", "diagrams/layout1.xml", False),
            ("rIdQs", f"{diagram}QuickStyle", "diagrams/quickStyle1.xml", False),
            ("rIdCs", f"{diagram}Colors", "diagrams/colors1.xml", False),
            ("rIdOlePreview", "image", "media/image1.png", False),
            ("rIdOle", "package", "embeddings/Microsoft_Excel_Worksheet.xlsx", False),
            ("rIdLinked", "image", "file:///C:/Compartida/arquitectura.png", True),
            ("rIdAlt", "aFChunk", "afchunk.htm", False),
            ("rIdChart2", "chart", "charts/chart2.xml", False),
            ("rIdMissing", "image", "media/image9.png", False)])),
        ("word/styles.xml", styles()),
        ("word/settings.xml", SETTINGS.replace("<w:trackRevisions/>", "")),
        ("word/diagrams/data1.xml", data),
        ("word/charts/chart2.xml", chart_xml(cached=False)),
        ("word/media/image1.png", png((230, 230, 230))),
        ("word/embeddings/Microsoft_Excel_Worksheet.xlsx", b"PK\x05\x06" + b"\x00" * 18),
        ("word/afchunk.htm", "<html><body><p>Tabla de costos importada desde la intranet.</p></body></html>"),
        ("docProps/core.xml", core("Anexos de la evaluación")),
    ])


# --- 4. Formato directo -------------------------------------------------------------

def formato_directo():
    def r(text, size=24, bold=False):
        b = "<w:b/>" if bold else ""
        return (f'<w:r><w:rPr><w:rFonts w:ascii="Times" w:hAnsi="Times"/>{b}<w:sz w:val="{size}"/></w:rPr>'
                f'<w:t xml:space="preserve">{escape(text)}</w:t></w:r>')

    def head(text, size):
        return para(r(text, size, bold=True), ppr='<w:spacing w:after="280"/>')

    def item(marker, text, level=0):
        left = 720 * (level + 1)
        mark = ('<w:r><w:rPr><w:rFonts w:ascii="Times" w:hAnsi="Times"/><w:sz w:val="24"/></w:rPr>'
                f'<w:tab/><w:t>{escape(marker)}</w:t><w:tab/></w:r>')
        return para(mark, r(text), ppr=f'<w:ind w:left="{left}" w:hanging="{left}"/>')

    text = para
    body = (head("Manual de operación del laboratorio", 48)
            + text(r("Este manual describe los procedimientos de seguridad, mantenimiento y registro del laboratorio de "
                     "cómputo. Se aplica a todo el personal y a los estudiantes que usan los equipos en horario extendido."))
            + head("Acceso y horarios", 36)
            + text(r("El laboratorio abre de lunes a sábado. El acceso fuera de horario requiere autorización escrita de la "
                     "coordinación y registro en la bitácora digital."))
            + item("1", "Solicite la autorización con dos días de anticipación.")
            + item("2", "Registre su ingreso con el carné.")
            + item("◦", "Si el lector falla, use la bitácora de papel.", 1)
            + item("◦", "Informe la falla a soporte.", 1)
            + item("3", "Cierre la sesión al salir.")
            + head("Horario extendido", 28)
            + text(r("Requisitos:", bold=True))
            + item("a)", "Tener matrícula vigente.")
            + item("b)", "Haber aprobado la inducción de seguridad.")
            + text(r("El horario extendido se publica cada semestre en la cartelera del edificio y en el portal de "
                     "soporte, junto con el calendario de mantenimiento."))
            + head("Mantenimiento de equipos", 36)
            + text(r("El equipo de soporte realiza mantenimiento preventivo cada mes. Consulte el calendario antes de "
                     "reservar una sala."))
            + item("•", "Limpieza de filtros y ventiladores.")
            + item("•", "Actualización del sistema operativo y del antivirus.")
            + item("•", "Verificación de las copias de seguridad.")
            + text(r("Nadie trabaja solo en el laboratorio", size=36))
            + head("Normas de seguridad", 36)
            + text(r("Está prohibido consumir alimentos o bebidas dentro del laboratorio. Los equipos no deben "
                     "desconectarse de la red eléctrica regulada ni moverse de su puesto sin autorización."))
            + para(r("En caso de emergencia, siga las rutas de evacuación señalizadas y diríjase al punto de encuentro."),
                   style="Cita"))
    parts = ["word/document.xml", "word/styles.xml"]
    write("formato_directo.docx", [
        ("[Content_Types].xml", content_types(parts)),
        ("_rels/.rels", rels([("rId1", "officeDocument", "word/document.xml", False)])),
        ("word/document.xml", document(body)),
        ("word/_rels/document.xml.rels", rels([("rId1", "styles", "styles.xml", False)])),
        ("word/styles.xml", styles().replace(
            "</w:styles>", style("paragraph", "Cita", "Quote", rpr="<w:i/>") + "</w:styles>")),
    ])


if __name__ == "__main__":
    informe()
    guia()
    anexos()
    formato_directo()
    print("informe_revisado.docx, guia_tecnica.docx, anexos_con_perdidas.docx y formato_directo.docx generados en", OUT)
