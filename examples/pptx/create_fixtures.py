"""Build the two small OOXML fixtures used by the PPTX acceptance tests."""
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile
import struct
import zlib

OUT = Path(__file__).parent
def png():
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
    w, h = 120, 72
    rows = []
    for y in range(h):
        row = bytearray([0])
        for x in range(w):
            row.extend((36, 110, 190) if x < 84 else (236, 160, 48))
        rows.append(row)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b""))
PNG = png()
P = "http://schemas.openxmlformats.org/presentationml/2006/main"
A = "http://schemas.openxmlformats.org/drawingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"

def rels(items):
    return f'<Relationships xmlns="{PKG}">' + "".join(
        f'<Relationship Id="{i}" Type="{t}" Target="{target}"/>' for i, t, target in items
    ) + "</Relationships>"

def shape(text, ident=2, title=False):
    ph = '<p:ph type="title"/>' if title else ""
    x, y = (457200, 250000) if title else (457200, 1650000)
    h = 1100000 if title else 1500000
    size = 2600 if title else 1800
    return (f'<p:sp><p:nvSpPr><p:cNvPr id="{ident}" name="Shape {ident}"/>'
            f'<p:cNvSpPr/><p:nvPr>{ph}</p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/>'
            f'<a:ext cx="8200000" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>'
            f'<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="{size}"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:rPr>'
            f'<a:t>{text}</a:t></a:r><a:endParaRPr lang="en-US"/></a:p></p:txBody></p:sp>')

def slide_number(number="1"):
    return (f'<p:sp><p:nvSpPr><p:cNvPr id="20" name="Slide number"/><p:cNvSpPr/>'
            '<p:nvPr><p:ph type="sldNum"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="8500000" y="4700000"/><a:ext cx="300000" cy="300000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>'
            f'<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:fld id="field-1" type="slidenum">'
            f'<a:rPr lang="en-US"/><a:t>{number}</a:t></a:fld></a:p></p:txBody></p:sp>')

def slide_xml(body, hidden=False):
    show = ' show="0"' if hidden else ""
    return (f'<p:sld xmlns:p="{P}" xmlns:a="{A}" xmlns:r="{R}"{show}>'
            f'<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/>'
            '<p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr>'
            '<a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/>'
            '<a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm>'
            f'</p:grpSpPr>{body}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>')

