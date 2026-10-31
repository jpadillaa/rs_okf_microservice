"""Build the two small DOCX fixtures used by the DOCX acceptance tests.

breve.docx       short document without headings: a single text concept.
estructura.docx  title, two heading-1 sections and the auxiliary content the profile
                 asks to keep: inline formatting, hyperlink, footnote, OMML equation,
                 numbered list with a nested bullet, table with merged cells, image
                 with alt text, text box (wps choice with VML fallback), tracked
                 insertion and deletion, review comment, hidden text, a PAGE field and
                 a page header.

Usage: python3 -I examples/docx/create_fixtures.py
"""
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo
import struct
import zlib

OUT = Path(__file__).parent
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"
WP = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
A = "http://schemas.openxmlformats.org/drawingml/2006/main"
PIC = "http://schemas.openxmlformats.org/drawingml/2006/picture"
M = "http://schemas.openxmlformats.org/officeDocument/2006/math"
MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"
WPS = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape"
V = "urn:schemas-microsoft-com:vml"
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
NS = (f'xmlns:w="{W}" xmlns:r="{R}" xmlns:wp="{WP}" xmlns:a="{A}" xmlns:pic="{PIC}" '
      f'xmlns:m="{M}" xmlns:mc="{MC}" xmlns:wps="{WPS}" xmlns:v="{V}" mc:Ignorable="wps"')


def png():
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
    w, h = 60, 30
    rows = b"".join(b"\x00" + bytes((36, 110, 190)) * w for _ in range(h))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


def rels(items):
    body = "".join(
        f'<Relationship Id="{i}" Type="{REL}/{t}" Target="{target}"{" TargetMode=\"External\"" if ext else ""}/>'
        for i, t, target, ext in items)
    return f'<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{PKG}">{body}</Relationships>'


def content_types(extra):
    over = "".join(f'<Override PartName="/{p}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{t}+xml"/>'
                   for p, t in extra)
    return ('<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/>'
            '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
            f'{over}</Types>')


STYLES = (f'<?xml version="1.0" encoding="UTF-8"?><w:styles xmlns:w="{W}">'
          '<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>'
          '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>'
          '<w:style w:type="paragraph" w:styleId="Ttulo"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:rPr><w:sz w:val="48"/></w:rPr></w:style>'
          '<w:style w:type="paragraph" w:styleId="Ttulo1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/>'
          '<w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>'
          '</w:styles>')

NUMBERING = (f'<?xml version="1.0" encoding="UTF-8"?><w:numbering xmlns:w="{W}">'
             '<w:abstractNum w:abstractNumId="0">'
             '<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl>'
             '<w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="•"/></w:lvl>'
             '</w:abstractNum>'
             '<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/><w:lvlText w:val="•"/></w:lvl></w:abstractNum>'
             '<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>'
             '<w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>'
             '</w:numbering>')


def run(text, props=""):
    rpr = f"<w:rPr>{props}</w:rPr>" if props else ""
    return f'<w:r>{rpr}<w:t xml:space="preserve">{text}</w:t></w:r>'


def para(content, style=None, num=None):
    ppr = ""
    if style or num:
        s = f'<w:pStyle w:val="{style}"/>' if style else ""
        n = f'<w:numPr><w:ilvl w:val="{num[1]}"/><w:numId w:val="{num[0]}"/></w:numPr>' if num else ""
        ppr = f"<w:pPr>{s}{n}</w:pPr>"
    return f"<w:p>{ppr}{content}</w:p>"


def document(body):
    return (f'<?xml version="1.0" encoding="UTF-8"?><w:document {NS}><w:body>{body}'
            '<w:sectPr><w:headerReference w:type="default" r:id="rIdHeader"/></w:sectPr></w:body></w:document>')


def write(name, parts):
    with ZipFile(OUT / name, "w", ZIP_DEFLATED) as z:
        for path, data in parts:
            info = ZipInfo(path, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            z.writestr(info, data)


ROOT_RELS = rels([("rId1", "officeDocument", "word/document.xml", False)])


def breve():
    body = (para(run("Nota breve", "<w:b/>"))
            + para(run("Una nota corta sin encabezados que forma un solo concepto."))
            + para(run("Primera idea"), num=(2, 0))
            + para(run("Segunda idea"), num=(2, 0))
            + para(""))
    write("breve.docx", [
        ("[Content_Types].xml", content_types([("word/styles.xml", "styles"), ("word/numbering.xml", "numbering")])),
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", document(body).replace('<w:headerReference w:type="default" r:id="rIdHeader"/>', "")),
        ("word/_rels/document.xml.rels", rels([("rIdStyles", "styles", "styles.xml", False),
                                               ("rIdNum", "numbering", "numbering.xml", False)])),
        ("word/styles.xml", STYLES),
        ("word/numbering.xml", NUMBERING),
    ])


def image(rid, descr):
    return (f'<w:r><w:drawing><wp:inline><wp:extent cx="1828800" cy="914400"/>'
            f'<wp:docPr id="1" name="Imagen 1" descr="{descr}"/>'
            f'<a:graphic><a:graphicData uri="{PIC}"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="image1.png"/><pic:cNvPicPr/></pic:nvPicPr>'
            f'<pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
            '<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="914400"/></a:xfrm><a:prstGeom prst="rect"/></pic:spPr>'
            '</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>')


def textbox(text):
    content = f'<w:txbxContent>{para(run(text))}</w:txbxContent>'
    return ('<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing><wp:anchor><wp:extent cx="1828800" cy="457200"/>'
            '<wp:docPr id="2" name="Cuadro de texto 2"/><a:graphic>'
            '<a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">'
            f'<wps:wsp><wps:spPr/><wps:txbx>{content}</wps:txbx><wps:bodyPr/></wps:wsp>'
            '</a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice>'
            f'<mc:Fallback><w:pict><v:shape><v:textbox>{content}</v:textbox></v:shape></w:pict></mc:Fallback>'
            '</mc:AlternateContent></w:r>')


def estructura():
    equation = ('<m:oMath><m:r><m:t>E=m</m:t></m:r><m:sSup><m:e><m:r><m:t>c</m:t></m:r></m:e>'
                '<m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath>')
    filler = " ".join(["El análisis compara las cifras de cada región y documenta los supuestos utilizados."] * 20)
    cell = lambda text, pr="": f"<w:tc>{f'<w:tcPr>{pr}</w:tcPr>' if pr else ''}{para(run(text) if text else '')}</w:tc>"
    table = ("<w:tbl><w:tblGrid><w:gridCol/><w:gridCol/><w:gridCol/></w:tblGrid>"
             f"<w:tr>{cell('Región')}{cell('Ventas', '<w:gridSpan w:val=\"2\"/>')}</w:tr>"
             f"<w:tr>{cell('Norte', '<w:vMerge w:val=\"restart\"/>')}{cell('100')}{cell('T1')}</w:tr>"
             f"<w:tr>{cell('', '<w:vMerge/>')}{cell('120')}{cell('T2')}</w:tr></w:tbl>")
    body = (para(run("Informe de prueba"), style="Ttulo")
            + para(run("Resultados"), style="Ttulo1")
            + para(run("Texto con ") + run("negrita", "<w:b/>") + run(" y ") + run("cursiva", "<w:i/>")
                   + run(", un ") + f'<w:hyperlink r:id="rIdLink">{run("enlace")}</w:hyperlink>'
                   + run(", una nota") + '<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteReference w:id="1"/></w:r>'
                   + run(" y la ecuación ") + equation + run("."))
            + para(run("Primer paso"), num=(1, 0))
            + para(run("Segundo paso"), num=(1, 0))
            + para(run("Detalle del segundo paso"), num=(1, 1))
            + para(run("Tercer paso"), num=(1, 0))
            + para(run(filler))
            + para(run("Datos"), style="Ttulo1")
            + table
            + para(image("rIdImage", "Diagrama del flujo") + textbox("Texto del recuadro"))
            + para(run("Cifra final de ") + '<w:commentRangeStart w:id="0"/>'
                   + f'<w:ins w:id="10" w:author="Ana" w:date="2026-10-30T10:00:00Z">{run("420")}</w:ins>'
                   + '<w:del w:id="11" w:author="Ana" w:date="2026-10-30T10:00:00Z"><w:r><w:delText>380</w:delText></w:r></w:del>'
                   + '<w:commentRangeEnd w:id="0"/><w:r><w:commentReference w:id="0"/></w:r>'
                   + run(" unidades.") + run(" Nota interna para el equipo.", "<w:vanish/>"))
            + para(run("Página ") + '<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>'
                   '<w:r><w:fldChar w:fldCharType="separate"/></w:r>' + run("7") + '<w:r><w:fldChar w:fldCharType="end"/></w:r>'))
    footnotes = (f'<?xml version="1.0" encoding="UTF-8"?><w:footnotes xmlns:w="{W}">'
                 '<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>'
                 '<w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>'
                 '<w:footnote w:id="1"><w:p><w:r><w:footnoteRef/></w:r>' + run(" Fuente de la cifra.") + '</w:p></w:footnote>'
                 '</w:footnotes>')
    comments = (f'<?xml version="1.0" encoding="UTF-8"?><w:comments xmlns:w="{W}">'
                '<w:comment w:id="0" w:author="Revisora" w:date="2026-10-30T11:00:00Z">'
                '<w:p><w:r><w:annotationRef/></w:r>' + run("Confirmar la cifra") + '</w:p></w:comment></w:comments>')
    header = f'<?xml version="1.0" encoding="UTF-8"?><w:hdr xmlns:w="{W}">{para(run("Informe interno"))}</w:hdr>'
    write("estructura.docx", [
        ("[Content_Types].xml", content_types([("word/styles.xml", "styles"), ("word/numbering.xml", "numbering"),
                                               ("word/footnotes.xml", "footnotes"), ("word/comments.xml", "comments"),
                                               ("word/header1.xml", "header")])),
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", document(body)),
        ("word/_rels/document.xml.rels", rels([
            ("rIdStyles", "styles", "styles.xml", False),
            ("rIdNum", "numbering", "numbering.xml", False),
            ("rIdFoot", "footnotes", "footnotes.xml", False),
            ("rIdComments", "comments", "comments.xml", False),
            ("rIdHeader", "header", "header1.xml", False),
            ("rIdImage", "image", "media/image1.png", False),
            ("rIdLink", "hyperlink", "https://example.com/okf", True),
        ])),
        ("word/styles.xml", STYLES),
        ("word/numbering.xml", NUMBERING),
        ("word/footnotes.xml", footnotes),
        ("word/comments.xml", comments),
        ("word/header1.xml", header),
        ("word/media/image1.png", png()),
    ])


if __name__ == "__main__":
    breve()
    estructura()
    print("breve.docx y estructura.docx generados en", OUT)