def package(slides, output):
    files = {
        "[Content_Types].xml": ('<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/>'
            '<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>'
            '<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>'
            '<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>'
            '<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>'
            + "".join(f'<Override PartName="/ppt/slides/slide{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>' for i in range(1, len(slides)+1))
            + '<Override PartName="/ppt/notesSlides/notesSlide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml"/>'
            '</Types>'),
        "_rels/.rels": rels([("rId1", f"{R}/officeDocument", "ppt/presentation.xml")]),
        "ppt/presentation.xml": (f'<p:presentation xmlns:p="{P}" xmlns:a="{A}" xmlns:r="{R}">'
            '<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rIdMaster"/></p:sldMasterIdLst>'
            '<p:sldIdLst>' + "".join(f'<p:sldId id="{256+i}" r:id="rId{i+1}"/>' for i in range(len(slides)))
            + '</p:sldIdLst><p:sldSz cx="9144000" cy="5143500" type="wide"/></p:presentation>'),
        "ppt/_rels/presentation.xml.rels": rels(
            [(f"rId{i+1}", f"{R}/slide", f"slides/slide{i+1}.xml") for i in range(len(slides))]
            + [("rIdMaster", f"{R}/slideMaster", "slideMasters/slideMaster1.xml")]
        ),
        "ppt/slideMasters/slideMaster1.xml": (f'<p:sldMaster xmlns:p="{P}" xmlns:a="{A}" xmlns:r="{R}">'
            '<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
            '<p:grpSpPr/></p:spTree></p:cSld><p:clrMap accent1="accent1" accent2="accent2" accent3="accent3" '
            'accent4="accent4" accent5="accent5" accent6="accent6" bg1="lt1" bg2="lt2" tx1="dk1" tx2="dk2" '
            'hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="1" r:id="rIdLayout"/></p:sldLayoutIdLst></p:sldMaster>'),
        "ppt/slideMasters/_rels/slideMaster1.xml.rels": rels([
            ("rIdLayout", f"{R}/slideLayout", "../slideLayouts/slideLayout1.xml"),
            ("rIdTheme", f"{R}/theme", "../theme/theme1.xml")]),
        "ppt/slideLayouts/slideLayout1.xml": (f'<p:sldLayout xmlns:p="{P}" xmlns:a="{A}" xmlns:r="{R}" type="blank">'
            '<p:cSld name="Blank"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
            '<p:grpSpPr/></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>'),
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels": rels([("rIdMaster", f"{R}/slideMaster", "../slideMasters/slideMaster1.xml")]),
        "ppt/theme/theme1.xml": (f'<a:theme xmlns:a="{A}" name="Basic"><a:themeElements><a:clrScheme name="Basic">'
            '<a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1>'
            '<a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>'
            + "".join(f'<a:{c}><a:srgbClr val="{v}"/></a:{c}>' for c, v in [("accent1","4472C4"),("accent2","ED7D31"),("accent3","A5A5A5"),("accent4","FFC000"),("accent5","5B9BD5"),("accent6","70AD47"),("hlink","0563C1"),("folHlink","954F72")])
            + '</a:clrScheme><a:fontScheme name="Basic"><a:majorFont><a:latin typeface="Arial"/></a:majorFont>'
            '<a:minorFont><a:latin typeface="Arial"/></a:minorFont></a:fontScheme><a:fmtScheme name="Basic">'
            '<a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst>'
            '<a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst>'
            '<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>'
            '<a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst>'
            '</a:fmtScheme></a:themeElements></a:theme>'),
    }
    for i, body in enumerate(slides, 1):
        files[f"ppt/slides/slide{i}.xml"] = slide_xml(body, hidden=i == 3)
        rel = [("rIdLayout", f"{R}/slideLayout", "../slideLayouts/slideLayout1.xml")]
        if i == 1:
            rel.append(("rIdNotes", f"{R}/notesSlide", "../notesSlides/notesSlide1.xml"))
        if i == 3:
            rel.append(("rIdImage", f"{R}/image", "../media/pixel.png"))
            files["ppt/media/pixel.png"] = PNG
        files[f"ppt/slides/_rels/slide{i}.xml.rels"] = rels(rel)
    notes = (f'<p:notes xmlns:p="{P}" xmlns:a="{A}" xmlns:r="{R}"><p:cSld><p:spTree>'
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>'
        '<p:sp><p:nvSpPr><p:cNvPr id="2" name="Notes Placeholder 2"/><p:cNvSpPr/><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr>'
        '<p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Presenter notes kept separately</a:t></a:r></a:p></p:txBody></p:sp>'
        '</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>')
    files["ppt/notesSlides/notesSlide1.xml"] = notes
    files["ppt/slides/_rels/slide1.xml.rels"] = rels([
        ("rIdLayout", f"{R}/slideLayout", "../slideLayouts/slideLayout1.xml"),
        ("rIdNotes", f"{R}/notesSlide", "../notesSlides/notesSlide1.xml")])
    multiple_body = slides[-1]
    table = (f'<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="10" name="Sample table"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>'
        '<p:xfrm><a:off x="457200" y="1600000"/><a:ext cx="3000000" cy="1000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table">'
        '<a:tbl><a:tblPr firstRow="1" bandRow="1"/><a:tblGrid><a:gridCol w="1500000"/><a:gridCol w="1500000"/></a:tblGrid>'
            + "".join('<a:tr h="400000">' + "".join(f'<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="1600"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:rPr><a:t>{v}</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc>' for v in row) + '</a:tr>' for row in [("Name","Value"),("alpha","1")])
        + '</a:tbl></a:graphicData></a:graphic></p:graphicFrame>')
    picture = (f'<p:pic><p:nvPicPr><p:cNvPr id="11" name="pixel.png"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rIdImage"/>'
        '<a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="4000000" y="2000000"/><a:ext cx="1000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>')
    files["ppt/slides/slide3.xml"] = slide_xml(shape("Hidden slide contains table and image", title=True) + table + picture, hidden=True)
    files["ppt/slides/_rels/slide3.xml.rels"] = rels([
        ("rIdLayout", f"{R}/slideLayout", "../slideLayouts/slideLayout1.xml"),
        ("rIdImage", f"{R}/image", "../media/pixel.png")])
    with ZipFile(output, "w", ZIP_DEFLATED) as z:
        for name, content in files.items():
            z.writestr(name, content)

def main():
    title = shape("A simple OKF test deck", title=True)
    body = shape("One short paragraph of slide content")
    package([title + body + slide_number()], OUT / "simple.pptx")
    no_title = shape("This slide has no title placeholder")
    hidden = shape("Hidden slide contains table and image", title=True)
    package([title + body, no_title, hidden], OUT / "multiple.pptx")

if __name__ == "__main__":
    main()
